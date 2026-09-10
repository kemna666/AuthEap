//! 命令行接口（使用 clap，Rust 风格重新设计）。

use clap::{ArgAction, Parser};

use crate::config::{Backend, BcastAddr, Config, DhcpType, RjOption};
use crate::util::hex_str_to_bytes;

#[derive(Parser, Debug)]
#[command(
    name = "autheap",
    version,
    about = "可扩展的 802.1x (EAP-MD5) 客户端（带锐捷 v3/v4 插件）—— Rust 版",
    after_help = "示例:\n  autheap -i eth0 -u 学号 -p 密码 --plugin rjv3\n  autheap -i en0 -u user -p pass --plugin rjv3 --dhcp-type double --dhcp-script 'dhclient en0'\n\n启用 pcap 后端需在编译时加 --features backend-pcap 并安装 libpcap。"
)]
pub struct Cli {
    /// 网络接口
    #[arg(short = 'i', long = "interface", value_name = "IFNAME")]
    pub ifname: Option<String>,

    /// 用户名
    #[arg(short = 'u', long = "username", value_name = "NAME")]
    pub username: Option<String>,

    /// 密码（也可通过环境变量 AUTHEAP_PASSWORD 提供）
    #[arg(short = 'p', long = "password", value_name = "PASS")]
    pub password: Option<String>,

    /// 收发包后端：raw（Linux 原生 AF_PACKET）或 pcap
    #[arg(short = 'b', long = "backend", value_enum, default_value = "raw")]
    pub backend: BackendArg,

    /// 混杂模式
    #[arg(long = "promisc")]
    pub promisc: bool,

    /// 启用的插件（可多次指定；如 rjv3 / printer）
    #[arg(short = 'm', long = "plugin", alias = "module", value_name = "NAME", action = ArgAction::Append)]
    pub plugins: Vec<String>,

    /// 列出本程序已编译进的所有插件（名字与描述）后退出，不做任何认证
    #[arg(long = "list-plugins")]
    pub list_plugins: bool,

    /// printer 插件：把收到的帧也打印到日志
    #[arg(long = "printer-log-recv")]
    pub printer_log_recv: bool,

    /// 心跳间隔（秒，0 关闭；默认 20）
    #[arg(short = 'e', long = "heartbeat", value_name = "SECS")]
    pub heartbeat: Option<u64>,

    /// EAPOL-Start 广播地址：0=标准 1=锐捷
    #[arg(short = 'a', long = "eap-bcast-addr", value_name = "0|1")]
    pub eap_bcast_addr: Option<u8>,

    /// DHCP 方式：none/double/after/before（或 0-3）
    #[arg(short = 'd', long = "dhcp-type", value_name = "TYPE")]
    pub dhcp_type: Option<String>,

    /// DHCP 脚本（二次认证/认证后执行）
    #[arg(long = "dhcp-script", value_name = "CMD")]
    pub dhcp_script: Option<String>,

    /// 自定义认证字段 type:hex[:r]，r 表示替换内置同类型字段
    #[arg(long = "rj-option", value_name = "TYPE:HEX[:r]", action = ArgAction::Append)]
    pub rj_options: Vec<String>,

    /// 自定义服务名
    #[arg(long = "service", value_name = "NAME")]
    pub service: Option<String>,

    /// 自定义版本字符串
    #[arg(long = "version-str", value_name = "STR")]
    pub version_str: Option<String>,

    /// 主 DNS（IPv4 点分）
    #[arg(long = "fake-dns1", value_name = "IP")]
    pub fake_dns1: Option<String>,

    /// 次 DNS（IPv4/IPv6）
    #[arg(long = "fake-dns2", value_name = "IP")]
    pub fake_dns2: Option<String>,

    /// 硬盘序列号
    #[arg(long = "fake-serial", value_name = "STR")]
    pub fake_serial: Option<String>,

    /// 二次认证等待 DHCP 允许的超时次数
    #[arg(long = "max-dhcp-count", value_name = "NUM")]
    pub max_dhcp_count: Option<usize>,

    /// 每阶段最大重试次数
    #[arg(long = "max-retries", value_name = "NUM")]
    pub max_retries: Option<usize>,

    /// 最大连续失败次数
    #[arg(long = "max-failures", value_name = "NUM")]
    pub max_failures: Option<usize>,

    /// 每阶段等待服务器响应超时（秒）
    #[arg(long = "stage-timeout", value_name = "SECS")]
    pub stage_timeout: Option<u64>,

    /// 掉线后自动重连
    #[arg(long = "restart-on-logoff", action = ArgAction::SetTrue)]
    pub restart_on_logoff: bool,

    /// 掉线后不重连
    #[arg(long = "no-restart-on-logoff", action = ArgAction::SetTrue)]
    pub no_restart_on_logoff: bool,

