//! 方块注册表和「程序化纹理图集」。
//!
//! 图集不打包任何 PNG：所有方块花纹都是这里用像素循环画出来的。
//! 一来守住站点「不含任何游戏素材」的原则，二来首包几乎只剩 wasm 本体。

use crate::noise::hash2;

pub const AIR: u8 = 0;
pub const GRASS: u8 = 1;
pub const DIRT: u8 = 2;
pub const STONE: u8 = 3;
pub const SAND: u8 = 4;
pub const WATER: u8 = 5;
pub const WOOD: u8 = 6;
pub const PLANK: u8 = 7;
pub const LEAF: u8 = 8;
pub const SNOW: u8 = 9;
pub const LAMP: u8 = 10;
pub const SNOW_GRASS: u8 = 11; // 生成时用的过渡块：草但顶面是雪
pub const WORKBENCH: u8 = 12; // 工作台：合成高阶配方的门槛，只能合成出来

/// 创造模式下给的全套（水也算：创造能凭空放出水来）。
pub const ALL_PLACEABLE: [u8; 11] = [
    GRASS, DIRT, STONE, SAND, WOOD, PLANK, LEAF, SNOW, LAMP, WORKBENCH, WATER,
];

// 纹理图集：14 张 16x16 的贴片排在同一行，尺寸 224x16。
pub const TILE_SIDE: u8 = 0;
pub const TILE_TOP: u8 = 1;
pub const TILE_BOTTOM: u8 = 2;
pub const TILE_GRASS_SIDE: u8 = 3;
pub const TILE_WOOD_SIDE: u8 = 4;
pub const TILE_WOOD_TOP: u8 = 5;
pub const TILE_LEAF: u8 = 6;
pub const TILE_SNOW_TOP: u8 = 7;
pub const TILE_SNOW_SIDE: u8 = 8;
pub const TILE_LAMP: u8 = 9;
pub const TILE_PLANK: u8 = 10;
pub const TILE_WATER: u8 = 11;
pub const TILE_BENCH_SIDE: u8 = 12;
pub const TILE_BENCH_TOP: u8 = 13;

pub const TILE_PX: u32 = 16;
pub const TILE_COUNT: u32 = 14;

/// 水的动画帧数。图集里水**只占一格**（TILE_WATER），运行时把第 k 帧覆写进那一格：
/// 不动网格、不重建 chunk、也不用给顶点加"第几帧"这一维（OpusMC 那边是靠纹理层号换帧，
/// 我们没有纹理数组，覆写同一格是最省事的等价做法）。
pub const WATER_FRAMES: usize = 3;
pub const ATLAS_W: u32 = TILE_PX * TILE_COUNT; // 224
pub const ATLAS_H: u32 = TILE_PX; // 16

/// 快捷栏：数字键 1..9 的顺序。
pub const HOTBAR: [u8; 9] = [GRASS, DIRT, STONE, SAND, WOOD, PLANK, LEAF, SNOW, LAMP];

pub fn is_air(id: u8) -> bool {
    id == AIR
}

/// 会挡碰撞、也会挡住邻居面（不生成面）的方块。
pub fn is_solid(id: u8) -> bool {
    matches!(
        id,
        GRASS | DIRT | STONE | SAND | WOOD | PLANK | LEAF | SNOW | LAMP | SNOW_GRASS | WORKBENCH
    )
}

/// 不参与遮挡的方块（空气和水）。水下单面可见，所以水也走「邻居不遮挡」这条路。
pub fn is_transparent(id: u8) -> bool {
    id == AIR || id == WATER
}

pub fn is_emissive(id: u8) -> bool {
    id == LAMP
}

/// 水：不能挖开放，碰撞和重力在 phys.rs 里单独处理。
pub fn is_water(id: u8) -> bool {
    id == WATER
}

/// 射线能不能穿过（水可以穿过，但不能被瞄准）。
pub fn ray_passable(id: u8) -> bool {
    id == AIR || id == WATER
}

