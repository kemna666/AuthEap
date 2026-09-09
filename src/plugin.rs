//! # 插件（Packet Plugin）公共接口
//!
//! 这是给“想要编写自己的报文修改插件”的用户使用的公开 API。它的设计对齐了上游
//! minieap 的 C 插件接口：认证核心在**发送一帧之前**调用所有插件的
//! [`Plugin::prepare`]，在**收到一帧之后**调用 [`Plugin::on_received`]。
//!
//! 协议事件（成功/失败/心跳/二次认证重试）通过 [`Plugin::on_auth_event`] 通知，
//! 插件通过 [`SessionHost`] 反过来请求核心做动作（重开一轮、进入心跳、运行脚本、
//! 直接发包）。这样**锐捷认证（rjv3）也是一个普通插件**，核心本体不再认识它。
//!
//! 插件通过 [`PluginManager`] 按顺序串成一个管线。

use std::any::Any;

use crate::config::Config;
use crate::error::Result;
use crate::eth_frame::Frame;

/// 允许把 `&dyn Plugin` / `&mut dyn Plugin` 转回具体类型。
pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// 认证核心向插件广播的协议事件。
///
/// 注意：这些事件是**已经过认证核心标准 EAP 判定**之后才产生的语义事件
/// （例如收到 Success/Failure 码），插件通常只需要关心与自己相关的那一个。
#[derive(Debug, Clone, Copy)]
pub enum AuthEvent<'a> {
    /// 认证成功（EAP-Success 或二次认证的某次成功）
    Success(&'a Frame),
    /// 认证失败 / 被服务器拒绝
    Failure(&'a Frame),
    /// 核心的心跳计时到点（已进入 keepalive 状态，间隔由插件通过
    /// [`SessionHost::set_keepalive`] 指定）
    KeepaliveTick,
    /// 二次认证等待 DHCP 的 5 秒重试计时到点
    DhcpRetryTick,
}

/// 插件在事件回调里用来反过来驱动认证核心的“会话宿主”。
///
/// 这是把 `rjv3` 等私有算法插件与认证核心解耦的关键：插件不再需要知道核心内部
/// 的定时器/状态，只需发出请求，核心负责执行。
pub trait SessionHost {
    /// 立即发送一帧（通常用于心跳包等插件自产的帧）
    fn send(&mut self, frame: &Frame) -> Result<()>;

    /// 请求：立即开始一轮新认证（核心会发送 EAPOL-Start 并重置阶段超时看门狗）。
    /// 常用于二次认证的第二轮。
    fn restart_round(&mut self);

    /// 请求：本次会话已完成，进入 keepalive（已认证）状态并启用心跳。
    /// `interval_secs` 为两次心跳的间隔；传 0 表示进入已认证但不周期心跳。
    fn set_keepalive(&mut self, interval_secs: u64);

    /// 请求：5 秒后触发 [`AuthEvent::DhcpRetryTick`]（等待 DHCP 时使用）。
    fn dhcp_retry(&mut self);

    /// 执行一条 shell 脚本（DHCP 脚本等）。
    fn run_script(&mut self, cmd: &str);
}

/// 一个报文修改 / 协议插件。
///
/// 实现方通常只需关心：
/// - [`Plugin::prepare`]：核心把标准帧发送出去**之前**（可改帧）；
/// - [`Plugin::on_received`]：核心收到一帧之后（更新状态）；
/// - [`Plugin::on_auth_event`]：认证结果等协议事件（可配合 [`SessionHost`] 反驱核心）。
pub trait Plugin: AsAny + Send + 'static {
    /// 插件名（与命令行 `--plugin <name>` 对应）
    fn name(&self) -> &str;
    /// 一句话描述
    fn description(&self) -> &str {
        ""
    }
    /// 版本串
    fn version(&self) -> &str {
        "0.1"
    }

    /// 出站帧发送前回调（可修改 `frame`）
    fn prepare(&mut self, _frame: &mut Frame) {}

    /// 入站帧回调（只读）
    fn on_received(&mut self, _frame: &Frame) {}

    /// 协议事件回调。默认什么都不做。
    fn on_auth_event(&mut self, _ev: &AuthEvent, _host: &mut dyn SessionHost) {}

    /// 认证核心重置会话（重连、失败重试、进程重启逻辑等）时调用。
    fn reset(&mut self) {}
}

