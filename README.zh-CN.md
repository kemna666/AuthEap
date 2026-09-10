# AuthEAP

可扩展的 802.1x (EAP-MD5) 客户端，带 **锐捷 v3/v4** 私有算法插件支持。
本工程是 [updateing/minieap](https://github.com/updateing/minieap)（C 语言）的
**Rust重写版本**，采用 Rust 惯用写法重新组织，目标平台 Linux

> ⚠️ 本程序涉及校园网认证客户端领域，仅用于学习研究与你拥有使用授权的网络。
> 由于锐捷 v3/v4 算法来自逆向工程，请遵守当地网络使用规定。

---

## 特性

- **标准 EAP-MD5 认证核心**：EAPOL-Start → Identity → MD5-Challenge → Success 全流程，
  含阶段超时重试、失败重试、被强制下线后自动重连。
- **锐捷 v3/v4 插件**（`--plugin rjv3`）：
  - **认证算法与本体完全解耦**：`rjv3` 就是一个实现公开 `Plugin` trait 的普通插件，
    构造时自包含配置与 MAC，通过 `AuthEvent` + `SessionHost` 反驱核心
    （二次认证、心跳、运行 DHCP 脚本等都由插件自己决策，核心不认识任何私有算法）。
  - 逐字节移植的锐捷专有哈希：**MD5 / SHA-1 / RIPEMD-128 / Tiger / Whirlpool 全部为
    自定义 IV/轮常量的变体**（并非标准算法），并移植 `computeV4` / `computePwd`。
  - 二次认证（double auth）与 Keep-Alive 心跳、DHCP 租约/网关/DNS/IPv6 等字段上报。
  - 支持 `--rj-option` 追加 / 覆盖私有字段（含 `:r` 替换语义）。
- **插件接口**：`src/plugin.rs` 提供公开 `Plugin` trait 与 `PluginManager`；
  写一个插件只需实现 `prepare`/`on_received`（可选 `on_auth_event`+`SessionHost`
  协议级反驱），在 `src/plugins/mod.rs` 登记名字即可被 `--plugin <name>` 启用。
  内置 `printer`（打印）、`stats`（统计示例）、`report`（如实上报本机真实信息）。
  教程见 `docs/plugin-development.md`。
- **OpenWrt 打包（24.10+）与 LuCI**：见 `packaging/openwrt/`——同时产出
  `.ipk`（opkg）与 `.apk`（24.10 apk）两种格式，并附新版 LuCI 界面
  （JS + ucode/rpcd + UCI），支持：**开机自启**开关（enable/disable 软链）、
  **认证网卡下拉选择**、以及**追加其他插件**（`extra_plugins`，空格/逗号分隔）。
- **模块化收发包后端**：
  - `raw`：Linux 原生 AF_PACKET（默认，无需外部库，需要 root）。
  - `pcap`：libpcap（跨平台，需编译时启用 `backend-pcap` feature）。
- **Rust 风格 CLI**（clap）：多参数、插件可重复指定、密码可走环境变量 `AUTHEAP_PASSWORD`。
- 全面单元测试：所有专有哈希与 `computeV4/computePwd` 均有由**原版 C 编译生成的黄金
  参考向量**逐字节校验。

## 目录结构

```
src/
├── main.rs              入口（信号、守护进程化）
├── cli.rs               clap 命令行定义
├── config.rs            运行配置与默认值
├── runtime.rs           认证状态机 + 事件循环 + 定时器 + 二次认证/心跳
├── packet_builder.rs    标准 EAP/EAPOL 帧构建
├── eth_frame.rs         协议常量与帧表示
├── net/                 Io 抽象、AF_PACKET raw、pcap（feature）、接口信息
└── packet_plugin/
    ├── rjv3/
    │   ├── hash/        锐捷专有哈希移植（含 S-box 与常量表）
    │   │   ├── md5.rs / sha1.rs / ripemd128.rs / tiger.rs / whirlpool.rs
    │   │   ├── md5_data.rs / v4_data.rs / *_sbox.rs（生成的数据表）
    │   │   ├── v4.rs    computeV4 / computePwd
    │   │   └── crc16.rs / rjencode.rs
    │   ├── plugin.rs    私有 trailer 构造、快照采集、二次认证流程
    │   ├── prop.rs      0x1a 私有字段序列化/解析
    │   └── keepalive.rs 心跳帧构建
    └── printer.rs       示例插件：打印流经帧
```

## 编译

工程本体是**纯 CLI**，默认在桌面 Linux 上直接 `cargo build` 即可（编译不依赖
OpenWrt/LuCI 任何内容）：

```bash
# 默认（raw 后端，Linux）
cargo build --release
# 结果在 target/release/autheap

# 如需 pcap 后端（需安装 libpcap 开发库）
#   Debian/Ubuntu: sudo apt install libpcap-dev
cargo build --release --features backend-pcap
```

### 只编译 CLI 并安装到桌面 Linux

想产出/安装一个干净的“仅 CLI”版本（统一命名 `autheap-client`），用现成脚本。
Debian/Ubuntu 等可用 **apt 安装/卸载**：

```bash
# 方式一：apt / dpkg（Debian、Ubuntu）——可安装、卸载、upgrade
./packaging/desktop/build-deb.sh                       # 产出 .deb
sudo apt install ./packaging/desktop/out/autheap_*.deb
sudo autheap -i eth0 -u 学号 --plugin rjv3            # 直接命令行启动
sudo apt remove autheap                                # 卸载
# （可选）./packaging/desktop/make-apt-repo.sh 生成本地仓库后用 apt update/install

# 方式二：源码安装（其它发行版）
./packaging/desktop/build-desktop.sh                   # 可选 --features backend-pcap
sudo ./packaging/desktop/install.sh                    # 装到 /usr/bin + systemd
sudo vim /etc/autheap.env                              # 填 AUTHEAP_PASSWORD
sudo systemctl enable --now autheap
```

详见 `packaging/desktop/README.md`。

## 运行

需要 root（raw 后端 / 打开网络接口）。

```bash
sudo autheap -i eth0 -u 201000000 -p 密码 --plugin rjv3

# 二次认证（先跑 dhcp 脚本拿地址，再进行第二轮认证）
sudo autheap -i enp5s0 -u 学号 -p 密码 \
     --plugin rjv3 --dhcp-type double --dhcp-script "dhclient enp5s0"

# 打印经过插件的帧（可叠加）
sudo autheap -i eth0 -u u -p p --plugin printer --plugin rjv3
```

常用参数摘要见 `autheap --help`。

### 二次认证 / 字段覆盖示例

```bash
# 覆盖内置生成的 0x6f（版本串）字段
--rj-option 6f:52472d535520466f72204c696e7578:r
# 追加一条自定义字段 0x6a
--rj-option 6a:000102
```

## 致谢与许可

- 上游：<https://github.com/updateing/minieap>（GPL-3.0），作者 updateing 等。
- 锐捷算法来源：Hu Yunrui 的 [MentoHUST](https://github.com/hyrathb/mentohust)，
  以及 rhash / ampheck 哈希库；
- 许可：GPL-3.0（见 [LICENSE](LICENSE) 与 [NOTICE](NOTICE)）。
