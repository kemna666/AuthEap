//! 网络测试（`autheap --nettest [HOST[:PORT]]`）。
//!
//! 给 LuCI「状态」页的“网络测试”用的诊断工具，输出人类可读的纯文本报告：
//!
//! - 【本机】接口 / MAC / IPv4 / 掩码 / 默认网关 / DNS；
//! - 【目标】DNS 解析、TCP 连接时延（默认端口 80，可写 `host:端口`）；
//! - 【ICMP】对默认网关与目标做 ping（需要 root / `CAP_NET_RAW`，失败会如实说明并跳过）；
//! - 【结论】是否有明显问题。
//!
//! 退出码：0 = 未发现明显问题；1 = 有失败项（供 LuCI 显示颜色/提示）。
//! 说明：本工具只做只读诊断，不修改任何配置，也不发送认证报文。

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::net::ifinfo;
use crate::util::mac_to_string;

/// TCP 连接测试超时（单个地址）
const TCP_TIMEOUT_SECS: u64 = 3;
/// 每个目标的 ICMP **总预算**（含该目标所有探测包，避免整体耗时太长）
const PING_BUDGET_MS: u64 = 2000;
/// 单个探测包的最大等待
const PING_PROBE_WAIT_MS: u64 = 800;
/// ICMP 发包次数
const PING_COUNT: u32 = 3;

struct FdGuard(libc::c_int);

