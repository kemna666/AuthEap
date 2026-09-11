//! AuthEAP —— Rust 版可执行入口。

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use autheap::cli::Cli;
use autheap::config::Config;
use autheap::lockfile::InstanceLock;
use autheap::logging::set_level;
use autheap::net::{Io, NetIo};
use autheap::runtime::App;
use autheap::VERSION;

/// 退出码：0=成功；1=网络测试发现问题/一般错误；2=命令行用法错误（clap）；3=已有实例在运行
const EXIT_ALREADY_RUNNING: u8 = 3;

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<u8, Box<dyn std::error::Error>> {
    let cli = Cli::parse_args();

    // 列出已编译插件后立即退出（供 LuCI / 脚本枚举；不要求网卡与配置）
    if cli.list_plugins {
        for (name, desc) in autheap::plugins::available() {
            println!("{name}\t{desc}");
        }
        return Ok(0);
    }

    // 网络测试（诊断用）：不需要账号，也不打开网卡
    if let Some(host) = cli.nettest.as_deref() {
        let ifname = cli
            .ifname
            .clone()
            .or_else(autheap::net::ifinfo::default_route_iface)
            .unwrap_or_else(|| "eth0".to_string());
        let port = cli.beacon_port.unwrap_or(80);
        return Ok(autheap::nettest::run(&ifname, host, port) as u8);
    }

    let cfg: Config = cli.into_config()?;

    // 日志文件（可选）：追加写入并立即 flush，保证 LuCI / tail -f 实时可见，
    // 且 --daemon 守护化（重定向标准流）后也不会丢日志。
    // 同时按策略自动轮转清理（超过 --log-max-kb 就轮转，只保留 --log-keep 份历史）。
    if let Some(lf) = cfg.log_file.clone() {
        let policy = autheap::logging::RotatePolicy {
            max_bytes: cfg.log_max_kb.max(1) * 1024,
            keep: cfg.log_keep,
            interval_secs: cfg.log_rotate_secs.max(1),
        };
        if let Err(e) = autheap::logging::set_log_file_with(&lf, policy) {
            eprintln!("无法打开日志文件 {lf}: {e}");
        }
    }

    set_level(if cfg.verbose >= 2 {
        autheap::logging::Level::Debug
    } else {
        autheap::logging::Level::Info
    });

    cfg.validate()?;

    // ---- 单实例：认证进程只允许一个 ----
    // 只有“跑认证”的进程会抢锁；--nettest / --list-plugins / --help 等
    // 诊断与查询功能在上面已短路返回，所以守护进程在跑时 CLI 照常可用。
    let mut lock: Option<InstanceLock> = None;
    if let Some(path) = cfg.pid_file.clone() {
        match InstanceLock::acquire(&path) {
            Ok(l) => {
                autheap::log_info!("已取得单实例锁：{path}（pid {}）", std::process::id());
                lock = Some(l);
            }
            // 已有实例：直接拒绝启动（这是唯一会阻止启动的情况）
            Err(e @ autheap::lockfile::LockError::AlreadyRunning(_)) => {
                eprintln!("错误: {e}");
                autheap::log_error!("{e}");
                return Ok(EXIT_ALREADY_RUNNING);
            }
            // 锁文件写不了（如目录只读）：只告警，不让服务启不来
            Err(e) => {
                autheap::log_warn!("跳过单实例检查：{e}（可用 --pid-file 指定可写路径，或传空字符串关闭）");
            }
        }
    }

    banner(&cfg);
    // 守护进程化（父进程退出）
    if cfg.daemon {
        #[cfg(unix)]
        daemonize()?;
        #[cfg(not(unix))]
        return Err("该平台不支持 --daemon".into());
        // fork 后锁由子进程继承（同一个文件描述），但 pid 文件里记的还是父进程 pid，
        // 这里刷新一次，便于“已有实例在运行（pid N）”提示准确。
        if let Some(l) = lock.as_mut() {
            l.refresh_pid();
        }
    }

    let io: Box<dyn Io> = Box::new(NetIo::open(cfg.backend, &cfg.ifname, false)?);
    let mut app = App::new(cfg, io)?;

    // Ctrl-C / SIGTERM 优雅退出
    let flag: Arc<AtomicBool> = app.shutdown.clone();
    unsafe {
        signal_hook::low_level::register(signal_hook::consts::SIGINT, move || {
            flag.store(true, Ordering::Relaxed);
        })?;
    }
    let flag2: Arc<AtomicBool> = app.shutdown.clone();
    unsafe {
        signal_hook::low_level::register(signal_hook::consts::SIGTERM, move || {
            flag2.store(true, Ordering::Relaxed);
        })?;
    }

    autheap::log_info!("AuthEAP {VERSION} 已启动");
    app.run()?;
    Ok(0)
}

fn banner(cfg: &Config) {
    autheap::log_info!(
        "AuthEAP {} —— 网卡: {} 后端: {:?} 插件: {:?}",
        VERSION,
        cfg.ifname,
        cfg.backend,
        cfg.plugins
    );
    if cfg.plugins.iter().any(|p| p == "rjv3") {
        autheap::log_info!("锐捷 v3/v4 插件已启用（算法移植自 hyrathb@GitHub / updateing）");
    }
    if cfg.plugins.iter().any(|p| p == "beacon") {
        let plen = cfg
            .beacon_payload
            .as_ref()
            .map(|p| p.len())
            .unwrap_or_else(|| autheap::plugins::beacon::default_payload().len());
        match cfg.beacon_dst.as_deref() {
            Some(d) if cfg.beacon_interval > 0 && plen > 0 => autheap::log_info!(
                "beacon 插件已启用：每 {} 秒向 {}:{} 发送 {} 字节数据包",
                cfg.beacon_interval,
                d,
                cfg.beacon_port,
                plen
            ),
            _ => autheap::log_warn!(
                "beacon 插件已启用但未配置目标 IP（--beacon-dst）或间隔为 0，已跳过；可在 LuCI“beacon 保活”里配置"
            ),
        }
    }
    if let Some(lf) = cfg.log_file.as_deref() {
        autheap::log_info!(
            "日志文件: {lf}（追加写入、实时刷新；超过 {} KB 自动轮转，保留 {} 份历史，每 {} 秒检查一次）",
            cfg.log_max_kb,
            cfg.log_keep,
            cfg.log_rotate_secs
        );
    }
}

/// 传统守护进程化（fork + setsid + 二次 fork），父进程退出。
#[cfg(unix)]
fn daemonize() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            return Err("fork 失败".into());
        }
        if pid > 0 {
            std::process::exit(0);
        }
        if libc::setsid() < 0 {
            return Err("setsid 失败".into());
        }
        let pid2 = libc::fork();
        if pid2 < 0 {
            return Err("第二次 fork 失败".into());
        }
        if pid2 > 0 {
            std::process::exit(0);
        }
    }
    // 重定向标准流到 /dev/null
    let devnull = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")?;
    let raw = devnull.as_raw_fd();
    unsafe {
        libc::dup2(raw, libc::STDIN_FILENO);
        libc::dup2(raw, libc::STDOUT_FILENO);
        libc::dup2(raw, libc::STDERR_FILENO);
    }
    // 避免 "broken pipe"
    let _ = std::io::stdout().flush();
    Ok(())
}
