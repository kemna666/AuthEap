//! 认证运行时：**通用**状态机、插件装配、定时器与事件循环。
//!
//! 本体只认识 [`crate::plugin::Plugin`] 接口，不认识任何具体认证算法
//! （rjv3 锐捷认证也只是插件之一）。私有算法通过 [`Plugin::on_auth_event`]
//! 收到成功/失败/心跳/二次认证重试事件，再经 [`SessionHost`] 反向请求本体：
//! 重开一轮、进入心跳、等待 DHCP、执行脚本或直接发包。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::Result;
use crate::eth_frame::{EapCode, EapolType, Frame, BCAST_STANDARD, FRAME_HDR_NO_TYPE};
use crate::net::Io;
use crate::packet_builder;
use crate::plugin::{AuthEvent, PluginManager, SessionHost};
use crate::plugins;
use crate::util::Prng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerKind {
    /// 阶段超时看门狗：重发上次报文
    Watchdog,
    /// 等待后重启认证（失败重试 / 掉线重连）
    RestartAuth,
    /// 心跳计时
    Keepalive,
    /// 二次认证等待 DHCP 的重试
    DhcpRetry,
}

#[derive(Debug, Clone, Copy)]
struct Timer {
    at: Instant,
    kind: TimerKind,
}

/// 认证应用（通用核心）
pub struct App {
    pub cfg: Config,
    pub io: Box<dyn Io>,
    pub mac: [u8; 6],
    pub ifname: String,
    /// 插件管线（rjv3 / printer / stats / 用户插件…… 一视同仁）
    pub plugins: PluginManager,

    // 状态机
    server_mac: [u8; 6],
    last_sent: Option<Frame>,
    retry_count: usize,
    fail_count: usize,
    authed: bool,
    keepalive_running: bool,
    /// 插件请求的心跳间隔（秒）
    keepalive_interval: u64,
    /// 是否处于“等待 DHCP 以进行二次认证”的中间状态（此间不启用阶段看门狗）
    dhcp_waiting: bool,
    timers: Vec<Timer>,
    _prng: Prng,

    /// 关闭标志（用于信号退出）
    pub shutdown: Arc<AtomicBool>,
    /// 测试用：处理完多少出站帧后退出事件循环
    pub stop_after_sent: Option<usize>,
}

/// 供插件反向驱动核心的会话宿主实现。
///
/// `send` 立即发包；其余请求先记录在标志位里，dispatch 结束后由 `App` 统一执行
/// （重开一轮 / 进入心跳 / 安排 DHCP 重试）。
struct SessionHostImpl<'a> {
    io: &'a dyn Io,
    restart: bool,
    keepalive: Option<u64>,
    dhcp_retry: bool,
}

impl<'a> SessionHostImpl<'a> {
    fn new(io: &'a dyn Io) -> SessionHostImpl<'a> {
        SessionHostImpl {
            io,
            restart: false,
            keepalive: None,
            dhcp_retry: false,
        }
    }
}

impl SessionHost for SessionHostImpl<'_> {
    fn send(&mut self, frame: &Frame) -> Result<()> {
        self.io.send_frame(frame)
    }

    fn restart_round(&mut self) {
        self.restart = true;
    }

    fn set_keepalive(&mut self, interval_secs: u64) {
        self.keepalive = Some(interval_secs);
    }

    fn dhcp_retry(&mut self) {
        self.dhcp_retry = true;
    }

    fn run_script(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        crate::log_info!("执行 DHCP 脚本: {cmd}");
        let _ = std::process::Command::new("sh").arg("-c").arg(cmd).status();
    }
}

impl App {
    /// 构建应用。`io` 提供网络；插件按 `cfg.plugins` 从注册表创建。
    pub fn new(cfg: Config, io: Box<dyn Io>) -> Result<App> {
        let mac = io.local_mac();
        let ifname = io.ifname().to_string();
        let plugins = plugins::build_manager(&cfg, mac, &cfg.plugins);
        Ok(App {
            cfg,
            io,
            mac,
            ifname,
            plugins,
            server_mac: BCAST_STANDARD,
            last_sent: None,
            retry_count: 0,
            fail_count: 0,
            authed: false,
            keepalive_running: false,
            keepalive_interval: 0,
            dhcp_waiting: false,
            timers: Vec::new(),
            _prng: Prng::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            stop_after_sent: None,
        })
    }