pub fn face_tile(id: u8, face: usize) -> u8 {
    // face: 0=-X 1=+X 2=-Y 3=+Y 4=-Z 5=+Z（和 mesh.rs 里的顺序一致）
    let side = matches!(face, 0 | 1 | 4 | 5);
    match id {
        GRASS => {
            if face == 3 {
                TILE_TOP
            } else if side {
                TILE_GRASS_SIDE
            } else {
                TILE_BOTTOM
            }
        }
        SNOW_GRASS => {
            if face == 3 {
                TILE_SNOW_TOP
            } else if side {
                TILE_SNOW_SIDE
            } else {
                TILE_BOTTOM
            }
        }
        SNOW => TILE_SNOW_TOP,
        DIRT => TILE_SIDE,
        STONE => TILE_SIDE,
        SAND => TILE_SIDE,
        WATER => TILE_WATER,
        WOOD => {
            if face == 2 || face == 3 {
                TILE_WOOD_TOP
            } else {
                TILE_WOOD_SIDE
            }
        }
        PLANK => TILE_PLANK,
        LEAF => TILE_LEAF,
        LAMP => TILE_LAMP,
        WORKBENCH => {
            if face == 3 {
                TILE_BENCH_TOP
            } else {
                TILE_BENCH_SIDE
            }
        }
        _ => TILE_SIDE,
    }
}

pub fn block_name(id: u8) -> &'static str {
    match id {
        GRASS => "草方块",
        SNOW_GRASS => "雪草",
        DIRT => "泥土",
        STONE => "石头",
        SAND => "沙子",
        WATER => "水",
        WOOD => "木头",
        PLANK => "木板",
        LEAF => "树叶",
        SNOW => "雪块",
        LAMP => "萤石灯",
        WORKBENCH => "工作台",
        _ => "空气",
    }
}

// ---------------------------------------------------------------------------
// 图集生成
// ---------------------------------------------------------------------------

#[inline]
fn rgb(r: u8, g: u8, b: u8) -> [u8; 3] {
    [r, g, b]
}

/// 每个贴片内的抖动量：用贴图坐标哈希，保证纹理稳定但看着不单调。
#[inline]
fn jitter(tx: u32, ty: u32, tile: u32) -> f32 {
    let h = hash2(tx as i32, ty as i32, 1000 + tile);
    (h - 0.5) * 2.0
}

#[inline]
fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    [
        (c[0] as f32 * k).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * k).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * k).clamp(0.0, 255.0) as u8,
    ]
}

