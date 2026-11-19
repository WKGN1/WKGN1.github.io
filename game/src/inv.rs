//! 背包、快捷栏和合成。
//!
//! 物品就是方块 id，没有另开一张物品表（少一层对不上的风险，也让存档和图集共用一套编号）：
//! 生存模式挖一个就进背包一个、放下去要扣一个、合成按配方扣料；创造模式不计数量
//! （格子记 0、界面画 ∞），挖不掉料、放也不扣。
//!
//! 快捷栏 9 格对数字键 1..9 和滚轮；背包 27 格只有在「物品栏」面板里点得到，
//! 点背包格是跟当前手里那格**对调**，这样把工作台从背包挪到手上是按一下的事。

use crate::blocks as b;

pub const HOTBAR_SLOTS: usize = 9;
pub const BAG_SLOTS: usize = 27;
pub const SLOTS: usize = HOTBAR_SLOTS + BAG_SLOTS;

/// 一格最多叠多少个。挖矿一次掉一个，64 个足够攒出一面墙。
pub const STACK_MAX: u16 = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot {
    pub id: u8,
    pub n: u16,
}

pub const EMPTY: Slot = Slot {
    id: b::AIR,
    n: 0,
};

#[derive(Clone, Debug)]
pub struct Inv {
    pub slots: [Slot; SLOTS],
    pub selected: usize,
}

impl Inv {
    /// 生存开局给的一点料：够搭个窝，也够合成出第一张工作台（木板 4 → 工作台）。
    pub fn new_survival() -> Self {
        let mut inv = Inv {
            slots: [EMPTY; SLOTS],
            selected: 0,
        };
        inv.slots[0] = Slot { id: b::GRASS, n: 8 };
        inv.slots[1] = Slot { id: b::DIRT, n: 8 };
        inv.slots[2] = Slot { id: b::STONE, n: 8 };
        inv.slots[3] = Slot { id: b::WOOD, n: 4 };
        inv
    }

    /// 创造：能放的方块一律铺上，数量记 0（界面显示 ∞）。
    pub fn new_creative() -> Self {
        let mut inv = Inv {
            slots: [EMPTY; SLOTS],
            selected: 0,
        };
        for (i, id) in b::ALL_PLACEABLE.iter().enumerate() {
            if i < SLOTS {
                inv.slots[i] = Slot { id: *id, n: 0 };
            }
        }
        inv
    }

    /// 选中格只在快捷栏里打转，背包不参与（不然数字键 1..9 会对不上号）。
    pub fn selected_index(&self) -> usize {
        self.selected.min(HOTBAR_SLOTS - 1)
    }

    pub fn selected_slot(&self) -> Slot {
        let i = self.selected_index();
        self.slots[i]
    }

    /// 手里这种方块还放得出来吗（创造只要不是空气就行，生存还得有数量）。
    pub fn can_place(&self, creative: bool) -> bool {
        let s = self.selected_slot();
        if s.id == b::AIR {
            return false;
        }
        // 生存模式下手里得有货。水不会进背包（射线穿得过水、挖不到），所以不用特判。
        creative || s.n > 0
    }

    /// 放一个出去：创造不扣，生存扣一个。返回真正放出去的方块 id。
    pub fn consume_selected(&mut self, creative: bool) -> Option<u8> {
        if !self.can_place(creative) {
            return None;
        }
        let i = self.selected_index();
        let id = self.slots[i].id;
        if !creative {
            self.slots[i].n -= 1;
            if self.slots[i].n == 0 {
                self.slots[i] = EMPTY;
            }
        }
        Some(id)
    }

    /// 往背包里加 n 个，返回**放不下**的量（>0 就是背包满了，调用方好告诉玩家）。
    pub fn add(&mut self, id: u8, n: u16) -> u16 {
        if id == b::AIR || n == 0 {
            return 0;
        }
        let mut left = n;
        for s in self.slots.iter_mut() {
            if left == 0 {
                break;
            }
            if s.id == id && s.n > 0 && s.n < STACK_MAX {
                let put = (STACK_MAX - s.n).min(left);
                s.n += put;
                left -= put;
            }
        }
        for s in self.slots.iter_mut() {
            if left == 0 {
                break;
            }
            if s.id == b::AIR || s.n == 0 {
                let put = STACK_MAX.min(left);
                *s = Slot { id, n: put };
                left -= put;
            }
        }
        left
    }

