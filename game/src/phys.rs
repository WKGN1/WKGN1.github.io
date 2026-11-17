//! 玩家控制与碰撞：AABB 对体素，分轴推进再贴边。
//!
//! 不做花哨的 swept AABB / 连续碰撞：方块世界 + 每帧位移远小于 1 格时，
//! 分轴解算足够稳，而且代码短、好调。速度上限在常量里写死，防穿模。

use crate::world::World;

pub const P_HALF: f32 = 0.3;
pub const P_HEIGHT: f32 = 1.78;
pub const EYE_HEIGHT: f32 = 1.62;

pub const GRAVITY: f32 = -24.0;
pub const JUMP_V: f32 = 8.4;
pub const WALK_SPEED: f32 = 4.4;
pub const SPRINT_MULT: f32 = 1.7;
pub const FLY_SPEED: f32 = 10.0;
/// 水里下沉/上浮都变慢，浮力这个数值是「手感」调出来的，不是物理算出来的。
pub const WATER_GRAVITY: f32 = -5.0;
pub const WATER_UP: f32 = 3.2;
pub const MAX_FALL: f32 = -38.0;

pub struct Player {
    /// 脚底中心
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fly: bool,
    pub on_ground: bool,
    pub in_water: bool,
}

impl Player {
    pub fn new(spawn: [f32; 3]) -> Self {
        Player {
            pos: spawn,
            vel: [0.0; 3],
            yaw: 0.6,
            pitch: -0.15,
            fly: false,
            on_ground: false,
            in_water: false,
        }
    }

    #[inline]
    pub fn eye(&self) -> [f32; 3] {
        [self.pos[0], self.pos[1] + EYE_HEIGHT, self.pos[2]]
    }

    /// yaw 绕 Y，pitch 上下。yaw=0 时朝 +Z，和网格/射线的坐标约定一致。
    pub fn look_dir(&self) -> [f32; 3] {
        let cp = self.pitch.cos();
        let d = [cp * self.yaw.sin(), self.pitch.sin(), cp * self.yaw.cos()];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < 1e-6 {
            return [0.0, 0.0, 1.0];
        }
        [d[0] / len, d[1] / len, d[2] / len]
    }

    /// wish 是水平移动意图（已经按 yaw 旋转过，长度 <= 1）。
    pub fn update(
        &mut self,
        w: &World,
        dt: f32,
        wish: [f32; 3],
        jump: bool,
        descend: bool,
        sprint: bool,
    ) {
        let dt = dt.min(0.05); // 切后台回来 dt 会很大，钳一下免得一步穿墙

        let foot = w.water_at(self.pos[0], self.pos[1] + 0.2, self.pos[2]);
        let head = w.water_at(self.pos[0], self.pos[1] + EYE_HEIGHT, self.pos[2]);
        self.in_water = foot || head;

        let want_speed = if self.fly {
            FLY_SPEED
        } else if self.in_water {
            WALK_SPEED * 0.55
        } else {
            WALK_SPEED * if sprint { SPRINT_MULT } else { 1.0 }
        };

        let target = [wish[0] * want_speed, 0.0, wish[2] * want_speed];
        if self.fly || self.in_water {
            // 飞行和游泳要「指哪走哪」，落地时才有加速感。
            let k = if self.fly { 1.0 } else { 0.35 };
            self.vel[0] += (target[0] - self.vel[0]) * k;
            self.vel[2] += (target[2] - self.vel[2]) * k;
        } else {
            let k = if self.on_ground { 0.85 } else { 0.25 };
            self.vel[0] += (target[0] - self.vel[0]) * k;
            self.vel[2] += (target[2] - self.vel[2]) * k;
        }

        // 竖直方向
        if self.fly {
            self.vel[1] = if jump {
                FLY_SPEED * 0.8
            } else if descend {
                -FLY_SPEED * 0.8
            } else {
                0.0
            };
        } else if self.in_water {
            self.vel[1] += WATER_GRAVITY * dt;
            if jump {
                self.vel[1] += WATER_UP * dt * 6.0;
            }
            self.vel[1] = self.vel[1].clamp(-6.0, 6.0);
        } else {
            self.vel[1] += GRAVITY * dt;
            if jump && self.on_ground {
                self.vel[1] = JUMP_V;
                self.on_ground = false;
            }
            if self.vel[1] < MAX_FALL {
                self.vel[1] = MAX_FALL;
            }
        }

        // 限速，免得大 dt 一步穿过一格墙。
        let hs = (self.vel[0] * self.vel[0] + self.vel[2] * self.vel[2]).sqrt();
        let max_h = if self.fly { FLY_SPEED * 2.0 } else { WALK_SPEED * 3.0 };
        if hs > max_h && hs > 0.0 {
            let s = max_h / hs;
            self.vel[0] *= s;
            self.vel[2] *= s;
        }
        let max_v = if self.fly { FLY_SPEED * 2.0 } else { MAX_FALL.abs() };
        self.vel[1] = self.vel[1].clamp(-max_v, max_v);

        self.on_ground = false;
        self.move_axis(w, self.vel[0] * dt, 0);
        self.move_axis(w, self.vel[2] * dt, 2);
        self.move_axis(w, self.vel[1] * dt, 1);

        if self.pos[1] < -20.0 {
            // 理论上不该发生（第 0 层是实心底），真发生了就当掉出世界，回出生点下方。
            self.pos[1] = 30.0;
            self.vel[1] = 0.0;
        }
        if self.pos[1] > crate::world::SY as f32 + 40.0 {
            self.pos[1] = crate::world::SY as f32 + 40.0;
            self.vel[1] = 0.0;
        }
    }

