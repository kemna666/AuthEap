# MiniEAP 插件开发指南

MiniEAP 的插件（Packet Plugin）与上游 C 版一致：**在认证核心发送一帧之前**，
每个插件都有机会修改该帧；**在收到一帧之后**，每个插件都会被通知。
你可以借此实现自定义的 802.1x 私有算法、调试/统计等任意行为。

- 核心公开 API 在 `src/plugin.rs`（Rust doc 也可直接 `cargo doc` 查看）。
- 内置插件：`printer`（打印帧）、`report`（如实上报本机真实信息）、`rjv3`（锐捷私有算法，系统插件）。
- 参考示例：`src/plugins/example.rs`（`stats`，流量统计）。

## 1. 概念：插件能做什么 / 不能做什么

| 时机 | 回调 | 含义 |
| --- | --- | --- |
| 出站前 | `prepare(&mut Frame)` | 可修改/追加帧内容；认证核心随后才真正发送 |
| 入站后 | `on_received(&Frame)` | 只读查看收到的帧，可更新自己的状态 |
| 协议事件 | `on_auth_event(&AuthEvent, &mut SessionHost)` | 收到 成功/失败/心跳/二次认证重试 时回调，可反驱核心 |
| 会话重置 | `reset()` | 重连/失败重试/重启认证时清空内部状态 |

**插件不是独立进程**：它与你写的代码一起编译进同一个 `autheap` 可执行文件
（源码级插件，无 ABI 风险）。

**认证核心不认识任何具体算法**：锐捷认证（`rjv3`）也是一个**普通插件**，通过
`on_auth_event` + [`SessionHost`](`crate::plugin::SessionHost`) 向核心请求
“重开一轮 / 进入心跳 / 等 DHCP / 执行脚本 / 直接发包”。如果你想写一个自定义
私有算法插件，可以照着 `rjv3` 做：构造时自包含配置，靠 `SessionHost` 驱动核心。

插件按 `--plugin` 给出的顺序放入统一的 `PluginManager` 管线，可同时启用多个，
例如 `--plugin printer --plugin rjv3 --plugin stats`。

## 2. 接口速览

```rust,ignore
// 来自 crate::plugin
pub trait Plugin: AsAny + Send + 'static {
    fn name(&self) -> &str;          // 必填，对应 --plugin <name>
    fn description(&self) -> &str { "" }
    fn version(&self) -> &str { "0.1" }

    fn prepare(&mut self, frame: &mut Frame) {}   // 出站前（可改帧）
    fn on_received(&mut self, frame: &Frame) {}   // 入站后
    fn reset(&mut self) {}                        // 会话重置
}
```

管线类型：

```rust,ignore
let mut m = PluginManager::new();
m.add_box(plugins::create("printer").unwrap());
m.prepare_all(&mut frame);     // 按顺序过一遍
m.on_received_all(&frame);
m.get_typed::<TrafficStats>("stats");   // 取回具体类型，读取内部统计
```

`Frame` 的字节布局见 `src/eth_frame.rs`（以太网头 14B + EAPOL 头 4B + 可选 EAP 头 5B），
偏移常量都是 `pub const`。

## 3. 最小示例（照抄即可）

文件 `src/plugins/my_plugin.rs`：

```rust,ignore
use minieap::eth_frame::Frame;
use minieap::plugin::Plugin;

/// 一个“尾随标记”示例：给每个出站帧追加一个 0x55 字节。
pub struct TailMarker {
    pub enabled: bool,
}

impl Plugin for TailMarker {
    fn name(&self) -> &str { "tail-marker" }
    fn description(&self) -> &str { "演示：向出站帧追加 0x55" }

    fn prepare(&mut self, frame: &mut Frame) {
        if self.enabled {
            frame.buf.push(0x55);
        }
    }
}
```

登记：编辑 `src/plugins/mod.rs`：

```rust,ignore
pub mod my_plugin;   // 顶部
// 在 create() 里：
"tail-marker" => Some(Box::new(my_plugin::TailMarker { enabled: true })),
```

编译并运行：

```bash
cargo build --release
sudo ./target/release/autheap -i eth0 -u <user> -p <pass> --plugin rjv3 --plugin tail-marker
```

> 注意 `--plugin` 可重复使用，插件按命令行出现顺序进入管线。

> 在 **OpenWrt/LuCI** 上部署时，不需要写命令行：把编译进客户端的插件名填到配置页
> “其他插件”（如 `tail-marker`，空格/逗号分隔可填多个）即可，LuCI/init 会以
> `--plugin <name>` 传入进程（见 `packaging/openwrt/README.md`）。

## 4. 内置插件

- `printer`：打印每个流经帧的类型与长度，适合抓包前先确认数据流向。
- `rjv3`：锐捷 v3/v4 认证算法插件（普通插件，`--plugin rjv3` 显式启用）。
  它示范了“私有算法插件”的完整写法：`prepare` 追加 trailer，
  `on_auth_event`（Success/Failure/KeepaliveTick/DhcpRetryTick）通过
  `SessionHost` 反驱核心完成二次认证与心跳。
- `stats`（示例）：统计收发帧数量与字节数；可用 `get_typed` 在退出前读出，
  或自行扩展为周期打印。
- `report`（如实上报）：在会话开始时把本机**当前真实**信息（网卡/MAC/IPv4/网关/
  DNS/主机名）与收发计数打到日志，方便入网前向使用者/网管如实核对环境；它不改帧、
  **不伪造任何“已安装软件/合规”类状态**（见 `src/plugins/report.rs` 头部注释）。
  可用 `--plugin report` 启用。

## 5. 写一个“协议级”插件（进阶）

简单插件只用 `prepare`/`on_received` 就够了；如果你想实现像 `rjv3` 这样能
驱动认证流程的私有算法插件，请：

1. 构造时自包含它需要的全部参数（配置、MAC……）；
2. 在 `on_auth_event` 里处理 [`AuthEvent`](`crate::plugin::AuthEvent`)；
3. 需要动作时调用 [`SessionHost`](`crate::plugin::SessionHost`)：
   `restart_round()` 触发下一轮认证（二次认证）、`set_keepalive(secs)` 进入心跳、
   `dhcp_retry()` 让核心 5 秒后重试本事件、`run_script()` 跑 DHCP 脚本、
   `send(&frame)` 立即发包（心跳帧）。
4. 核心会在适当时候再次回调（如 `KeepaliveTick` / `DhcpRetryTick`）。

以 `rjv3` 为模板复制修改是最快的上手方式。

## 6. 文档与测试

- `cargo doc --open`：查看 `minieap::plugin` 的 API 文档。
- `cargo test`：`plugin.rs` 内含管线分发与类型取回的单元测试。
- 本文件（`docs/plugin-development.md`）即插件文档；如需中英文双语版本可自行扩展。
