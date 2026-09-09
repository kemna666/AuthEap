//! 锐捷私有 Keep-Alive（心跳）报文构建。

use crate::eth_frame::{EapolType, Frame, ETH_P_PAE_BYTES};
use crate::util::bit_reverse;

/// EAPOL 私有 Keep-Alive 载荷模板（27 字节）。
const KA_TEMPLATE: [u8; 27] = [
    0xFF, 0xFF, 0x37, 0x77, 0x7F, 0x9F, 0xFF, 0xFF, 0xD9, 0x13, 0xFF, 0xFF, 0x37,
    0x77, 0x7F, 0x9F, 0xFF, 0xFF, 0xF7, 0x2B, 0xFF, 0xFF, 0x37, 0x77, 0x7F, 0x3F,
    0xFF,
];

/// 构建一条 Keep-Alive 帧。`echono` 传入后自动 +1（对应 C 的 `g_echono++`）。
pub fn build_keepalive_frame(
    echokey: u32,
    echono: &mut u32,
    dest: &[u8; 6],
    src: &[u8; 6],
) -> Frame {
    let num1 = echokey.wrapping_add(*echono);
    let num2 = *echono;
    *echono = echono.wrapping_add(1);

    let mut t = KA_TEMPLATE;
    t[6] = !bit_reverse((num1 >> 24) as u8);
    t[7] = !bit_reverse((num1 >> 16) as u8);
    t[8] = !bit_reverse((num1 >> 8) as u8);
    t[9] = !bit_reverse(num1 as u8);

    t[16] = !bit_reverse((num2 >> 24) as u8);
    t[17] = !bit_reverse((num2 >> 16) as u8);
    t[18] = !bit_reverse((num2 >> 8) as u8);
    t[19] = !bit_reverse(num2 as u8);

    // 与 C send_echo_frame 一致：eth(14)+eapol(4)，eapol len 域 = {0, 30}，
    // 后面接 27 字节模板，总长 45。
    let mut buf = vec![0u8; 18];
    buf[0..6].copy_from_slice(dest);
    buf[6..12].copy_from_slice(src);
    buf[12..14].copy_from_slice(&ETH_P_PAE_BYTES);
    buf[14] = 1; // eapol version
    buf[15] = EapolType::RjKeepalive as u8;
    buf[16] = 0;
    buf[17] = 30; // 见 C 注释（实际 27 但填 30）
    buf.extend_from_slice(&t);

    Frame { buf }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keepalive_structure() {
        let mut echono: u32 = 0x1234;
        let dest = [0x01u8, 0xd0, 0xf8, 0, 0, 3];
        let src = [0x00u8, 0x11, 0x22, 0x33, 0x44, 0x55];
        let f = build_keepalive_frame(0xdeadbeef, &mut echono, &dest, &src);
        assert_eq!(f.buf.len(), 45);
        assert_eq!(&f.buf[0..6], &dest);
        assert_eq!(&f.buf[6..12], &src);
        assert_eq!(f.buf[15], 0xbf);
        assert_eq!(&f.buf[16..18], &[0, 30]);
        assert_eq!(echono, 0x1235);

        // 载荷（帧内偏移 18..45）
        let p = &f.buf[18..];
        assert_eq!(p.len(), 27);
        // 未改写区与模板一致
        assert_eq!(&p[0..6], &KA_TEMPLATE[0..6]);
        assert_eq!(&p[10..16], &KA_TEMPLATE[10..16]);
        assert_eq!(&p[20..27], &KA_TEMPLATE[20..27]);
        // 改写区符合 key/no 计算
        let num1 = 0xdeadbeefu32.wrapping_add(0x1234);
        for (i, k) in [24usize, 16, 8, 0].iter().enumerate() {
            let expected = !bit_reverse(((num1 >> k) & 0xff) as u8);
            assert_eq!(p[6 + i], expected);
        }
        let num2 = 0x1234u32;
        for (i, k) in [24usize, 16, 8, 0].iter().enumerate() {
            let expected = !bit_reverse(((num2 >> k) & 0xff) as u8);
            assert_eq!(p[16 + i], expected);
        }
    }
}
