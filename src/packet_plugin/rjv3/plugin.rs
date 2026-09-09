//! 锐捷 v3/v4 认证插件实现（对应原版 `packet_plugin_rjv3*`）。
//!
//! 职责：把锐捷私有 trailer 附加到每个出站帧；处理二次认证（double auth）与
//! 心跳（keepalive）。
//!
//! **重要**：本插件已完全与认证核心解耦——它实现了 [`Plugin`]，构造时自包含
//! 配置与本机 MAC；收到 [`AuthEvent`] 后经 [`SessionHost`] 反驱核心（重开一轮 /
//! 进入心跳 / 等待 DHCP / 执行脚本）。认证核心（`runtime`）不引用本模块任何类型。
//!
//! 接口快照（IP/网关/DNS/IPv6/MAC）由 `Snapshot::gather()` 在需要时实时采集；
//! 采集不到的字段以 0/空回退并记录告警。

use std::net::IpAddr;

use crate::config::{BcastAddr, Config, DhcpType};
use crate::error::{Error, Result};
use crate::eth_frame::{
    EapCode, EapType, EapolType, Frame, BCAST_RJ, EAP_CODE, EAPOL_TYPE,
};
use crate::log_warn;
use crate::net::ifinfo;
use crate::packet_plugin::rjv3::hash::v4::{compute_pwd, compute_v4};
use crate::packet_plugin::rjv3::keepalive;
use crate::packet_plugin::rjv3::prop::*;
use crate::plugin::{AuthEvent, Plugin, SessionHost};
use crate::util::bit_reverse;

/// 私有 trailer 的“外部头”固定字节
const HDR_MAGIC: [u8; 4] = [0x00, 0x00, 0x13, 0x11];
const PROG_NAME: [u8; 32] = {
    let mut n = [0u8; 32];
    let s = b"8021x.exe";
    let mut i = 0;
    while i < s.len() {
        n[i] = s[i];
        i += 1;
    }
    n
};
const VERSION_BYTES: [u8; 4] = [0x01, 0x1f, 0x01, 0x02];

/// 一次采集得到的接口快照（供一次报文构造使用）。
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub ip4: Option<[u8; 4]>,
    pub mask4: Option<[u8; 4]>,
    pub gw4: Option<[u8; 4]>,
    /// 主 DNS（IPv4 点分，已解析）
    pub dns1: Option<[u8; 4]>,
    /// 次 DNS 原文（可为 IPv4/IPv6）
    pub sec_dns: String,
    pub ll_slaac: [u8; 16],
    pub ll_temp: [u8; 16],
    pub global: [u8; 16],
    pub hdd_serial: Vec<u8>,
}

fn dns_str_to_v4(s: &str) -> Option<[u8; 4]> {
    s.trim().parse::<std::net::Ipv4Addr>().ok().map(|x| x.octets())
}

/// 读取 Linux 根分区设备序列号（HDIO_GET_IDENTITY）
#[cfg(target_os = "linux")]
fn read_hdd_serial() -> Vec<u8> {
    let mtab = match std::fs::read_to_string("/etc/mtab") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut root_dev: Option<String> = None;
    for line in mtab.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 2 && f[1] == "/" {
            root_dev = Some(f[0].to_string());
        }
    }
    let Some(dev) = root_dev else {
        return Vec::new();
    };
    use std::os::unix::io::AsRawFd;
    let file = match std::fs::OpenOptions::new().read(true).open(&dev) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut ident = [0u8; 512];
    let ret = unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            // HDIO_GET_IDENTITY = 0x030d
            0x030d,
            ident.as_mut_ptr() as *mut libc::c_void,
        )
    };
    if ret != 0 {
        return Vec::new();
    }
    // struct hd_driveid.serial_no 位于偏移 20，长度 20
    let raw = &ident[20..40];
    let mut end = raw.len();
    while end > 0 && (raw[end - 1] == b' ' || raw[end - 1] == 0) {
        end -= 1;
    }
    raw[..end].to_vec()
}

#[cfg(not(target_os = "linux"))]
fn read_hdd_serial() -> Vec<u8> {
    Vec::new()
}

