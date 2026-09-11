//! `beacon` —— 周期性向指定 IP:端口 发送自定义 TCP 数据包。
//!
//! 用途：把一段 TCP 应用层载荷（默认用内置载荷，见 [`DEFAULT_PAYLOAD_HEX`]）按固定
//! 间隔重新发送给**用户指定**的服务器，用于在线保活等场景。
//!
//! 与原始抓包的差异：
//!
//! - **不硬编码任何 IP**：发送方是**运行本程序的机器**（源地址由内核按路由决定），
//!   接收方由用户指定（`--beacon-dst` / 配置文件 `beacon_dst` / LuCI 里填目标 IP）；
//! - **载荷可覆盖**：默认使用内置载荷；`--beacon-payload` / 配置文件 `beacon_payload`
//!   / LuCI 配置页可填自己的抓包 `Data` 十六进制覆盖（长度需为偶数）；
//! - **时间 = 构造数据包的时刻**：每次发送前重新构造一份载荷，因此数据包的“时间”
//!   就是当时时间，抓包里的固定时间不会被复用；
//! - **间隔可配置**：`--beacon-interval`（秒），并可在 LuCI 界面里随时调整。
//!
//! 实现说明：插件在工作线程里用普通 TCP 连接（`connect` + `write`）发送载荷，
//! 于是 IP/TCP 头与校验和都由内核生成——源地址自然是本机真实地址，目的地址是用户
//! 配置值。
//!
//! > 郑重提示：请仅在你**拥有使用授权**的网络上使用本功能。

use std::io::{self, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::plugin::Plugin;
use crate::util::hex_str_to_bytes;
use crate::{log_info, log_warn};

/// 内置默认载荷（抓包 `Data` 字段的十六进制）。
///
/// 说明：Wireshark 显示该字段时省略了尾部，因此这里的十六进制长度为**奇数**（末尾
/// 半个字节）；[`default_payload()`] 会丢弃这半个字节后按字节解析。
/// 可用 `--beacon-payload` / 配置文件 `beacon_payload` / LuCI 配置页覆盖。
pub const DEFAULT_PAYLOAD_HEX: &str = concat!(
    "89000000bb92cc046fc8ccc56ceda1e6e2633d7c",
    "ea77bce95f6f512aad117a15f26cd2cadedd389c",
    "99b8de2693b1d1ba83cb2f7fb24f00dd357828d4",
    "4669ec559103e48b8e6e562a97c4ac26cc989583",
    "597ffb6a09d91d7ed8e6756c65689c2f6e91db77",
    "4f75d25b01992557938e684b8db",
);

/// 首次发送前的等待（秒）：给接口/路由一点就绪时间。
const FIRST_DELAY_SECS: u64 = 2;
/// 单次连接/写入超时（秒）。
const IO_TIMEOUT_SECS: u64 = 3;

/// 解析内置默认载荷（奇数长度时丢弃末尾半个字节）。
pub fn default_payload() -> Vec<u8> {
    let hex = if DEFAULT_PAYLOAD_HEX.len() % 2 == 1 {
        &DEFAULT_PAYLOAD_HEX[..DEFAULT_PAYLOAD_HEX.len() - 1]
    } else {
        DEFAULT_PAYLOAD_HEX
    };
    hex_str_to_bytes(hex).unwrap_or_default()
}

/// 解析目标地址：支持 `1.2.3.4`、`host`、`1.2.3.4:80`、`[::1]:80`。
fn resolve_target(dst: &str, port: u16) -> io::Result<SocketAddr> {
    let d = dst.trim();
    if let Ok(a) = d.parse::<SocketAddr>() {
        return Ok(a);
    }
    // 已带端口的写法（host:port / [v6]:port）直接用；否则补上配置端口
    let target = if d.starts_with('[') || d.matches(':').count() == 1 {
        d.to_string()
    } else {
        format!("{d}:{port}")
    };
    target
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, format!("无法解析目标地址: {target}")))
}