    fn schedule(&mut self, kind: TimerKind, after: Duration) {
        self.timers.push(Timer {
            at: Instant::now() + after,
            kind,
        });
    }

    fn cancel_kind(&mut self, kind: TimerKind) {
        self.timers.retain(|t| t.kind != kind);
    }

    fn cancel_watchdog(&mut self) {
        self.cancel_kind(TimerKind::Watchdog);
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.timers.iter().map(|t| t.at).min()
    }

    /// 把当前认证状态写入状态文件（供 LuCI 状态页读取；失败静默忽略）
    fn write_auth_state(&self, state: &str) {
        let _ = std::fs::write("/var/run/autheap-client.auth", state);
    }

    /// 发送帧（先按顺序交给插件 prepare）
    fn send_frame(&mut self, frame: &mut Frame) -> Result<()> {
        self.plugins.prepare_all(frame);
        self.io.send_frame(frame)
    }

    /// 清理并开始一轮新认证（发送 EAPOL-Start）
    pub fn start(&mut self) -> Result<()> {
        self.authed = false;
        self.write_auth_state("connecting");
        self.keepalive_running = false;
        self.dhcp_waiting = false;
        self.cancel_kind(TimerKind::Watchdog);
        self.cancel_kind(TimerKind::Keepalive);
        self.cancel_kind(TimerKind::DhcpRetry);
        self.cancel_kind(TimerKind::RestartAuth);
        self.retry_count = 0;
        self.plugins.reset_all();
        crate::log_info!("正在查找认证服务器……");
        let frame = packet_builder::start_frame(&self.server_mac, &self.mac);
        self.send_and_watchdog(frame)
    }

    /// 发送一帧并启动阶段看门狗
    fn send_and_watchdog(&mut self, mut frame: Frame) -> Result<()> {
        self.last_sent = Some(frame.clone());
        self.send_frame(&mut frame)?;
        self.arm_watchdog();
        Ok(())
    }

    fn arm_watchdog(&mut self) {
        self.cancel_watchdog();
        self.schedule(TimerKind::Watchdog, Duration::from_secs(self.cfg.stage_timeout));
    }

    // ---------- 事件分发 ----------

    /// 把协议事件广播给所有插件；`settle=true` 时统一执行插件提出的请求。
    fn dispatch(&mut self, ev: &AuthEvent, settle: bool) {
        let io: &dyn Io = &*self.io;
        let mut host = SessionHostImpl::new(io);
        self.plugins.on_auth_event_all(ev, &mut host);
        let req = (host.restart, host.keepalive, host.dhcp_retry);
        drop(host); // 结束对 io 的借用，避免与 mutable self 冲突
        if settle {
            self.settle_req(req);
        }
    }

    /// 依据插件请求决定下一步（重开一轮 / 进入心跳 / 等待 DHCP / 默认完成）。
    fn settle_req(&mut self, req: (bool, Option<u64>, bool)) {
        let (restart, keepalive, dhcp_retry) = req;
        if restart {
            // 二次认证的第二轮
            self.dhcp_waiting = false;
            crate::log_info!("正在开始第二轮认证……");
            let frame = packet_builder::start_frame(&self.server_mac, &self.mac);
            let _ = self.send_and_watchdog(frame);
            return;
        }
        if let Some(interval) = keepalive {
            // 认证完成，进入心跳
            crate::log_info!("认证成功");
            self.enter_keepalive(interval);
            return;
        }
        if dhcp_retry {
            self.dhcp_waiting = true;
            self.cancel_watchdog();
            self.schedule(TimerKind::DhcpRetry, Duration::from_secs(5));
            return;
        }
        // 无插件请求：标准 EAP 直接成功
        self.cancel_watchdog();
        self.dhcp_waiting = false;
        crate::log_info!("认证成功");
        self.authed = true;
        self.write_auth_state("success");
    }

    fn enter_keepalive(&mut self, interval: u64) {
        self.authed = true;
        self.write_auth_state("success");
        self.dhcp_waiting = false;
        self.cancel_watchdog();
        self.cancel_kind(TimerKind::DhcpRetry);
        self.keepalive_interval = interval;
        if interval > 0 {
            if !self.keepalive_running {
                self.keepalive_running = true;
                // 首次 1 秒后发第一条心跳，之后按 interval
                self.schedule(TimerKind::Keepalive, Duration::from_secs(1));
            }
        } else {
            self.keepalive_running = false;
        }
    }