impl Snapshot {
    /// 采集（尽力而为）。`mac` 由上层传入（从后端获得）。
    pub fn gather(ifname: &str, cfg: &Config) -> Snapshot {
        let mut snap = Snapshot::default();

        // IPv4 / 掩码 / 网关
        if let Ok(addrs) = ifinfo::iface_addrs(ifname) {
            for a in &addrs {
                match a.ip {
                    IpAddr::V4(v4) => {
                        if snap.ip4.is_none() {
                            snap.ip4 = Some(v4.octets());
                            snap.mask4 = match &a.netmask {
                                Some(IpAddr::V4(m)) => Some(m.octets()),
                                _ => None,
                            };
                        }
                    }
                    IpAddr::V6(v6) => {
                        let b = v6.octets();
                        let top = b[0] & 0xf0;
                        if top == 0x20 {
                            // 2xxx:: 全局
                            snap.global = b;
                        } else if b[0] == 0xfe && b[1] == 0x80 {
                            if b[11] == 0xff && b[12] == 0xfe {
                                snap.ll_slaac = b;
                            } else {
                                snap.ll_temp = b;
                            }
                        }
                    }
                }
            }
        }
        if let Some(gw) = ifinfo::default_gateway_v4(ifname) {
            snap.gw4 = Some(gw.octets());
        }

        // DNS
        let (d1, d2) = ifinfo::dns_primary_secondary(&cfg.fake_dns1, &cfg.fake_dns2)
            .unwrap_or((None, None));
        snap.dns1 = d1.as_deref().and_then(dns_str_to_v4);
        snap.sec_dns = d2.unwrap_or_default();

        // 硬盘序列号
        snap.hdd_serial = match &cfg.fake_serial {
            Some(s) => s.as_bytes().to_vec(),
            None => read_hdd_serial(),
        };

        // 诊断告警
        if snap.ip4.is_none() {
            log_warn!("无法获取接口 {ifname} 的 IPv4 地址");
        }
        if snap.gw4.is_none() {
            log_warn!("无法获取接口 {ifname} 的 IPv4 网关");
        }
        if snap.dns1.is_none() && cfg.fake_dns1.is_none() {
            log_warn!("无法获取主 DNS，若认证失败请用 --fake-dns1 指定");
        }
        snap
    }
}

/// 二次认证 / DHCP 判断需要的最小租约信息是否齐备
fn lease_ready(s: &Snapshot) -> bool {
    s.ip4.is_some() && s.gw4.is_some() && s.dns1.is_some()
}

/// 某帧是否是 MD5-Challenge（用于判断取种子）
pub fn is_md5_challenge(f: &Frame) -> bool {
    f.buf.len() >= 23
        && f.buf[EAPOL_TYPE] == EapolType::EapPacket as u8
        && f.buf[23 - 1] == EapType::Md5Challenge as u8
}

/// 取出 MD5 挑战种子（16 字节），非 MD5 帧返回零。
fn challenge_seed_or_zero(f: &Frame) -> [u8; 16] {
    let mut seed = [0u8; 16];
    if is_md5_challenge(f) && f.buf.len() >= 40 {
        seed.copy_from_slice(&f.buf[24..40]);
    }
    seed
}

/// rjv3 插件状态
pub struct Rjv3 {
    /// 自包含的运行配置（用户名/密码/接口/服务名/二次认证等）。
    pub cfg: Config,
    /// 本机 MAC。
    pub mac: [u8; 6],
    pub bcast_addr: BcastAddr,
    pub dhcp_type: DhcpType,
    pub max_dhcp_count: usize,
    // 动态状态
    pub succ_count: usize,
    pub dhcp_count: usize,
    pub last_recv: Option<Frame>,
    pub duplicated: Option<Frame>,
    // 心跳状态
    pub echokey: u32,
    pub echono: u32,
    pub ka_dest: [u8; 6],
    // 用户自定义字段
    pub prop_add: Vec<RjProp>,
    pub prop_mod: Vec<RjProp>,
}