/// 发送一次：建立 TCP 连接并写入载荷。内核负责 IP/TCP 头与校验和（源=本机）。
fn send_once(addr: SocketAddr, payload: &[u8]) -> io::Result<()> {
    let timeout = Duration::from_secs(IO_TIMEOUT_SECS);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(payload)?;
    stream.flush()?;
    let _ = stream.shutdown(Shutdown::Write);
    Ok(())
}

/// 可被停止标志打断的 sleep；返回 false 表示收到停止请求。
fn sleep_interruptible(stop: &AtomicBool, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        std::thread::sleep(deadline.saturating_duration_since(now).min(Duration::from_millis(200)));
    }
}

fn format_clock(secs: u64) -> String {
    let tm = unsafe {
        let mut t: libc::time_t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&mut t, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[allow(clippy::too_many_arguments)]
fn worker(
    dst: String,
    port: u16,
    interval: u64,
    payload: Vec<u8>,
    stop: Arc<AtomicBool>,
    sent: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
    last_ok: Arc<AtomicUsize>,
) {
    log_info!(
        "[beacon] 已启动：目标 {}:{}，间隔 {} 秒，载荷 {} 字节",
        dst,
        port,
        interval,
        payload.len()
    );
    if !sleep_interruptible(&stop, FIRST_DELAY_SECS) {
        return;
    }
    while !stop.load(Ordering::Relaxed) {
        // 每次发送前重新构造数据包：数据包的时间即“构造时刻”
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let bytes = payload.clone();
        match resolve_target(&dst, port).and_then(|addr| {
            send_once(addr, &bytes).map(|()| addr)
        }) {
            Ok(addr) => {
                sent.fetch_add(1, Ordering::Relaxed);
                last_ok.store(now as usize, Ordering::Relaxed);
                log_info!(
                    "[beacon] 已发送 {} 字节 → {}（构造/发送时间 {}，累计成功 {} 次）",
                    bytes.len(),
                    addr,
                    format_clock(now),
                    sent.load(Ordering::Relaxed)
                );
            }
            Err(e) => {
                failed.fetch_add(1, Ordering::Relaxed);
                log_warn!(
                    "[beacon] 发送失败 → {}:{}: {e}（累计失败 {} 次）",
                    dst,
                    port,
                    failed.load(Ordering::Relaxed)
                );
            }
        }
        if !sleep_interruptible(&stop, interval) {
            break;
        }
    }
    log_info!("[beacon] 已停止");
}

/// beacon 插件。
pub struct Beacon {
    dst: String,
    port: u16,
    interval: u64,
    payload: Vec<u8>,
    enabled: bool,
    stop: Arc<AtomicBool>,
    sent: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
    last_ok: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

impl Beacon {
    pub fn new(cfg: &Config) -> Beacon {
        let dst = cfg.beacon_dst.clone().unwrap_or_default();
        let payload = cfg.beacon_payload.clone().unwrap_or_else(default_payload);
        let enabled = !dst.trim().is_empty() && cfg.beacon_interval > 0 && !payload.is_empty();
        let mut b = Beacon {
            dst,
            port: cfg.beacon_port,
            interval: cfg.beacon_interval,
            payload,
            enabled,
            stop: Arc::new(AtomicBool::new(false)),
            sent: Arc::new(AtomicUsize::new(0)),
            failed: Arc::new(AtomicUsize::new(0)),
            last_ok: Arc::new(AtomicUsize::new(0)),
            handle: None,
        };
        if !b.enabled {
            log_warn!(
                "[beacon] 未配置目标 IP（--beacon-dst）或间隔为 0，插件保持空闲；\
                 在 LuCI“beacon 保活”里填目标 IP 即可启用"
            );
        } else {
            b.spawn();
        }
        b
    }

    fn spawn(&mut self) {
        let (dst, port, interval, payload) = (
            self.dst.clone(),
            self.port,
            self.interval,
            self.payload.clone(),
        );
        let (stop, sent, failed, last_ok) = (
            self.stop.clone(),
            self.sent.clone(),
            self.failed.clone(),
            self.last_ok.clone(),
        );
        match std::thread::Builder::new()
            .name("autheap-beacon".into())
            .spawn(move || worker(dst, port, interval, payload, stop, sent, failed, last_ok))
        {
            Ok(h) => self.handle = Some(h),
            Err(e) => {
                self.enabled = false;
                log_warn!("[beacon] 无法创建工作线程: {e}");
            }
        }
    }

    /// (成功次数, 失败次数, 最近成功时间 unix 秒)
    pub fn stats(&self) -> (usize, usize, u64) {
        (
            self.sent.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
            self.last_ok.load(Ordering::Relaxed) as u64,
        )
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Plugin for Beacon {
    fn name(&self) -> &str {
        "beacon"
    }
    fn description(&self) -> &str {
        "周期性向指定 IP:端口 发送自定义 TCP 数据包（间隔/目标/载荷可在 LuCI 调整）"
    }
    fn version(&self) -> &str {
        "0.1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_payload_is_parsed_and_non_empty() {
        let p = default_payload();
        assert!(!p.is_empty(), "应内置默认载荷");
        // 奇数长度时丢弃末尾半个字节：226 个 hex 字符 → 113 字节
        assert_eq!(p.len(), (DEFAULT_PAYLOAD_HEX.len() - DEFAULT_PAYLOAD_HEX.len() % 2) / 2);
        // 抓包载荷以长度前缀 0x89 00 00 00 开头
        assert_eq!(&p[..4], &[0x89, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn resolve_target_accepts_common_forms() {
        // 使用 RFC 5737 文档用地址，避免示例中出现真实目标
        let a = resolve_target("203.0.113.10", 8083).unwrap();
        assert_eq!(a.port(), 8083);
        assert_eq!(a.ip().to_string(), "203.0.113.10");

        let b = resolve_target("203.0.113.10:9000", 8083).unwrap();
        assert_eq!(b.port(), 9000);

        assert!(resolve_target("999.999.999.999", 1).is_err());
    }

    #[test]
    fn enables_with_builtin_payload_when_payload_not_configured() {
        let mut cfg = Config::default();
        cfg.beacon_dst = Some("203.0.113.10".into());
        cfg.beacon_interval = 60;
        // 未配置载荷 → 使用内置默认载荷，插件正常工作
        let b = Beacon::new(&cfg);
        assert!(b.is_enabled());
        assert_eq!(b.payload(), default_payload().as_slice());
    }

    #[test]
    fn disabled_without_target_and_reports_name() {
        let cfg = Config::default();
        let b = Beacon::new(&cfg);
        assert_eq!(b.name(), "beacon");
        assert!(!b.is_enabled());
        assert_eq!(b.stats(), (0, 0, 0));
    }
    /// 真正建立 TCP 连接发送一次载荷（本机回环，无需 root；载荷为合成示例数据）。
    #[test]
    fn sends_payload_to_local_listener() {
        use std::io::Read;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let payload = vec![0x00, 0x11, 0x22, 0x33, 0xde, 0xad, 0xbe, 0xef];
        let expect = payload.clone();

        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        });

        let mut cfg = Config::default();
        cfg.beacon_dst = Some("127.0.0.1".into());
        cfg.beacon_port = port;
        cfg.beacon_interval = 30; // 只发一次即可
        cfg.beacon_payload = Some(payload);

        let b = Beacon::new(&cfg);
        assert!(b.is_enabled(), "配置了目标与载荷后应启用");        assert_eq!(b.payload(), &expect[..]);

        let got = server.join().expect("listener thread");
        assert_eq!(got, expect, "服务端应收到完全一致的载荷");
        assert!(b.stats().0 >= 1, "成功计数应 ≥ 1");
    }
}