    /// 失败后等待重试秒数
    #[arg(long = "wait-after-fail", value_name = "SECS")]
    pub wait_after_fail: Option<u64>,

    /// 转为后台守护进程
    #[arg(long = "daemon")]
    pub daemon: bool,

    /// 日志级别（可重复，越多越详细）
    #[arg(short = 'v', action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendArg {
    Raw,
    Pcap,
}

fn parse_dhcp_type(s: &str) -> Option<DhcpType> {
    match s.to_ascii_lowercase().as_str() {
        "none" | "0" => Some(DhcpType::None),
        "double" | "doubleauth" | "1" => Some(DhcpType::DoubleAuth),
        "after" | "afterauth" | "2" => Some(DhcpType::AfterAuth),
        "before" | "beforeauth" | "3" => Some(DhcpType::BeforeAuth),
        _ => None,
    }
}

impl Cli {
    pub fn parse_args() -> Cli {
        Cli::parse()
    }

    /// 转换为运行配置
    pub fn into_config(self) -> Result<Config, String> {
        let mut cfg = Config::default();

        if let Some(v) = self.ifname {
            cfg.ifname = v;
        }
        if let Some(v) = self.username {
            cfg.username = v;
        }
        if let Some(v) = self.password {
            cfg.password = v;
        } else if let Ok(p) = std::env::var("AUTHEAP_PASSWORD") {
            cfg.password = p;
        } else if let Ok(p) = std::env::var("MINIEAP_PASSWORD") {
            // 兼容旧环境变量名
            cfg.password = p;
        }
        cfg.backend = match self.backend {
            BackendArg::Raw => Backend::Raw,
            BackendArg::Pcap => Backend::Pcap,
        };
        cfg.plugins = self.plugins;
        if let Some(v) = self.heartbeat {
            cfg.heartbeat_interval = v;
        }
        if let Some(v) = self.eap_bcast_addr {
            cfg.eap_bcast_addr = BcastAddr::from_u8(v);
        }
        if let Some(v) = self.dhcp_type {
            cfg.dhcp_type = parse_dhcp_type(&v).ok_or_else(|| {
                format!("无法识别的 --dhcp-type: {v}（可用 none/double/after/before 或 0-3）")
            })?;
        }
        if let Some(v) = self.dhcp_script {
            cfg.dhcp_script = v;
        }
        for opt in self.rj_options {
            cfg.rj_options.push(parse_rj_option(&opt)?);
        }
        if let Some(v) = self.service {
            cfg.service_name = v;
        }
        if let Some(v) = self.version_str {
            cfg.ver_str = v;
        }
        cfg.fake_dns1 = self.fake_dns1;
        cfg.fake_dns2 = self.fake_dns2;
        cfg.fake_serial = self.fake_serial;
        if let Some(v) = self.max_dhcp_count {
            cfg.max_dhcp_count = v;
        }
        if let Some(v) = self.max_retries {
            cfg.max_retries = v;
        }
        if let Some(v) = self.max_failures {
            cfg.max_failures = v;
        }
        if let Some(v) = self.stage_timeout {
            cfg.stage_timeout = v;
        }
        if self.no_restart_on_logoff {
            cfg.restart_on_logoff = false;
        } else if self.restart_on_logoff {
            cfg.restart_on_logoff = true;
        }
        if let Some(v) = self.wait_after_fail {
            cfg.wait_after_fail_secs = v;
        }
        cfg.daemon = self.daemon;
        cfg.printer_log_recv = self.printer_log_recv;
        cfg.verbose = self.verbose;

        // 若没有显式指定插件但指定了相关锐捷参数，不自动启用（避免意外）
        if cfg.username.is_empty()
            && cfg.password.is_empty()
            && cfg.ifname.is_empty()
            && cfg.plugins.is_empty()
            && cfg.rj_options.is_empty()
        {
            return Err("未提供任何认证参数（-u/-p/-i）。请用 --help 查看用法。".into());
        }
        Ok(cfg)
    }
}

/// 解析 --rj-option：type:hex[:r]
fn parse_rj_option(s: &str) -> Result<RjOption, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!("--rj-option 格式应为 type:hex[:r]，实际: {s}"));
    }
    let t = u8::from_str_radix(parts[0], 16)
        .map_err(|_| format!("--rj-option 的 type 不是十六进制: {}", parts[0]))?;
    let content = hex_str_to_bytes(parts[1])
        .ok_or_else(|| format!("--rj-option 的 value 不是合法十六进制: {}", parts[1]))?;
    let replace = parts.len() == 3 && parts[2] == "r";
    if parts.len() == 3 && !replace {
        return Err(format!("--rj-option 第三段只能是 r: {s}"));
    }
    Ok(RjOption {
        prop_type: t,
        content,
        replace,
    })
}