impl Drop for FdGuard {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

/// 运行网络测试并打印报告；返回退出码（0 = 无明显问题，1 = 有失败项）。
pub fn run(ifname: &str, target: &str, default_port: u16) -> i32 {
    let mut problems = 0usize;

    println!("== AuthEAP 网络测试 ==");
    println!();

    // ---------------- 本机 ----------------
    println!("[本机]");
    println!("  接口      : {ifname}");
    match ifinfo::iface_mac(ifname) {
        Ok(m) => println!("  MAC       : {}", mac_to_string(&m)),
        Err(e) => {
            println!("  MAC       : ✘ 读取失败（{e}）");
            problems += 1;
        }
    }

    let mut have_v4 = false;
    match ifinfo::iface_addrs(ifname) {
        Ok(list) => {
            let v4: Vec<String> = list
                .iter()
                .filter(|a| a.ip.is_ipv4())
                .map(|a| a.ip.to_string())
                .collect();
            let v6: Vec<String> = list
                .iter()
                .filter(|a| a.ip.is_ipv6())
                .map(|a| a.ip.to_string())
                .collect();
            if !v4.is_empty() {
                have_v4 = true;
                println!("  IPv4      : {}", v4.join(", "));
            }
            if !v6.is_empty() {
                println!("  IPv6      : {}", v6.join(", "));
            }
            if v4.is_empty() && v6.is_empty() {
                println!("  IP        : ✘ 接口上没有地址（未获取到 DHCP？）");
                problems += 1;
            } else if let Some(a) = list.iter().find(|a| a.ip.is_ipv4()) {
                if let Some(m) = &a.netmask {
                    println!("  子网掩码  : {m}");
                }
            }
        }
        Err(e) => {
            println!("  IP        : ✘ 查询失败（{e}）");
            problems += 1;
        }
    }

    let gw = ifinfo::default_gateway_v4(ifname);
    println!(
        "  默认网关  : {}",
        gw.map(|g| g.to_string()).unwrap_or_else(|| "（无）".to_string())
    );
    match ifinfo::dns_list() {
        Ok(d) if !d.is_empty() => println!("  DNS       : {}", d.join(", ")),
        _ => println!("  DNS       : （未配置）"),
    }
    println!();

    // ---------------- 目标 ----------------
    let (host, port) = split_target(target, default_port);
    let mut target_v4: Option<Ipv4Addr> = None;
    if let Some(h) = host.as_deref() {
        println!("[目标] {h}:{port}");
        let t0 = Instant::now();
        match (h, port).to_socket_addrs() {
            Ok(iter) => {
                let addrs: Vec<SocketAddr> = iter.collect();
                let ms = t0.elapsed().as_millis();
                if addrs.is_empty() {
                    println!("  DNS 解析  : ✘ 没有解析到地址");
                    problems += 1;
                } else {
                    let shown: Vec<String> = addrs.iter().take(4).map(|a| a.to_string()).collect();
                    println!(
                        "  DNS 解析  : ✔ {} 个地址（{ms} ms）：{}",
                        addrs.len(),
                        shown.join(", ")
                    );
                }

                // TCP 连接测试（优先 IPv4，其次任意可用的地址）
                let chosen = addrs
                    .iter()
                    .find(|a| a.is_ipv4())
                    .or_else(|| addrs.first())
                    .copied();
                if let Some(addr) = chosen {
                    if let SocketAddr::V4(v4) = addr {
                        target_v4 = Some(*v4.ip());
                    }
                    let t1 = Instant::now();
                    match TcpStream::connect_timeout(&addr, Duration::from_secs(TCP_TIMEOUT_SECS)) {
                        Ok(_) => println!(
                            "  TCP 连接  : ✔ {addr} 可连接（{} ms）",
                            t1.elapsed().as_millis()
                        ),
                        Err(e) => {
                            println!("  TCP 连接  : ✘ {addr} 连接失败：{e}");
                            problems += 1;
                        }
                    }
                }
            }
            Err(e) => {
                println!("  DNS 解析  : ✘ 失败：{e}");
                problems += 1;
            }
        }
        println!();
    }

    // ---------------- ICMP ----------------
    println!("[ICMP ping]");
    if !have_v4 {
        println!("  跳过：本机没有 IPv4 地址");
    } else {
        match gw {
            Some(g) => ping_report("  网关 ", IpAddr::V4(g), PING_COUNT, &mut problems),
            None => println!("  网关 : 跳过（没有默认网关）"),
        }
        if let Some(ip) = target_v4 {
            ping_report("  目标 ", IpAddr::V4(ip), PING_COUNT, &mut problems);
        } else if host.is_some() {
            println!("  目标 : 跳过（未解析到 IPv4 地址）");
        }
    }
    println!();

    // ---------------- 结论 ----------------
    if problems == 0 {
        println!("[结论] ✔ 未发现明显问题");
        0
    } else {
        println!("[结论] ✘ 发现 {problems} 处异常（详见上面标 ✘ 的行）");
        1
    }
}

fn ping_report(label: &str, ip: IpAddr, count: u32, problems: &mut usize) {
    match ping(ip, count, Duration::from_millis(PING_BUDGET_MS)) {
        Ok(results) => {
            let ok = results.iter().filter(|r| r.is_some()).count();
            let times: Vec<f64> = results
                .iter()
                .flatten()
                .map(|d| d.as_secs_f64() * 1000.0)
                .collect();
            if ok == 0 {
                println!("{label} {ip}: ✘ 不通（{count} 个包全部超时）");
                *problems += 1;
            } else {
                let avg = times.iter().sum::<f64>() / times.len() as f64;
                let min = times.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                println!(
                    "{label} {ip}: ✔ {ok}/{count}，时延 min/avg/max = {min:.1}/{avg:.1}/{max:.1} ms"
                );
            }
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::PermissionDenied {
                println!("{label} {ip}: 跳过（需要 root / CAP_NET_RAW 权限）");
            } else {
                println!("{label} {ip}: 跳过（{e}）");
            }
        }
    }
}

/// 拆分 `host[:port]`；空字符串表示没有目标。
fn split_target(target: &str, default_port: u16) -> (Option<String>, u16) {
    let t = target.trim();
    if t.is_empty() {
        return (None, default_port);
    }
    // [v6]:port / host:port
    if let Some(rest) = t.strip_prefix('[') {
        if let Some((h, p)) = rest.split_once("]:") {
            if let Ok(port) = p.parse::<u16>() {
                return (Some(h.to_string()), port);
            }
        }
        return (Some(rest.trim_end_matches(']').to_string()), default_port);
    }
    if t.matches(':').count() == 1 {
        if let Some((h, p)) = t.split_once(':') {
            if let Ok(port) = p.parse::<u16>() {
                return (Some(h.to_string()), port);
            }
        }
    }
    (Some(t.to_string()), default_port)
}

/// 发送 ICMP Echo 并收集每个包的往返时延（`None` = 超时）。仅支持 IPv4。
///
/// `budget` 是**整个目标的总耗时上限**：单包最多等 `PING_PROBE_WAIT_MS`，
/// 预算用尽后剩余的包直接记为超时，保证这个函数最慢 `budget` 左右返回。
fn ping(ip: IpAddr, count: u32, budget: Duration) -> io::Result<Vec<Option<Duration>>> {
    let IpAddr::V4(v4) = ip else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "仅支持 IPv4 的 ICMP 测试",
        ));
    };

    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_ICMP);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let _guard = FdGuard(fd);

        // 接收超时：取小值，循环里用总预算做真正截止（避免一次 recv 就吃掉全部时间）
        let probe_wait = Duration::from_millis(PING_PROBE_WAIT_MS).min(budget);
        let tv = libc::timeval {
            tv_sec: probe_wait.as_secs() as libc::time_t,
            tv_usec: probe_wait.subsec_micros() as libc::suseconds_t,
        };
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );

        let dst = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: 0,
            sin_addr: libc::in_addr {
                s_addr: u32::from(v4).to_be(),
            },
            sin_zero: [0; 8],
        };

        let id = (libc::getpid() as u16) & 0xffff;
        let payload: Vec<u8> = (0..32u8).map(|i| i.wrapping_add(0x40)).collect();
        let mut results = Vec::with_capacity(count as usize);
        let hard_deadline = Instant::now() + budget;

        for seq in 0..count {
            if Instant::now() >= hard_deadline {
                results.push(None); // 总预算已用尽，剩余包记为超时
                continue;
            }
            let seq16 = seq as u16;
            let mut pkt = vec![
                8u8,
                0,
                0,
                0,
                (id >> 8) as u8,
                (id & 0xff) as u8,
                (seq16 >> 8) as u8,
                (seq16 & 0xff) as u8,
            ];
            pkt.extend_from_slice(&payload);
            let ck = checksum(&pkt);
            pkt[2] = (ck >> 8) as u8;
            pkt[3] = (ck & 0xff) as u8;

            let t0 = Instant::now();
            let sent = libc::sendto(
                fd,
                pkt.as_ptr() as *const libc::c_void,
                pkt.len(),
                0,
                &dst as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            );
            if sent < 0 {
                results.push(None);
                continue;
            }

            let deadline = (Instant::now() + probe_wait).min(hard_deadline);
            let mut got = None;
            let mut buf = [0u8; 1500];
            while Instant::now() < deadline {
                let n = libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0);
                if n < 0 {
                    match io::Error::last_os_error().kind() {
                        io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted => continue, // 到点前继续等
                        _ => break,
                    }
                }
                let n = n as usize;
                if n < 20 + 8 {
                    continue;
                }
                let ihl = ((buf[0] & 0x0f) as usize) * 4;
                if n < ihl + 8 || buf[ihl] != 0 {
                    continue; // 只看 Echo Reply
                }
                let rid = u16::from_be_bytes([buf[ihl + 4], buf[ihl + 5]]);
                let rseq = u16::from_be_bytes([buf[ihl + 6], buf[ihl + 7]]);
                if rid == id && rseq == seq16 {
                    got = Some(t0.elapsed());
                    break;
                }
            }
            results.push(got);
        }

        Ok(results)
    }
}

