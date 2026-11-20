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

/// 3D 格点哈希。跟 hash2 是同一套 splitmix64，只是把三根轴一起搅进去，可复现。
#[inline]
pub fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let h = (x as u64)
        .wrapping_mul(0x9e3779b97f4a7c15)
        ^ (y as u64).wrapping_mul(0xbf58476d1ce4e5b9).rotate_left(17)
        ^ (z as u64).wrapping_mul(0x94d049bb133111eb).rotate_left(31)
        ^ (seed as u64).wrapping_mul(0x100000001b3);
    let h = mix64(h);
    (h >> 40) as f32 / ((1u64 << 24) as f32)
}

/// 三线插值的 3D 值噪声，返回 0..1。洞穴必须三根轴都平滑：只拿 2D 噪声去雕，
/// 出来的是一根根竖柱子，不是能走进去的隧道。
pub fn noise3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let zi = z.floor();
    let xf = smooth(x - xi);
    let yf = smooth(y - yi);
    let zf = smooth(z - zi);
    let (x0, y0, z0) = (xi as i32, yi as i32, zi as i32);

    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    // 先把 z 那两根面各自在 x/y 上插平，再沿 z 插一次：三根轴都过了 smoothstep。
    let n = |dx: i32, dy: i32, dz: i32| hash3(x0 + dx, y0 + dy, z0 + dz, seed);
    let b00 = l(n(0, 0, 0), n(1, 0, 0), xf);
    let b10 = l(n(0, 1, 0), n(1, 1, 0), xf);
    let b01 = l(n(0, 0, 1), n(1, 0, 1), xf);
    let b11 = l(n(0, 1, 1), n(1, 1, 1), xf);
    let b0 = l(b00, b10, yf);
    let b1 = l(b01, b11, yf);
    l(b0, b1, zf).clamp(0.0, 1.0)
}

/// 3D fbm。只要两阶：我们要的是大隧道，不是表面细节，第三阶在 wasm 上是白花钱。
pub fn fbm3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let a = noise3(x, y, z, seed);
    let b = noise3(x * 2.07, y * 2.07, z * 2.07, seed.wrapping_add(0x9e37_79b9));
    (a * 0.64 + b * 0.36).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 三维噪声同种子必须同结果换种子要不一样() {
        assert_eq!(fbm3(3.5, 2.25, -1.5, 77), fbm3(3.5, 2.25, -1.5, 77));
        assert_ne!(
            fbm3(3.5, 2.25, -1.5, 77),
            fbm3(3.5, 2.25, -1.5, 78),
            "换个种子结果一模一样，那 seed 等于没传进去"
        );
        assert_ne!(noise3(1.0, 2.0, 3.0, 7), noise3(1.0, 2.0, 4.0, 7), "z 轴没进哈希");
        assert_ne!(noise3(1.0, 2.0, 3.0, 7), noise3(1.0, 3.0, 3.0, 7), "y 轴没进哈希");
        assert_ne!(noise3(1.0, 2.0, 3.0, 7), noise3(2.0, 2.0, 3.0, 7), "x 轴没进哈希");
    }

    #[test]
    fn 三维噪声取值在零到一之间() {
        for i in 0..24 {
            for j in 0..24 {
                for k in 0..6 {
                    let v = fbm3(i as f32 * 0.31, j as f32 * 0.19, k as f32 * 0.53, 5);
                    assert!((0.0..=1.0).contains(&v), "越界：{v}");
                    let w = noise3(i as f32 * 0.31, j as f32 * 0.19, k as f32 * 0.53, 5);
                    assert!((0.0..=1.0).contains(&w), "越界：{w}");
                }
            }
        }
    }

    /// 平滑性：每挪一小步，噪声不许跳一大截。不插值的哈希噪声雕出来的洞是"一格一方块"的洞，
    /// 人走不进去；这条钉住三根轴真的都在插值。
    #[test]
    fn 三维噪声沿每条轴都得是渐变不能一格一跳() {
        let 步长 = 0.06f32;
        let 上限 = 0.25f32; // 一格周期内最大斜率约 1.5，0.06 步最多跳 ~0.09；不插值会跳到接近 1
        for i in 0..40 {
            let t = i as f32 * 0.31;
            let dx = noise3(t + 步长, 1.0, 2.0, 9) - noise3(t, 1.0, 2.0, 9);
            let dy = noise3(1.0, t + 步长, 2.0, 9) - noise3(1.0, t, 2.0, 9);
            let dz = noise3(1.0, 2.0, t + 步长, 9) - noise3(1.0, 2.0, t, 9);
            for (v, 轴) in [(dx, "x"), (dy, "y"), (dz, "z")] {
                assert!(
                    v.abs() <= 上限,
                    "沿 {} 走 {:?} 就跳了 {:.3}：这条轴的插值没生效",
                    轴,
                    步长,
                    v
                );
            }
        }
    }

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