impl Rjv3 {
    /// 由注册表构造：插件自包含配置与本机 MAC。
    pub fn new(cfg: &Config, mac: [u8; 6]) -> Rjv3 {
        let mut add = Vec::new();
        let mut mods = Vec::new();
        for o in &cfg.rj_options {
            let p = RjProp::new(o.prop_type, o.content.clone());
            if o.replace {
                mods.push(p);
            } else {
                add.push(p);
            }
        }
        Rjv3 {
            cfg: cfg.clone(),
            mac,
            bcast_addr: cfg.eap_bcast_addr,
            dhcp_type: cfg.dhcp_type,
            max_dhcp_count: cfg.max_dhcp_count,
            succ_count: 0,
            dhcp_count: 0,
            last_recv: None,
            duplicated: None,
            echokey: 0,
            echono: 0,
            ka_dest: [0; 6],
            prop_add: add,
            prop_mod: mods,
        }
    }

    pub fn reset(&mut self) {
        self.succ_count = 0;
        self.dhcp_count = 0;
        self.duplicated = None;
        self.echokey = 0;
        self.echono = 0;
        self.ka_dest = [0; 6];
    }

    /// 每次收到帧时调用：更新 last_recv（供后续 prepare 取种子）。
    pub fn on_frame_received(&mut self, frame: &Frame) {
        self.last_recv = Some(frame.clone());
    }

    // ---------- 协议流程（由 on_auth_event 调用）----------

    /// 处理一次成功。根据 DHCP 类型决定二次认证 / 心跳。
    fn handle_success(&mut self, frame: &Frame, host: &mut dyn SessionHost) {
        self.succ_count += 1;
        match self.dhcp_type {
            DhcpType::DoubleAuth => {
                if self.succ_count < 2 {
                    crate::log_info!("首次认证成功，正在执行 DHCP 脚本以准备第二次认证");
                    self.duplicated = Some(frame.clone());
                    host.run_script(&self.cfg.dhcp_script);
                    self.maybe_start_second_auth(host);
                } else {
                    crate::log_info!("二次认证成功");
                    self.succ_count = 0;
                    self.dhcp_count = 0;
                    self.duplicated = None;
                    self.finish_success(frame, host);
                }
            }
            DhcpType::AfterAuth => {
                host.run_script(&self.cfg.dhcp_script);
                self.finish_success(frame, host);
            }
            _ => self.finish_success(frame, host),
        }
    }

    /// 成功收尾：解析 echo key，进入心跳。
    fn finish_success(&mut self, frame: &Frame, host: &mut dyn SessionHost) {
        if let Err(e) = self.process_result(frame) {
            crate::log_warn!("{e}");
        }
        host.set_keepalive(self.cfg.heartbeat_interval);
    }

    /// DHCP 就绪则重开第二轮；否则安排重试或放弃进入心跳。
    fn maybe_start_second_auth(&mut self, host: &mut dyn SessionHost) {
        let snap = Snapshot::gather(&self.cfg.ifname, &self.cfg);
        if lease_ready(&snap) {
            crate::log_info!("DHCP 完成，正在开始第二次认证");
            self.duplicated = None;
            host.restart_round();
        } else {
            self.dhcp_count += 1;
            if self.dhcp_count > self.max_dhcp_count {
                // 放弃二次认证，直接心跳（用缓存的成功包解析 echo key）
                if let Some(dup) = self.duplicated.clone() {
                    let _ = self.process_result(&dup);
                }
                self.duplicated = None;
                crate::log_warn!("无法获取 IPv4 地址等信息，将不会进行第二次认证而直接开始心跳");
                host.set_keepalive(self.cfg.heartbeat_interval);
            } else {
                crate::log_warn!("DHCP 可能尚未完成，将继续等待……");
                host.dhcp_retry();
            }
        }
    }

    /// 心跳计时到点：构造并发送心跳帧。
    fn send_keepalive(&mut self, host: &mut dyn SessionHost) {
        if self.echokey == 0 && self.ka_dest == [0u8; 6] {
            return; // 尚未获得 echo key，无法心跳
        }
        let f = keepalive::build_keepalive_frame(
            self.echokey,
            &mut self.echono,
            &self.ka_dest,
            &self.mac,
        );
        if let Err(e) = host.send(&f) {
            crate::log_error!("心跳包发送失败: {e}");
        }
    }

