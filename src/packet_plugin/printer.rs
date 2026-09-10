//! printer 示例插件：打印流经的帧长度（以 [`crate::plugin::Plugin`] 方式实现）。

use crate::eth_frame::{EapolType, Frame, EAPOL_TYPE};
use crate::log_info;
use crate::plugin::Plugin;

/// 打印流经帧的插件。
pub struct Printer {
    /// 是否把收到的帧也打印到日志
    log_recv: bool,
}

impl Printer {
    pub fn new(log_recv: bool) -> Printer {
        Printer { log_recv }
    }
}

impl Default for Printer {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Plugin for Printer {
    fn name(&self) -> &str {
        "printer"
    }
    fn description(&self) -> &str {
        "打印流经本插件的报文大小"
    }

    fn prepare(&mut self, frame: &mut Frame) {
        if frame.buf.len() >= EAPOL_TYPE + 1 {
            let t = EapolType::from_u8(frame.buf[EAPOL_TYPE]);
            log_info!(
                "[printer] 准备发送 {}（{} 字节）",
                t.as_str(),
                frame.buf.len()
            );
        } else {
            log_info!("[printer] 准备发送帧（{} 字节）", frame.buf.len());
        }
    }

    fn on_received(&mut self, frame: &Frame) {
        if !self.log_recv {
            return;
        }
        log_info!("[printer] 收到帧（{} 字节）", frame.buf.len());
    }
}