    /// 处理入站帧
    pub fn on_frame(&mut self, frame: &Frame) {
        // 先让插件看到帧（rjv3 借此记录 last_recv 供 prepare 取种子）
        self.plugins.on_received_all(frame);

        if frame.eapol_type() != Some(EapolType::EapPacket) {
            return;
        }
        let Some(code) = frame.eap_code() else {
            return;
        };
        match code {
            EapCode::Request => {
                if let Some(sm) = frame.src_mac() {
                    self.server_mac = sm;
                }
                let etype = frame.buf.get(crate::eth_frame::EAP_TYPE_BYTE).copied();
                let id = frame.eap_id().unwrap_or(0);
                match etype {
                    Some(1) => {
                        crate::log_info!("正在回应用户名请求");
                        let f = packet_builder::identity_response(
                            &self.server_mac,
                            &self.mac,
                            id,
                            &self.cfg.username,
                        );
                        let _ = self.send_and_watchdog(f);
                    }
                    Some(4) => {
                        crate::log_info!("正在回应密码请求");
                        let seed = if frame.buf.len() >= 40 {
                            let mut s = [0u8; 16];
                            s.copy_from_slice(&frame.buf[24..40]);
                            s
                        } else {
                            [0u8; 16]
                        };
                        let f = packet_builder::md5_challenge_response(
                            &self.server_mac,
                            &self.mac,
                            id,
                            &self.cfg.username,
                            &self.cfg.password,
                            &seed,
                        );
                        let _ = self.send_and_watchdog(f);
                    }
                    _ => {}
                }
            }
            EapCode::Success => {
                if let Some(sm) = frame.src_mac() {
                    self.server_mac = sm;
                }
                // 结束当前阶段的看门狗，交给插件决定“是否已完成 / 是否二次认证”
                self.cancel_watchdog();
                self.dispatch(&AuthEvent::Success(frame), true);
            }
            EapCode::Failure => {
                self.handle_failure(frame);
            }
            _ => {}
        }
    }

    /// 失败处理（协议通用逻辑）
    fn handle_failure(&mut self, frame: &Frame) {
        if let Some(sm) = frame.src_mac() {
            self.server_mac = sm;
        }
        // 插件（如 rjv3）会解出服务器附带的失败原因并打印；
        // 这里在 -vv（debug）下把失败帧的原始附加数据也打出来，便于逐字节排查。
        if frame.buf.len() > FRAME_HDR_NO_TYPE {
            crate::log_debug!(
                "失败帧附加数据（{} 字节）: {}",
                frame.buf.len() - FRAME_HDR_NO_TYPE,
                crate::util::hex_preview(&frame.buf[FRAME_HDR_NO_TYPE..], 96)
            );
        }
        // 先通知插件（rjv3 会借此复位自身状态）
        self.dispatch(&AuthEvent::Failure(frame), false);

        if self.authed {
            // 已认证后被服务器强制下线
            if self.cfg.restart_on_logoff {
                crate::log_warn!("认证掉线，稍后将重新开始认证……");
                self.teardown_session();
                self.schedule(TimerKind::RestartAuth, Duration::from_secs(1));
            } else {
                crate::log_error!("认证掉线，正在退出……");
                self.write_auth_state("failed");
                self.shutdown.store(true, Ordering::Relaxed);
            }
            return;
        }
        // 认证过程中失败
        self.fail_count += 1;
        // max_failures == 0 表示永不放弃，一直重试（便于观察日志 / 等服务器恢复）
        if self.cfg.max_failures != 0 && self.fail_count >= self.cfg.max_failures {
            crate::log_error!("认证失败 {} 次，已达到指定次数，正在退出……", self.fail_count);
            crate::log_warn!("排查提示：请确认用户名/密码是否正确，以及服务器是否要求锐捷 v3/v4 插件；");
            crate::log_warn!("把「最大失败次数」设为 0 可让客户端一直重试而不退出。");
            self.write_auth_state("failed");
            self.shutdown.store(true, Ordering::Relaxed);
        } else {
            crate::log_warn!(
                "认证失败 {} 次，将在 {} 秒或服务器请求后重试……",
                self.fail_count,
                self.cfg.wait_after_fail_secs
            );
            self.teardown_session();
            self.schedule(
                TimerKind::RestartAuth,
                Duration::from_secs(self.cfg.wait_after_fail_secs),
            );
        }
    }

