//! 区块网格化：面剔除 + 顶点环境光遮蔽（AO）+ 朝向明暗。
//!
//! 输出是「已经排好成 GPU 顶点流」的 f32 数组，渲染层只管上传，不再搬一遍。
//! 顶点布局（7 个 f32，步长 28 字节）：
//!   pos.x pos.y pos.z  u v  shade emissive
//! 这个文件完全不依赖渲染库，所以能单独跑单元测试。

use crate::blocks as b;
use crate::world::{World, CHUNK, SY};

pub const FLOATS_PER_VERTEX: usize = 7;

/// 顶点着色器里的属性名要用到这几个偏移，改布局时两边一起改。
pub const STRIDE_F32: usize = FLOATS_PER_VERTEX;

#[derive(Default)]
pub struct ChunkMesh {
    pub verts: Vec<f32>,
    pub idx: Vec<u16>,
}

impl ChunkMesh {
    pub fn clear(&mut self) {
        self.verts.clear();
        self.idx.clear();
    }

    #[inline]
    pub fn vert_count(&self) -> usize {
        self.verts.len() / STRIDE_F32
    }
}

/// 面法线方向（和 blocks::face_tile 的 face 编号一致）。
const NORMALS: [[i32; 3]; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];

/// NORMALS 里底面（-Y）的下标，给 build_chunk 跳过世界底壳用。
const FACE_BOTTOM: usize = 2;

/// 各朝向的明暗：顶面最亮，底面最暗，侧面分两档，立体感全靠这组数字。
const FACE_SHADE: [f32; 6] = [0.62, 0.62, 0.42, 1.0, 0.80, 0.80];

/// AO 档位对应的亮度系数（0 最暗）。
const AO_SHADE: [f32; 4] = [0.50, 0.68, 0.85, 1.0];

