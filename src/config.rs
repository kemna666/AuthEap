//! 运行时配置与默认值。
//!
//! 除了命令行参数，本模块还能从**配置文件**（`key = value` 文本，见
//! [`Config::load_file`]）读取认证参数；LuCI/init 会把 UCI 配置落成该文件，
//! 后端（`autheap --config <FILE>`）据此完成认证。

/// 用户名/密码最长长度（与原版一致）
pub const USERNAME_MAX_LEN: usize = 64;
pub const PASSWORD_MAX_LEN: usize = 64;

/// DHCP 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DhcpType {
    None = 0,
    DoubleAuth = 1,
    AfterAuth = 2,
    BeforeAuth = 3,
}

impl DhcpType {
    pub fn from_u8(v: u8) -> Option<DhcpType> {
        match v {
            0 => Some(DhcpType::None),
            1 => Some(DhcpType::DoubleAuth),
            2 => Some(DhcpType::AfterAuth),
            3 => Some(DhcpType::BeforeAuth),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            DhcpType::None => "不使用 DHCP",
            DhcpType::DoubleAuth => "二次认证",
            DhcpType::AfterAuth => "认证后 DHCP",
            DhcpType::BeforeAuth => "认证前 DHCP",
        }
    }

    /// 解析 DHCP 方式（命令行/配置文件共用）：none/double/after/before 或 0-3。
    pub fn parse(s: &str) -> Option<DhcpType> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "0" | "no" => Some(DhcpType::None),
            "double" | "doubleauth" | "1" => Some(DhcpType::DoubleAuth),
            "after" | "afterauth" | "2" => Some(DhcpType::AfterAuth),
            "before" | "beforeauth" | "3" => Some(DhcpType::BeforeAuth),
            _ => None,
        }
    }
}

/// EAPOL-Start 报文目的地址
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BcastAddr {
    Standard = 0,
    Rj = 1,
    Cer = 2,
}

impl BcastAddr {
    pub fn from_u8(v: u8) -> BcastAddr {
        match v % 2 {
            0 => BcastAddr::Standard,
            _ => BcastAddr::Rj,
        }
    }
}

/// 收发包后端
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Linux 原生 AF_PACKET 原始套接字
    Raw,
    /// libpcap
    Pcap,
}

/// 一次命令行追加/修改的锐捷私有字段（--rj-option 的解析结果）
#[derive(Debug, Clone)]
pub struct RjOption {
    pub prop_type: u8,
    pub content: Vec<u8>,
    /// true 表示替换内置生成的同类型字段（:r）
    pub replace: bool,
}

