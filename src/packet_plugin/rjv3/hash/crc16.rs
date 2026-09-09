//! CRC16 (XMODEM / CRC-16/ACORN) —— 移植自 `rjcrc16.c`。
//! Poly=0x1021，Init=0，输入输出均不反转。参考向量："123456789" → 0x31C3。

/// 计算 CRC16-XMODEM。
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmodem_check() {
        assert_eq!(crc16(b"123456789"), 0x31c3);
    }
}
