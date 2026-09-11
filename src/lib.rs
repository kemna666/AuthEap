//! AuthEAP —— 可扩展 802.1x (EAP-MD5) 客户端（Rust 重写版）。
//!
//! 本工程是 [updateing/minieap](https://github.com/updateing/minieap)（C 语言）
//! 的功能等价格移植，采用 Rust 惯用写法重新组织代码结构。
//! 包含标准 EAP-MD5 认证核心、插件化报文修改器（锐捷 v3/v4 私有算法 `rjv3`、
//! 示例 `printer`），以及 raw socket / pcap 两种收发包后端。

pub mod cli;
pub mod config;
pub mod error;
pub mod eth_frame;
pub mod lockfile;
pub mod logging;
pub mod net;
pub mod nettest;
pub mod packet_builder;
pub mod packet_plugin;
pub mod plugin;
pub mod plugins;
pub mod runtime;
pub mod util;

// 简单日志宏（输出到 stderr）
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::logging::log_at($crate::logging::Level::Info, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::logging::log_at($crate::logging::Level::Warn, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::logging::log_at($crate::logging::Level::Error, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::logging::log_at($crate::logging::Level::Debug, format_args!($($arg)*)) };
}
// 无前缀/无换行的原始输出
#[macro_export]
macro_rules! log_raw {
    ($($arg:tt)*) => { $crate::logging::log_raw(format_args!($($arg)*)) };
}

/// 版本号（可被 CI 覆盖）。
///
/// 默认取 `Cargo.toml` 的 `version`；若编译时设置了环境变量
/// `AUTHEAP_BUILD_VERSION`（如 `scripts/ci-version.sh` 计算出的发布版本），
/// 则优先使用它——这样 CI 无需修改源码就能给二进制打上正确的版本号。
pub const VERSION: &str = match option_env!("AUTHEAP_BUILD_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
