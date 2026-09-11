//! 极简日志系统。
//!
//! - 默认所有日志输出到 stderr，带时间戳；级别由命令行 `-v` 控制。
//! - 可用 `--log-file <FILE>`（或配置文件的 `log_file = ...`）把日志**追加**写入
//!   文件。写入后**立即 flush**，因此 `tail -f` 与 LuCI 的增量读取都能实时看到新行；
//!   并且 `--daemon` 守护模式下也不会丢日志（守护化只重定向标准流，不影响本文件的
//!   自有文件句柄）。
//! - **自动清理/轮转**：每隔 [`RotatePolicy::interval_secs`] 秒（或写入量到阈值时）
//!   检查一次，日志文件超过 [`RotatePolicy::max_bytes`] 就轮转成 `xxx.log.1`，
//!   旧的依次后移，只保留 `keep` 份历史，超出者自动删除。策略来自
//!   `--log-max-kb` / `--log-keep` / `--log-rotate-secs`（或配置文件同名键）。
//!
//! 原项目大量日志为中文，这里沿用其风格。

use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// 每写入多少字节就顺带检查一次是否该轮转（避免每次都 stat）。
const CHECK_EVERY_BYTES: usize = 32 * 1024;

/// 日志轮转 / 自动清理策略。
#[derive(Debug, Clone, Copy)]
pub struct RotatePolicy {
    /// 单个日志文件大小上限（字节）
    pub max_bytes: u64,
    /// 保留的历史文件份数（`.1`、`.2`…）；0 表示超限直接清空、不留历史
    pub keep: usize,
    /// 两次检查之间的最小间隔（秒）
    pub interval_secs: u64,
}

impl Default for RotatePolicy {
    fn default() -> Self {
        RotatePolicy {
            max_bytes: 256 * 1024,
            keep: 3,
            interval_secs: 300,
        }
    }
}

struct LogState {
    file: Option<std::fs::File>,
    path: PathBuf,
    policy: RotatePolicy,
    /// 自上次检查以来写入的字节数
    since_check: usize,
    last_check: Instant,
}

/// 文件日志状态（追加模式）。设置后所有日志只写这里（不再写 stderr），
/// 每次写入立即 flush，保证实时可见。
static LOG_STATE: Mutex<Option<LogState>> = Mutex::new(None);
/// 已启用的日志文件路径（供状态展示）
static LOG_PATH: Mutex<Option<String>> = Mutex::new(None);
/// 已发生的轮转次数
static ROTATIONS: AtomicUsize = AtomicUsize::new(0);

fn open_append(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() && !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// 启用日志文件（追加、自动建父目录），使用默认轮转策略。
pub fn set_log_file(path: &str) -> std::io::Result<()> {
    set_log_file_with(path, RotatePolicy::default())
}

/// 启用日志文件并指定轮转/自动清理策略。
pub fn set_log_file_with(path: &str, policy: RotatePolicy) -> std::io::Result<()> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(());
    }
    let p = PathBuf::from(path);
    let f = open_append(&p)?;
    let policy = RotatePolicy {
        max_bytes: policy.max_bytes.max(4 * 1024),
        keep: policy.keep,
        interval_secs: policy.interval_secs.max(1),
    };
    *LOG_STATE.lock().unwrap() = Some(LogState {
        file: Some(f),
        path: p,
        policy,
        since_check: 0,
        last_check: Instant::now(),
    });
    *LOG_PATH.lock().unwrap() = Some(path.to_string());
    Ok(())
}

/// 当前日志文件路径（未启用则 None）。
pub fn log_file_path() -> Option<String> {
    LOG_PATH.lock().unwrap().clone()
}

/// 已发生的日志轮转次数（供状态展示）。
pub fn rotation_count() -> usize {
    ROTATIONS.load(Ordering::Relaxed)
}

/// 立即按策略检查并轮转（忽略时间间隔），返回是否真的轮转了。
pub fn rotate_now() -> bool {
    let mut guard = LOG_STATE.lock().unwrap();
    match guard.as_mut() {
        Some(st) => rotate_if_needed(st, true),
        None => false,
    }
}

/// 由外部定时器周期调用：到点则检查一次（未到间隔时几乎零开销）。
pub fn tick_rotate() {
    let mut guard = LOG_STATE.lock().unwrap();
    if let Some(st) = guard.as_mut() {
        rotate_if_needed(st, false);
    }
}

fn suffixed(path: &Path, idx: usize) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(format!(".{idx}"));
    PathBuf::from(s)
}

