//! 自带一套可复现的整数哈希 + 值噪声 + fbm，不引第三方噪声库。
//! 目的只有两个：存档能靠 seed 重建、wasm 里少带依赖。

/// splitmix64 的一轮，用来做格子点的哈希。比 rand 便宜且确定性。
#[inline]
pub fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// 2D 格点哈希，返回 0..1。seed 让同一个种子在任何机器上生成同一张地图。
#[inline]
pub fn hash2(x: i32, z: i32, seed: u32) -> f32 {
    let h = mix64(((x as i64 as u64) << 32) ^ (z as i64 as u64) ^ (seed as u64).wrapping_mul(0x100000001b3));
    (h >> 40) as f32 / ((1u64 << 24) as f32)
}

/// 一维 smoothstep（值噪声需要平滑过渡，线性会看到明显的方格棱）。
#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// 双线性插值的值噪声，返回 0..1。
pub fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let xi = x.floor();
    let zi = z.floor();
    let xf = smooth(x - xi);
    let zf = smooth(z - zi);

    let x0 = xi as i32;
    let z0 = zi as i32;

    let a = hash2(x0, z0, seed);
    let b = hash2(x0 + 1, z0, seed);
    let c = hash2(x0, z0 + 1, seed);
    let d = hash2(x0 + 1, z0 + 1, seed);

    let top = a + (b - a) * xf;
    let bottom = c + (d - c) * xf;
    let v = top + (bottom - top) * zf;
    v.clamp(0.0, 1.0)
}

/// 分形叠加（fbm）。八度越多细节越多，wasm 上按需取舍。
pub fn fbm(x: f32, z: f32, seed: u32, octaves: u32) -> f32 {
    let mut sum = 0.0f32;
    let mut amp = 0.5f32;
    let mut freq = 1.0f32;
    let mut norm = 0.0f32;
    for o in 0..octaves {
        sum += value_noise(x * freq, z * freq, seed.wrapping_add(o as u32 * 0x9e37)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 同种子同输入必须同结果() {
        assert_eq!(fbm(3.5, -2.25, 77, 4), fbm(3.5, -2.25, 77, 4));
        assert_ne!(fbm(3.5, -2.25, 77, 4), fbm(3.5, -2.25, 78, 4));
    }

    #[test]
    fn 噪声取值在零到一之间() {
        for i in -40..40 {
            for j in -40..40 {
                let v = fbm(i as f32 * 0.37, j as f32 * 0.19, 5, 4);
                assert!((0.0..=1.0).contains(&v), "越界: {v}");
            }
        }
    }
}