/// 标准 16 位反码校验和（用于 ICMP 头）。
fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_target_forms() {
        assert_eq!(split_target("", 80), (None, 80));
        assert_eq!(
            split_target("203.0.113.10", 8083),
            (Some("203.0.113.10".into()), 8083)
        );
        assert_eq!(
            split_target("203.0.113.10:443", 8083),
            (Some("203.0.113.10".into()), 443)
        );
        assert_eq!(
            split_target("example.com:8083", 80),
            (Some("example.com".into()), 8083)
        );
        // 非法端口 → 当作普通主机名
        assert_eq!(
            split_target("host:notaport", 80),
            (Some("host:notaport".into()), 80)
        );
    }

    #[test]
    fn checksum_matches_known_value() {
        // 一个 ICMP echo 头（校验和字段为 0）的经典结果
        let pkt = [
            8u8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
        ];
        let ck = checksum(&pkt);
        // 把校验和写回后再算一次应为 0
        let mut pkt2 = pkt;
        pkt2[2] = (ck >> 8) as u8;
        pkt2[3] = (ck & 0xff) as u8;
        assert_eq!(checksum(&pkt2), 0);
    }

    #[test]
    fn run_without_target_is_diagnostic_only() {
        // 用回环接口跑一遍：只验证不 panic 且能输出（退出码 0/1 都算正常）
        let code = run("lo", "", 80);
        assert!(code == 0 || code == 1);
    }
}