    fn teardown_session(&mut self) {
        self.authed = false;
        self.write_auth_state("connecting");
        self.keepalive_running = false;
        self.cancel_kind(TimerKind::Keepalive);
        self.cancel_kind(TimerKind::DhcpRetry);
        self.cancel_watchdog();
        self.retry_count = 0;
    }

    /// 处理到期定时器；返回是否请求退出
    fn process_due_timers(&mut self, now: Instant) -> bool {
        let all = std::mem::take(&mut self.timers);
        let (mut due, pending): (Vec<Timer>, Vec<Timer>) =
            all.into_iter().partition(|t| t.at <= now);
        self.timers = pending;

        let mut stop = false;
        for t in due.drain(..) {
            match t.kind {
                TimerKind::Watchdog => {
                    if self.authed || self.dhcp_waiting {
                        continue;
                    }
                    if self.retry_count < self.cfg.max_retries {
                        self.retry_count += 1;
                        crate::log_warn!("等待服务器响应超时，正在重试……");
                        if let Some(mut last) = self.last_sent.clone() {
                            let _ = self.send_frame(&mut last);
                        }
                        self.arm_watchdog();
                    } else {
                        crate::log_error!("认证超时（重试 {} 次），正在退出……", self.retry_count);
                        stop = true;
                    }
                }
                TimerKind::RestartAuth => {
                    let _ = self.start();
                }
                TimerKind::Keepalive => {
                    if self.authed {
                        // 通知插件发心跳
                        self.dispatch(&AuthEvent::KeepaliveTick, false);
                        if self.keepalive_running && self.keepalive_interval > 0 {
                            self.schedule(
                                TimerKind::Keepalive,
                                Duration::from_secs(self.keepalive_interval),
                            );
                        }
                    }
                }
                TimerKind::DhcpRetry => {
                    self.dhcp_waiting = true;
                    self.dispatch(&AuthEvent::DhcpRetryTick, true);
                }
            }
            if stop {
                self.shutdown.store(true, Ordering::Relaxed);
            }
        }
        stop
    }

