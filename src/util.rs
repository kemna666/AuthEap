//! 各类小工具：十六进制转换、位反转、简单的非加密伪随机数等。

use std::time::{SystemTime, UNIX_EPOCH};

/// 将单个十六进制字符（0-9a-fA-F）转换为数值。
pub fn hex_char(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// 解析十六进制字符串为字节序列（忽略空白与 `0x` 前缀）。
pub fn hex_str_to_bytes(s: &str) -> Option<Vec<u8>> {
    let t: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace("0x", "");
    if t.len() % 2 != 0 {
        return None;
    }
    let b = t.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    for i in (0..b.len()).step_by(2) {
        let hi = hex_char(b[i])?;
        let lo = hex_char(b[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

/// 将单个字节转为两位小写十六进制字符。
pub fn byte_to_hex(b: u8, out: &mut [u8; 2]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out[0] = HEX[(b >> 4) as usize];
    out[1] = HEX[(b & 0xf) as usize];
}

/// 字节序列 → 小写十六进制字符串。
pub fn bytes_to_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    let mut pair = [0u8; 2];
    for &x in b {
        byte_to_hex(x, &mut pair);
        s.push(pair[0] as char);
        s.push(pair[1] as char);
    }
    s
}

/// 按位反转一个字节（原 C 实现 `bit_reverse`）。
pub fn bit_reverse(b: u8) -> u8 {
    b.reverse_bits()
}

/// 解析形如 `aa:bb:cc:dd:ee:ff` 或 `aabbccddeeff` 的 MAC 地址。
pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let compact: String = s.chars().filter(|c| *c != ':' && *c != '-').collect();
    let b = hex_str_to_bytes(&compact)?;
    if b.len() != 6 {
        return None;
    }
    let mut out = [0u8; 6];
    out.copy_from_slice(&b);
    Some(out)
}

/// 格式化 MAC 地址 `aa:bb:cc:dd:ee:ff`。
pub fn mac_to_string(mac: &[u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

/// 简单 xorshift64 伪随机数生成器（足够用于 echo no 等非安全场合）。
pub struct Prng(u64);

impl Prng {
    pub fn new() -> Self {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        let mut s = Prng(t | 1);
        // 预热几次，避免种子不良导致的退化序列
        for _ in 0..4 {
            s.next_u64();
        }
        s
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// 返回 [0, 2^32) 区间随机数（对应 C 的 rand() 语义下限）。
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
}

impl Default for Prng {
    fn default() -> Self {
        Self::new()
    }
}

/// 将 `val` 数值转换为十六进制字符串（用于调试）。
#[allow(dead_code)]
pub fn u32_hex(val: u32) -> String {
    format!("{val:08x}")
}