    /// 单轴推进 + 贴边解算。先动 X/Z 再动 Y，这样贴墙滑动和落地判定都顺。
    fn move_axis(&mut self, w: &World, delta: f32, axis: usize) {
        if delta.abs() < 1e-7 {
            if axis == 1 {
                // Y 不动时也要看一眼脚下，站在地上不能算「要掉下去」。
                self.check_ground(w);
            }
            return;
        }

        self.pos[axis] += delta;
        let mut min = [
            self.pos[0] - P_HALF,
            self.pos[1],
            self.pos[2] - P_HALF,
        ];
        let mut max = [
            self.pos[0] + P_HALF,
            self.pos[1] + P_HEIGHT,
            self.pos[2] + P_HALF,
        ];
        // 碰撞箱往里缩，不是往外扩。往外扩会让站着的人把脚下那一层也算成横向障碍，
        // 于是每走一步都被「贴」回去一点，直线走成了倒退（实测被拖到 -79 格）。
        let eps = 1e-4;
        for m in min.iter_mut() {
            *m += eps;
        }
        for m in max.iter_mut() {
            *m -= eps;
        }

        let x0 = min[0].floor() as i32;
        let x1 = max[0].floor() as i32;
        let y0 = min[1].floor() as i32;
        let y1 = max[1].floor() as i32;
        let z0 = min[2].floor() as i32;
        let z1 = max[2].floor() as i32;

        let mut snap: Option<f32> = None;
        let moving_pos = delta > 0.0;

        for cy in y0..=y1 {
            for cz in z0..=z1 {
                for cx in x0..=x1 {
                    if !w.blocks_movement(cx, cy, cz) {
                        continue;
                    }
                    let cand = match (axis, moving_pos) {
                        (0, true) => cx as f32 - P_HALF,
                        (0, false) => (cx + 1) as f32 + P_HALF,
                        (2, true) => cz as f32 - P_HALF,
                        (2, false) => (cz + 1) as f32 + P_HALF,
                        (1, true) => cy as f32 - P_HEIGHT,
                        (1, false) => (cy + 1) as f32,
                        _ => unreachable!(),
                    };
                    snap = Some(match snap {
                        Some(s) => {
                            if moving_pos {
                                s.min(cand)
                            } else {
                                s.max(cand)
                            }
                        }
                        None => cand,
                    });
                }
            }
        }

        if let Some(s) = snap {
            self.pos[axis] = s;
            self.vel[axis] = 0.0;
            if axis == 1 && !moving_pos {
                self.on_ground = true;
            }
        } else if axis == 1 {
            self.check_ground(w);
        }
    }