    /// 事件循环（真实运行）
    pub fn run(&mut self) -> Result<()> {
        self.start()?;
        let mut sent_count: usize = 0;
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                crate::log_info!("AuthEAP 已退出");
                return Ok(());
            }
            let now = Instant::now();
            let timeout = match self.next_deadline() {
                Some(d) => d.saturating_duration_since(now),
                None => Duration::from_millis(500),
            };
            match self.io.recv_timeout(timeout) {
                Ok(Some(bytes)) => {
                    let frame = Frame::from_bytes(&bytes);
                    self.on_frame(&frame);
                    sent_count += 1;
                    if let Some(limit) = self.stop_after_sent {
                        if sent_count >= limit {
                            return Ok(());
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => return Err(e),
            }
            let now = Instant::now();
            if self.process_due_timers(now) {
                crate::log_info!("AuthEAP 已退出");
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::eth_frame::{EapType, ETH_P_PAE_BYTES};
    use crate::net::LoopbackIo;
    use crate::packet_plugin::rjv3::hash::std_md5;

    const SERVER: [u8; 6] = [0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e];

    fn base_cfg(plugin: bool) -> Config {
        let mut c = Config::default();
        c.username = "testuser".into();
        c.password = "secret".into();
        c.ifname = "lo".into();
        c.plugins = if plugin { vec!["rjv3".into()] } else { vec![] };
        c.dhcp_script = String::new();
        c.stage_timeout = 3600; // 测试期间不让看门狗打扰
        c
    }

    /// 构造一条 EAP-Packet 请求/成功/失败帧。
    fn eap_frame(mac: [u8; 6], code: u8, id: u8, etype: u8, body: &[u8]) -> Frame {
        let mut buf = Vec::new();
        buf.extend_from_slice(&mac); // dst = 本机
        buf.extend_from_slice(&SERVER); // src = 服务器
        buf.extend_from_slice(&ETH_P_PAE_BYTES);
        let eap_len = 5 + body.len();
        buf.push(1);
        buf.push(EapolType::EapPacket as u8);
        buf.extend_from_slice(&(eap_len as u16).to_be_bytes());
        buf.push(code);
        buf.push(id);
        buf.extend_from_slice(&(eap_len as u16).to_be_bytes());
        buf.push(etype);
        buf.extend_from_slice(body);
        Frame { buf }
    }

    fn identity_req(id: u8) -> Frame {
        eap_frame(
            [0u8; 6].map(|_| 0xaa),
            EapCode::Request as u8,
            id,
            EapType::Identity as u8,
            b"",
        )
    }

    fn challenge_req(id: u8, seed: &[u8; 16]) -> Frame {
        let mut body = vec![16u8];
        body.extend_from_slice(seed);
        eap_frame(
            [0xaa; 6],
            EapCode::Request as u8,
            id,
            EapType::Md5Challenge as u8,
            &body,
        )
    }

    fn success_frame() -> Frame {
        eap_frame([0xaa; 6], EapCode::Success as u8, 0, 0, b"")
    }

    fn assert_last_is(io: &LoopbackIo, nth: usize, etype: EapolType) {
        let out = io.outbound.lock().unwrap();
        assert!(
            out.len() > nth,
            "期望至少 {} 个出站帧，实际 {}",
            nth + 1,
            out.len()
        );
        assert_eq!(EapolType::from_u8(out[nth].buf[15]), etype);
    }

    #[test]
    fn standard_single_auth() {
        let io = Arc::new(LoopbackIo::new());
        let mut app = App::new(base_cfg(false), Box::new(io.clone())).unwrap();
        app.start().unwrap();
        assert_last_is(&io, 0, EapolType::Start);

        app.on_frame(&identity_req(1));
        assert_last_is(&io, 1, EapolType::EapPacket);
        let idout = io.outbound.lock().unwrap()[1].clone();
        assert_eq!(&idout.buf[23..], b"testuser");

        let seed = [0x42u8; 16];
        app.on_frame(&challenge_req(2, &seed));
        assert_last_is(&io, 2, EapolType::EapPacket);
        let chout = io.outbound.lock().unwrap()[2].clone();
        let mut expect = vec![2u8];
        expect.extend_from_slice(b"secret");
        expect.extend_from_slice(&seed);
        let digest = std_md5(&expect);
        assert_eq!(&chout.buf[24..40], &digest);
        // 无插件：帧长 = 头 18 + EAP 头 5 + 1 + 16 + 用户名长度
        assert_eq!(
            chout.buf.len(),
            18 + 5 + 1 + 16 + app.cfg.username.len()
        );

        app.on_frame(&success_frame());
        assert!(app.authed);
    }

    #[test]
    fn rjv3_appends_trailer() {
        let io = Arc::new(LoopbackIo::new());
        let mut app = App::new(base_cfg(true), Box::new(io.clone())).unwrap();
        app.start().unwrap();
        assert_last_is(&io, 0, EapolType::Start);

        // Start 帧也应带 trailer
        let start = io.outbound.lock().unwrap()[0].clone();
        assert!(start.buf.len() > 18);
        // trailer 中含版本字符串（ver_str 属性）
        let joined = String::from_utf8_lossy(&start.buf);
        assert!(
            joined.contains("RG-SU For Linux V1.0"),
            "Start 帧缺少版本串 trailer"
        );

        let seed = [0x24u8; 16];
        app.on_frame(&identity_req(1));
        app.on_frame(&challenge_req(2, &seed));
        let n = io.outbound.lock().unwrap().len();
        let challenge = io.outbound.lock().unwrap()[n - 1].clone();
        // 标准应答长度 18+5+1+16+9
        let std_len = 18 + 5 + 1 + 16 + 9;
        assert!(challenge.buf.len() > std_len, "挑战应答应追加 trailer");
        // trailer 中应含版本串
        let hex_region = String::from_utf8_lossy(&challenge.buf[std_len..]);
        assert!(
            hex_region.contains("RG-SU For Linux V1.0"),
            "应答帧缺少版本串 trailer"
        );
        // v3 hash 属性内容是 computeV4 产生的 128 个连续 hex 字符
        let found_v3 = challenge.buf[std_len..]
            .windows(128)
            .any(|w| w.iter().all(|b| b.is_ascii_hexdigit()));
        assert!(found_v3, "应答帧应含 computeV4 生成的 128 hex");
    }
}