    /// 是否应填充 DHCP 租约字段（私有头里的 lease）
    fn should_fill_dhcp(&self) -> bool {
        self.dhcp_type == DhcpType::BeforeAuth
            || (self.dhcp_type == DhcpType::DoubleAuth && self.succ_count >= 2)
    }

    /// 插件入口：对出站帧追加全部私有字段（先自行采集接口快照）。
    pub fn prepare(&mut self, frame: &mut Frame) {
        let snap = Snapshot::gather(&self.cfg.ifname, &self.cfg);
        self.prepare_with(frame, &snap);
    }

    /// 使用已采集快照追加全部私有字段。
    fn prepare_with(&mut self, frame: &mut Frame, snap: &Snapshot) {
        // 1. Start 目的地址（锐捷私有广播）
        if self.bcast_addr == BcastAddr::Rj && frame.buf.len() >= EAPOL_TYPE + 1 {
            let is_start = frame.buf[EAPOL_TYPE] == EapolType::Start as u8;
            if is_start {
                frame.buf[0..6].copy_from_slice(&BCAST_RJ);
            }
        }

        // 2. 私有头
        self.append_priv_header(frame, snap);

        // 3. 构造字段（append_pwd 表示出站帧是否为 MD5 challenge 应答）
        let append_pwd = is_md5_challenge(frame);
        let mut props = self.build_common_fields(snap, &self.mac, &self.cfg, append_pwd);

        // 4. 应用修改
        for m in &self.prop_mod {
            if let Some(p) = find_prop_mut(&mut props, m.prop_type) {
                p.content = m.content.clone();
            }
        }

        // 5. 序列化 + 追加用户字段
        let mut buf = props_to_buffer(&props);
        for p in &self.prop_add {
            p.write_to(&mut buf);
        }

        // 6. 外部头（logoffReason/magic/len）
        let props_len = buf.len();
        let mut header = [0u8; 7];
        header[3..7].copy_from_slice(&HDR_MAGIC);
        header[5] = ((props_len >> 8) & 0xff) as u8;
        header[6] = (props_len & 0xff) as u8;
        frame.buf.extend_from_slice(&header);
        frame.buf.extend_from_slice(&buf);
    }

    /// 私有 trailer 头：rj 编码的 DHCP 信息 + magic + 程序名 + 版本。
    fn append_priv_header(&mut self, frame: &mut Frame, snap: &Snapshot) {
        // DHCP_INFO_PROP: magic(4)+dhcp_enabled(1)+lease(16)+crc16(2) = 23
        let mut dhcp_prop = [0u8; 23];
        dhcp_prop[0..4].copy_from_slice(&HDR_MAGIC);
        dhcp_prop[4] = (self.dhcp_type != DhcpType::None) as u8;

        if self.should_fill_dhcp() {
            // lease: ip(4) netmask(4) gateway(4) dns(4)
            if let (Some(ip), Some(mask), Some(gw), Some(dns)) =
                (snap.ip4, snap.mask4, snap.gw4, snap.dns1)
            {
                dhcp_prop[5..9].copy_from_slice(&ip);
                dhcp_prop[9..13].copy_from_slice(&mask);
                dhcp_prop[13..17].copy_from_slice(&gw);
                dhcp_prop[17..21].copy_from_slice(&dns);
            }
        }
        // crc16 作用前 21 字节，大端存放于 21..23
        let crc = crate::packet_plugin::rjv3::hash::crc16::crc16(&dhcp_prop[..21]);
        dhcp_prop[21..23].copy_from_slice(&crc.to_be_bytes());
        // rj 位交换编码
        crate::packet_plugin::rjv3::hash::rjencode::rj_encode(&mut dhcp_prop);
        frame.buf.extend_from_slice(&dhcp_prop);

        frame.buf.extend_from_slice(&HDR_MAGIC);
        frame.buf.extend_from_slice(&PROG_NAME);
        frame.buf.extend_from_slice(&VERSION_BYTES);
    }

