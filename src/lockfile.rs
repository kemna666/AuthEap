//! 单实例锁（pid 文件 + flock）。
//!
//! 只有**认证进程**会去抢这把锁：同一个 pid 文件同时只允许一个实例。
//! `--nettest` / `--list-plugins` / `--help` 这类诊断或查询功能**不加锁**，
//! 所以守护进程在跑的时候，CLI 依然可以随时用来做网络测试等操作。
//!
//! 实现要点：
//! - 用 `flock(LOCK_EX|LOCK_NB)`，锁随「打开的文件描述」存在，进程退出（含被 `kill -9`）
//!   时由内核自动释放，不会留下“死锁”；
//! - 成功拿到锁后把 pid 写进文件（守护化 fork 之后再刷新一次，保证是子进程 pid）；
//! - 拿不到锁时尽量读出里面记录的 pid，便于提示“已有实例在运行”；
//! - 释放时只删除“内容仍是自己 pid”的锁文件，避免误删后来者的。

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 默认锁文件路径（与 procd 用的 `/var/run/autheap-client.pid` 分开，互不干扰）
pub const DEFAULT_PID_FILE: &str = "/var/run/autheap.pid";

#[derive(Debug)]
pub enum LockError {
    /// 已经有实例在运行（携带锁文件里记录的 pid，读不到时为 None）
    AlreadyRunning(Option<u32>),
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::AlreadyRunning(Some(pid)) => write!(
                f,
                "已有认证进程在运行（pid {pid}），本次启动已取消；\
                 需要重启请用 `/etc/init.d/autheap-client restart`，或先停止该进程"
            ),
            LockError::AlreadyRunning(None) => write!(
                f,
                "已有认证进程在运行，本次启动已取消（锁文件被占用；\
                 可用 `pidof autheap` 查看，或用 `/etc/init.d/autheap-client restart` 重启）"
            ),
            LockError::Io(e) => write!(f, "无法创建/打开锁文件: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

/// 持有的单实例锁；`drop` 时释放。
#[derive(Debug)]
pub struct InstanceLock {
    path: PathBuf,
    file: File,
}

impl InstanceLock {
    /// 尝试独占 `path`；失败返回 [`LockError::AlreadyRunning`]。
    pub fn acquire(path: impl AsRef<Path>) -> Result<Self, LockError> {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(dir);
            }
        }

        let mut file = Self::open(&path)?;

        if !try_lock(&file) {
            let pid = read_pid(&mut file);
            return Err(LockError::AlreadyRunning(pid));
        }

        write_pid(&mut file, std::process::id());
        Ok(Self { path, file })
    }

    #[cfg(unix)]
    fn open(path: &Path) -> Result<File, LockError> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map_err(LockError::Io)
    }

    // 非 unix 平台退化为“文件已存在即视为在运行”（无 flock）
    #[cfg(not(unix))]
    fn open(path: &Path) -> Result<File, LockError> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    LockError::AlreadyRunning(std::fs::read_to_string(path).ok().and_then(|s| {
                        s.trim().parse::<u32>().ok()
                    }))
                } else {
                    LockError::Io(e)
                }
            })
    }

    /// 锁文件路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 把当前进程 pid 写回锁文件（守护化 fork 之后调用，让文件记录子进程 pid）。
    pub fn refresh_pid(&mut self) {
        write_pid(&mut self.file, std::process::id());
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let me = std::process::id();
        // 只有文件里还是自己的 pid 时才删，避免把后来者的锁文件删掉
        if read_pid(&mut self.file) == Some(me) {
            let _ = std::fs::remove_file(&self.path);
        }
        unlock(&self.file);
    }
}

#[cfg(unix)]
fn try_lock(file: &File) -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

#[cfg(not(unix))]
fn try_lock(_file: &File) -> bool {
    true
}

#[cfg(unix)]
fn unlock(file: &File) {
    use std::os::unix::io::AsRawFd;
    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }
}

#[cfg(not(unix))]
fn unlock(_file: &File) {}

fn write_pid(file: &mut File, pid: u32) {
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    let _ = writeln!(file, "{pid}");
    let _ = file.flush();
}

fn read_pid(file: &mut File) -> Option<u32> {
    let mut buf = String::new();
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_to_string(&mut buf).ok()?;
    buf.trim().parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("autheap-lock-{tag}-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn second_acquire_fails_with_pid() {
        let p = tmp_path("dup");
        let held = InstanceLock::acquire(&p).expect("首次获取锁应成功");
        assert_eq!(std::fs::read_to_string(&p).unwrap().trim(), std::process::id().to_string());

        match InstanceLock::acquire(&p) {
            Err(LockError::AlreadyRunning(Some(pid))) => assert_eq!(pid, std::process::id()),
            other => panic!("第二次获取锁应失败并带上 pid，实际: {other:?}"),
        }

        drop(held);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn lock_is_reusable_after_release() {
        let p = tmp_path("reuse");
        {
            let _held = InstanceLock::acquire(&p).expect("首次获取锁应成功");
            assert!(p.exists());
        }
        // 释放后（文件已删除）应能再次获取
        let again = InstanceLock::acquire(&p).expect("释放后应能重新获取");
        assert_eq!(again.path(), p.as_path());
        drop(again);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn stale_file_does_not_block() {
        let p = tmp_path("stale");
        std::fs::write(&p, "999999\n").unwrap(); // 没有实际持锁的残留文件
        let lock = InstanceLock::acquire(&p).expect("残留文件不应阻塞");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap().trim(),
            std::process::id().to_string()
        );
        drop(lock);
        let _ = std::fs::remove_file(&p);
    }
}
