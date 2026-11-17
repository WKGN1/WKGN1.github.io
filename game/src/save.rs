//! 存档编码：行程编码（RLE）。
//!
//! 世界只有一张 96x96x48 的图，原始 432 KB；地形里全是长串重复（同一列上下几十格
//! 都是石头），所以只做起行编码就压得足够狠，落盘通常几十 KB —— 存 IndexedDB 和
//! 「点导出下载」都够轻。注意 flate2 其实还在依赖树里（macroquad 经 image→png 用着它），
//! 所以不直接调它省的是「自己维护的代码」，不是 wasm 体积——体积结论以实测为准。
//! 顺带一个好处：导出的文件能直接发给别人。

use crate::world::World;

const MAGIC: &[u8; 4] = b"MCL1";
const VERSION: u8 = 1;
const MAX_RUN: u64 = 255;

/// 行程编码：一串 (count, value)，count 1..=255。超过 255 的长串自动断开。
fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 8 + 16);
    let mut i = 0usize;
    while i < data.len() {
        let v = data[i];
        let mut n = 1u64;
        while i + (n as usize) < data.len() && data[i + (n as usize)] == v && n < MAX_RUN {
            n += 1;
        }
        out.push(n as u8);
        out.push(v);
        i += n as usize;
    }
    out
}

fn rle_decode(data: &[u8], expect_len: usize) -> Option<Vec<u8>> {
    if data.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(expect_len);
    let mut pairs = data.chunks_exact(2);
    while let Some(p) = pairs.next() {
        let (n, v) = (p[0] as usize, p[1]);
        if n == 0 {
            return None; // count 为 0 一定是坏数据
        }
        if out.len() + n > expect_len {
            return None; // 超长，说明文件被截断或被改坏
        }
        out.resize(out.len() + n, v);
    }
    if out.len() != expect_len {
        return None;
    }
    Some(out)
}

/// 把整个世界打包成一个可下载的字节流。
pub fn encode(w: &World) -> Vec<u8> {
    let raw = w.take_bytes();
    let body = rle_encode(raw);

    let mut out = Vec::with_capacity(body.len() + 17);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&w.seed.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[derive(Debug)]
pub enum LoadError {
    BadFormat,
    WrongSize,
}

/// 解包。坏数据一律返回 Err，让调用方回落到「按 seed 新建世界」。
pub fn decode(bytes: &[u8]) -> Result<(u32, Vec<u8>), LoadError> {
    if bytes.len() < 17 || &bytes[0..4] != MAGIC || bytes[4] != VERSION {
        return Err(LoadError::BadFormat);
    }
    let seed = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
    let raw_len = u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]) as usize;
    let body_len = u32::from_le_bytes([bytes[13], bytes[14], bytes[15], bytes[16]]) as usize;
    if bytes.len() < 17 + body_len {
        return Err(LoadError::BadFormat);
    }
    let raw = rle_decode(&bytes[17..17 + body_len], raw_len).ok_or(LoadError::WrongSize)?;
    Ok((seed, raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks as b;

    #[test]
    fn 存一遍再读回来必须一模一样() {
        let w = World::new(20261117);
        let bytes = encode(&w);
        let (seed, raw) = decode(&bytes).expect("自己存的该能读回来");
        assert_eq!(seed, w.seed);
        assert_eq!(raw, w.take_bytes());
    }

    #[test]
    fn 存档体积要小到能当附件发() {
        let w = World::new(7);
        let bytes = encode(&w);
        let raw = w.take_bytes().len();
        assert!(
            bytes.len() * 8 < raw,
            "压缩得不够狠：{} -> {}",
            raw,
            bytes.len()
        );
    }

    #[test]
    fn 坏数据必须报错而不是崩或读到错图() {
        assert!(matches!(decode(b"nope"), Err(LoadError::BadFormat)));
        assert!(matches!(decode(&[]), Err(LoadError::BadFormat)));

        let w = World::new(9);
        let mut good = encode(&w);
        good.truncate(good.len() - 5); // 截断
        assert!(decode(&good).is_err(), "截断的文件该报错");

        let mut w2 = World::new(9);
        w2.set(10, 10, 10, b::LAMP);
        let bytes = encode(&w2);
        let (seed, raw) = decode(&bytes).unwrap();
        assert_eq!(seed, w2.seed);
        assert_eq!(raw, w2.take_bytes());
    }

    #[test]
    fn rle_往返对拍随机器数据也必须成立() {
        // 造一串有长有短的随机-ish 数据，覆盖 255 断开那条路径。
        let mut data = Vec::new();
        let mut x = 12345u32;
        for _ in 0..400 {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = ((x >> 24) & 7) as u8;
            let n = ((x >> 12) % 400) as usize;
            data.resize(data.len() + n, v);
        }
        let enc = rle_encode(&data);
        let dec = rle_decode(&enc, data.len()).expect("对拍该成功");
        assert_eq!(dec, data);
        assert!(rle_decode(&enc, data.len() + 1).is_none(), "期望长度不对时报错");
    }
}
