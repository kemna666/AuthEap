//! MiniEAP —— Rust 版可执行入口。

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use minieap::cli::Cli;
use minieap::config::Config;
use minieap::logging::set_level;
use minieap::net::{Io, NetIo};
use minieap::runtime::App;
use minieap::VERSION;

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse_args();

    // 列出已编译插件后立即退出（供 LuCI / 脚本枚举；不要求网卡与配置）
    if cli.list_plugins {
        for (name, desc) in minieap::plugins::available() {
            println!("{name}\t{desc}");
        }
        return Ok(());
    }

    let cfg: Config = cli.into_config()?;

    set_level(if cfg.verbose >= 2 {
        minieap::logging::Level::Debug
    } else {
        minieap::logging::Level::Info
    });

    cfg.validate()?;

    banner(&cfg);

    // 守护进程化（父进程退出）
    if cfg.daemon {
        #[cfg(unix)]
        daemonize()?;
        #[cfg(not(unix))]
        return Err("该平台不支持 --daemon".into());
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

    minieap::log_info!("MiniEAP {VERSION} 已启动");
    app.run()?;
    Ok(())
}

fn banner(cfg: &Config) {
    minieap::log_info!(
        "MiniEAP {} —— 网卡: {} 后端: {:?} 插件: {:?}",
        VERSION,
        cfg.ifname,
        cfg.backend,
        cfg.plugins
    );
    if cfg.plugins.iter().any(|p| p == "rjv3") {
        minieap::log_info!("锐捷 v3/v4 插件已启用（算法移植自 hyrathb@GitHub / updateing）");
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
