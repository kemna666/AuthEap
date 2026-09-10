//! 网络收发后端抽象。
//!
//! `Io` trait 描述认证核心对网络的全部需求：发送帧、带超时接收帧、本机 MAC。
//! 真实实现 `NetIo` 使用后台线程接收帧；另有供测试的 `LoopbackIo`。

pub mod ifinfo;

#[cfg(target_os = "linux")]
mod raw;

#[cfg(feature = "backend-pcap")]
pub mod pcap_backend;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crate::config::Backend;
use crate::error::{Error, Result};
use crate::eth_frame::Frame;

/// 收发包后端（原始帧，不含过滤）。
pub trait FrameBackend: Send + Sync {
    fn ifname(&self) -> &str;
    fn local_mac(&self) -> [u8; 6];
    /// 发送一帧（须包含完整以太网头）
    fn send(&self, bytes: &[u8]) -> io::Result<()>;
    /// 阻塞接收一帧，写入 `out`，返回实际长度。内部需周期性超时以便调用方检查停止。
    fn recv(&self, out: &mut [u8]) -> io::Result<usize>;
    /// 请求停止接收。
    fn request_stop(&self);
}

/// 认证核心所依赖的网络 IO 抽象。
pub trait Io: Send {
    fn send_frame(&self, frame: &Frame) -> Result<()>;
    /// 返回 None 表示已停止/通道关闭。
    fn recv_timeout(&self, timeout: Duration) -> Result<Option<Vec<u8>>>;
    fn local_mac(&self) -> [u8; 6];
    fn ifname(&self) -> &str;
}

/// 打开指定后端。
pub fn open_backend(backend: Backend, ifname: &str, promisc: bool) -> Result<Box<dyn FrameBackend>> {
    match backend {
        Backend::Raw => {
            #[cfg(target_os = "linux")]
            {
                Ok(Box::new(raw::RawSocket::open(ifname, promisc)?))
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (ifname, promisc);
                Err(Error::Net(
                    "raw 后端仅支持 Linux；请使用 pcap 后端 (--backend pcap)".into(),
                ))
            }
        }
        Backend::Pcap => {
            #[cfg(feature = "backend-pcap")]
            {
                Ok(Box::new(pcap_backend::PcapBackend::open(ifname, promisc)?))
            }
            #[cfg(not(feature = "backend-pcap"))]
            {
                let _ = (ifname, promisc);
                Err(Error::Config(
                    "pcap 后端未编译。请启用 feature: cargo build --features backend-pcap（需安装 libpcap 开发库）".into(),
                ))
            }
        }
    }
}

fn spawn_reader(backend: Arc<dyn FrameBackend>, tx: Sender<Vec<u8>>, stop: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("autheap-recv".into())
        .spawn(move || {
            let mut buf = vec![0u8; 2048];
            while !stop.load(Ordering::Relaxed) {
                match backend.recv(&mut buf) {
                    Ok(0) => continue,
                    Ok(n) => {
                        // 只把 PAE 类型帧传给上层
                        if n >= 14 && buf[12] == 0x88 && buf[13] == 0x8e {
                            let _ = tx.send(buf[..n].to_vec());
                        }
                    }
                    Err(_) => {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
            drop(tx);
        })
        .expect("无法创建接收线程");
}

/// 基于真实后端、带接收线程的 IO。
pub struct NetIo {
    backend: Arc<dyn FrameBackend>,
    rx: Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
}

impl NetIo {
    /// 打开后端并启动接收线程。
    pub fn open(backend: Backend, ifname: &str, promisc: bool) -> Result<NetIo> {
        let backend = open_backend(backend, ifname, promisc)?;
        let backend: Arc<dyn FrameBackend> = Arc::from(backend);
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        spawn_reader(backend.clone(), tx, stop.clone());
        Ok(NetIo {
            backend,
            rx,
            stop,
        })
    }
}

impl Io for NetIo {
    fn send_frame(&self, frame: &Frame) -> Result<()> {
        self.backend.send(&frame.buf).map_err(Error::Os)
    }

    fn recv_timeout(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        match self.rx.recv_timeout(timeout) {
            Ok(f) => Ok(Some(f)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn local_mac(&self) -> [u8; 6] {
        self.backend.local_mac()
    }

    fn ifname(&self) -> &str {
        self.backend.ifname()
    }
}

impl Drop for NetIo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.backend.request_stop();
    }
}

/// 测试用回环 IO：可预置入站帧、捕获出站帧。
#[cfg(test)]
pub struct LoopbackIo {
    pub outbound: std::sync::Mutex<Vec<Frame>>,
    pub inbound: std::sync::Mutex<std::collections::VecDeque<Vec<u8>>>,
    pub mac: [u8; 6],
}

#[cfg(test)]
impl LoopbackIo {
    pub fn new() -> LoopbackIo {
        LoopbackIo {
            outbound: std::sync::Mutex::new(Vec::new()),
            inbound: std::sync::Mutex::new(std::collections::VecDeque::new()),
            mac: [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
        }
    }

    pub fn push_frame(&self, frame: Frame) {
        self.inbound.lock().unwrap().push_back(frame.buf);
    }
}

#[cfg(test)]
impl Io for LoopbackIo {
    fn send_frame(&self, frame: &Frame) -> Result<()> {
        self.outbound.lock().unwrap().push(frame.clone());
        Ok(())
    }

    fn recv_timeout(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut q = self.inbound.lock().unwrap();
        let start = std::time::Instant::now();
        loop {
            if let Some(f) = q.pop_front() {
                return Ok(Some(f));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            drop(q);
            std::thread::sleep(Duration::from_millis(2));
            q = self.inbound.lock().unwrap();
        }
    }

    fn local_mac(&self) -> [u8; 6] {
        self.mac
    }

    fn ifname(&self) -> &str {
        "lo"
    }
}

#[cfg(test)]
impl Io for Arc<LoopbackIo> {
    fn send_frame(&self, frame: &Frame) -> Result<()> {
        (**self).send_frame(frame)
    }
    fn recv_timeout(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        (**self).recv_timeout(timeout)
    }
    fn local_mac(&self) -> [u8; 6] {
        (**self).local_mac()
    }
    fn ifname(&self) -> &str {
        (**self).ifname()
    }
}
