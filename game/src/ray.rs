//! 视线求交：体素 DDA（Amanatides & Woo 那个经典走法）。
//!
//! 不做通用三角形求交，直接一格一格跨过去：便宜、精确，命中格子还能顺手给出法线，
//! 放方块要用「命中格子 + 法线」，这一点用射线求交反而麻烦。

use crate::blocks as b;
use crate::world::World;

pub struct Hit {
    /// 命中的格子坐标
    pub cell: [i32; 3],
    /// 命中面的法线（指向来的一侧），放方块就是 cell + normal
    pub normal: [i32; 3],
/// 命中距离。单元测试用它验证走格子的距离算得对；线上暂时没用到。
#[allow(dead_code)]
pub dist: f32,
}

/// 从 origin 沿 dir（必须是单位向量）往前走，最多 max_dist 格。
/// 起点那格不可通行（玩家埋在石头里）时沿 -dir 最多退 3 格另起炉灶，退不动才返回 None。
pub fn cast(w: &World, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<Hit> {
    let (dx, dy, dz) = (dir[0], dir[1], dir[2]);

    let mut x = origin[0].floor() as i32;
    let mut y = origin[1].floor() as i32;
    let mut z = origin[2].floor() as i32;

    let step = [
        if dx > 0.0 { 1 } else { -1 },
        if dy > 0.0 { 1 } else { -1 },
        if dz > 0.0 { 1 } else { -1 },
    ];

    // t_max：走到下一个格界面所需的行进距离；t_delta：走满一整格需要的距离。
    let guard = |d: f32| {
        if d.abs() < 1e-8 {
            f32::MAX
        } else {
            1.0 / d.abs()
        }
    };

    // 起点那格先单独看一眼，但别一撞上就整条射线作废：玩家飞进一格高的缝里时眼睛就埋在
    // 石头里，那时十字线对什么都没目标、瞄准框也不画、挖和放全失灵（原来是直接 return None）。
    // 沿 -dir 最多退 3 格，退到第一个可通行的格子再开始走 DDA；退不到才真的没目标。
    // 退格只沿真有分量的轴退：dir 分量为 0 时上面那个 step 给的是 -1（DDA 里那根轴永远
    // 轮不到走，所以没影响），照抄它退会把 y/z 一起挪走 —— 平着射进柱子成了往斜上方挪格，
    // 柱子根本不在挪后的路上，照样没目标（这条错是被单元测试逮到的）。
    let back = [
        if dx > 0.0 {
            -1
        } else if dx < 0.0 {
            1
        } else {
            0
        },
        if dy > 0.0 {
            -1
        } else if dy < 0.0 {
            1
        } else {
            0
        },
        if dz > 0.0 {
            -1
        } else if dz < 0.0 {
            1
        } else {
            0
        },
    ];
    let mut backed = 0usize;
    while !b::ray_passable(w.get(x, y, z)) && backed < 3 {
        x += back[0];
        y += back[1];
        z += back[2];
        backed += 1;
    }
    if !b::ray_passable(w.get(x, y, z)) {
        return None;
    }

    // t_max 必须等退完格再算：它说的是「当前这一格的那个界面」，退回前一格界面就换了。
    // 算出来可能是负数（原点已经越过那个界面），这不要紧：DDA 每步挑的都是最小的 t，
    // 负的那几步先走，正好把刚退回来的那几格按顺序重新检查一遍 —— 命中的往往就是埋住
    // 眼睛的那格，挖掉它就出来了。
    let mut t_max = [
        if dx > 0.0 {
            (x as f32 + 1.0 - origin[0]) / dx
        } else if dx < 0.0 {
            (origin[0] - x as f32) / -dx
        } else {
            f32::MAX
        },
        if dy > 0.0 {
            (y as f32 + 1.0 - origin[1]) / dy
        } else if dy < 0.0 {
            (origin[1] - y as f32) / -dy
        } else {
            f32::MAX
        },
        if dz > 0.0 {
            (z as f32 + 1.0 - origin[2]) / dz
        } else if dz < 0.0 {
            (origin[2] - z as f32) / -dz
        } else {
            f32::MAX
        },
    ];
    let t_delta = [guard(dx), guard(dy), guard(dz)];

    let mut t;
    let mut axis;
    for _ in 0..256 {
        // 走下一步：挑最近的那个格界面。
        if t_max[0] < t_max[1] && t_max[0] < t_max[2] {
            x += step[0];
            t = t_max[0];
            t_max[0] += t_delta[0];
            axis = 0;
        } else if t_max[1] < t_max[2] {
            y += step[1];
            t = t_max[1];
            t_max[1] += t_delta[1];
            axis = 1;
        } else {
            z += step[2];
            t = t_max[2];
            t_max[2] += t_delta[2];
            axis = 2;
        }

        if t > max_dist {
            return None;
        }

        let id = w.get(x, y, z);
        if b::ray_passable(id) {
            continue;
        }

        let mut normal = [0i32; 3];
        normal[axis] = -step[axis];
        return Some(Hit {
            cell: [x, y, z],
            normal,
            dist: t,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{SX, SY, SZ, WATER_LEVEL};

    /// 一块从 0 到 WATER_LEVEL 全是石头、上面全空的平地。
    /// 注意必须连上面一起清掉：World::new 生成的山和树还留在原地。
    fn flat_world() -> World {
        let mut w = World::new(2);
        for z in 0..SZ {
            for x in 0..SX {
                for y in 0..SY {
                    let id = if y <= WATER_LEVEL { b::STONE } else { b::AIR };
                    w.set(x, y, z, id);
                }
            }
        }
        w
    }

    #[test]
    fn 从上往下打正中方块命中顶面() {
        let w = flat_world();
        let h = cast(&w, [40.5, 25.0, 40.5], [0.0, -1.0, 0.0], 20.0).expect("应该命中");
        assert_eq!(h.cell, [40, 15, 40]);
        assert_eq!(h.normal, [0, 1, 0]);
        // 命中距离是「走到格界面」的距离：从 25.0 落到 y=15 那格的界面在 16.0，所以是 9。
        assert!((h.dist - 9.0).abs() < 1e-3, "距离 {}", h.dist);
    }

    #[test]
    fn 平着打中柱子会给出侧法线() {
        let mut w = flat_world();
        for y in 16..21 {
            w.set(50, y, 40, b::STONE);
        }
        let h = cast(&w, [55.5, 18.5, 40.5], [-1.0, 0.0, 0.0], 20.0).expect("该撞到柱子");
        assert_eq!(h.cell, [50, 18, 40]);
        assert_eq!(h.normal, [1, 0, 0]);

        // 平打空处什么都不该命中。
        assert!(cast(&w, [45.5, 25.5, 40.5], [-1.0, 0.0, 0.0], 20.0).is_none());

        // 起点埋在实心里、往后退 3 格也还在石头里（这里整块 0..=15 都是石头）才给 None。
        assert!(cast(&w, [40.5, 10.5, 40.5], [0.0, -1.0, 0.0], 20.0).is_none());
    }

    #[test]
    fn 眼睛埋在石头里也要往后退格找目标() {
        let mut w = flat_world();
        for y in 16..21 {
            w.set(50, y, 40, b::STONE);
        }
        // 起点就在柱子里（x=50 那格实心），柱子后面 x=51 是空气 → 退一格就有目标。
        // 飞进一格高的缝里时十字线不能是空的，不然挖也挖不了、放也放不了。
        let h = cast(&w, [50.5, 18.5, 40.5], [-1.0, 0.0, 0.0], 20.0).expect("埋在石头里也该有目标");
        assert_eq!(h.cell, [50, 18, 40], "退一格再打，命中的就是埋住起点那格");
        assert_eq!(h.normal, [1, 0, 0], "法线该指向退出来的那一侧");
        assert!(h.dist < 0.0, "命中在起点之后（退回来的路上），距离本该是负的");
    }

    #[test]
    fn 水可以穿过但不能被瞄准() {
        let w = World::new(4242);
        // 找一列「水面下几格就有底」的浅水：深水环底太深，射不到，测不出这条规则。
        let mut found = None;
        for z in 1..(SZ - 1) {
            for x in 1..(SX - 1) {
                if w.get(x, WATER_LEVEL, z) == b::WATER {
                    let bottom = w.top_solid(x, z);
                    if bottom >= WATER_LEVEL - 6 {
                        found = Some((x, z, bottom));
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (x, z, bottom) = found.expect("这个种子应该有浅水");
        let h = cast(
            &w,
            [x as f32 + 0.5, WATER_LEVEL as f32 + 4.0, z as f32 + 0.5],
            [0.0, -1.0, 0.0],
            12.0,
        )
        .expect("浅水往下看必须有底");
        assert_eq!(h.cell, [x, bottom, z], "应该穿过整层水打到底");
        assert_ne!(h.cell[1], WATER_LEVEL, "水不该被瞄到");
        assert!(b::is_solid(w.get(h.cell[0], h.cell[1], h.cell[2])));
    }

    #[test]
    fn 超出视距返回空() {
        let w = flat_world();
        assert!(cast(&w, [40.5, 30.0, 40.5], [0.0, -1.0, 0.0], 3.0).is_none());
    }
}
