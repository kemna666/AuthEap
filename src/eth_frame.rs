//! 802.1x / EAPOL / EAP 协议常量与以太网帧表示。
//!
//! 字节布局与原版 C 实现完全一致，方便逐字节对照：
//!
//! ```text
//! +----------------- Ethernet Header (14) ---------------+
//! | dst_mac(6) | src_mac(6) | ethertype(2, =0x888e)      |
//! +------------------------------------------------------+
//! | eapol ver(1) | eapol type(1) | eapol len(2)          |   EAPOL Header (4)
//! +------------------------------------------------------+
//! | eap code(1) | eap id(1) | eap len(2) | eap type(1)   |   EAP Header (5), 仅 EAP_PACKET
//! +------------------------------------------------------+
//! | payload ...                                           |
//! +------------------------------------------------------+
//! ```

use crate::log_warn;

// ---- 以太网头部偏移 ----
pub const ETH_DST: usize = 0;
pub const ETH_SRC: usize = 6;
pub const ETH_PROTO: usize = 12;
pub const ETHER_HDR_LEN: usize = 14;

// ---- EAPOL 头部偏移（相对帧首）----
pub const EAPOL_VER: usize = 14;
pub const EAPOL_TYPE: usize = 15;
pub const EAPOL_LEN: usize = 16;
pub const EAPOL_HDR_LEN: usize = 4;

// ---- EAP 头部偏移（仅当 EAPOL type == EAP_PACKET）----
pub const EAP_CODE: usize = 18;
pub const EAP_ID: usize = 19;
pub const EAP_LEN: usize = 20;
pub const EAP_TYPE_BYTE: usize = 22;
pub const EAP_HDR_LEN: usize = 5;

/// 不含 EAP 类型字段的帧头长度：eth(14)+eapol(4)+eap code/id/len(4)
pub const FRAME_HDR_NO_TYPE: usize = 22;
/// 完整帧头长度：FRAME_HDR_NO_TYPE + type(1)
pub const FRAME_HDR_LEN: usize = 23;

/// EAPOL 报文最大缓冲（与原版 FRAME_BUF_SIZE 相近，足够承载锐捷私有附加字段）
pub const FRAME_BUF_SIZE: usize = 2048;

/// PAE 组地址（标准）
pub const BCAST_STANDARD: [u8; 6] = [0x01, 0x80, 0xc2, 0x00, 0x00, 0x03];
/// 锐捷私有 Start 报文目的地址
pub const BCAST_RJ: [u8; 6] = [0x01, 0xd0, 0xf8, 0x00, 0x00, 0x03];
/// PAE EtherType 0x888e 的大端字节
pub const ETH_P_PAE_BYTES: [u8; 2] = [0x88, 0x8e];

/// EAP Code（自 1 起）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EapCode {
    Request = 1,
    Response = 2,
    Success = 3,
    Failure = 4,
}

impl EapCode {
    pub fn from_u8(v: u8) -> Option<EapCode> {
        match v {
            1 => Some(EapCode::Request),
            2 => Some(EapCode::Response),
            3 => Some(EapCode::Success),
            4 => Some(EapCode::Failure),
            _ => None,
        }
    }
}

/// EAPOL Packet Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EapolType {
    EapPacket = 0,
    Start = 1,
    Logoff = 2,
    /// 锐捷私有 Keep-Alive 类型
    RjKeepalive = 0xbf,
}

impl EapolType {
    pub fn from_u8(v: u8) -> EapolType {
        match v {
            0 => EapolType::EapPacket,
            1 => EapolType::Start,
            2 => EapolType::Logoff,
            0xbf => EapolType::RjKeepalive,
            other => {
                // 未知类型，保留原值以便调试
                log_warn!("收到未知 EAPOL 类型: 0x{other:02x}");
                EapolType::EapPacket
            }
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EapolType::EapPacket => "EAP-Packet",
            EapolType::Start => "EAPOL-Start",
            EapolType::Logoff => "EAPOL-Logoff",
            EapolType::RjKeepalive => "EAPOL-RJ-Keepalive",
        }
    }
}

/// EAP Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EapType {
    Identity = 1,
    Md5Challenge = 4,
}

impl EapType {
    pub fn from_u8(v: u8) -> Option<EapType> {
        match v {
            1 => Some(EapType::Identity),
            4 => Some(EapType::Md5Challenge),
            _ => None,
        }
    }
}

/// 一个以太网 EAPOL/EAP 帧。`buf` 自以太网头开始。
#[derive(Debug, Clone)]
pub struct Frame {
    pub buf: Vec<u8>,
}

impl Frame {
    pub fn from_bytes(bytes: &[u8]) -> Frame {
        Frame {
            buf: bytes.to_vec(),
        }
    }

    pub fn new_zeroed(len: usize) -> Frame {
        Frame {
            buf: vec![0u8; len],
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn dst_mac(&self) -> Option<[u8; 6]> {
        if self.buf.len() < ETH_DST + 6 {
            return None;
        }
        let mut m = [0u8; 6];
        m.copy_from_slice(&self.buf[ETH_DST..ETH_DST + 6]);
        Some(m)
    }

    pub fn src_mac(&self) -> Option<[u8; 6]> {
        if self.buf.len() < ETH_SRC + 6 {
            return None;
        }
        let mut m = [0u8; 6];
        m.copy_from_slice(&self.buf[ETH_SRC..ETH_SRC + 6]);
        Some(m)
    }

    /// 是否为 PAE(0x888e) 帧
    pub fn is_pae(&self) -> bool {
        self.buf.len() >= ETHER_HDR_LEN && self.buf[ETH_PROTO..ETH_PROTO + 2] == ETH_P_PAE_BYTES
    }

    pub fn eapol_type(&self) -> Option<EapolType> {
        if self.buf.len() < EAPOL_TYPE + 1 {
            return None;
        }
        Some(EapolType::from_u8(self.buf[EAPOL_TYPE]))
    }

    /// EAP Code（仅对 EAP-Packet 有意义）
    pub fn eap_code(&self) -> Option<EapCode> {
        if self.buf.len() < EAP_CODE + 1 {
            return None;
        }
        EapCode::from_u8(self.buf[EAP_CODE])
    }

    pub fn eap_id(&self) -> Option<u8> {
        if self.buf.len() < EAP_ID + 1 {
            return None;
        }
        Some(self.buf[EAP_ID])
    }

    /// 从偏移 `off` 读取长度字节（大端）累计
    fn be16_at(&self, off: usize) -> Option<u16> {
        if self.buf.len() < off + 2 {
            return None;
        }
        Some(u16::from_be_bytes([self.buf[off], self.buf[off + 1]]))
    }

    pub fn eapol_len(&self) -> Option<u16> {
        self.be16_at(EAPOL_LEN)
    }

    pub fn eap_len(&self) -> Option<u16> {
        self.be16_at(EAP_LEN)
    }
}