/// 认证凭据与流程参数
#[derive(Debug, Clone)]
pub struct Config {
    // 凭据
    pub username: String,
    pub password: String,
    // 网络
    pub ifname: String,
    pub backend: Backend,
    // 通用流程
    pub max_retries: usize,
    pub max_failures: usize,
    pub stage_timeout: u64,
    pub auth_round: usize,
    pub restart_on_logoff: bool,
    pub wait_after_fail_secs: u64,
    // 插件
    pub plugins: Vec<String>,
    /// printer 插件：是否把“收到的帧”也打印到日志（默认否，避免刷屏）
    pub printer_log_recv: bool,
    // rjv3 选项
    /// 心跳包间隔（秒）。默认 20；0 表示禁用心跳。
    pub heartbeat_interval: u64,
    pub eap_bcast_addr: BcastAddr,
    pub dhcp_type: DhcpType,
    pub max_dhcp_count: usize,
    pub service_name: String,
    pub ver_str: String,
    pub dhcp_script: String,
    pub fake_dns1: Option<String>,
    pub fake_dns2: Option<String>,
    pub fake_serial: Option<String>,
    pub rj_options: Vec<RjOption>,
    // 日志
    /// 日志文件（追加写入，立即 flush）；None = 只写 stderr
    pub log_file: Option<String>,
    /// 单个日志文件大小上限（KB）；超过则自动轮转清理
    pub log_max_kb: u64,
    /// 保留的历史日志份数（.1/.2/…），0 表示超限直接清空
    pub log_keep: usize,
    /// 日志检查/自动清理间隔（秒）
    pub log_rotate_secs: u64,
    // beacon 插件（周期性向指定 IP:端口 发送自定义 TCP 数据包）
    /// 目标 IP / 主机名（None 或空表示不启用）
    pub beacon_dst: Option<String>,
    /// 目标端口（默认 8083）
    pub beacon_port: u16,
    /// 发送间隔（秒；0 表示禁用）
    pub beacon_interval: u64,
    /// 载荷字节（None = 使用插件内置的抓包示例载荷）
    pub beacon_payload: Option<Vec<u8>>,
    // 其它
    pub daemon: bool,
    /// 单实例锁文件（认证进程只允许一个实例；None = 不做单实例限制）
    pub pid_file: Option<String>,
    pub verbose: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            username: String::new(),
            password: String::new(),
            ifname: String::new(),
            backend: Backend::Raw,
            max_retries: 3,
            max_failures: 3,
            stage_timeout: 5,
            auth_round: 1,
            restart_on_logoff: true,
            wait_after_fail_secs: 30,
            plugins: Vec::new(),
            printer_log_recv: false,
            heartbeat_interval: 20, // 默认心跳间隔（秒），0 表示禁用
            eap_bcast_addr: BcastAddr::Standard,
            dhcp_type: DhcpType::AfterAuth,
            max_dhcp_count: 3,
            service_name: "internet".to_string(),
            ver_str: "RG-SU For Linux V1.0".to_string(),
            dhcp_script: String::new(),
            fake_dns1: None,
            fake_dns2: None,
            fake_serial: None,
            rj_options: Vec::new(),
            log_file: None,
            log_max_kb: 256,
            log_keep: 3,
            log_rotate_secs: 300,
            beacon_dst: None,
            beacon_port: 8083,
            beacon_interval: 60,
            beacon_payload: None,
            daemon: false,
            pid_file: Some(crate::lockfile::DEFAULT_PID_FILE.to_string()),
            verbose: 0,
        }
    }
}

