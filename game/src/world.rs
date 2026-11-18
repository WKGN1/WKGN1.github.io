//! 世界存储与生成。
//!
//! 刻意把地图做小（96x96x48）：小到可以一次性把整张图的网格生成完，
//! 于是「区块流式加载 + 线程池 + 主线程不卡顿」这一整套系统就不用写了。
//! 这是这个小彩蛋最大的省力点。

use crate::blocks::{self as b, AIR};
use crate::noise::{fbm, hash2};

pub const SX: i32 = 96;
pub const SY: i32 = 48;
pub const SZ: i32 = 96;

/// 网格按 16x16 列切块（整高），共 6x6 = 36 个网格。
pub const CHUNK: i32 = 16;
pub const CHUNKS_X: i32 = SX / CHUNK;
pub const CHUNKS_Z: i32 = SZ / CHUNK;
pub const CHUNK_COUNT: usize = (CHUNKS_X * CHUNKS_Z) as usize;

pub const WATER_LEVEL: i32 = 15;

pub struct World {
    pub seed: u32,
    pub data: Vec<u8>,
    /// 每个网格的脏标记：挖/放方块后只要重建这一格（跨边界时多带一格）。
    pub dirty: Vec<bool>,
    pub changed: u32,
}

#[inline]
pub fn idx(x: i32, y: i32, z: i32) -> usize {
    (((y as usize) * (SZ as usize)) + (z as usize)) * (SX as usize) + (x as usize)
}

#[inline]
pub fn in_bounds(x: i32, y: i32, z: i32) -> bool {
    x >= 0 && x < SX && y >= 0 && y < SY && z >= 0 && z < SZ
}

impl World {
    pub fn new(seed: u32) -> Self {
        let mut w = World {
            seed,
            data: vec![AIR; SX as usize * SY as usize * SZ as usize],
            dirty: vec![true; CHUNK_COUNT],
            changed: 0,
        };
        w.generate();
        w
    }

    /// 从存档恢复：只灌数据，脏标记全置真，网格会全部重建。
    pub fn from_raw(seed: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != SX as usize * SY as usize * SZ as usize {
            return None;
        }
        // 「改动」得跟着存档走：它的意思是"和种子初始相比被玩家动过多少格"，那就照同一颗种子
        // 重建一份对比着数出来。以前这里写死 0，于是刷新之后画面里坑明明还在、HUD 却说
        // 你一个方块都没动过（实测：挖一格 → 刷新 → 画面差 5.4%，改动显示 0）。
        let base = World::new(seed);
        let mut changed = 0u32;
        for (i, &v) in data.iter().enumerate() {
            if base.data.get(i) != Some(&v) {
                changed += 1;
            }
        }
        Some(World {
            seed,
            data,
            dirty: vec![true; CHUNK_COUNT],
            changed,
        })
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if !in_bounds(x, y, z) {
            return AIR; // 世界外按空气算，这样岛的侧面会被网格化，看得出轮廓
        }
        self.data[idx(x, y, z)]
    }

    /// 碰撞专用的「挡不挡」：出界（水平方向）当墙，避免读者走出地图掉进虚空。
    #[inline]
    pub fn blocks_movement(&self, x: i32, y: i32, z: i32) -> bool {
        if y < 0 {
            return true;
        }
        if y >= SY {
            return false;
        }
        if x < 0 || x >= SX || z < 0 || z >= SZ {
            return true;
        }
        b::is_solid(self.data[idx(x, y, z)])
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u8) {
        if !in_bounds(x, y, z) {
            return;
        }
        let old = self.data[idx(x, y, z)];
        if old == id {
            return;
        }
        self.data[idx(x, y, z)] = id;
        self.changed += 1;
        self.mark_dirty_around(x, y, z);
    }

