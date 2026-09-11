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
  内置 `printer`（打印）、`stats`（统计示例）、`report`（如实上报本机真实信息）、
  `beacon`（**周期向指定 IP:端口 发送自定义 TCP 数据包**，目标/间隔/载荷可在 LuCI 调整，
  **载荷默认内置**，需要时用 `--beacon-payload` 或 LuCI 覆盖）。
  教程见 `docs/plugin-development.md`。
- **LuCI 界面（OpenWrt 24.10+）**：配置页按**每个插件/功能各自一个 Tab** 组织
  —— 基本设置 / 插件 / rjv3 锐捷 / beacon 保活 / 日志（下方固定显示后端配置文件）；
  另有**实时日志页**（状态页已移除全部日志相关内容），可查看历史 `.1/.2…` 日志。
  状态页自带**网络测试**：填目标 `IP / 主机[:端口]` 后一键检查本机接口/网关/DNS、
  目标域名解析、TCP 连接与 ICMP 连通性（留空只测本机与网关）。详见
  `packaging/openwrt/`。
- **配置文件认证**：`autheap --config /etc/autheap-client.conf`——LuCI 保存配置时会把
  UCI 渲染成该文件，服务启动时也自动刷新并用它认证（`key = value` 文本，命令行参数
  优先）。单独跑官方命令也不再需要一长串参数。
- **日志实时可读 + 自动清理**：`--log-file` 追加写入并逐行 flush（LuCI 增量读取、
  `tail -f` 都是实时的，守护模式也不丢日志）；超过 `--log-max-kb` 自动轮转为
  `xxx.log.1/.2/…`，只保留 `--log-keep` 份（`--log-rotate-secs` 控制检查间隔），
  默认每 5 分钟检查一次、单文件上限 256KB、保留 3 份历史。

## 目录结构

```
.github/workflows/        CI/CD（ci.yml：构建测试 + 打包）
.forgejo/workflows/       CI/CD（自建 Forgejo runner：ci.yml + release.yml 发布）
scripts/                  版本递增 / 静态检查 / 交叉编译打包 / 发布脚本
src/
├── main.rs              入口（信号、守护进程化、日志文件）
├── cli.rs               clap 命令行定义
├── config.rs            运行配置、默认值与配置文件解析（--config）
├── logging.rs           日志（文件追加/flush/自动轮转清理）
├── runtime.rs           认证状态机 + 事件循环 + 定时器 + 二次认证/心跳
├── packet_builder.rs    标准 EAP/EAPOL 帧构建
├── eth_frame.rs         协议常量与帧表示
├── net/                 Io 抽象、AF_PACKET raw、pcap（feature）、接口信息
├── nettest.rs           网络测试（本机/网关/DNS/目标连通性诊断，--nettest）
├── plugins/             普通插件注册表（printer/stats/report/beacon…）
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

### 版本号与发布

- **版本号规则：`0.1.x`**，**每发布一次 release 就把 `x` 加一**：
  `0.1.1` → `0.1.2` → `0.1.3` …（不换主/次版本号，也不用 `-2`、`-r2` 这类发布号来区分）。
- 包文件名里的 `-1`（apk 为 `-r1`）是包管理器自己的 revision，**发新版本时保持 `1`**。
- 递增版本号（同时改 `Cargo.toml` 与 `Cargo.lock`）：

```bash
./scripts/bump-version.sh -n          # 只看下一个版本号
./scripts/bump-version.sh             # 0.1.2 → 0.1.3
./scripts/bump-version.sh --set 0.1.5 # 指定版本
```

- **CI 自动处理**：推送 tag（如 `0.1.3`）或手动触发 release 流程时，流水线会取远端
  同主版本号的最大版本 +1（未发布过的 `Cargo.toml` 版本则原样发布），交叉编译并
  上传 `.ipk` / `.apk` / `.deb`，然后在 Codeberg/Forgejo 创建 Release。
- 打包脚本的默认版本号在 `packaging/openwrt/scripts/common.sh`、`build-luci.sh`、
  `packaging/desktop/build-deb.sh`、`packaging/openwrt/autheap-client/Makefile`，
  发版时用 `bump-version.sh` 后同步这几处默认值即可（CI 里由 `ci-version.sh` 注入）。

## 运行

需要 root（raw 后端 / 打开网络接口）。

```bash
sudo autheap -i eth0 -u 你的账号 -p 密码 --plugin rjv3

