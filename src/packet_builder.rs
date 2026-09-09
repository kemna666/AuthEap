//! 标准 EAP / EAPOL 帧构建（对应原版 `packet_builder.c`）。
//!
//! 生成的帧仅包含标准字段；锐捷私有 trailer 由插件在发送前追加。

use crate::eth_frame::{EapCode, EapType, EapolType, Frame, ETH_P_PAE_BYTES};
use crate::packet_plugin::rjv3::hash::std_md5;

fn push_eth(out: &mut Vec<u8>, dst: &[u8; 6], src: &[u8; 6]) {
    out.extend_from_slice(dst);
    out.extend_from_slice(src);
    out.extend_from_slice(&ETH_P_PAE_BYTES);
}

fn push_eapol(out: &mut Vec<u8>, eapol_type: u8, eap_len: u16) {
    out.push(1); // EAPOL version = 1
    out.push(eapol_type);
    out.extend_from_slice(&eap_len.to_be_bytes());
}

fn push_eap_header(out: &mut Vec<u8>, code: u8, id: u8, eap_len: u16, eap_type: u8) {
    out.push(code);
    out.push(id);
    out.extend_from_slice(&eap_len.to_be_bytes());
    out.push(eap_type);
}

/// EAPOL-Start（发往 `dst`）
pub fn start_frame(dst: &[u8; 6], src: &[u8; 6]) -> Frame {
    let mut buf = Vec::with_capacity(18);
    push_eth(&mut buf, dst, src);
    push_eapol(&mut buf, EapolType::Start as u8, 0);
    Frame { buf }
}

/// EAPOL-Logoff
pub fn logoff_frame(dst: &[u8; 6], src: &[u8; 6]) -> Frame {
    let mut buf = Vec::with_capacity(18);
    push_eth(&mut buf, dst, src);
    push_eapol(&mut buf, EapolType::Logoff as u8, 0);
    Frame { buf }
}

/// Response-Identity
pub fn identity_response(dst: &[u8; 6], src: &[u8; 6], id: u8, username: &str) -> Frame {
    let ulen = username.len();
    let eap_len = (5 + ulen) as u16;
    let mut buf = Vec::new();
    push_eth(&mut buf, dst, src);
    push_eapol(&mut buf, EapolType::EapPacket as u8, eap_len);
    push_eap_header(
        &mut buf,
        EapCode::Response as u8,
        id,
        eap_len,
        EapType::Identity as u8,
    );
    buf.extend_from_slice(username.as_bytes());
    Frame { buf }
}

/// Response-MD5-Challenge。`seed` 为服务器发来的 16 字节挑战。
pub fn md5_challenge_response(
    dst: &[u8; 6],
    src: &[u8; 6],
    id: u8,
    username: &str,
    password: &str,
    seed: &[u8; 16],
) -> Frame {
    // 标准 EAP-MD5：MD5(id || password || seed)
    let mut md5src = Vec::with_capacity(1 + password.len() + 16);
    md5src.push(id);
    md5src.extend_from_slice(password.as_bytes());
    md5src.extend_from_slice(seed);
    let digest = std_md5(&md5src);

    let ulen = username.len();
    let eap_len = (5 + 1 + 16 + ulen) as u16; // header + value-size + digest + user
    let mut buf = Vec::new();
    push_eth(&mut buf, dst, src);
    push_eapol(&mut buf, EapolType::EapPacket as u8, eap_len);
    push_eap_header(
        &mut buf,
        EapCode::Response as u8,
        id,
        eap_len,
        EapType::Md5Challenge as u8,
    );
    buf.push(16); // MD5-Value-Size
    buf.extend_from_slice(&digest);
    buf.extend_from_slice(username.as_bytes());
    Frame { buf }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_layout() {
        let f = identity_response(&[1; 6], &[2; 6], 7, "abc");
        assert_eq!(f.buf.len(), 26); // 18 + 5 + 3
        assert_eq!(&f.buf[12..14], &ETH_P_PAE_BYTES);
        assert_eq!(f.buf[15], 0); // EAP-Packet
        assert_eq!(f.buf[18], 2); // Response
        assert_eq!(f.buf[19], 7); // id
        assert_eq!(&f.buf[20..22], &[0, 8]); // eap len = 5+3
        assert_eq!(f.buf[22], 1); // Identity
        assert_eq!(&f.buf[23..], b"abc");
    }

    #[test]
    fn challenge_layout() {
        let f = md5_challenge_response(&[1; 6], &[2; 6], 9, "u", "p", &[0x11; 16]);
        assert_eq!(f.buf.len(), 41); // 18 + 5 + 1 + 16 + 1
        assert_eq!(f.buf[19], 9);
        assert_eq!(f.buf[22], 4); // MD5-Challenge
        assert_eq!(f.buf[23], 16); // value size
        // digest 与手算一致
        let mut src = vec![9u8];
        src.extend_from_slice(b"p");
        src.extend_from_slice(&[0x11; 16]);
        let d = std_md5(&src);
        assert_eq!(&f.buf[24..40], &d);
        assert_eq!(&f.buf[40..], b"u");
    }
}