impl Config {
    /// 校验必备字段是否就绪
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        use crate::error::Error;
        if self.username.is_empty() {
            return Err(Error::Config("用户名不能为空，请用 --username 指定".into()));
        }
        if self.password.is_empty() {
            return Err(Error::Config("密码不能为空，请用 --password 指定".into()));
        }
        if self.ifname.is_empty() {
            return Err(Error::Config("网卡名不能为空，请用 --interface 指定".into()));
        }
        if self.username.len() > USERNAME_MAX_LEN || self.password.len() > PASSWORD_MAX_LEN {
            return Err(Error::Config("用户名或密码过长".into()));
        }
        if self.heartbeat_interval == 0 {
            // 允许关闭心跳
        }
        Ok(())
    }

    /// 从配置文件加载（`key = value` 文本格式）。
    ///
    /// 该文件由 LuCI/init 从 UCI 生成（`/etc/autheap-client.conf`），也可以手工编写：
    /// 每行 `键 = 值`，`#` 或 `;` 开头为注释，值可用单/双引号包裹；键名兼容
    /// `snake_case` 与 `kebab-case`（如 `interface` / `ifname` 等价）。
    pub fn load_file(path: &str) -> Result<Config, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("无法读取配置文件 {path}: {e}"))?;
        let mut cfg = Config::default();
        for (idx, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            // 去掉行尾注释（仅当 # 前有空白时，避免误伤密码里的 #）
            let line = match line.find(" #") {
                Some(p) => line[..p].trim_end(),
                None => line,
            };
            let (k, v) = match line.split_once('=') {
                Some((k, v)) => (k.trim(), unquote(v.trim())),
                None => (line, String::new()),
            };
            if k.is_empty() {
                continue;
            }
            cfg.apply_kv(k, &v)
                .map_err(|e| format!("{path}: 第 {} 行: {e}", idx + 1))?;
        }
        Ok(cfg)
    }

    /// 应用一条 `key = value` 配置（键名大小写/连字符不敏感）。
    pub fn apply_kv(&mut self, key: &str, value: &str) -> Result<(), String> {
        let k = key.trim().to_ascii_lowercase().replace('-', "_");
        let v = value.trim();
        let bad = || format!("值无法解析: {k} = {v}");
        match k.as_str() {
            // 凭据与网卡
            "username" | "user" | "login" => self.username = v.to_string(),
            "password" | "pass" | "pwd" => self.password = v.to_string(),
            "interface" | "ifname" | "iface" | "device" => self.ifname = v.to_string(),
            "backend" => {
                self.backend = match v.to_ascii_lowercase().as_str() {
                    "raw" | "socket" | "af_packet" => Backend::Raw,
                    "pcap" => Backend::Pcap,
                    _ => return Err(bad()),
                }
            }
            // 插件
            "plugins" | "plugin" | "modules" => {
                self.plugins = v
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
            }
            "extra_plugins" => {
                for p in v.split(|c: char| c.is_whitespace() || c == ',') {
                    if !p.is_empty() {
                        self.plugins.push(p.to_string());
                    }
                }
            }
            "printer_log_recv" => self.printer_log_recv = parse_bool(v).ok_or_else(bad)?,
            // 流程
            "heartbeat" | "heartbeat_interval" => {
                self.heartbeat_interval = parse_u64(v).ok_or_else(bad)?
            }
            "eap_bcast_addr" | "bcast_addr" => {
                self.eap_bcast_addr = BcastAddr::from_u8(parse_u64(v).ok_or_else(bad)? as u8)
            }
            "dhcp_type" => self.dhcp_type = DhcpType::parse(v).ok_or_else(bad)?,
            "dhcp_script" => self.dhcp_script = v.to_string(),
            "service" | "service_name" => self.service_name = v.to_string(),
            "version_str" | "version" => self.ver_str = v.to_string(),
            "fake_dns1" | "dns1" => self.fake_dns1 = opt_string(v),
            "fake_dns2" | "dns2" => self.fake_dns2 = opt_string(v),
            "fake_serial" | "serial" => self.fake_serial = opt_string(v),
            "max_dhcp_count" => self.max_dhcp_count = parse_usize(v).ok_or_else(bad)?,
            "max_retries" => self.max_retries = parse_usize(v).ok_or_else(bad)?,
            "max_failures" => self.max_failures = parse_usize(v).ok_or_else(bad)?,
            "stage_timeout" => self.stage_timeout = parse_u64(v).ok_or_else(bad)?,
            "wait_after_fail" | "wait_after_fail_secs" => {
                self.wait_after_fail_secs = parse_u64(v).ok_or_else(bad)?
            }
            "restart_on_logoff" => self.restart_on_logoff = parse_bool(v).ok_or_else(bad)?,
            "daemon" | "background" => self.daemon = parse_bool(v).ok_or_else(bad)?,
            // 单实例锁文件；显式留空 = 不做单实例限制
            "pid_file" | "pidfile" | "pid" | "lock_file" | "lockfile" => {
                self.pid_file = opt_string(v)
            }
            "verbose" | "debug" => self.verbose = parse_u64(v).ok_or_else(bad)? as u8,
            "rj_option" | "rj_options" => {
                for one in v.split(|c: char| c.is_whitespace() || c == ',') {
                    if !one.is_empty() {
                        self.rj_options.push(parse_rj_option(one)?);
                    }
                }
            }
            // 日志
            "log_file" | "logfile" | "log" => self.log_file = opt_string(v),
            "log_max_kb" | "log_max_kbytes" | "log_size" => {
                self.log_max_kb = parse_u64(v).ok_or_else(bad)?
            }
            "log_keep" | "log_history" | "log_backups" => {
                self.log_keep = parse_usize(v).ok_or_else(bad)?
            }
            "log_rotate_secs" | "log_interval" | "log_check_secs" => {
                self.log_rotate_secs = parse_u64(v).ok_or_else(bad)?
            }
            // beacon 插件
            "beacon_dst" | "beacon_target" | "beacon_host" | "beacon_ip" => {
                self.beacon_dst = opt_string(v)
            }
            "beacon_port" => self.beacon_port = parse_u64(v).ok_or_else(bad)? as u16,
            "beacon_interval" | "beacon_secs" => {
                self.beacon_interval = parse_u64(v).ok_or_else(bad)?
            }
            "beacon_payload" => {
                if v.is_empty() {
                    self.beacon_payload = None;
                } else {
                    let bytes = crate::util::hex_str_to_bytes(v)
                        .ok_or_else(|| format!("beacon_payload 不是合法十六进制: {v}"))?;
                    self.beacon_payload = Some(bytes);
                }
            }
            _ => return Err(format!("未知配置项: {k}")),
        }
        Ok(())
    }
}

