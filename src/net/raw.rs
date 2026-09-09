//! Linux AF_PACKET 原始套接字后端（对应原版 `if_impl_sockraw`）。

use std::io;
use std::mem;
use std::os::unix::io::RawFd;

use crate::error::{Error, Result};

pub struct RawSocket {
    fd: RawFd,
    ifindex: libc::c_int,
    ifname: String,
    mac: [u8; 6],
}

const ETH_P_PAE: u16 = 0x888e;
const PACKET_IGNORE_OUTGOING: libc::c_int = 23;

fn htons(v: u16) -> u16 {
    u16::to_be(v)
}

fn err() -> Error {
    Error::Os(io::Error::last_os_error())
}

impl RawSocket {
    pub fn open(ifname: &str, promisc: bool) -> Result<RawSocket> {
        // 获取 ifindex
        let cname = std::ffi::CString::new(ifname).map_err(|_| Error::Config("网卡名含 NUL".into()))?;
        let ifindex = unsafe { libc::if_nametoindex(cname.as_ptr()) } as libc::c_int;
        if ifindex == 0 {
            return Err(Error::Net(format!("无法找到网卡 {ifname}")));
        }
        let mac = crate::net::ifinfo::iface_mac(ifname)?;

        // socket(AF_PACKET, SOCK_RAW, htons(0x888e))
        let proto = htons(ETH_P_PAE) as libc::c_int;
        let fd = unsafe { libc::socket(libc::AF_PACKET, libc::SOCK_RAW | libc::SOCK_CLOEXEC, proto) };
        if fd < 0 {
            return Err(err());
        }

        // 绑定接口
        let mut sll: libc::sockaddr_ll = unsafe { mem::zeroed() };
        sll.sll_family = libc::AF_PACKET as libc::sa_family_t;
        sll.sll_protocol = proto as libc::c_ushort;
        sll.sll_ifindex = ifindex as libc::c_int;
        let ret = unsafe {
            libc::bind(
                fd,
                &sll as *const libc::sockaddr_ll as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            let e = err();
            unsafe { libc::close(fd) };
            return Err(e);
        }

        // 忽略本进程发出的帧，避免回环
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_PACKET,
                PACKET_IGNORE_OUTGOING,
                &one as *const libc::c_int as *const libc::c_void,
                mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // 接收超时 300ms（便于停止线程）
        let tv = libc::timeval {
            tv_sec: 0,
            tv_usec: 300_000,
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const libc::timeval as *const libc::c_void,
                mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
        }

        // 混杂模式提示（802.1x 不需要；设置接口 IFF_PROMISC 需要额外特权，故忽略）
        let _ = promisc;

        Ok(RawSocket {
            fd,
            ifindex,
            ifname: ifname.to_string(),
            mac,
        })
    }

    fn sendto(&self, bytes: &[u8]) -> io::Result<()> {
        let mut sll: libc::sockaddr_ll = unsafe { mem::zeroed() };
        sll.sll_family = libc::AF_PACKET as libc::sa_family_t;
        sll.sll_ifindex = self.ifindex;
        sll.sll_halen = 6;
        // 目的 MAC 取自帧头（若不足 14 字节则忽略）
        if bytes.len() >= 14 {
            sll.sll_addr[..6].copy_from_slice(&bytes[0..6]);
        }
        let ret = unsafe {
            libc::sendto(
                self.fd,
                bytes.as_ptr() as *const libc::c_void,
                bytes.len(),
                0,
                &sll as *const libc::sockaddr_ll as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for RawSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

impl crate::net::FrameBackend for RawSocket {
    fn ifname(&self) -> &str {
        &self.ifname
    }
    fn local_mac(&self) -> [u8; 6] {
        self.mac
    }
    fn send(&self, bytes: &[u8]) -> io::Result<()> {
        self.sendto(bytes)
    }
    fn recv(&self, out: &mut [u8]) -> io::Result<usize> {
        let n = unsafe {
            libc::recv(
                self.fd,
                out.as_mut_ptr() as *mut libc::c_void,
                out.len(),
                0,
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }
    fn request_stop(&self) {
        // recv 有 300ms 超时，线程会自行退出；无需额外处理
    }
}
