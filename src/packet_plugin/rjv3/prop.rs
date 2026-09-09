//! 锐捷 v3 私有字段（prop）的类型、常量与序列化/解析。
//!
//! 每个 0x1a 型字段在报文中的布局：
//! ```text
//! header1: type(0x1a) | header_len            (2 字节)
//! header2: magic(00 00 13 11) | type | len    (6 字节)
//! content: len - 2 字节
//! ```
//! 其中 `header1.header_len = content+8`，`header2.len = content+2`。

use crate::error::{Error, Result};

pub const PROP_MAGIC: [u8; 4] = [0x00, 0x00, 0x13, 0x11];
pub const PROP_HEADER1_TYPE: u8 = 0x1a;
pub const HEADER1_LEN: usize = 2;
pub const HEADER2_LEN: usize = 6;

// ---- 字段类型与定长（与 packet_plugin_rjv3_priv.h 一致）----
pub const TYPE_DHCP: u8 = 0x18; // 4B，dhcp 开关（高位）
pub const SIZE_DHCP: usize = 4;
pub const TYPE_MAC: u8 = 0x2d; // 6B
pub const SIZE_MAC: usize = 6;
pub const TYPE_PWD_HASH: u8 = 0x2f; // 16B（computePwd）
pub const SIZE_PWD_HASH: usize = 16;
pub const TYPE_SEC_DNS: u8 = 0x76; // 第二 DNS 的 ASCII
pub const TYPE_MISC_2: u8 = 0x35; // 1B（0x01）
pub const SIZE_MISC_2: usize = 1;
pub const TYPE_LL_IPV6: u8 = 0x36; // 16B 链路本地(SLAAC)
pub const SIZE_LL_IPV6: usize = 16;
pub const TYPE_LL_IPV6_T: u8 = 0x38; // 16B 临时
pub const SIZE_LL_IPV6_T: usize = 16;
pub const TYPE_ACCOUNTING_MSG: u8 = 0x3c;
pub const TYPE_GLB_IPV6: u8 = 0x4e; // 16B 全局
pub const SIZE_GLB_IPV6: usize = 16;
pub const TYPE_V3_HASH: u8 = 0x4d; // 128B hex ASCII
pub const SIZE_V3_HASH: usize = 0x80;
pub const TYPE_SERVICE: u8 = 0x39; // 服务名 32B 定长
pub const SIZE_SERVICE: usize = 0x20;
pub const TYPE_HDD_SER: u8 = 0x54; // 硬盘序列号 64B 定长
pub const SIZE_HDD_SER: usize = 0x40;
pub const TYPE_MISC_6: u8 = 0x55; // 空
pub const TYPE_MISC_7: u8 = 0x62; // 1B（0x00）
pub const SIZE_MISC_7: usize = 1;
pub const TYPE_OS_BITS: u8 = 0x70; // 1B（0x40 = 64 位）
pub const SIZE_OS_BITS: usize = 1;
pub const TYPE_VER_STR: u8 = 0x6f; // 版本串，0 结尾
pub const TYPE_ECHOKEY: u8 = 0x01; // 心跳密钥所在字段类型

pub const RJV3_PAD_SIZE: usize = 16;

/// 一个字段（仅内容与类型，头信息生成时确定）。
#[derive(Clone, Debug)]
pub struct RjProp {
    pub prop_type: u8,
    pub content: Vec<u8>,
}

impl RjProp {
    pub fn new(prop_type: u8, content: Vec<u8>) -> RjProp {
        RjProp { prop_type, content }
    }

    /// 序列化后的总长度
    pub fn serialized_len(&self) -> usize {
        HEADER1_LEN + HEADER2_LEN + self.content.len()
    }

    /// 写入字节缓冲
    pub fn write_to(&self, out: &mut Vec<u8>) {
        let content_len = self.content.len();
        out.push(PROP_HEADER1_TYPE);
        out.push((content_len + HEADER1_LEN + HEADER2_LEN) as u8);
        out.extend_from_slice(&PROP_MAGIC);
        out.push(self.prop_type);
        out.push((content_len + 2) as u8);
        out.extend_from_slice(&self.content);
    }
}

/// 在字段列表中追加字段；若 `prop_type` 相同且 nonempty 则覆盖同类型（供修改用）。
pub fn append_prop(props: &mut Vec<RjProp>, prop_type: u8, content: Vec<u8>) {
    props.push(RjProp::new(prop_type, content));
}

/// 按类型查找
pub fn find_prop<'a>(props: &'a [RjProp], prop_type: u8) -> Option<&'a RjProp> {
    props.iter().find(|p| p.prop_type == prop_type)
}

/// 按类型查找可变引用
pub fn find_prop_mut<'a>(props: &'a mut [RjProp], prop_type: u8) -> Option<&'a mut RjProp> {
    props.iter_mut().find(|p| p.prop_type == prop_type)
}

/// 将 props 列表序列化为字节（各字段带 0x1a 头）
pub fn props_to_buffer(props: &[RjProp]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in props {
        p.write_to(&mut out);
    }
    out
}

/// 该字段的 header2.len 是否可靠（用于解析）
fn length_field_ok(prop_type: u8, h2len: u8) -> bool {
    if prop_type == 0 || prop_type == 1 {
        return false;
    }
    h2len >= (HEADER2_LEN as u8 - 4) // >= 2
}

/// 在缓冲中查找下一个 magic 位置
fn find_next_magic(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == PROP_MAGIC)
}

/// 解析裸字段序列（每个字段以 header2.magic 开头；用于服务端报文）。
pub fn parse_bare(buf: &[u8]) -> Result<Vec<RjProp>> {
    let mut out = Vec::new();
    let mut read = 0usize;
    while read < buf.len() {
        if read + HEADER2_LEN > buf.len() {
            return Err(Error::Proto("字段数据不完整".into()));
        }
        let h2 = &buf[read..read + HEADER2_LEN];
        if h2[0..4] != PROP_MAGIC {
            return Err(Error::Proto("未发现 magic".into()));
        }
        let prop_type = h2[4];
        let h2len = h2[5];
        read += HEADER2_LEN;

        let content_len = if length_field_ok(prop_type, h2len) {
            (h2len as usize) - (HEADER2_LEN - 4) // h2len - 2
        } else {
            // 长度不可靠：搜索下一个 magic 界定
            match find_next_magic(&buf[read..]) {
                Some(pos) => pos,
                None => buf.len() - read,
            }
        };
        let content = buf[read..read + content_len].to_vec();
        read += content_len;
        out.push(RjProp::new(prop_type, content));
    }
    Ok(out)
}