fn paint_tile(tile: u8, x: u32, y: u32) -> [u8; 3] {
    let j = jitter(x, y, tile as u32);
    match tile {
        TILE_TOP => {
            let base = rgb(104, 154, 62);
            shade(base, 1.0 + j * 0.13)
        }
        TILE_GRASS_SIDE => {
            let dirt = rgb(122, 86, 55);
            let grass = rgb(98, 146, 58);
            let fringe = 4.0 + (hash2(x as i32, 0, 77) * 3.0).floor(); // 参差不齐的草根
            if (y as f32) < fringe {
                shade(grass, 1.0 + j * 0.10)
            } else {
                shade(dirt, 1.0 + j * 0.12)
            }
        }
        TILE_SIDE => {
            // 通用「石/土」质感：大斑块 + 细噪点
            let blob = hash2((x / 4) as i32, (y / 4) as i32, 31);
            let base = rgb(136, 136, 143);
            shade(base, 1.0 + (blob - 0.5) * 0.16 + j * 0.06)
        }
        TILE_BOTTOM => shade(rgb(116, 82, 52), 1.0 + j * 0.12),
        TILE_WOOD_SIDE => {
            let stripe = if (x / 3) % 2 == 0 { 1.0 } else { 0.0 }; // 竖向木纹
            let base = rgb(107, 74, 43);
            shade(base, 0.88 + stripe * 0.22 + j * 0.06)
        }
        TILE_WOOD_TOP => {
            let dx = x as f32 - 7.5;
            let dy = y as f32 - 7.5;
            let r = (dx * dx + dy * dy).sqrt();
            let ring = ((r * 1.6).sin() + 1.0) * 0.5; // 年轮
            let base = rgb(150, 112, 66);
            shade(base, 0.82 + ring * 0.26 + j * 0.05)
        }
        TILE_LEAF => {
            let hole = hash2(x as i32, y as i32, 404) < 0.16;
            let base = rgb(74, 120, 44);
            if hole {
                shade(base, 0.55)
            } else {
                shade(base, 1.0 + j * 0.16)
            }
        }
        TILE_SNOW_TOP => shade(rgb(232, 238, 246), 1.0 + j * 0.05),
        TILE_SNOW_SIDE => {
            if y < 5 {
                shade(rgb(232, 238, 246), 1.0 + j * 0.05)
            } else {
                shade(rgb(122, 86, 55), 1.0 + j * 0.12)
            }
        }
        TILE_LAMP => {
            let dx = x as f32 - 7.5;
            let dy = y as f32 - 7.5;
            let r = (dx * dx + dy * dy).sqrt();
            let dot = (r * 1.9).sin() > 0.55;
            if dot {
                rgb(255, 236, 168)
            } else {
                shade(rgb(214, 168, 66), 1.0 + j * 0.10)
            }
        }
        TILE_PLANK => {
            let seam = if y % 8 == 0 { 1.0 } else { 0.0 }; // 横向板缝
            let base = rgb(181, 138, 78);
            shade(base, 1.0 - seam * 0.30 + j * 0.07)
        }
        TILE_BENCH_SIDE => {
            // 侧面：木板上沿压一道深色边，中间两条工具痕
            let base = rgb(168, 126, 72);
            if y < 3 {
                shade(base, 0.66 + j * 0.05)
            } else if y == 7 || y == 11 {
                shade(base, 0.78 + j * 0.05)
            } else {
                shade(base, 1.0 + j * 0.07)
            }
        }
        TILE_BENCH_TOP => {
            // 顶面：四块板拼的井字格，正是「工作台」那张脸
            let base = rgb(188, 144, 84);
            let seam = (x % 8 == 0) as u32 + (y % 8 == 0) as u32;
            if seam > 0 {
                shade(base, 0.68 + j * 0.04)
            } else {
                shade(base, 1.0 + j * 0.07)
            }
        }
        TILE_WATER => paint_water(x, y, 0),
        _ => rgb(200, 200, 200),
    }
}

/// 水的一帧。只有波纹的相位随 frame 走，**噪点抖动不动**：连噪点一起换帧就是一锅乱闪，
/// 不像水在动，像电视没信号。
fn paint_water(x: u32, y: u32, frame: usize) -> [u8; 3] {
    let step = std::f32::consts::PI * 2.0 / WATER_FRAMES as f32;
    let band = (((y as f32 + x as f32 * 0.35) * 1.1 + frame as f32 * step).sin() + 1.0) * 0.5;
    let j = jitter(x, y, TILE_WATER as u32);
    shade(rgb(56, 108, 176), 0.90 + band * 0.22 + j * 0.05)
}

