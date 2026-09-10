//! 插件注册表（`--plugin <name>` 能选的插件都在这里登记）。
//!
//! 说明：`rjv3`（锐捷认证）也**只是一个普通插件**——认证核心本体不再认识它。
//! 它在构造时自包含配置与本机 MAC，通过 `Plugin` 的 `on_auth_event` +
//! `SessionHost` 反向驱动核心（二次认证/心跳/运行脚本等）。所以这里与
//! `printer`、`stats` 一样登记即可。
//!
//! # 如何加入你自己的插件
//!
//! 1. 在 `src/plugins/` 下新建文件（或新子目录），例如 `my_plugin.rs`，里面：
//!    ```no_run
//!    use autheap::eth_frame::Frame;
//!    use autheap::plugin::Plugin;
//!
//!    pub struct MyPlugin;
//!    impl Plugin for MyPlugin {
//!        fn name(&self) -> &str { "my-plugin" }
//!        fn description(&self) -> &str { "我的插件" }
//!        // fn prepare(&mut self, frame: &mut Frame) { ... }
//!        // fn on_received(&mut self, frame: &Frame) { ... }
//!    }
//!    ```
//! 2. 在本文件顶部加入 `mod my_plugin;`；
//! 3. 在 [`create`] 中登记一个分支：`"my-plugin" => Some(Box::new(my_plugin::MyPlugin))`；
//! 4. `cargo build --release` 后运行：`autheap ... --plugin my-plugin`。
//!    （在 OpenWrt/LuCI 里，把 `my-plugin` 填进“其他插件”即可，见
//!    `packaging/openwrt/README.md`；`--plugin` 对未知名字只会告警并忽略。）
//!
//! 完整教程见 `docs/plugin-development.md`。

pub mod example;
pub mod report;

use std::boxed::Box;

use crate::config::Config;
use crate::packet_plugin::printer::Printer;
use crate::packet_plugin::rjv3::plugin::Rjv3;
use crate::plugin::{Plugin, PluginManager};

/// 按名字构造插件；不认识的名字返回 None。
///
/// `cfg`/`mac` 只对需要它们的插件（如 rjv3）有意义，其它插件忽略。
pub fn create(name: &str, cfg: &Config, mac: [u8; 6]) -> Option<Box<dyn Plugin>> {
    match name {
        // 锐捷认证：一个普通插件（构造时自包含配置与 MAC）
        "rjv3" => Some(Box::new(Rjv3::new(cfg, mac))),
        "printer" => Some(Box::new(Printer::new(cfg.printer_log_recv))),
        // 用户示例插件：流量统计
        "stats" | "example" => Some(Box::new(example::TrafficStats::default())),
        // 如实上报本机真实信息的独立插件（不改帧、不伪造状态）
        "report" => Some(Box::new(report::Report::new(cfg, mac))),
        // ============ 在此加入你自己的插件 ============
        // "my-plugin" => Some(Box::new(my_plugin::MyPlugin)),
        _ => None,
    }
}

/// 把 `names` 里能识别的插件装进一个管线（会跳过不认识的名字并告警）。
pub fn build_manager(cfg: &Config, mac: [u8; 6], names: &[String]) -> PluginManager {
    let mut m = PluginManager::new();
    for n in names {
        match create(n, cfg, mac) {
            Some(p) => m.add_box(p),
            None => crate::log_warn!("未知插件名: {n}，已忽略（可用 --plugin printer / stats / report / rjv3）"),
        }
    }
    m
}

/// 列出本程序已编译进的所有插件（name, description）。
///
/// 供 `autheap --list-plugins` 使用，LuCI 的“启用插件”多选下拉会根据它动态生成选项：
/// 以后新增插件只需在 [`create`] 里登记，这里同步补一行即可（也可从各插件的
/// `name()/description()` 提取，但保持静态表最简单、免构造实例）。
pub fn available() -> Vec<(&'static str, &'static str)> {
    vec![
        ("rjv3", "锐捷 v3/v4 私有认证算法插件"),
        ("printer", "打印流经本插件的报文大小"),
        ("stats", "统计认证期间收发报文数量与字节数（示例插件）"),
        ("report", "如实上报本机真实信息（网卡/MAC/IP/网关/DNS/主机名）与收发计数；不改帧、不伪造状态"),
    ]
}