    /// 构造常见字段（顺序与 C 一致）
    fn build_common_fields(
        &self,
        snap: &Snapshot,
        mac: &[u8; 6],
        cfg: &Config,
        append_pwd: bool,
    ) -> Vec<RjProp> {
        let mut props = Vec::new();

        // DHCP 开关 4 字节（{0,0,0, enabled}）
        let mut dhcp_en = [0u8; SIZE_DHCP];
        dhcp_en[3] = (self.dhcp_type != DhcpType::None) as u8;
        append_prop(&mut props, TYPE_DHCP, dhcp_en.to_vec());

        append_prop(&mut props, TYPE_MAC, mac.to_vec());

        // PWD_HASH：种子取自已收到的挑战
        let seed = match &self.last_recv {
            Some(f) => challenge_seed_or_zero(f),
            None => [0u8; 16],
        };
        if append_pwd {
            let h = compute_pwd(&cfg.username, &cfg.password, &seed);
            append_prop(&mut props, TYPE_PWD_HASH, h.to_vec());
        } else {
            append_prop(&mut props, TYPE_PWD_HASH, Vec::new());
        }

        // 次 DNS（ASCII）
        append_prop(
            &mut props,
            TYPE_SEC_DNS,
            snap.sec_dns.as_bytes().to_vec(),
        );

        append_prop(&mut props, TYPE_MISC_2, vec![0x01]);
        append_prop(&mut props, TYPE_LL_IPV6, snap.ll_slaac.to_vec());
        append_prop(&mut props, TYPE_LL_IPV6_T, snap.ll_temp.to_vec());
        append_prop(&mut props, TYPE_GLB_IPV6, snap.global.to_vec());

        // V3_HASH（computeV4）
        let v4out = compute_v4(&seed).unwrap_or([0u8; 128]);
        append_prop(&mut props, TYPE_V3_HASH, v4out.to_vec());

        // SERVICE：32 字节定长
        let mut svc = [0u8; SIZE_SERVICE];
        let sb = cfg.service_name.as_bytes();
        let n = sb.len().min(SIZE_SERVICE);
        svc[..n].copy_from_slice(&sb[..n]);
        append_prop(&mut props, TYPE_SERVICE, svc.to_vec());

        // HDD_SER：64 字节定长
        let mut hdd = [0u8; SIZE_HDD_SER];
        let hb = &snap.hdd_serial;
        let n = hb.len().min(SIZE_HDD_SER);
        hdd[..n].copy_from_slice(&hb[..n]);
        append_prop(&mut props, TYPE_HDD_SER, hdd.to_vec());

        append_prop(&mut props, TYPE_MISC_6, Vec::new());
        append_prop(&mut props, TYPE_MISC_7, vec![0x00]);
        append_prop(&mut props, TYPE_OS_BITS, vec![0x40]);

        // VER_STR：内容 = 字符串 + 0 结尾
        let mut v = cfg.ver_str.as_bytes().to_vec();
        v.push(0);
        append_prop(&mut props, TYPE_VER_STR, v);

        props
    }

    /// 处理服务器结果（成功/失败帧尾部）：打印服务器通知、解析 echo key。
    /// 返回是否成功找到 echo key（成功时需返回 true）。
    pub fn process_result(&mut self, frame: &Frame) -> Result<()> {
        let buf = &frame.buf;
        if buf.len() < 22 {
            return Ok(());
        }
        let region = &buf[22..];
        let props = parse_bare(region)?;

        // 第一条属性内容视为“服务器通知”（GBK）
        if let Some(first) = props.first() {
            if !first.content.is_empty() {
                gbk_log("服务器通知", &first.content);
            }
        }

        let is_success = buf.len() >= EAP_CODE + 1 && buf[EAP_CODE] == EapCode::Success as u8;
        if is_success {
            if let Some(acc) = find_prop(&props, TYPE_ACCOUNTING_MSG) {
                if !acc.content.is_empty() {
                    gbk_log("计费通知", &acc.content);
                }
            }
            // echo key 属性：type==1 且内容非空
            if let Some(ek) = props.iter().find(|p| p.prop_type == TYPE_ECHOKEY && !p.content.is_empty())
            {
                if ek.content.len() >= 10 {
                    let c = &ek.content;
                    let mut key = 0u32;
                    key |= (bit_reverse(!c[6]) as u32) << 24;
                    key |= (bit_reverse(!c[7]) as u32) << 16;
                    key |= (bit_reverse(!c[8]) as u32) << 8;
                    key |= bit_reverse(!c[9]) as u32;
                    self.echokey = key;
                    self.ka_dest = frame.src_mac().unwrap_or([0; 6]);
                    return Ok(());
                }
            }
            // 找不到 echo key
            return Err(Error::Proto("无法找到 echo key 的位置，将不能进行心跳".into()));
        }
        Ok(())
    }
}

