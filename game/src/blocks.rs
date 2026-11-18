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

// 纹理图集：12 张 16x16 的贴片排在同一行，尺寸 192x16。
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

pub const TILE_PX: u32 = 16;
pub const TILE_COUNT: u32 = 12;
pub const ATLAS_W: u32 = TILE_PX * TILE_COUNT; // 192
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
        GRASS | DIRT | STONE | SAND | WOOD | PLANK | LEAF | SNOW | LAMP | SNOW_GRASS
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
        TILE_WATER => {
            // 横向波纹 + 细噪点：不动图仿动图，省下时间做多卡动画
            let band = (((y as f32 + (x as f32 * 0.35)) * 1.1).sin() + 1.0) * 0.5;
            let base = rgb(56, 108, 176);
            shade(base, 0.90 + band * 0.22 + j * 0.05)
        }
        _ => rgb(200, 200, 200),
    }
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
    fn 图集尺寸和声明一致() {
        let a = atlas_rgba();
        assert_eq!(a.len(), (ATLAS_W * ATLAS_H * 4) as usize);
        for px in a.chunks_exact(4) {
            assert_eq!(px[3], 255, "图集不该有透明像素，透明会让方块面漏光");
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
