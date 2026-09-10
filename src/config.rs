//! 运行时配置与默认值。

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
    // 其它
    pub daemon: bool,
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
            daemon: false,
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
}