/// 去掉值两端成对的引号。
fn unquote(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn opt_string(v: &str) -> Option<String> {
    if v.is_empty() || v == "-" {
        None
    } else {
        Some(v.to_string())
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enable" | "enabled" => Some(true),
        "0" | "false" | "no" | "off" | "disable" | "disabled" | "" => Some(false),
        _ => None,
    }
}

fn parse_u64(v: &str) -> Option<u64> {
    v.trim().parse::<u64>().ok()
}

fn parse_usize(v: &str) -> Option<usize> {
    v.trim().parse::<usize>().ok()
}

/// 解析自定义认证字段：`type:hex[:r]`（r 表示替换内置同类型字段）。
pub fn parse_rj_option(s: &str) -> Result<RjOption, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!("rj-option 格式应为 type:hex[:r]，实际: {s}"));
    }
    let t = u8::from_str_radix(parts[0], 16)
        .map_err(|_| format!("rj-option 的 type 不是十六进制: {}", parts[0]))?;
    let content = crate::util::hex_str_to_bytes(parts[1])
        .ok_or_else(|| format!("rj-option 的 value 不是合法十六进制: {}", parts[1]))?;
    let replace = parts.len() == 3 && parts[2] == "r";
    if parts.len() == 3 && !replace {
        return Err(format!("rj-option 第三段只能是 r: {s}"));
    }
    Ok(RjOption {
        prop_type: t,
        content,
        replace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_file_parses_all_key_styles() {
        let path = std::env::temp_dir().join(format!("autheap-cfg-{}.conf", std::process::id()));
        std::fs::write(
            &path,
            "# 注释\n\
             interface = eth0\n\
             username = testuser\n\
             password = 'p@ss#word'\n\
             backend = raw\n\
             plugins = rjv3 printer\n\
             extra_plugins = beacon\n\
             heartbeat = 30\n\
             dhcp-type = double\n\
             log-file = /tmp/autheap.log  # 行尾注释\n\
             beacon-dst = 203.0.113.10\n\
             beacon_port = 8083\n\
             beacon_interval = 15\n\
             beacon_payload = 00112233aabb\n\
             restart_on_logoff = yes\n",
        )
        .unwrap();

        let cfg = Config::load_file(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.ifname, "eth0");
        assert_eq!(cfg.username, "testuser");
        assert_eq!(cfg.password, "p@ss#word");
        assert_eq!(cfg.backend, Backend::Raw);
        assert_eq!(cfg.plugins, vec!["rjv3", "printer", "beacon"]);
        assert_eq!(cfg.heartbeat_interval, 30);
        assert_eq!(cfg.dhcp_type, DhcpType::DoubleAuth);
        assert_eq!(cfg.log_file.as_deref(), Some("/tmp/autheap.log"));
        assert_eq!(cfg.beacon_dst.as_deref(), Some("203.0.113.10"));
        assert_eq!(cfg.beacon_port, 8083);
        assert_eq!(cfg.beacon_interval, 15);
        assert_eq!(
            cfg.beacon_payload.as_deref(),
            Some(&[0x00, 0x11, 0x22, 0x33, 0xaa, 0xbb][..])
        );
        assert!(cfg.restart_on_logoff);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_key_reports_line_number() {
        let err = Config::default().apply_kv("no_such_key", "1").unwrap_err();
        assert!(err.contains("未知配置项"), "{err}");
        assert_eq!(
            parse_rj_option("6f:0102:r").unwrap().prop_type,
            0x6f
        );
    }
}
