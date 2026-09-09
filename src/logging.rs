//! 极简日志系统。所有日志输出到 stderr，带时间戳；级别由命令行 `-v` 控制。
//! 原项目大量日志为中文，这里沿用其风格。

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

pub fn set_level(l: Level) {
    LEVEL.store(l as u8, Ordering::Relaxed);
}

pub fn level() -> Level {
    match LEVEL.load(Ordering::Relaxed) {
        0 => Level::Error,
        1 => Level::Warn,
        2 => Level::Info,
        _ => Level::Debug,
    }
}

fn timestamp() -> String {
    let now = SystemTime::now();
    // 尽量给出可读时间
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (h, m, s) = {
        // 本地时间由 libc 处理
        let tm = unsafe {
            let mut t: libc::time_t = secs as libc::time_t;
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&mut t, &mut tm);
            tm
        };
        (tm.tm_hour, tm.tm_min, tm.tm_sec)
    };
    format!("{h:02}:{m:02}:{s:02}")
}

fn tag(l: Level) -> &'static str {
    match l {
        Level::Error => "ERR ",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DBG ",
    }
}

pub fn log_at(l: Level, args: fmt::Arguments<'_>) {
    if l > level() {
        return;
    }
    if l == Level::Info {
        // info 输出更接近原版的可读样式
        eprintln!("{} [{}] {}", timestamp(), tag(l), args);
    } else {
        eprintln!("{} [{}] {}", timestamp(), tag(l), args);
    }
}

/// 无前缀、不强制换行的原始输出（如插件的逐行帮助文本）。
pub fn log_raw(args: fmt::Arguments<'_>) {
    eprint!("{}", args);
}

#[allow(dead_code)]
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