/// 到点/超额则轮转。`force` 时忽略时间与写入量阈值。
fn rotate_if_needed(st: &mut LogState, force: bool) -> bool {
    let due = force
        || st.since_check >= CHECK_EVERY_BYTES
        || st.last_check.elapsed() >= Duration::from_secs(st.policy.interval_secs);
    if !due {
        return false;
    }
    st.since_check = 0;
    st.last_check = Instant::now();
    let size = st
        .file
        .as_ref()
        .and_then(|f| f.metadata().ok())
        .map(|m| m.len())
        .unwrap_or(0);
    if size < st.policy.max_bytes {
        return false;
    }
    rotate(st);
    true
}

/// 执行轮转：当前文件 → `.1`，`.1` → `.2`…，最旧一份删除（自动清理）。
fn rotate(st: &mut LogState) {
    st.file = None; // 先关闭，保证 Unix 上重命名/删除的是已落盘的文件
    let path = st.path.clone();
    let keep = st.policy.keep;
    if keep == 0 {
        let _ = std::fs::remove_file(&path);
    } else {
        // 删除会超出保留份数的最旧文件
        let _ = std::fs::remove_file(suffixed(&path, keep + 1));
        for i in (1..=keep).rev() {
            let from = if i == 1 { path.clone() } else { suffixed(&path, i - 1) };
            let _ = std::fs::rename(&from, suffixed(&path, i));
        }
    }
    match open_append(&path) {
        Ok(f) => {
            ROTATIONS.fetch_add(1, Ordering::Relaxed);
            st.file = Some(f);
            let notice = format!(
                "{} [INFO] 日志已自动轮转：超过 {} KB，保留 {} 份历史（{}）\n",
                timestamp(),
                st.policy.max_bytes / 1024,
                keep,
                path.display()
            );
            if let Some(f) = st.file.as_mut() {
                let _ = f.write_all(notice.as_bytes());
                let _ = f.flush();
            }
        }
        Err(e) => {
            eprintln!("无法重新打开日志文件 {}: {e}", path.display());
        }
    }
}

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
    let line = format!("{} [{}] {}", timestamp(), tag(l), args);
    let mut guard = LOG_STATE.lock().unwrap();
    match guard.as_mut() {
        Some(st) => {
            // 到点/超额先做自动清理（轮转）
            rotate_if_needed(st, false);
            if let Some(f) = st.file.as_mut() {
                // 立即 flush：保证 LuCI 增量读取 / tail -f 实时可见
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
                st.since_check += line.len() + 1;
            } else {
                eprintln!("{line}");
            }
        }
        None => eprintln!("{line}"),
    }
}

/// 无前缀、不强制换行的原始输出（如插件的逐行帮助文本）。
pub fn log_raw(args: fmt::Arguments<'_>) {
    let mut guard = LOG_STATE.lock().unwrap();
    match guard.as_mut() {
        Some(st) => {
            if let Some(f) = st.file.as_mut() {
                let _ = write!(f, "{args}");
                let _ = f.flush();
            } else {
                eprint!("{args}");
            }
        }
        None => eprint!("{args}"),
    }
}

#[allow(dead_code)]
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_info;

    /// 日志状态是进程级全局的，两个测试必须串行执行。
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("autheap-log-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn file_logging_appends_and_flushes() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tmp_path("basic");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("autheap.log");
        set_log_file(path.to_str().unwrap()).expect("open log file");
        assert_eq!(log_file_path().as_deref(), Some(path.to_str().unwrap()));

        log_info!("hello {}", 42);

        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.contains("hello 42"), "日志应写入文件: {s}");
        assert!(s.ends_with('\n'), "每行应以换行结束");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_moves_old_file_and_keeps_history() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tmp_path("rotate");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("autheap.log");
        // 上限 4KB、保留 2 份历史
        set_log_file_with(
            path.to_str().unwrap(),
            RotatePolicy {
                max_bytes: 4 * 1024,
                keep: 2,
                interval_secs: 1,
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(1100));

        for i in 0..400 {
            log_info!("填充日志 {i} {}", "x".repeat(40));
        }
        assert!(rotate_now() || path.with_file_name("autheap.log.1").exists());

        // 触发一次轮转后，当前文件应远小于“全部内容”，且历史文件存在
        assert!(rotation_count() >= 1, "应至少轮转过一次");
        let hist = PathBuf::from(format!("{}.1", path.display()));
        assert!(hist.exists(), "应保留 .1 历史日志");

        let cur = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(cur <= 8 * 1024, "当前日志应已被清理，实际 {cur} 字节");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
