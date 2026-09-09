//! `report` —— 如实上报本机真实信息的独立插件。
//!
//! 定位：只读取并打印设备**当前真实**的网络信息（网卡 / MAC / IPv4 / 网关 / DNS /
//! 主机名）以及本次会话的收发计数，用于入网前向使用者/网管如实核对环境。
//!
//! 边界（重要）：本插件**绝不修改认证报文、绝不伪造任何“已安装某软件 / EDR /
//! 合规”等状态字段**——所有输出都来自系统实际值（`ifinfo`/`/proc`），读不到就如实
//! 显示“（无）”。如需自定义字段，请在确有把握的前提下另写/参考其它插件。

use std::net::IpAddr;

use crate::config::Config;
use crate::eth_frame::Frame;
use crate::log_info;
use crate::net::ifinfo;
use crate::plugin::{AuthEvent, Plugin, SessionHost};

/// 如实上报插件：统计 + 会话开始时打印一次本机真实信息。
pub struct Report {
    ifname: String,
    mac: [u8; 6],
    sent: u64,
    recv: u64,
    /// 本会话是否已打印过报告（避免对 keepalive 等重复打印）
    reported: bool,
}

impl Report {
    pub fn new(cfg: &Config, mac: [u8; 6]) -> Report {
        Report {
            ifname: cfg.ifname.clone(),
            mac,
            sent: 0,
            recv: 0,
            reported: false,
        }
    }

    fn mac_str(&self) -> String {
        self.mac
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    /// 当前真实 IPv4（读不到则如实显示“（无）”）
    fn real_ipv4(&self) -> String {
        match ifinfo::first_ip(&self.ifname, true) {
            Ok(IpAddr::V4(a)) => a.to_string(),
            Ok(_) => "(非 IPv4)".to_string(),
            Err(_) => "（无）".to_string(),
        }
    }

    /// 当前真实默认网关（读不到则如实显示“（无）”）
    fn real_gateway(&self) -> String {
        match ifinfo::default_gateway_v4(&self.ifname) {
            Some(g) => g.to_string(),
            None => "（无）".to_string(),
        }
    }

    /// 当前真实 DNS 列表（读不到则如实显示“（无）”）
    fn real_dns(&self) -> String {
        match ifinfo::dns_list() {
            Ok(v) if !v.is_empty() => v.join(", "),
            _ => "（无）".to_string(),
        }
    }

    /// 主机名（/proc/sys/kernel/hostname，读不到显示 unknown）
    fn hostname(&self) -> String {
        std::fs::read_to_string("/proc/sys/kernel/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    /// 如实打印一次本机当前真实信息
    fn print_report(&self) {
        log_info!("[report] ==== 如实上报本机真实信息（会话开始） ====");
        log_info!("[report]   主机名: {}", self.hostname());
        log_info!("[report]   接口  : {}", self.ifname);
        log_info!("[report]   MAC   : {}", self.mac_str());
        log_info!("[report]   IPv4  : {}", self.real_ipv4());
        log_info!("[report]   网关  : {}", self.real_gateway());
        log_info!("[report]   DNS   : {}", self.real_dns());
        log_info!("[report] ==== （以上均为系统当前实际值，未伪造任何状态） ====");
    }
}

impl Plugin for Report {
    fn name(&self) -> &str {
        "report"
    }
    fn description(&self) -> &str {
        "如实上报本机真实信息（网卡/MAC/IP/网关/DNS/主机名）与收发计数；不改帧、不伪造状态"
    }
    fn version(&self) -> &str {
        "0.1"
    }

    fn prepare(&mut self, _frame: &mut Frame) {
        self.sent += 1;
        if !self.reported {
            self.reported = true;
            self.print_report();
        }
    }

    fn on_received(&mut self, _frame: &Frame) {
        self.recv += 1;
    }

    fn on_auth_event(&mut self, ev: &AuthEvent, _host: &mut dyn SessionHost) {
        match ev {
            AuthEvent::Success(_) => {
                log_info!(
                    "[report] 认证成功：本次共发送 {sent} 帧 / 接收 {recv} 帧",
                    sent = self.sent,
                    recv = self.recv
                );
            }
            AuthEvent::Failure(_) => {
                log_info!(
                    "[report] 认证失败：本次共发送 {sent} 帧 / 接收 {recv} 帧",
                    sent = self.sent,
                    recv = self.recv
                );
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.sent = 0;
        self.recv = 0;
        self.reported = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_lifecycle_counts_and_no_panic() {
        let cfg = Config::default();
        let mut p = Report::new(&cfg, [0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
        assert_eq!(p.name(), "report");
        assert!(!p.description().is_empty());
        assert_eq!(p.mac_str(), "02:00:00:00:00:01");

        p.on_received(&Frame::new_zeroed(64));
        // 首帧 prepare 会触发一次如实打印，不应崩溃
        p.prepare(&mut Frame::new_zeroed(64));
        p.prepare(&mut Frame::new_zeroed(64));
        assert_eq!(p.sent, 2);
        assert_eq!(p.recv, 1);
        assert!(p.reported);

        p.reset();
        assert_eq!(p.sent, 0);
        assert_eq!(p.recv, 0);
        assert!(!p.reported);
    }
}
