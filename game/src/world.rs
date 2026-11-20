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

/// 六个邻居方向。光照 BFS 用它摊光：方向少一个，屋里就会出现一条永远不亮的缝。
const NEIGH: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

pub struct World {
    pub seed: u32,
    pub data: Vec<u8>,
    /// 一字节两截光照：高 4 位 = 天光（0..15），低 4 位 = 方块光（0..15）。
    /// **不进存档**：它完全能由方块推出来，读档之后整张重算一遍就行，省下半个存档体积，
    /// 也省掉「存档里的光照和方块对不上」这类脏状态。
    pub light: Vec<u8>,
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
            light: vec![0; SX as usize * SY as usize * SZ as usize],
            dirty: vec![true; CHUNK_COUNT],
            changed: 0,
        };
        w.generate();
        w.relight_all();
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
        let mut w = World {
            seed,
            data,
            light: vec![0; SX as usize * SY as usize * SZ as usize],
            dirty: vec![true; CHUNK_COUNT],
            changed,
        };
        // 光照没存进存档，读档之后现推一遍（不然第一帧整个世界是黑的）
        w.relight_all();
        Some(w)
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if !in_bounds(x, y, z) {
            return AIR; // 世界外按空气算，这样岛的侧面会被网格化，看得出轮廓
        }
        self.data[idx(x, y, z)]
    }

    /// 某一格的天光等级（0..15）。世界外一律 0：洞里不该「凭空气」透进光。
    #[inline]
    pub fn sky_light(&self, x: i32, y: i32, z: i32) -> u8 {
        if y >= SY {
            return 15; // 世界顶上就是露天：最上面那一层的顶面不该是黑的
        }
        if !in_bounds(x, y, z) {
            return 0;
        }
        self.light[idx(x, y, z)] >> 4
    }

    /// 某一格的方块光等级（0..15）。
    #[inline]
    pub fn block_light(&self, x: i32, y: i32, z: i32) -> u8 {
        if !in_bounds(x, y, z) {
            return 0;
        }
        self.light[idx(x, y, z)] & 15
    }

    /// 整张重算光照。挖一格、放一格之后就叫这一条：整张图才 44 万格，release 下几毫秒就完，
    /// 比「只重算附近一小盒、还得处理光源被挖掉时外面残留的亮光」那套便宜得多，
    /// 而且不会算错（那套的残留是我以后一定会踩的坑）。
    pub fn relight_all(&mut self) {
        let n = self.data.len();
        self.light.clear();
        self.light.resize(n, 0);

        // 1) 天光：从上往下扫每一列。头顶一整列都不挡光就一路 15 不掉级（真 MC 也是这条；
        //    不然露天会平白无故带一层渐变）。撞上挡光的方块就按它的透明度掉级，掉到 0 之后
        //    剩下靠横向摊（第 2 步）。
        for z in 0..SZ {
            for x in 0..SX {
                let mut lvl: i32 = 15;
                for y in (0..SY).rev() {
                    let i = idx(x, y, z);
                    let op = b::opacity(self.data[i]) as i32;
                    if op >= 15 {
                        lvl = 0;
                    } else if lvl == 15 && op == 0 {
                        // 满级天光从全透明的头顶直接漏下来，不掉级
                    } else {
                        lvl = (lvl - op.max(1)).max(0);
                    }
                    self.light[i] = (lvl as u8) << 4;
                }
            }
        }

        // 2) 两趟 BFS：一趟把天光横向摊开，一趟从发光的方块往外摊方块光。
        //    规则都是「比已经记的更亮才写，写过就再往里推」，所以队列顺序不对也不会算错，
        //    只是多推几步 —— 我故意不用有序队列，少一层容易写错的东西。
        let mut sky_seeds: Vec<usize> = Vec::new();
        let mut blk_seeds: Vec<usize> = Vec::new();
        for i in 0..self.light.len() {
            if self.light[i] >> 4 > 0 {
                sky_seeds.push(i);
            }
            let e = b::light_emission(self.data[i]);
            if e > 0 {
                // 光源自己那一格直接给满级，哪怕它自己是不透光的实心块（不然灯里面是黑的）
                self.light[i] = (self.light[i] & 0xF0) | e;
                blk_seeds.push(i);
            }
        }
        self.flood(true, sky_seeds, 0, SX - 1, 0, SZ - 1);
        self.flood(false, blk_seeds, 0, SX - 1, 0, SZ - 1);
    }

    /// 光照摊开（BFS）。`sky=true` 摊天光、否则摊方块光；`seeds` 是起点格子；
    /// 只允许改 x0..=x1 / z0..=z1 这几列（竖直方向全取），盒子外的格子值一律当已知、当光源喂进来。
    /// 规则：「比已经记的更亮才写，写过就再往里推」，所以起点顺序错了也不会算错，只是多推几步
    /// —— 我故意不用有序队列，少一层容易写错、以后也难查的东西。
    fn flood(&mut self, sky: bool, seeds: Vec<usize>, x0: i32, x1: i32, z0: i32, z1: i32) {
        let mut q: Vec<usize> = seeds;
        let mut head = 0usize;
        while head < q.len() {
            let i = q[head];
            head += 1;
            let byte = self.light[i];
            let lvl = if sky {
                (byte >> 4) as i32
            } else {
                (byte & 15) as i32
            };
            if lvl <= 1 {
                continue; // 1 级再往外推就是 0，白跑
            }
            let x = (i % SX as usize) as i32;
            let rest = i / SX as usize;
            let z = (rest % SZ as usize) as i32;
            let y = (rest / SZ as usize) as i32;
            for d in NEIGH.iter() {
                let (nx, ny, nz) = (x + d[0], y + d[1], z + d[2]);
                if !in_bounds(nx, ny, nz) || nx < x0 || nx > x1 || nz < z0 || nz > z1 {
                    continue; // 盒子外面不动：那些格子的值本来就是对的
                }
                let j = idx(nx, ny, nz);
                let op = b::opacity(self.data[j]) as i32;
                if op >= 15 {
                    continue; // 实心方块不收光
                }
                let nl = (lvl - 1 - op).max(0) as u8;
                let cur = if sky {
                    self.light[j] >> 4
                } else {
                    self.light[j] & 15
                };
                if nl > cur {
                    self.light[j] = if sky {
                        (self.light[j] & 15) | (nl << 4)
                    } else {
                        (self.light[j] & 0xF0) | nl
                    };
                    q.push(j);
                }
            }
        }
    }

    /// 挖一格 / 放一格之后要重算的那一小坨：整根柱子重扫 + 半径 15 的盒子里重摊。
    /// 为什么半径 15 就够、而且不会算错：光最远只能走 15 步（每步至少掉 1 级），
    /// 所以离改动点 16 格以外的格子不可能被这次改动影响到，盒外的值当已知是对的。
    /// 唯一能"无限远"影响的是天光 —— 它沿自己那根柱子往下不掉级，所以我把改动点那一整根
    /// 柱子（竖直 48 格全取）重扫一遍，其它列的扫列结果不会因这一格变。
    pub fn relight_around(&mut self, x: i32, _y: i32, z: i32) {
        const R: i32 = 15;
        let x0 = (x - R).max(0);
        let x1 = (x + R).min(SX - 1);
        let z0 = (z - R).max(0);
        let z1 = (z + R).min(SZ - 1);

        // 1) 盒内的光照全清（两截都清），然后把盒内每一列的天光重新扫一遍。
        //    扫列只花 48 次/列，比"只扫改动那一根、别的列指望 BFS 补"干净。
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                let mut lvl: i32 = 15;
                // 先把这根柱子（含盒外那些 y）清一遍再扫：盒子竖直方向是全取的，没有遗漏
                for y in 0..SY {
                    self.light[idx(cx, y, cz)] = 0;
                }
                for y in (0..SY).rev() {
                    let i = idx(cx, y, cz);
                    let op = b::opacity(self.data[i]) as i32;
                    if op >= 15 {
                        lvl = 0;
                    } else if lvl == 15 && op == 0 {
                        // 满级天光从全透明的头顶直接漏下来，不掉级
                    } else {
                        lvl = (lvl - op.max(1)).max(0);
                    }
                    self.light[i] = (lvl as u8) << 4;
                }
            }
        }

        // 2) 起点：盒内所有还亮着的天光格子 + 紧贴盒子外面一圈亮着的外来光（它们不许被改，
        //    但要把光喂进来）+ 盒内的发光方块。
        let mut sky_seeds: Vec<usize> = Vec::new();
        let mut blk_seeds: Vec<usize> = Vec::new();
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                for y in 0..SY {
                    let i = idx(cx, y, cz);
                    if self.light[i] >> 4 > 1 {
                        sky_seeds.push(i);
                    }
                    if b::light_emission(self.data[i]) > 0 {
                        let e = b::light_emission(self.data[i]);
                        self.light[i] = (self.light[i] & 0xF0) | e;
                        blk_seeds.push(i);
                    }
                }
            }
        }
        // 紧贴盒子外面那一圈：只喂光、不许被改（flood 里卡了范围，越界那几列写不进去）
        for cz in (z0 - 1).max(0)..=(z1 + 1).min(SZ - 1) {
            for cx in (x0 - 1).max(0)..=(x1 + 1).min(SX - 1) {
                if cx >= x0 && cx <= x1 && cz >= z0 && cz <= z1 {
                    continue; // 盒内的起点上面收过了
                }
                for y in 0..SY {
                    let i = idx(cx, y, cz);
                    if self.light[i] >> 4 > 1 {
                        sky_seeds.push(i);
                    }
                    if (self.light[i] & 15) > 1 {
                        blk_seeds.push(i);
                    }
                }
            }
        }

        self.flood(true, sky_seeds, x0, x1, z0, z1);
        self.flood(false, blk_seeds, x0, x1, z0, z1);
        self.mark_dirty_box(x0, x1, z0, z1);
    }

    /// 光照改动会连带影响哪些网格：网格边缘往外的面是被邻格照的，所以半径 15 的盒子
    /// 会跨进旁边那些网格 —— 只标自己那一个网格就会留下"挖开放了灯、旁边那格还是黑的"。
    fn mark_dirty_box(&mut self, x0: i32, x1: i32, z0: i32, z1: i32) {
        let c0 = (x0 / CHUNK).max(0);
        let c1 = (x1 / CHUNK).min(CHUNKS_X - 1);
        let d0 = (z0 / CHUNK).max(0);
        let d1 = (z1 / CHUNK).min(CHUNKS_Z - 1);
        for cz in d0..=d1 {
            for cx in c0..=c1 {
                self.dirty[(cz * CHUNKS_X + cx) as usize] = true;
            }
        }
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

    /// 造一个说得清的场景：把 (bx..bx+11, 18..26, bz..bz+11) 腾空，y=20 铺地板、
    /// y=25 盖一层石头顶。要露天天光就自己把顶顶穿。
    fn 一块带顶的空间(bx: i32, bz: i32) -> World {
        let mut w = World::new(11);
        for x in bx..bx + 11 {
            for z in bz..bz + 11 {
                for y in 18..26 {
                    w.set(x, y, z, b::AIR);
                }
                w.set(x, 20, z, b::STONE);
                w.set(x, 25, z, b::STONE);
            }
        }
        w.relight_all();
        w
    }

    /// 造一条被石头包死的 21 格长地道（y=21，z 固定），两头、上下、左右都实。
    fn 一条密封地道() -> World {
        let mut w = World::new(11);
        let z = 45i32;
        for x in 30..=50 {
            for y in [20i32, 22] {
                w.set(x, y, z, b::STONE);
            }
            for dz in [-1i32, 1] {
                w.set(x, 21, z + dz, b::STONE);
            }
            w.set(x, 21, z, b::AIR);
        }
        w.set(29, 21, z, b::STONE);
        w.set(51, 21, z, b::STONE);
        w.relight_all();
        w
    }

    #[test]
    fn 顶穿屋顶底下又变回满级天光而满级沿柱子一路不掉级() {
        let mut w = 一块带顶的空间(40, 40);
        let (cx, cz) = (45i32, 45i32);
        assert_eq!(w.get(cx, 25, cz), b::STONE);
        let under = w.sky_light(cx, 24, cz);
        assert!(
            under < 15,
            "头顶压着一层实心石头，天光只能从边上绕进来，不该满级，拿到 {}",
            under
        );
        // 正上方顶穿：满级天光沿柱子直落，一格都不该掉（天光最关键就是这条）
        w.set(cx, 25, cz, b::AIR);
        w.relight_around(cx, 25, cz);
        assert_eq!(w.sky_light(cx, 24, cz), 15, "顶穿之后正下方该是满级天光");
        assert_eq!(w.sky_light(cx, 21, cz), 15, "满级天光沿柱子往下不掉级，21 层也该 15");
        // 局部重算必须和整张重算一模一样（半径 15 = 光的射程，所以这是做得到也该有的标准）
        let r = World::from_raw(w.seed, w.data.clone()).unwrap();
        assert_eq!(
            r.light,
            w.light,
            "局部重算和整张重算对不上：不同的光照字节个数 = 应说清楚"
        );
    }

    #[test]
    fn 密封地道里全黑而一盏灯把光一格推到十五格以外() {
        let mut w = 一条密封地道();
        let z = 45i32;
        assert_eq!(w.sky_light(40, 21, z), 0, "被石头包死的地道里不该有天光");
        assert_eq!(w.block_light(40, 21, z), 0, "没灯就该是全黑");
        w.set(31, 21, z, b::LAMP);
        w.relight_around(31, 21, z);
        assert_eq!(w.block_light(31, 21, z), 15, "灯自己那一格必须亮，不然灯里面是黑的");
        assert_eq!(w.block_light(32, 21, z), 14, "往外一格该掉一级");
        assert_eq!(w.block_light(40, 21, z), 6, "离灯 9 格：15-9=6");
        assert_eq!(w.block_light(45, 21, z), 1, "离灯 14 格还剩 1");
        assert_eq!(w.block_light(46, 21, z), 0, "离灯 15 格就是黑的了，光的射程就 15 步");
        assert_eq!(w.block_light(40, 23, z), 0, "石头顶上面不该有光，实心方块吃光");
        let r = World::from_raw(w.seed, w.data.clone()).unwrap();
        assert_eq!(r.light, w.light, "放灯之后的局部重算和整张重算对不上");
    }

    #[test]
    fn 光照重算不许慢到卡手() {
        let mut w = World::new(3);
        let t = std::time::Instant::now();
        w.relight_all();
        let all = t.elapsed();
        let t2 = std::time::Instant::now();
        w.relight_around(48, 20, 48);
        let part = t2.elapsed();
        println!(
            "光照耗时：整张 {:?}｜局部（挖一格、放一格走这条） {:?}",
            all, part
        );
        // 跑测试是 debug 编译，比 wasm 的 release 慢好几倍，所以这条只卡宽上限；
        // 真实数字看 cargo test --release 打出来的那一行。
        assert!(all < std::time::Duration::from_millis(900), "整张重算太慢：{:?}", all);
        assert!(
            part < std::time::Duration::from_millis(200),
            "局部重算太慢，手机上点一下就卡：{:?}",
            part
        );
    }

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
