//! 命令行接口（使用 clap，Rust 风格重新设计）。

use clap::{ArgAction, Parser};

use crate::config::{parse_rj_option, Backend, BcastAddr, Config, DhcpType};
use crate::util::hex_str_to_bytes;

#[derive(Parser, Debug)]
#[command(
    name = "autheap",
    version = crate::VERSION,
    about = "可扩展的 802.1x (EAP-MD5) 客户端（带锐捷 v3/v4 插件）—— Rust 版",
    after_help = "示例:\n  autheap -i eth0 -u 学号 -p 密码 --plugin rjv3\n  autheap -i en0 -u user -p pass --plugin rjv3 --dhcp-type double --dhcp-script 'dhclient en0'\n  autheap --config /etc/autheap-client.conf --log-file /var/log/autheap-client.log\n  autheap -i eth0 -u u -p p --plugin rjv3 --plugin beacon \\\n          --beacon-dst 203.0.113.10 --beacon-port 8083 --beacon-interval 60 \\\n          --beacon-payload 00112233deadbeef\n\n启用 pcap 后端需在编译时加 --features backend-pcap 并安装 libpcap。"
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
    #[arg(short = 'b', long = "backend", value_enum)]
    pub backend: Option<BackendArg>,

    /// 从配置文件读取认证参数（key = value；命令行显式参数优先）
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: Option<String>,

    /// 认证状态文件（LuCI 状态页读取；多线模式下每线不同）
    #[arg(long = "state-file", value_name = "FILE")]
    pub state_file: Option<String>,

    /// 把日志追加写入该文件（立即 flush；守护模式下也不会丢日志）
    #[arg(long = "log-file", value_name = "FILE")]
    pub log_file: Option<String>,

    /// 单个日志文件大小上限（KB，默认 256）；超过则自动轮转清理
    #[arg(long = "log-max-kb", value_name = "KB")]
    pub log_max_kb: Option<u64>,

    /// 保留的历史日志份数（默认 3；0 表示超限直接清空）
    #[arg(long = "log-keep", value_name = "N")]
    pub log_keep: Option<usize>,

    /// 日志检查/自动清理间隔（秒，默认 300）
    #[arg(long = "log-rotate-secs", value_name = "SECS")]
    pub log_rotate_secs: Option<u64>,

    /// 混杂模式
    #[arg(long = "promisc")]
    pub promisc: bool,

    /// 启用的插件（可多次指定；如 rjv3 / printer）
    #[arg(short = 'm', long = "plugin", alias = "module", value_name = "NAME", action = ArgAction::Append)]
    pub plugins: Vec<String>,

    /// 列出本程序已编译进的所有插件（名字与描述）后退出，不做任何认证
    #[arg(long = "list-plugins")]
    pub list_plugins: bool,

    /// 网络测试：检查本机网络/网关/目标连通性后退出（不需要账号；可写 HOST 或 HOST:端口，留空只测本机与默认网关）
    #[arg(long = "nettest", value_name = "HOST[:PORT]", num_args = 0..=1, default_missing_value = "")]
    pub nettest: Option<String>,

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

    /// beacon 插件：目标 IP / 主机名（必填；在 LuCI 里填目标 IP 即启用）
    #[arg(long = "beacon-dst", value_name = "IP")]
    pub beacon_dst: Option<String>,

    /// beacon 插件：目标端口（默认 8083）
    #[arg(long = "beacon-port", value_name = "PORT")]
    pub beacon_port: Option<u16>,

    /// beacon 插件：发送间隔秒数（默认 60；0 禁用）
    #[arg(long = "beacon-interval", value_name = "SECS")]
    pub beacon_interval: Option<u64>,

    /// beacon 插件：载荷十六进制（默认使用内置抓包示例载荷）
    #[arg(long = "beacon-payload", value_name = "HEX")]
    pub beacon_payload: Option<String>,

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

    /// 单实例锁文件（认证进程只允许一个实例在跑；传空字符串表示不做限制）
    #[arg(long = "pid-file", value_name = "PATH", default_missing_value = "")]
    pub pid_file: Option<String>,

    /// 日志级别（可重复，越多越详细）
    #[arg(short = 'v', action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendArg {
    Raw,
    Pcap,
}