/// GBK → UTF-8 后打印（用 encoding_rs）
pub fn gbk_log(prefix: &str, bytes: &[u8]) {
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    crate::log_info!("{prefix}:\n{decoded}");
}

// ============ 把 rjv3 实现为公开的通用 Plugin ============
//
// 认证核心不再认识 Rjv3 类型：它只是 PluginManager 里一个名叫 "rjv3" 的插件。
// 私有算法需要的所有配置与 MAC 都在构造时自包含；二次认证 / DHCP / 心跳等
// “反驱核心”的请求通过 SessionHost 发出，由核心统一执行。

impl Plugin for Rjv3 {
    fn name(&self) -> &str {
        "rjv3"
    }
    fn description(&self) -> &str {
        "锐捷 v3/v4 私有认证算法插件"
    }
    fn version(&self) -> &str {
        "0.1"
    }

    /// 出站帧发送前：追加锐捷私有 trailer。
    fn prepare(&mut self, frame: &mut Frame) {
        // 调用同名固有方法（避免与 trait 方法形成递归）
        Rjv3::prepare(self, frame);
    }

    fn on_received(&mut self, frame: &Frame) {
        self.on_frame_received(frame);
    }

    fn on_auth_event(&mut self, ev: &AuthEvent, host: &mut dyn SessionHost) {
        match ev {
            AuthEvent::Success(frame) => self.handle_success(frame, host),
            AuthEvent::Failure(_frame) => {
                // 认证失败/被踢下线：清空本插件状态（核心负责重连策略）
                self.reset();
            }
            AuthEvent::KeepaliveTick => self.send_keepalive(host),
            AuthEvent::DhcpRetryTick => self.maybe_start_second_auth(host),
        }
    }

    fn reset(&mut self) {
        Rjv3::reset(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{AuthEvent, Plugin, SessionHost};

    /// 记录插件通过 SessionHost 提出的请求
    struct Recorder {
        retry: bool,
        restart: bool,
        ka: Option<u64>,
        sent: usize,
    }
    impl SessionHost for Recorder {
        fn send(&mut self, _f: &Frame) -> Result<()> {
            self.sent += 1;
            Ok(())
        }
        fn restart_round(&mut self) {
            self.restart = true;
        }
        fn set_keepalive(&mut self, i: u64) {
            self.ka = Some(i);
        }
        fn dhcp_retry(&mut self) {
            self.retry = true;
        }
        fn run_script(&mut self, _c: &str) {}
    }

    fn success_frame() -> Frame {
        let mut buf = vec![0u8; 23];
        buf[15] = EapolType::EapPacket as u8;
        buf[18] = EapCode::Success as u8;
        Frame { buf }
    }

    #[test]
    fn double_auth_first_success_waits_dhcp() {
        // rjv3 作为插件：第一次成功后，若 DHCP 未就绪（lo 无网关/主DNS），
        // 应向核心请求 dhcp_retry 而非 restart/keepalive。
        let mut cfg = Config::default();
        cfg.ifname = "lo".into();
        cfg.dhcp_type = DhcpType::DoubleAuth;
        cfg.dhcp_script = String::new();
        cfg.username = "u".into();
        cfg.password = "p".into();
        let mut rjv3 = Rjv3::new(&cfg, [0x11; 6]);

        let mut rec = Recorder {
            retry: false,
            restart: false,
            ka: None,
            sent: 0,
        };
        let f = success_frame();
        rjv3.on_auth_event(&AuthEvent::Success(&f), &mut rec);

        // lo 上通常拿不到“网关+主DNS”都齐的租约 → 走 dhcp_retry
        assert!(rec.retry, "第一次成功且 DHCP 未就绪应请求 dhcp_retry");
        assert!(!rec.restart && rec.ka.is_none());
        assert_eq!(rec.sent, 0);
    }
}

