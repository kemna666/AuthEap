//! libpcap 后端（可移植，跨平台），对应原版 `if_impl_libpcap`。
//! 仅在 feature `backend-pcap` 下编译，需系统安装 libpcap 开发库。

use std::io;

use crate::error::{Error, Result};

pub struct PcapBackend {
    cap: pcap::Capture<pcap::Active>,
    ifname: String,
    mac: Option<[u8; 6]>,
}

impl PcapBackend {
    pub fn open(ifname: &str, promisc: bool) -> Result<PcapBackend> {
        let cap = pcap::Capture::from_device(ifname)
            .map_err(|e| Error::Net(format!("打开设备 {ifname} 失败: {e}")))?
            .promisc(promisc)
            .snaplen(2048)
            .timeout(250)
            .immediate_mode(true)
            .open()
            .map_err(|e| Error::Net(format!("pcap open 失败: {e}")))?;

        // 只捕获 PAE EtherType，减少噪音
        let _ = cap.filter("ether proto 0x888e", true);

        // 尽力获取 MAC
        let mac = crate::net::ifinfo::iface_mac(ifname).ok();

        Ok(PcapBackend {
            cap,
            ifname: ifname.to_string(),
            mac,
        })
    }
}

impl crate::net::FrameBackend for PcapBackend {
    fn ifname(&self) -> &str {
        &self.ifname
    }
    fn local_mac(&self) -> [u8; 6] {
        self.mac.unwrap_or([0; 6])
    }
    fn send(&self, bytes: &[u8]) -> io::Result<()> {
        self.cap
            .sendpacket(bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
    fn recv(&self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.cap.next_packet() {
                Ok(pkt) => {
                    let data = pkt.data;
                    if data.len() > out.len() {
                        continue; // 溢出则丢弃
                    }
                    out[..data.len()].copy_from_slice(data);
                    return Ok(data.len());
                }
                Err(pcap::Error::TimeoutExpired) => {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "timeout"));
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
            }
        }
    }
    fn request_stop(&self) {
        // recv 有 250ms 超时，线程自行退出
    }
}