    pub fn count_of(&self, id: u8) -> u32 {
        self.slots
            .iter()
            .filter(|s| s.id == id)
            .map(|s| s.n as u32)
            .sum()
    }

    /// 这种方块还塞得下多少个（合成前得先确认放得下产物，不然料扣了产物掉地上）。
    pub fn room_of(&self, id: u8) -> u32 {
        let mut room = 0u32;
        for s in self.slots.iter() {
            if s.id == id && s.n > 0 && s.n < STACK_MAX {
                room += (STACK_MAX - s.n) as u32;
            } else if s.id == b::AIR || s.n == 0 {
                room += STACK_MAX as u32;
            }
        }
        room
    }

    /// 扣掉 n 个某种方块（合成用）。不够就一个都不扣，返回有没有扣成。
    pub fn take(&mut self, id: u8, n: u16) -> bool {
        if self.count_of(id) < n as u32 {
            return false;
        }
        let mut left = n;
        for s in self.slots.iter_mut() {
            if left == 0 {
                break;
            }
            if s.id == id && s.n > 0 {
                let t = s.n.min(left);
                s.n -= t;
                left -= t;
                if s.n == 0 {
                    *s = EMPTY;
                }
            }
        }
        left == 0
    }

    /// 这种方块在有货的格子里的第一个下标（中键对着地上的方块按、手上那格优先）。
    pub fn find(&self, id: u8, creative: bool) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.id == id && (creative || s.n > 0))
    }

    /// 把第 idx 格和当前手里那格对调（面板里点背包格就是这个）。
    pub fn swap_with_selected(&mut self, idx: usize) {
        let i = self.selected_index();
        if idx >= SLOTS || idx == i {
            return;
        }
        self.slots.swap(i, idx);
    }

    /// 滚轮 / 上一个下一个：只在快捷栏 9 格里转圈。
    pub fn cycle_selected(&mut self, dir: i32) {
        let len = HOTBAR_SLOTS as i32;
        self.selected = ((self.selected as i32) + dir).rem_euclid(len) as usize;
    }
}

// ---------------------------------------------------------------------------
// 配方
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Recipe {
    pub out: (u8, u16),
    pub need: &'static [(u8, u16)],
    /// 合成成功时给玩家看的那句话。写成静态的：HUD 那一栏吃的是 &'static str。
    pub note: &'static str,
    /// true = 只有站在工作台旁边才许合成。第一张工作台故意不设门槛，不然做不出来。
    pub bench: bool,
}

pub const RECIPES: &[Recipe] = &[
    Recipe {
        out: (b::PLANK, 4),
        need: &[(b::WOOD, 1)],
        bench: false,
        note: "合成出 木板 ×4",
    },
    Recipe {
        out: (b::WORKBENCH, 1),
        need: &[(b::PLANK, 4)],
        bench: false,
        note: "合成出 工作台 —— 放地上再站到旁边",
    },
    Recipe {
        out: (b::WOOD, 1),
        need: &[(b::LEAF, 4)],
        bench: true,
        note: "合成出 木头 ×1",
    },
    Recipe {
        out: (b::LAMP, 1),
        need: &[(b::STONE, 3), (b::WOOD, 1)],
        bench: true,
        note: "合成出 萤石灯 ×1",
    },
    Recipe {
        out: (b::SNOW_GRASS, 1),
        need: &[(b::SNOW, 2), (b::LEAF, 1)],
        bench: true,
        note: "合成出 雪草 ×1",
    },
];

/// 这条配方现在能不能合成：料够、产物还塞得下、要工作台的得在旁边。
pub fn craftable(inv: &Inv, r: &Recipe, bench: bool) -> bool {
    if r.bench && !bench {
        return false;
    }
    if inv.room_of(r.out.0) < r.out.1 as u32 {
        return false;
    }
    r.need
        .iter()
        .all(|(id, n)| inv.count_of(*id) >= *n as u32)
}

