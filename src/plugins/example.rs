//! 示例用户插件：简单流量统计。
//!
//! 这是给插件作者照抄的**最小可用模板**：实现 [`Plugin`]，登记到 `plugins/mod.rs`
//! 后即可用 `--plugin stats` 启用。统计结果可通过 [`PluginManager::get_typed`]
//! 在认证结束时读取（此处仅演示，不主动打印）。

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::eth_frame::Frame;
use crate::plugin::Plugin;

/// 统计出/入帧数量与字节数的插件。
pub struct TrafficStats {
    sent_frames: u64,
    sent_bytes: u64,
    recv_frames: u64,
    recv_bytes: u64,
}

impl Default for TrafficStats {
    fn default() -> Self {
        TrafficStats {
            sent_frames: 0,
            sent_bytes: 0,
            recv_frames: 0,
            recv_bytes: 0,
        }
    }
}

impl TrafficStats {
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.sent_frames,
            self.sent_bytes,
            self.recv_frames,
            self.recv_bytes,
        )
    }
}

impl Plugin for TrafficStats {
    fn name(&self) -> &str {
        "stats"
    }
    fn description(&self) -> &str {
        "统计认证期间收发报文数量与字节数（示例插件）"
    }
    fn version(&self) -> &str {
        "0.1"
    }

    fn prepare(&mut self, frame: &mut Frame) {
        self.sent_frames += 1;
        self.sent_bytes += frame.buf.len() as u64;
        // 注意：这里不要修改 frame，除非你真的要改报文
    }

    fn on_received(&mut self, frame: &Frame) {
        self.recv_frames += 1;
        self.recv_bytes += frame.buf.len() as u64;
    }

    fn reset(&mut self) {
        self.sent_frames = 0;
        self.sent_bytes = 0;
        self.recv_frames = 0;
        self.recv_bytes = 0;
    }
}

// 用原子静态演示“可选：跨会话共享的全局计数”，说明插件可持有静态状态。
// 注意用 AtomicUsize（而非 AtomicU64）：便于在 32 位目标（如 MT7621/MIPS）上编译。
static GLOBAL_SENT: AtomicUsize = AtomicUsize::new(0);
#[allow(dead_code)]
fn _touch_global() -> u64 {
    GLOBAL_SENT.fetch_add(1, Ordering::Relaxed) as u64
}