    fn check_ground(&mut self, w: &World) {
        let y = (self.pos[1] - 0.06).floor() as i32;
        let x0 = (self.pos[0] - P_HALF).floor() as i32;
        let x1 = (self.pos[0] + P_HALF).floor() as i32;
        let z0 = (self.pos[2] - P_HALF).floor() as i32;
        let z1 = (self.pos[2] + P_HALF).floor() as i32;
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                if w.blocks_movement(cx, y, cz) {
                    self.on_ground = true;
                    return;
                }
            }
        }
    }

    /// 放方块前检查新方块会不会把人卡进去。
    pub fn overlaps_cell(&self, cell: [i32; 3]) -> bool {
        let min = [self.pos[0] - P_HALF, self.pos[1], self.pos[2] - P_HALF];
        let max = [self.pos[0] + P_HALF, self.pos[1] + P_HEIGHT, self.pos[2] + P_HALF];
        min[0] < (cell[0] + 1) as f32
            && max[0] > cell[0] as f32
            && min[1] < (cell[1] + 1) as f32
            && max[1] > cell[1] as f32
            && min[2] < (cell[2] + 1) as f32
            && max[2] > cell[2] as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks as b;
    use crate::world::{SX, SY, SZ, WATER_LEVEL};

    /// 0..=WATER_LEVEL 实心、上面全空的平地（必须连上面一起清，生成的山和树还在）。
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
    fn 从高处落下会稳稳站在地上() {
        let w = flat_world();
        let mut p = Player::new([40.5, 24.0, 40.5]);
        for _ in 0..200 {
            p.update(&w, 1.0 / 60.0, [0.0, 0.0, 0.0], false, false, false);
        }
        assert!(p.on_ground, "200 帧后该落地了");
        assert!((p.pos[1] - 16.0).abs() < 0.05, "脚底高度 {}", p.pos[1]);
        assert!(p.vel[1].abs() < 1e-3, "落地后还在动 {}", p.vel[1]);
    }

    #[test]
    fn 平地往前走不会沉下去也不会穿墙() {
        let w = flat_world();
        let mut p = Player::new([40.5, 16.0, 40.5]);
        p.on_ground = true;
        for _ in 0..120 {
            p.update(&w, 1.0 / 60.0, [0.0, 0.0, 1.0], false, false, false);
        }
        assert!(p.pos[2] > 41.0, "往前走了但没位移 {}", p.pos[2]);
        assert!(p.pos[2] < 60.0, "一步走太快 {}", p.pos[2]);
        assert!((p.pos[1] - 16.0).abs() < 0.06, "走着走着沉了 {}", p.pos[1]);
    }

    #[test]
    fn 跳一下会起来再落回原地() {
        let w = flat_world();
        let mut p = Player::new([40.5, 16.0, 40.5]);
        p.on_ground = true;
        let start = p.pos[1];
        let mut peak = start;
        for i in 0..150 {
            // 只按头 6 帧，之后松手：一直按着它会连跳，最后一帧可能正悬在半空
            let jump = i < 6;
            p.update(&w, 1.0 / 60.0, [0.0, 0.0, 0.0], jump, false, false);
            peak = peak.max(p.pos[1]);
        }
        assert!(peak > start + 0.8, "跳得太矮 {}", peak - start);
        assert!(p.on_ground, "跳完没落回地面");
        assert!((p.pos[1] - start).abs() < 0.05, "落点偏了 {}", p.pos[1]);
    }

    #[test]
    fn 撞墙会被贴住不穿过去() {
        let mut w = flat_world();
        // 在 x=50 立一面墙
        for y in 0..30 {
            for z in 35..46 {
                w.set(50, y, z, b::STONE);
            }
        }
        let mut p = Player::new([45.5, 16.0, 40.5]);
        p.on_ground = true;
        for _ in 0..240 {
            p.update(&w, 1.0 / 60.0, [1.0, 0.0, 0.0], false, false, true);
        }
        assert!(p.pos[0] < 50.0, "穿墙了 {}", p.pos[0]);
        assert!((p.pos[0] - (50.0 - P_HALF)).abs() < 0.02, "没贴住墙 {}", p.pos[0]);
    }

    #[test]
    fn 飞行模式不受重力影响() {
        let w = flat_world();
        let mut p = Player::new([40.5, 30.0, 40.5]);
        p.fly = true;
        for _ in 0..120 {
            p.update(&w, 1.0 / 60.0, [0.0, 0.0, 0.0], false, false, false);
        }
        assert!((p.pos[1] - 30.0).abs() < 0.01, "悬停却掉了 {}", p.pos[1]);
    }

    #[test]
    fn 放方块不会把自己卡住() {
        let p = Player::new([40.5, 16.0, 40.5]);
        assert!(p.overlaps_cell([40, 16, 40]), "脚下这格有人");
        assert!(p.overlaps_cell([40, 17, 40]), "身体中段");
        assert!(!p.overlaps_cell([42, 16, 40]), "隔壁格不该冲突");
    }
}
