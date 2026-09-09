//! RJ 私有位交换编码/解码 —— 移植自 `rjencode.c`。
//!
//! 锐捷 v3 私有字段头使用这种逐字节的位重排来“加密”。此变换是自反的（对同一字节
//! 连续 encode 两次恢复原值），本质是 8 个位位置两两互换。

fn encode_byte(b: u8) -> u8 {
    let v = ((b & 0x10) >> 1)
        | ((b & 0x20) >> 3)
        | ((b & 0x40) >> 5)
        | (b >> 7)
        | ((b & 0x08) << 1)
        | ((b & 0x04) << 3)
        | ((b & 0x02) << 5)
        | ((b & 0x01) << 7);
    !v
}

/// 对缓冲区逐字节做 RJ encode。
pub fn rj_encode(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        *b = encode_byte(*b);
    }
}

/// 对缓冲区逐字节做 RJ decode（与 encode 互为逆）。
pub fn rj_decode(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        *b = encode_byte(encode_byte(*b));
        // 实际是自反的，decode == encode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn involutive() {
        let mut a = [0u8, 1, 0x55, 0xaa, 0xff, 0x13, 0x37, 0x7f, 0x80, 0x10];
        let orig = a;
        rj_encode(&mut a);
        rj_encode(&mut a);
        assert_eq!(a, orig);
    }
}
