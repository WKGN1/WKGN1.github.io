//! 存档编码：行程编码（RLE）。
//!
//! 世界只有一张 96x96x48 的图，原始 432 KB；地形里全是长串重复（同一列上下几十格
//! 都是石头），所以只做起行编码就压得足够狠，落盘通常几十 KB —— 存 IndexedDB 和
//! 「点导出下载」都够轻。注意 flate2 其实还在依赖树里（macroquad 经 image→png 用着它），
//! 所以不直接调它省的是「自己维护的代码」，不是 wasm 体积——体积结论以实测为准。
//! 顺带一个好处：导出的文件能直接发给别人。

use crate::world::{World, SX, SY, SZ};

const MAGIC: &[u8; 4] = b"MCL1";
const VERSION: u8 = 1;
const MAX_RUN: u64 = 255;

/// 头部：MAGIC(4) + VERSION(1) + seed(4) + raw_len(4) + body_len(4)。
const HEADER: usize = 17;

/// 世界体素数。raw_len 必须正好等于它，读回来才敢照着它分配内存。
const RAW_LEN: u64 = SX as u64 * SY as u64 * SZ as u64;

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
    // 别拿 expect_len 直接当容量：它是从文件里读出来的数，wasm32 的 usize 只有 32 位，
    // 一句 with_capacity(4e9) 就能把整个页面顶爆。上限夹一下，真不够 resize 会自己长。
    let mut out = Vec::with_capacity(expect_len.min(1 << 20));
    let mut pairs = data.chunks_exact(2);
    while let Some(p) = pairs.next() {
        let (n, v) = (p[0] as usize, p[1]);
        if n == 0 {
            return None; // count 为 0 一定是坏数据
        }
        // 这里也不能裸加：32 位 usize + 关了溢出检查，越界不会 panic 而是回绕成小数，
        // 于是「超长」这条判据会失效，坏数据就被放过去了。
        let next = match out.len().checked_add(n) {
            Some(v) => v,
            None => return None,
        };
        if next > expect_len {
            return None; // 超长，说明文件被截断或被改坏
        }
        out.resize(next, v);
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
    if bytes.len() < HEADER || &bytes[0..4] != MAGIC || bytes[4] != VERSION {
        return Err(LoadError::BadFormat);
    }
    let seed = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
    // 长度全部在 u64 上算完再比。wasm32 的 usize 是 32 位、release 又关了溢出检查：
    // body_len 是 0xFFFFFFFF 时「17 + body_len」回绕成 16，检查被放行，
    // 紧接着那个切片 bytes[17..17+body_len] 就从 17 越到 16 → panic。
    // panic=abort 的 wasm 里那就是 trap：主循环连一帧都不续，画面冻住，刷新还冻。
    let raw_len = u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]) as u64;
    let body_len = u32::from_le_bytes([bytes[13], bytes[14], bytes[15], bytes[16]]) as u64;
    let total = (HEADER as u64).checked_add(body_len).ok_or(LoadError::BadFormat)?;
    if (bytes.len() as u64) < total {
        return Err(LoadError::BadFormat);
    }
    let body_len = body_len as usize; // 到这儿 body_len <= bytes.len() - HEADER，截断是安全的

    // raw_len 也必须核：它是 rle_decode 的期望长度（以前完全没查）。
    // 核成「正好等于体素数」之后，expect_len 就不可能是几十亿，with_capacity 那条风险也一并没了。
    if raw_len != RAW_LEN {
        return Err(LoadError::WrongSize);
    }
    let raw = rle_decode(&bytes[HEADER..HEADER + body_len], raw_len as usize)
        .ok_or(LoadError::WrongSize)?;
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

        // body_len 写成 u32 最大值：32 位 usize 上「17 + body_len」回绕成 16，
        // 那条长度检查被放行，接着的切片 [17..16] 直接 panic（wasm 里就是整个彩蛋掐死）。
        let mut wrap = Vec::new();
        wrap.extend_from_slice(MAGIC);
        wrap.push(VERSION);
        wrap.extend_from_slice(&1234u32.to_le_bytes());
        wrap.extend_from_slice(&(RAW_LEN as u32).to_le_bytes());
        wrap.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        wrap.extend_from_slice(&[1, 2, 3]);
        assert!(
            matches!(decode(&wrap), Err(LoadError::BadFormat)),
            "body_len 回绕该报错，不该 panic"
        );

        // raw_len 被改坏：它是 rle_decode 的期望长度，也决定 with_capacity 的量。
        let mut bad_raw = encode(&w);
        bad_raw[9..13].copy_from_slice(&((RAW_LEN as u32) ^ 0xFFFF_FFFF).to_le_bytes());
        assert!(
            matches!(decode(&bad_raw), Err(LoadError::WrongSize)),
            "raw_len 不对时必须报错，不能照着它分配内存"
        );
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