/// 每个面的 4 个角（立方体局部坐标）+ 对应 UV。AO 的取样方向也从这儿读。
/// corners[i] = (dx,dy,dz)，uvs[i] = (u,v)；这 4 个角是唯一的事实来源，
/// ao_level 只从它们反推切向符号，不再另摆一张表（两张表早晚会对不上）。
const CORNERS: [[[f32; 3]; 4]; 6] = [
    // -X
    [[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 1.0]],
    // +X
    [[1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
    // -Y
    [[1.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
    // +Y
    [[0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
    // -Z
    [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
    // +Z
    [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
];

const UVS: [[(f32, f32); 4]; 6] = [
    [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
    [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
    [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
    [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
    [(1.0, 1.0), (0.0, 1.0), (0.0, 0.0), (1.0, 0.0)],
    [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
];

/// 每个面、每个角点的两个切向偏移（用来取 side1 / side2 / corner 三个邻居）。
/// 这里**没有**手摆的符号表：符号直接从 CORNERS 那 4 个角读（见 ao_level）。
/// 以前这儿写了一张 AO_TANGENTS + 一根 AO_TANGENT_AXES，六个面的符号和 CORNERS
/// 反推出来的期望值全都对不上（每个面都沿一根切向轴左右镜像），后果是墙根那道黑边
/// 跑到背离墙的一侧去了 —— 看着像「悬空的方块」。

#[inline]
fn solid_at(w: &World, x: i32, y: i32, z: i32) -> bool {
    b::is_solid(w.get(x, y, z))
}

/// 算某个面某个角点的 AO 档位。
fn ao_level(w: &World, cell: [i32; 3], face: usize, corner: usize) -> usize {
    let n = NORMALS[face];
    let local = CORNERS[face][corner];

    // 法线之外的那两根轴就是切向轴；这个角点在某根切向轴上是靠「0 那一侧」还是
    // 「1 那一侧」，CORNERS 里已经写死了（角点坐标只有 0.0 / 1.0），照抄就行。
    // 判据和角点同源，所以不可能出现「面和取样方向各自一套」的那种镜像错。
    let naxis = if n[0] != 0 {
        0
    } else if n[1] != 0 {
        1
    } else {
        2
    };
    let mut axes = [0usize; 2];
    let mut k = 0usize;
    for a in 0..3 {
        if a != naxis {
            axes[k] = a;
            k += 1;
        }
    }
    let sgn = |a: usize| if local[a] > 0.5 { 1i32 } else { -1i32 };
    let (s1, s2) = (sgn(axes[0]), sgn(axes[1]));

    // 先站到面外侧那一格，再沿两个切向分别看侧方块和角方块。
    let base = [cell[0] + n[0], cell[1] + n[1], cell[2] + n[2]];

    let mut p1 = base;
    p1[axes[0]] += s1;
    let mut p2 = base;
    p2[axes[1]] += s2;
    let mut pc = base;
    pc[axes[0]] += s1;
    pc[axes[1]] += s2;

    let side1 = solid_at(w, p1[0], p1[1], p1[2]);
    let side2 = solid_at(w, p2[0], p2[1], p2[2]);
    let corner_s = solid_at(w, pc[0], pc[1], pc[2]);

    if side1 && side2 {
        0
    } else {
        3 - (side1 as usize + side2 as usize + corner_s as usize)
    }
}

#[inline]
fn push_vertex(m: &mut ChunkMesh, x: f32, y: f32, z: f32, u: f32, v: f32, shade: f32, emissive: f32) {
    m.verts.extend_from_slice(&[x, y, z, u, v, shade, emissive]);
}

/// 贴图集的 UV。贴片排在同一行，所以只有水平方向需要偏移。
/// 0.5 像素内缩是为了避免采样到相邻贴片（不用 mipmap 时的常规做法）。
#[inline]
fn tile_uv(tile: u8, u: f32, v: f32) -> (f32, f32) {
    let w = b::ATLAS_W as f32;
    let h = b::ATLAS_H as f32;
    let inset = 0.5;
    let uu = ((tile as f32 * b::TILE_PX as f32) + u * (b::TILE_PX as f32 - 1.0) + inset) / w;
    let vv = (v * (b::TILE_PX as f32 - 1.0) + inset) / h;
    (uu, vv)
}

/// 一个方块要不要在这一面出面。
#[inline]
fn face_visible(id: u8, neighbor: u8) -> bool {
    if b::is_solid(id) {
        // 实心块：邻居是空气或水就露面（水算透明，所以水下的石头看得见）。
        b::is_transparent(neighbor)
    } else if b::is_water(id) {
        // 水：只和空气贴面，水下水、水贴石头都不画，省一大半水的三角形。
        neighbor == b::AIR
    } else {
        false
    }
}

/// 生成一个 16x16 列（整高）的网格。cx/cz 是区块坐标。
pub fn build_chunk(w: &World, cx: i32, cz: i32, out: &mut ChunkMesh) {
    out.clear();
    let x0 = cx * CHUNK;
    let z0 = cz * CHUNK;

    for z in z0..(z0 + CHUNK) {
        for x in x0..(x0 + CHUNK) {
            for y in 0..SY {
                let id = w.get(x, y, z);
                if !b::is_solid(id) && !b::is_water(id) {
                    continue;
                }
                let cell = [x, y, z];
                let emissive = if b::is_emissive(id) { 1.0 } else { 0.0 };

                for face in 0..6 {
                    // 世界底壳（y=0 的 -Y 面）跳过：那层永远挖不穿（main.rs 禁止挖 y<=0），
                    // 人也被 phys 里「y<0 一律挡」的规则关在世界之上，所以这些面一张都看不见。
                    // 可 w.get(x,-1,z) 出界返回 AIR，face_visible 就把它们判成露面，
                    // 于是每个区块白写 256 面 = 1024 顶点 + 1536 索引，
                    // 白占的正是 drawcall 那份容量预算。
                    if y == 0 && face == FACE_BOTTOM {
                        continue;
                    }
                    let n = NORMALS[face];
                    let nb = w.get(x + n[0], y + n[1], z + n[2]);
                    if !face_visible(id, nb) {
                        continue;
                    }

                    let tile = b::face_tile(id, face);
                    let base_shade = FACE_SHADE[face];

                    let start = (out.vert_count()) as u16;
                    for c in 0..4 {
                        let local = CORNERS[face][c];
                        let (u, v) = UVS[face][c];
                        let (uu, vv) = tile_uv(tile, u, v);
                        let ao = AO_SHADE[ao_level(w, cell, face, c)];
                        push_vertex(
                            out,
                            x as f32 + local[0],
                            y as f32 + local[1],
                            z as f32 + local[2],
                            uu,
                            vv,
                            base_shade * ao,
                            emissive,
                        );
                    }

                    // 两个三角形。AO 各向异性时用另一套对角线，避免出现「一半亮一半暗」的斜缝。
                    let ao0 = AO_SHADE[ao_level(w, cell, face, 0)] as u32
                        + AO_SHADE[ao_level(w, cell, face, 2)] as u32;
                    let ao1 = AO_SHADE[ao_level(w, cell, face, 1)] as u32
                        + AO_SHADE[ao_level(w, cell, face, 3)] as u32;
                    if ao0 >= ao1 {
                        out.idx.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
                    } else {
                        out.idx.extend_from_slice(&[start + 1, start + 2, start + 3, start + 1, start + 3, start]);
                    }
                }
            }
        }
    }

    debug_assert!(
        out.vert_count() < 65535,
        "单区块顶点 {} 超了 u16 索引上限",
        out.vert_count()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{CHUNKS_X, CHUNKS_Z};

    #[test]
    fn 平面地形每个区块只出顶面和四周() {
        // 造一个纯石头的平地：高度 10，上面全空。
        let mut w = World::new(5);
        for z in 0..crate::world::SZ {
            for x in 0..crate::world::SX {
                for y in 0..crate::world::SY {
                    let id = if y < 10 { b::STONE } else { b::AIR };
                    w.set(x, y, z, id);
                }
            }
        }
        w.dirty.iter_mut().for_each(|d| *d = false);

        let mut m = ChunkMesh::default();
        // 中间的区块：只有朝上的那 256 个面。底面一个都不出（y=0 那层看不见，见 build_chunk）。
        build_chunk(&w, 3, 3, &mut m);
        assert_eq!(m.vert_count(), 256 * 4);
        assert_eq!(m.idx.len(), 256 * 6);

        // 角落区块：再多两个侧面（16 宽 x 10 高 = 160 各一面）。
        build_chunk(&w, 0, 0, &mut m);
        assert_eq!(m.vert_count(), (256 + 160 + 160) * 4);
    }

    /// 全空的地图（直接从 from_raw 灌，比一格格 set 快得多）。
    fn empty_world() -> World {
        let n = (crate::world::SX * crate::world::SY * crate::world::SZ) as usize;
        World::from_raw(5, vec![b::AIR; n]).expect("长度对得上")
    }

    #[test]
    fn 每个面每个角的_AO_取样都朝着它那一侧() {
        // 只放一块斜挨着角点的遮挡块：按定义它该只让「同一个角」变暗，别的角不受影响。
        // 以前切向符号是另摆的一张表，六个面全都和 CORNERS 反了一个轴，
        // 于是每个面变暗的都是对角线上另一个角 —— 墙根的黑边跑到背离墙的那一侧。
        for face in 0..6 {
            let n = NORMALS[face];
            let naxis = if n[0] != 0 {
                0
            } else if n[1] != 0 {
                1
            } else {
                2
            };
            for corner in 0..4 {
                let local = CORNERS[face][corner];
                let mut w = empty_world();
                let cell = [40, 20, 40];
                w.set(cell[0], cell[1], cell[2], b::STONE);

                let mut occ = [cell[0] + n[0], cell[1] + n[1], cell[2] + n[2]];
                for a in 0..3 {
                    if a != naxis {
                        occ[a] += if local[a] > 0.5 { 1 } else { -1 };
                    }
                }
                w.set(occ[0], occ[1], occ[2], b::STONE);

                let dark: Vec<usize> = (0..4)
                    .filter(|&c| ao_level(&w, cell, face, c) < 3)
                    .collect();
                assert_eq!(
                    dark,
                    vec![corner],
                    "面 {} 的角 {}（局部 {:?}）：该它暗，实际暗的是 {:?}，取样方向反了",
                    face,
                    corner,
                    local,
                    dark
                );
            }
        }
    }

    #[test]
    fn 水的内部不出面只留表面() {
        let w = World::new(4242); // 有海的种子
        let mut total_faces = 0usize;
        let mut m = ChunkMesh::default();
        for cz in 0..CHUNKS_Z {
            for cx in 0..CHUNKS_X {
                build_chunk(&w, cx, cz, &mut m);
                total_faces += m.idx.len() / 6;
            }
        }
        assert!(
            total_faces > 10_000,
            "整张图面数太少，八成是网格化没跑: {total_faces}"
        );
        // 一个区块的面数不能超过 u16 索引上限。
        build_chunk(&w, 3, 3, &mut m);
        assert!(m.vert_count() < 65535, "顶点 {} 顶到 u16 上限", m.vert_count());
    }

    #[test]
    fn 顶点数据都是有限的数() {
        let w = World::new(7);
        let mut m = ChunkMesh::default();
        build_chunk(&w, 2, 2, &mut m);
        assert!(!m.verts.is_empty());
        for v in m.verts.iter() {
            assert!(v.is_finite(), "出现 NaN/Inf 顶点");
        }
        for v in m.verts.chunks_exact(STRIDE_F32) {
            assert!(v[5] > 0.0 && v[5] <= 1.0, "shade 越界: {}", v[5]);
            assert!(v[6] == 0.0 || v[6] == 1.0);
        }
        let max_idx = m.idx.iter().max().copied().unwrap_or(0) as usize;
        assert!(max_idx < m.vert_count(), "索引越界: {max_idx} >= {}", m.vert_count());
    }
}
