//! 网络接口信息查询：MAC、IPv4/IPv6 地址与掩码、默认网关、DNS。
//!
//! - MAC：Linux 下读取 `/sys/class/net/<if>/address`；
//! - IP / 掩码：`libc::getifaddrs`（Linux/macOS/BSD 通用）；
//! - IPv4 默认网关：Linux 解析 `/proc/net/route`（其它平台返回 None）；
//! - DNS：解析 `/etc/resolv.conf`（Unix）。

use std::net::IpAddr;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct IfAddr {
    pub ip: IpAddr,
    pub netmask: Option<IpAddr>,
}

/// 查询某个接口的全部 IP 与掩码。
pub fn iface_addrs(ifname: &str) -> Result<Vec<IfAddr>> {
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return Err(Error::Os(std::io::Error::last_os_error()));
        }
        let mut out = Vec::new();
        let mut cur = head;
        while !cur.is_null() {
            let ia = &*cur;
            let name_bytes = std::ffi::CStr::from_ptr(ia.ifa_name).to_bytes();
            if name_bytes == ifname.as_bytes() && !ia.ifa_addr.is_null() {
                let family = (*ia.ifa_addr).sa_family as i32;
                let ip = match family {
                    libc::AF_INET => {
                        let sin = &*(ia.ifa_addr as *const libc::sockaddr_in);
                        Some(IpAddr::V4(std::net::Ipv4Addr::from(
                            u32::from_be(sin.sin_addr.s_addr),
                        )))
                    }
                    libc::AF_INET6 => {
                        let sin6 = &*(ia.ifa_addr as *const libc::sockaddr_in6);
                        Some(IpAddr::V6(std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr)))
                    }
                    _ => None,
                };
                let netmask = if !ia.ifa_netmask.is_null() {
                    match (*ia.ifa_netmask).sa_family as i32 {
                        libc::AF_INET => {
                            let sin = &*(ia.ifa_netmask as *const libc::sockaddr_in);
                            Some(IpAddr::V4(std::net::Ipv4Addr::from(
                                u32::from_be(sin.sin_addr.s_addr),
                            )))
                        }
                        libc::AF_INET6 => {
                            let sin6 = &*(ia.ifa_netmask as *const libc::sockaddr_in6);
                            Some(IpAddr::V6(std::net::Ipv6Addr::from(
                                sin6.sin6_addr.s6_addr,
                            )))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(ip) = ip {
                    out.push(IfAddr { ip, netmask });
                }
            }
            cur = ia.ifa_next;
        }
        libc::freeifaddrs(head);
        Ok(out)
    }
}

/// 查询接口 MAC（仅 Linux sysfs；其它平台尽力而为或返回错误）。
pub fn iface_mac(ifname: &str) -> Result<[u8; 6]> {
    #[cfg(target_os = "linux")]
    {
        let p = format!("/sys/class/net/{ifname}/address");
        let s = std::fs::read_to_string(&p)
            .map_err(|e| Error::Net(format!("读取 {p} 失败: {e}")))?;
        crate::util::parse_mac(s.trim())
            .ok_or_else(|| Error::Net(format!("无法解析 MAC 地址: {s}")))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ifname;
        Err(Error::Net("该平台暂不支持自动获取 MAC，请通过后端提供".into()))
    }
}

/// 首个指定家族的地址
pub fn first_ip(ifname: &str, want_v4: bool) -> Result<IpAddr> {
    let list = iface_addrs(ifname)?;
    let want = if want_v4 {
        |a: &IpAddr| a.is_ipv4()
    } else {
        |a: &IpAddr| a.is_ipv6()
    };
    list.into_iter()
        .map(|x| x.ip)
        .find(want)
        .ok_or_else(|| Error::Net(format!("接口 {ifname} 上没有所需 IP")))
}

/// IPv4 默认网关（Linux: /proc/net/route）
pub fn default_gateway_v4(ifname: &str) -> Option<std::net::Ipv4Addr> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/net/route").ok()?;
        for line in content.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 3 {
                continue;
            }
            let name = f[0];
            if name != ifname {
                continue;
            }
            let dest = u32::from_str_radix(f[1], 16).ok()?;
            let gw = u32::from_str_radix(f[2], 16).ok()?;
            // dest==0 表示默认路由
            if dest == 0 {
                return Some(std::net::Ipv4Addr::from(gw.to_be()));
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ifname;
        None
    }
}

/// 默认路由所在的接口名（Linux: /proc/net/route，dest 为 0 的那行）
pub fn default_route_iface() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/net/route").ok()?;
        for line in content.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 2 && f[1] == "00000000" {
                return Some(f[0].to_string());
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// 读取 DNS 服务器列表（/etc/resolv.conf）
pub fn dns_list() -> Result<Vec<String>> {
    let content = std::fs::read_to_string("/etc/resolv.conf")
        .map_err(|e| Error::Net(format!("读取 /etc/resolv.conf 失败: {e}")))?;
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("nameserver") {
            let v = rest.trim();
            if !v.is_empty() {
                out.push(v.to_string());
            }
        }
    }
    Ok(out)
}

/// 获取主/次 DNS 地址（返回 IPv4/IPv6 均可的字符串列表）。
/// `fake` 指定时优先使用。
pub fn dns_primary_secondary(
    fake1: &Option<String>,
    fake2: &Option<String>,
) -> Result<(Option<String>, Option<String>)> {
    let list = dns_list()?;
    let d1 = fake1.clone().or_else(|| list.first().cloned());
    let d2 = fake2.clone().or_else(|| list.get(1).cloned());
    Ok((d1, d2))
}
