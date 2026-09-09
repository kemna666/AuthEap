//! 报文修改插件（Packet Plugin）。
//!
//! 原版设计为可插拔的“报文修改器”：标准 EAP 报文在发送前流经 `prepare`，
//! 收到的报文先经过 `on_frame_received`。本工程将其组织为对具体插件的直接调用：
//! - `rjv3`：锐捷私有算法插件（见 [`rjv3`]）；
//! - `printer`：打印流经帧长度的示例插件（见 [`printer`]）。
//!
//! 认证核心通过 [`crate::runtime::App`] 决定启用哪些插件并按顺序调用。

pub mod printer;
pub mod rjv3;

/// 判断某帧是否是需要标准 EAP 处理的帧（EAP-Packet 且 code 已知）
pub fn eapol_type_of(f: &crate::eth_frame::Frame) -> crate::eth_frame::EapolType {
    use crate::eth_frame::EapolType;
    if f.buf.len() >= crate::eth_frame::EAPOL_TYPE + 1 {
        EapolType::from_u8(f.buf[crate::eth_frame::EAPOL_TYPE])
    } else {
        EapolType::EapPacket
    }
}