impl Cli {
    pub fn parse_args() -> Cli {
        Cli::parse()
    }

    /// 转换为运行配置。
    ///
    /// 优先级：命令行显式参数 > 配置文件 > 环境变量 > 内置默认值。
    pub fn into_config(self) -> Result<Config, String> {
        // 先读配置文件（若有），后面再用命令行显式参数覆盖
        let mut cfg = match self.config.as_deref() {
            Some(p) if !p.trim().is_empty() => Config::load_file(p)?,
            _ => Config::default(),
        };

        if let Some(v) = self.ifname {
            cfg.ifname = v;
        }
        if let Some(v) = self.username {
            cfg.username = v;
        }
        if let Some(v) = self.password {
            cfg.password = v;
        } else if cfg.password.is_empty() {
            // 配置文件没有密码时，才回退到环境变量（兼容旧用法）
            if let Ok(p) = std::env::var("AUTHEAP_PASSWORD") {
                cfg.password = p;
            } else if let Ok(p) = std::env::var("MINIEAP_PASSWORD") {
                cfg.password = p;
            }
        }
        if let Some(b) = self.backend {
            cfg.backend = match b {
                BackendArg::Raw => Backend::Raw,
                BackendArg::Pcap => Backend::Pcap,
            };
        }
        if let Some(v) = self.state_file {
            if !v.trim().is_empty() {
                cfg.state_file = v.trim().to_string();
            }
        }
        if let Some(v) = self.log_file {
            cfg.log_file = if v.trim().is_empty() { None } else { Some(v) };
        }
        if let Some(v) = self.log_max_kb {
            cfg.log_max_kb = v;
        }
        if let Some(v) = self.log_keep {
            cfg.log_keep = v;
        }
        if let Some(v) = self.log_rotate_secs {
            cfg.log_rotate_secs = v;
        }
        if !self.plugins.is_empty() {
            cfg.plugins = self.plugins;
        }
        if let Some(v) = self.heartbeat {
            cfg.heartbeat_interval = v;
        }
        if let Some(v) = self.eap_bcast_addr {
            cfg.eap_bcast_addr = BcastAddr::from_u8(v);
        }
        if let Some(v) = self.dhcp_type {
            cfg.dhcp_type = DhcpType::parse(&v).ok_or_else(|| {
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
        if self.fake_dns1.is_some() {
            cfg.fake_dns1 = self.fake_dns1;
        }
        if self.fake_dns2.is_some() {
            cfg.fake_dns2 = self.fake_dns2;
        }
        if self.fake_serial.is_some() {
            cfg.fake_serial = self.fake_serial;
        }
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
        if let Some(v) = self.beacon_dst {
            cfg.beacon_dst = if v.trim().is_empty() { None } else { Some(v) };
        }
        if let Some(v) = self.beacon_port {
            cfg.beacon_port = v;
        }
        if let Some(v) = self.beacon_interval {
            cfg.beacon_interval = v;
        }
        if let Some(v) = self.beacon_payload {
            if v.trim().is_empty() {
                cfg.beacon_payload = None;
            } else {
                cfg.beacon_payload = Some(
                    hex_str_to_bytes(&v).ok_or_else(|| format!("--beacon-payload 不是合法十六进制: {v}"))?,
                );
            }
        }
        if self.daemon {
            cfg.daemon = true;
        }
        // 单实例锁文件：显式传空字符串表示不做单实例限制
        if let Some(p) = self.pid_file.as_deref() {
            cfg.pid_file = if p.trim().is_empty() {
                None
            } else {
                Some(p.trim().to_string())
            };
        }
        if self.printer_log_recv {
            cfg.printer_log_recv = true;
        }
        if self.verbose > 0 {
            cfg.verbose = self.verbose;
        }

        // 若既没有命令行参数也没有配置文件，则提示用法（真正的字段校验交给 validate）
        if self.config.is_none()
            && cfg.username.is_empty()
            && cfg.password.is_empty()
            && cfg.ifname.is_empty()
            && cfg.plugins.is_empty()
            && cfg.rj_options.is_empty()
        {
            return Err("未提供任何认证参数（-u/-p/-i）或配置文件（--config）。请用 --help 查看用法。".into());
        }
        Ok(cfg)
    }
}