/// 插件按顺序组成的管线。
#[derive(Default)]
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> PluginManager {
        PluginManager {
            plugins: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }

    /// 加入一个插件（会打印启用信息）
    pub fn add<P: Plugin>(&mut self, plugin: P) {
        crate::log_info!(
            "已启用插件: {} — {}",
            plugin.name(),
            plugin.description()
        );
        self.plugins.push(Box::new(plugin));
    }

    /// 加入一个 Box 形式的插件
    pub fn add_box(&mut self, plugin: Box<dyn Plugin>) {
        crate::log_info!(
            "已启用插件: {} — {}",
            plugin.name(),
            plugin.description()
        );
        self.plugins.push(plugin);
    }

    pub fn remove(&mut self, name: &str) -> Option<Box<dyn Plugin>> {
        if let Some(i) = self.plugins.iter().position(|p| p.name() == name) {
            Some(self.plugins.remove(i))
        } else {
            None
        }
    }

    /// 出站前：按顺序交给所有插件
    pub fn prepare_all(&mut self, frame: &mut Frame) {
        for p in self.plugins.iter_mut() {
            p.prepare(frame);
        }
    }

    /// 入站后：按顺序通知所有插件
    pub fn on_received_all(&mut self, frame: &Frame) {
        for p in self.plugins.iter_mut() {
            p.on_received(frame);
        }
    }

    /// 协议事件：按顺序通知所有插件
    pub fn on_auth_event_all(&mut self, ev: &AuthEvent, host: &mut dyn SessionHost) {
        for p in self.plugins.iter_mut() {
            p.on_auth_event(ev, host);
        }
    }

    pub fn reset_all(&mut self) {
        for p in self.plugins.iter_mut() {
            p.reset();
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins
            .iter()
            .find(|p| p.name() == name)
            .map(|b| b.as_ref())
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut dyn Plugin> {
        self.plugins
            .iter_mut()
            .find(|p| p.name() == name)
            .map(|b| b.as_mut())
    }

    pub fn get_typed<T: Any>(&self, name: &str) -> Option<&T> {
        self.get(name)?.as_any().downcast_ref::<T>()
    }

    pub fn get_typed_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.get_mut(name)?.as_any_mut().downcast_mut::<T>()
    }
}

/// 从插件注册表（[`crate::plugins::create`]）按名字构造插件管线。
pub fn manager_from_names(cfg: &Config, mac: [u8; 6], names: &[String]) -> PluginManager {
    let mut m = PluginManager::new();
    for n in names {
        if let Some(p) = crate::plugins::create(n, cfg, mac) {
            m.add_box(p);
        } else {
            crate::log_warn!("未知插件名: {n}，已忽略（可用 --plugin printer / stats / rjv3）");
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eth_frame::Frame;

    /// 一个会在帧尾追加标记字节的“用户插件”
    struct MarkerPlugin {
        sent: usize,
        recv: usize,
        marker: u8,
    }
    impl Plugin for MarkerPlugin {
        fn name(&self) -> &str {
            "marker"
        }
        fn description(&self) -> &str {
            "测试用：向出站帧追加一个标记字节"
        }
        fn prepare(&mut self, frame: &mut Frame) {
            self.sent += 1;
            frame.buf.push(self.marker);
        }
        fn on_received(&mut self, _f: &Frame) {
            self.recv += 1;
        }
    }

    #[test]
    fn manager_dispatches_and_typed_access() {
        let mut m = PluginManager::new();
        m.add(MarkerPlugin {
            sent: 0,
            recv: 0,
            marker: 0xAA,
        });

        let mut f = Frame::from_bytes(&[1, 2, 3]);
        m.prepare_all(&mut f);
        assert_eq!(f.buf, vec![1, 2, 3, 0xAA]);

        m.on_received_all(&Frame::from_bytes(&[9]));
        assert!(m.get("marker").is_some());
        {
            let mp: &MarkerPlugin = m.get_typed("marker").expect("typed get");
            assert_eq!(mp.sent, 1);
            assert_eq!(mp.recv, 1);
        } // 作用域结束，借用随即释放

        m.reset_all();
    }

    #[test]
    fn auth_event_reaches_plugin() {
        struct HostStub {
            restart: bool,
        }
        impl SessionHost for HostStub {
            fn send(&mut self, _f: &Frame) -> Result<()> {
                Ok(())
            }
            fn restart_round(&mut self) {
                self.restart = true;
            }
            fn set_keepalive(&mut self, _i: u64) {}
            fn dhcp_retry(&mut self) {}
            fn run_script(&mut self, _c: &str) {}
        }

        struct ProtoPlugin;
        impl Plugin for ProtoPlugin {
            fn name(&self) -> &str {
                "proto"
            }
            fn on_auth_event(&mut self, ev: &AuthEvent, host: &mut dyn SessionHost) {
                if matches!(ev, AuthEvent::Success(_)) {
                    host.restart_round();
                }
            }
        }

        let mut m = PluginManager::new();
        m.add(ProtoPlugin);
        let mut host = HostStub { restart: false };
        m.on_auth_event_all(&AuthEvent::Success(&Frame::from_bytes(&[1])), &mut host);
        assert!(host.restart);
    }
}