# 二次认证（先跑 dhcp 脚本拿地址，再进行第二轮认证）
sudo autheap -i enp5s0 -u 学号 -p 密码 \
     --plugin rjv3 --dhcp-type double --dhcp-script "dhclient enp5s0"

# 打印经过插件的帧（可叠加）
sudo autheap -i eth0 -u u -p p --plugin printer --plugin rjv3
```

常用参数摘要见 `autheap --help`。

### 网络测试（诊断，不需要账号）

认证不上时先分清“网络本身不通”还是“账号/协议问题”：

```bash
sudo autheap --nettest                    # 只测本机接口/网关/DNS
sudo autheap --nettest 203.0.113.10       # 目标：IP / 主机名（默认 TCP 80）
sudo autheap --nettest 203.0.113.10:8083 -i eth0   # 指定端口与接口
```

输出分【本机】【目标】【ICMP ping】【结论】四段：接口地址/掩码/默认网关/DNS、
目标域名解析、TCP 连接（默认 80，可用 `主机:端口` 指定）、ICMP 往返时延；
有问题的那一行标 ✘，**退出码 0 = 未发现问题，1 = 有异常**（便于脚本判断）。
ICMP 需要 root / `CAP_NET_RAW`，权限不足时会写“跳过”而不是报错。
OpenWrt 的 LuCI 状态页也内置了同样的测试入口。

### 单实例守护进程

- **跑认证的进程会先抢一把单实例锁**（默认 `/var/run/autheap.pid`，可用 `--pid-file <PATH>`
  指定、传空字符串关闭）：已经有实例在跑时，再启动会**直接拒绝**并写明原因，例如
  `已有认证进程在运行（pid 1234）`，**退出码 3**（便于脚本判断）；锁由内核在进程退出时
  自动释放，`kill -9` 也不会留下死锁。
- **诊断 / 查询类命令不加锁**，守护进程在跑的时候随时可用：

```bash
autheap --nettest 203.0.113.10   # 网络测试（不占网卡、不需账号）
autheap --list-plugins           # 列出已编译插件
autheap --help                   # 帮助与全部参数
```

- OpenWrt 上由 `/etc/init.d/autheap-client restart|stop|start` 管理；procd 自己用的
  pidfile 是 `/var/run/autheap-client.pid`，与上面这把锁互不影响。

### 配置文件方式（推荐用于服务/OpenWrt）

```bash
# 例：/etc/autheap-client.conf 内容形如
#   interface = eth0
#   username = 你的账号
#   password = 你的密码
#   plugins = rjv3 beacon
#   log_file = /var/log/autheap-client.log
#   log_max_kb = 256
#   log_keep = 3
#   log_rotate_secs = 300
#   beacon_dst = 203.0.113.10      # 换成你自己的目标地址
#   beacon_port = 8083
#   beacon_interval = 60
#   beacon_payload =              # 可留空（用内置默认载荷），或填自己的抓包 Data 十六进制
sudo autheap --config /etc/autheap-client.conf
```

命令行上显式给出的参数会覆盖配置文件里的同名项。

### beacon 插件：周期向指定 IP 发送数据包

```bash
# 每 60 秒向指定 IP:端口 发送一段 TCP 载荷
# （目标地址由你指定；载荷默认用内置的，可用 --beacon-payload 覆盖）
sudo autheap -i eth0 -u 学号 -p 密码 --plugin rjv3 --plugin beacon \
     --beacon-dst 203.0.113.10 --beacon-port 8083 --beacon-interval 60
```

要点：

- **不写死任何 IP**：**源地址是运行本机的真实地址**（内核按路由填写），
  **目的地址由你指定**；数据包的“时间”就是每次构造/发送的时刻；
- **载荷默认内置**：不传 `--beacon-payload`（或 LuCI 里留空）即使用内置默认载荷；
  需要更换时，填自己抓包 `Data` 的十六进制即可（长度需为偶数）。

请仅在你拥有使用授权的网络上使用。

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