/// 第 frame 帧水的 16×16 RGBA（1024 字节，alpha 一律 255：水在我们这儿是不透明的）。
/// 运行时拿它覆写图集里 TILE_WATER 那一格。
pub fn water_frame_rgba(frame: usize) -> Vec<u8> {
    let f = frame % WATER_FRAMES;
    let mut out = Vec::with_capacity((TILE_PX * TILE_PX * 4) as usize);
    for y in 0..TILE_PX {
        for x in 0..TILE_PX {
            let c = paint_water(x, y, f);
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    out
}

/// 生成 RGBA 图集。水用的是它自己的 11 号贴片（TILE_WATER，波纹直接画在像素里），
/// mesh 里没有为水单开的 tint 分支：水的明暗和其它方块一样只走「朝向明暗 × AO」。
pub fn atlas_rgba() -> Vec<u8> {
    let mut out = vec![255u8; (ATLAS_W * ATLAS_H * 4) as usize];
    for tile in 0..TILE_COUNT {
        for y in 0..TILE_PX {
            for x in 0..TILE_PX {
                let c = paint_tile(tile as u8, x, y);
                let i = (((y * ATLAS_W) + (tile * TILE_PX + x)) * 4) as usize;
                out[i] = c[0];
                out[i + 1] = c[1];
                out[i + 2] = c[2];
                out[i + 3] = 255;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 水有三帧彼此不同而图集里那一格就是第零帧() {
        let f0 = water_frame_rgba(0);
        let f1 = water_frame_rgba(1);
        let f2 = water_frame_rgba(2);
        assert_eq!(f0.len(), (TILE_PX * TILE_PX * 4) as usize);
        assert_eq!(f1.len(), f0.len());
        assert_ne!(f0, f1, "两帧一模一样就没有动画");
        assert_ne!(f1, f2);
        assert_ne!(f2, f0);
        // 帧数按 WATER_FRAMES 取模，超出别越界
        assert_eq!(water_frame_rgba(WATER_FRAMES * 7 + 1), f1);
        // alpha 全不透明（图集一直是这个约定，改了就有人透贴漏底）
        assert!(f0.iter().skip(3).step_by(4).all(|&a| a == 255));
        // 三帧的平均亮度得接近：只换相位不该整体变明变暗
        let lum = |v: &[u8]| -> i64 {
            v.iter().take(v.len() / 4 * 4).step_by(4).map(|&c| c as i64).sum()
        };
        assert!((lum(&f0) - lum(&f1)).abs() < lum(&f0) / 20, "换帧把整体亮度换了，看着会闪");
        // 图集里那一格必须等于第 0 帧，不然运行时第一覆写就会"跳一下"
        let a = atlas_rgba();
        let x0 = (TILE_WATER as u32 * TILE_PX) as usize;
        for y in 0..(TILE_PX as usize) {
            let row = ((y * ATLAS_W as usize) + x0) * 4;
            assert_eq!(&a[row..row + (TILE_PX as usize) * 4], &f0[y * 64..(y + 1) * 64]);
        }
    }

    #[test]
    fn 图集尺寸和声明一致() {
        let a = atlas_rgba();
        assert_eq!(a.len(), (ATLAS_W * ATLAS_H * 4) as usize);
        for px in a.chunks_exact(4) {
            assert_eq!(px[3], 255, "图集不该有透明像素，透明会让方块面漏光");
        }
    }

    #[test]
    fn 每种方块的每一面都用得上图集里的贴片() {
        for id in 1..=WORKBENCH {
            for face in 0..6 {
                let t = face_tile(id, face);
                assert!((t as u32) < TILE_COUNT, "方块 {} 第 {} 面指到了图外的贴片 {}", id, face, t);
            }
        }
    }

    #[test]
    fn 工作台是实体方块也在创造全套里() {
        assert!(is_solid(WORKBENCH));
        assert!(!is_transparent(WORKBENCH));
        assert_eq!(block_name(WORKBENCH), "工作台");
        assert!(ALL_PLACEABLE.contains(&WORKBENCH));
        assert_eq!(ALL_PLACEABLE.len(), 11);
        for id in ALL_PLACEABLE {
            assert!(!is_air(id));
        }
    }

    #[test]
    fn 快捷栏里的方块都实体可放置() {
        for id in HOTBAR {
            assert!(!is_air(id));
            assert!(!is_water(id));
            assert!(!is_transparent(id), "{id} 不能放进修造栏");
        }
    }
}