pub fn craft(inv: &mut Inv, r: &Recipe, bench: bool) -> bool {
    if !craftable(inv, r, bench) {
        return false;
    }
    for (id, n) in r.need {
        // craftable 已经核过料够，这里扣不成就是逻辑错了
        assert!(inv.take(*id, *n), "配方扣料失败：{}", b::block_name(*id));
    }
    let left = inv.add(r.out.0, r.out.1);
    if left > 0 {
        // room_of 已经预留过格子，塞不下同样是逻辑错
        assert_eq!(left, 0, "产物该有格子放：{}", b::block_name(r.out.0));
    }
    true
}

// ---------------------------------------------------------------------------
// 给页面渲染「物品栏」的那一串文本（走 HUD 管子的第 10 段）
// ---------------------------------------------------------------------------

/// 页面要的东西打包成一行：管子按 `|` 切段，所以这里一个 `|` 都不能出现。
/// 段内用 `;` 分字段、`,` 分项、`:` 分键值、`-` 拼名字、`>` 和 `+` 写配方。
///
/// `C:创造开关;B:旁边有没有工作台;S:选中格;N:方块名字表;H:快捷栏;G:背包;R:配方;A:每条能不能合`
pub fn inv_text(inv: &Inv, creative: bool, bench_near: bool) -> String {
    let mut out = String::new();
    out.push_str(if creative { "C:1;" } else { "C:0;" });
    out.push_str(if bench_near { "B:1;" } else { "B:0;" });
    out.push_str(&format!("S:{};", inv.selected_index() + 1));

    // 名字表：合成里出现的方块 + 创造给的那些，页面照着它显示中文，省得两边各维护一份。
    let mut ids: Vec<u8> = Vec::new();
    for id in b::ALL_PLACEABLE.iter() {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    for id in b::HOTBAR.iter() {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    for r in RECIPES {
        if !ids.contains(&r.out.0) {
            ids.push(r.out.0);
        }
        for (id, _) in r.need {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
    }
    ids.sort_unstable();
    out.push_str("N:");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{}-{}", id, b::block_name(*id)));
    }
    out.push(';');

    for (i, s) in inv.slots.iter().enumerate() {
        if i == HOTBAR_SLOTS {
            out.push(';');
            out.push('G');
            out.push(':');
        } else if i > 0 {
            out.push(',');
        }
        if i == 0 {
            out.push_str("H:");
        }
        out.push_str(&format!("{}:{}", s.id, s.n));
    }
    out.push_str(";R:");

    for (i, r) in RECIPES.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{}:{}>", r.out.0, r.out.1));
        for (k, (id, n)) in r.need.iter().enumerate() {
            if k > 0 {
                out.push('+');
            }
            out.push_str(&format!("{}:{}", id, n));
        }
        if r.bench {
            out.push('*');
        }
    }
    out.push_str(";A:");

    for (i, r) in RECIPES.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(if craftable(inv, r, bench_near) { "1" } else { "0" });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 生存开局够合成出第一张工作台() {
        let mut inv = Inv::new_survival();
        assert!(craftable(&inv, &RECIPES[0], false), "4 个木头该能开板");
        assert!(craft(&mut inv, &RECIPES[0], false));
        assert_eq!(inv.count_of(b::WOOD), 3);
        assert_eq!(inv.count_of(b::PLANK), 4);
        // 第一张工作台不该要求旁边已经有工作台，不然永远做不出来
        assert!(!RECIPES[1].bench);
        assert!(craft(&mut inv, &RECIPES[1], false));
        assert_eq!(inv.count_of(b::PLANK), 0);
        assert_eq!(inv.count_of(b::WORKBENCH), 1);
    }

    #[test]
    fn 要工作台的配方在旁边不够时不许合() {
        let mut inv = Inv::new_survival();
        inv.add(b::LEAF, 4);
        assert!(!craftable(&inv, &RECIPES[2], false), "没工作台不该能压叶子");
        assert!(!craft(&mut inv, &RECIPES[2], false));
        assert_eq!(inv.count_of(b::LEAF), 4, "合不成一份料都不该扣");
        assert!(craftable(&inv, &RECIPES[2], true));
        assert!(craft(&mut inv, &RECIPES[2], true));
        assert_eq!(inv.count_of(b::LEAF), 0);
        assert_eq!(inv.count_of(b::WOOD), 5, "开局 4 个 + 压出来 1 个");
    }

    #[test]
    fn 背包满了要报告放不下多少() {
        let mut inv = Inv {
            slots: [Slot { id: b::STONE, n: STACK_MAX }; SLOTS],
            selected: 0,
        };
        assert_eq!(inv.add(b::STONE, 1), 1, "满背包该说放不下");
        assert_eq!(inv.count_of(b::STONE), (STACK_MAX * SLOTS as u16) as u32);
        inv.slots[20] = Slot { id: b::DIRT, n: 3 };
        assert_eq!(inv.add(b::DIRT, 10), 0);
        assert_eq!(inv.count_of(b::DIRT), 13);
    }

    #[test]
    fn 生存放一个扣一个空了就放不出来() {
        let mut inv = Inv::new_survival();
        inv.selected = 0;
        assert_eq!(inv.selected_slot().n, 8);
        for i in 0..8 {
            assert_eq!(inv.consume_selected(false), Some(b::GRASS), "第 {} 次该放得出去", i);
        }
        assert_eq!(inv.count_of(b::GRASS), 0);
        assert_eq!(inv.consume_selected(false), None, "空手了还放就得凭空变方块了");
        // 创造不扣
        let mut c = Inv::new_creative();
        c.selected = 0;
        for _ in 0..20 {
            assert_eq!(c.consume_selected(true), Some(b::GRASS));
        }
        assert_eq!(c.selected_slot().n, 0, "创造记的数量永远是 0");
    }

    #[test]
    fn 点背包格是跟手上那格对调() {
        let mut inv = Inv::new_survival();
        inv.slots[12] = Slot { id: b::LAMP, n: 3 };
        inv.selected = 4; // 现在是空的
        inv.swap_with_selected(12);
        assert_eq!(inv.slots[4], Slot { id: b::LAMP, n: 3 });
        assert_eq!(inv.slots[12], EMPTY);
        assert_eq!(inv.selected_slot().id, b::LAMP);
        assert!(inv.can_place(false));
    }

    #[test]
    fn 滚轮只在快捷栏里转圈() {
        let mut inv = Inv::new_survival();
        inv.selected = 0;
        inv.cycle_selected(-1);
        assert_eq!(inv.selected, 8, "往前一格该绕到第 9 格");
        inv.cycle_selected(1);
        assert_eq!(inv.selected, 0);
        inv.selected = 8;
        inv.cycle_selected(1);
        assert_eq!(inv.selected, 0, "往后也不该跑到背包那 27 格里去");
    }

    #[test]
    fn 文本里没有管道符而且字段齐全() {
        let inv = Inv::new_survival();
        let t = inv_text(&inv, false, true);
        assert!(!t.contains('|'), "第 10 段里不能出现 |：{}", t);
        for want in ["C:0;", "B:1;", "S:1;", "N:", "H:", "G:", "R:", "A:"] {
            assert!(t.contains(want), "少了 {}：{}", want, t);
        }
        // 快捷栏 9 项、背包 27 项、配方条数和 A 的位数一致
        let f: Vec<&str> = t.split(';').collect();
        // C, B, S, N, H, G, R, A
        assert_eq!(f.len(), 8, "字段数不对：{}", t);
        assert_eq!(f[4].split(',').count(), HOTBAR_SLOTS, "快捷栏格子数不对：{}", f[4]);
        assert_eq!(f[5].split(',').count(), BAG_SLOTS, "背包格子数不对：{}", f[5]);
        assert_eq!(
            f[6].split(',').count(),
            RECIPES.len(),
            "配方条数不对：{}",
            f[6]
        );
        assert_eq!(f[7].split(',').count(), RECIPES.len(), "能不能合的位数和配方条数得对上：{}", f[7]);
        assert!(f[3].contains("草方块"), "名字表该带方块名：{}", f[3]);
    }

    #[test]
    fn 合成的产物塞不下时不许合() {
        // 整背包都是石头，只有手上那格留着一点点木板 —— 4 个木板没格子放，就该不许合
        let mut inv = Inv {
            slots: [Slot { id: b::STONE, n: STACK_MAX }; SLOTS],
            selected: 3,
        };
        inv.slots[3] = Slot { id: b::WOOD, n: 1 };
        assert!(!craftable(&inv, &RECIPES[0], false), "没格子放产物就不该扣料");
    }
}