    fn mark_dirty_around(&mut self, x: i32, _y: i32, z: i32) {
        let cx = x / CHUNK;
        let cz = z / CHUNK;
        let touch = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dz) in touch {
            let nx = cx + dx;
            let nz = cz + dz;
            if nx < 0 || nz < 0 || nx >= CHUNKS_X || nz >= CHUNKS_Z {
                continue;
            }
            // 只有贴着边界时才需要重建邻居，否则邻居的可见面不受影响。
            let edge_x = dx != 0 && (x % CHUNK == 0 && dx < 0 || x % CHUNK == CHUNK - 1 && dx > 0);
            let edge_z = dz != 0 && (z % CHUNK == 0 && dz < 0 || z % CHUNK == CHUNK - 1 && dz > 0);
            if dx == 0 && dz == 0 || edge_x || edge_z {
                self.dirty[(nz * CHUNKS_X + nx) as usize] = true;
            }
        }
    }

    fn generate(&mut self) {
        let seed = self.seed;
        let cx = SX as f32 * 0.5;
        let cz = SZ as f32 * 0.5;

        for z in 0..SZ {
            for x in 0..SX {
                // 大形状 + 中尺度起伏 + 一点细节，三档够看了。
                let shape = fbm(x as f32 / 34.0, z as f32 / 34.0, seed, 3);
                let detail = fbm(x as f32 / 7.0, z as f32 / 7.0, seed.wrapping_add(0x51ed), 2);

                // 岛形衰减：边缘沉进海里，从远处看是一座岛而不是一块切平的地板。
                let dx = (x as f32 - cx) / cx;
                let dz = (z as f32 - cz) / cz;
                let d = (dx * dx + dz * dz).sqrt();
                let fall = (1.0 - (d - 0.42).max(0.0) / 0.62).clamp(0.0, 1.0);

                let mut h = (9.0 + shape * 16.0 + detail * 4.0) * (0.25 + 0.75 * fall);
                h = h.round().clamp(1.0, (SY - 12) as f32);
                let h = h as i32;

                let top = if h <= WATER_LEVEL {
                    b::SAND
                } else if h >= 25 {
                    b::SNOW_GRASS
                } else {
                    b::GRASS
                };

                for y in 0..=h {
                    let id = if y < h - 3 {
                        b::STONE
                    } else if y < h {
                        b::DIRT
                    } else {
                        top
                    };
                    self.data[idx(x, y, z)] = id;
                }
                for y in (h + 1)..=WATER_LEVEL {
                    self.data[idx(x, y, z)] = b::WATER;
                }
            }
        }

        self.grow_trees();
    }

    fn grow_trees(&mut self) {
        let seed = self.seed;
        for z in 2..(SZ - 2) {
            for x in 2..(SX - 2) {
                let h = self.top_solid(x, z);
                if h < 0 || self.get(x, h, z) != b::GRASS {
                    continue;
                }
                if hash2(x, z, seed.wrapping_add(991)) >= 0.012 {
                    continue;
                }
                let trunk = 4 + ((hash2(x, z, seed.wrapping_add(13)) * 3.0) as i32); // 4..6
                let top_y = h + trunk;
                if top_y + 3 >= SY {
                    continue;
                }
                for y in (h + 1)..=top_y {
                    self.data[idx(x, y, z)] = b::WOOD;
                }
                // 叶冠：半径 2 的钝球 + 顶上一小块。
                for dy in -2..=1 {
                    let r = if dy == 1 { 1 } else if dy == -2 { 1 } else { 2 };
                    for oz in -r..=r {
                        for ox in -r..=r {
                            if ox * ox + oz * oz > r * r + 1 {
                                continue;
                            }
                            let (lx, ly, lz) = (x + ox, top_y + dy, z + oz);
                            if !in_bounds(lx, ly, lz) {
                                continue;
                            }
                            let i = idx(lx, ly, lz);
                            if self.data[i] == AIR {
                                self.data[i] = b::LEAF;
                            }
                        }
                    }
                }
            }
        }
    }

    /// 从上往下找第一个实心方块的高度，找不到返回 -1。
    pub fn top_solid(&self, x: i32, z: i32) -> i32 {
        for y in (0..SY).rev() {
            if b::is_solid(self.get(x, y, z)) {
                return y;
            }
        }
        -1
    }

    /// 出生点：地图中心附近最平整的一块草地顶上。
    pub fn spawn(&self) -> [f32; 3] {
        let mut best = (SX / 2, SZ / 2);
        let mut best_score = f32::MAX;
        for z in (SZ / 2 - 12)..(SZ / 2 + 12) {
            for x in (SX / 2 - 12)..(SX / 2 + 12) {
                let h = self.top_solid(x, z);
                if h <= WATER_LEVEL {
                    continue; // 不往水里扔人
                }
                let flat = (self.top_solid(x + 1, z) - h).abs() + (self.top_solid(x, z + 1) - h).abs();
                // 偏好平坦、且不太高的地方。
                let score = flat as f32 * 2.0 + (h as f32 - 18.0).abs() * 0.3;
                if flat <= 1 && score < best_score {
                    best_score = score;
                    best = (x, z);
                }
            }
        }
        let h = self.top_solid(best.0, best.1).max(WATER_LEVEL + 1);
        [
            best.0 as f32 + 0.5,
            (h + 1) as f32,
            best.1 as f32 + 0.5,
        ]
    }

    /// 玩家脚底所在格子是不是水（用来切换浮力和移动速度）。
    #[inline]
    pub fn water_at(&self, x: f32, y: f32, z: f32) -> bool {
        let cell = self.get(x.floor() as i32, y.floor() as i32, z.floor() as i32);
        b::is_water(cell)
    }

    pub fn take_bytes(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 生成的地图不会把人生成在水里或空处() {
        for seed in [1u32, 7, 42, 1337, 90218] {
            let w = World::new(seed);
            let [x, y, z] = w.spawn();
            let (xi, zi) = (x.floor() as i32, z.floor() as i32);
            assert!(y > (WATER_LEVEL + 1) as f32, "seed {seed} 出生点太低");
            assert!(
                b::is_solid(w.get(xi, (y - 1.0) as i32, zi)),
                "seed {seed} 出生点脚下没地面"
            );
            assert!(!b::is_solid(w.get(xi, y as i32, zi)), "seed {seed} 出生点卡在人");
        }
    }

    #[test]
    fn 脏标记只在贴边时才波及邻居网格() {
        let mut w = World::new(3);
        w.dirty.iter_mut().for_each(|d| *d = false);
        w.set(40, 20, 40, b::STONE); // 区块内部 (40 % 16 == 8)
        let dirty: Vec<usize> = w
            .dirty
            .iter()
            .enumerate()
            .filter(|(_, d)| **d)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(dirty, vec![2 * CHUNKS_X as usize + 2]);

        w.dirty.iter_mut().for_each(|d| *d = false);
        w.set(32, 20, 40, b::STONE); // 正好压在区块左边界
        let n = w.dirty.iter().filter(|d| **d).count();
        assert_eq!(n, 2, "贴边必须带上一条邻居");
    }

    #[test]
    fn 存档字节长度必须等于世界体素数() {
        let w = World::new(11);
        assert_eq!(w.take_bytes().len(), SX as usize * SY as usize * SZ as usize);
        let r = World::from_raw(w.seed, w.take_bytes().to_vec()).expect("长度对得上");
        assert_eq!(r.take_bytes(), w.take_bytes());
    }
}
