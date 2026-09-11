# OpenWrt 打包（24.10 及以后）

本目录为 **AuthEAP-Client**（Rust 版 802.1x/EAP-MD5 客户端，锐捷 v3/v4 插件支持）
提供 OpenWrt 打包支持，涵盖两种格式：

- **`.ipk`**：opkg 传统格式（OpenWrt ≤ 23.05 默认）；
- **`.apk`**：24.10 起的默认包管理（apk）格式。

以及配套的 **LuCI 界面**（新版 JS + ucode/rpcd，24.10+）。

> 标识映射：OpenWrt 系统软件包名 **AuthEAP-Client**；安装后**命令行命令为 `autheap`**
> （`/usr/bin/autheap`，可直接带参数运行）；服务/init、UCI 配置与 LuCI 的内部标识为
> 小写 **`autheap-client`**（`/etc/init.d/autheap-client`、`/etc/config/autheap-client`）。



## 目录

```
packaging/openwrt/
├── README.md
├── autheap-client/           # 主程序包：feed/SDK 风格 Makefile
│   ├── Makefile
│   └── root/                 # 安装到设备的文件
│       ├── etc/init.d/autheap-client
│       ├── etc/config/autheap-client
│       └── usr/libexec/autheap-client/gen-config   # UCI → 后端配置文件
├── luci-app-autheap-client/  # LuCI 界面包
│   ├── Makefile
│   └── root/usr/share/{luci,rpcd}/...  及 www/luci-static/resources/view/...
├── scripts/
│   ├── common.sh             # 公共变量与函数
│   ├── fetch-musl-cross.sh   # 下载 mipsel soft-float 工具链（MT7621 必需）
│   ├── cross-build.sh        # 交叉编译出 autheap 二进制（自动处理 build-std）
│   ├── build-ipk.sh          # 用产物构建 .ipk
│   ├── build-apk.sh          # 用产物构建 .apk
│   └── build-luci.sh         # 构建 LuCI 界面 .ipk / .apk
└── staging/                  # 运行脚本时生成的中间/产物目录（自动创建）
```

> **CI/CD**：`.github/workflows/ci.yml`（GitHub Actions）与 `.forgejo/workflows/*.yml`
> （自建 Forgejo runner）都会交叉编译三种 musl 目标并打 `.ipk`/`.apk`/`.deb`，
> 在 tag 或手动触发时自动**递增版本号**并创建 Release（共用 `scripts/` 下的脚本）。
> 本地等价命令见下文。

> **版本号规则**：`0.1.x`，**每发布一次 release 把 `x` 加一**（`0.1.1` → `0.1.2` → `0.1.3`…）；
> 包名的 `-1`（apk 的 `-r1`）是包管理系统自己的 revision，**发行新版本时保持 `1`**，
> 发版本只动 `x`。递增命令：`./scripts/bump-version.sh`（`--commit --tag --push` 可直接发布）。

## 快速开始（不依赖完整 OpenWrt SDK）

在**宿主机**（x86_64 Linux 即可）执行：

```bash
# 1) 交叉编译。ARCH 用你设备的目标三/二元组，例如：
./packaging/openwrt/scripts/cross-build.sh aarch64-unknown-linux-musl
#   产物在 packaging/openwrt/staging/autheap

# 1b) mipsel/mipsel_24kc（MT7621 等旧机型）：Tier-3 目标，需 soft-float 工具链
./packaging/openwrt/scripts/fetch-musl-cross.sh
MIPSEL_SF_GCC=$PWD/musl-cross/bin/mipsel-sf-gcc \
  ./packaging/openwrt/scripts/cross-build.sh mipsel-unknown-linux-musl

# 2) 打 ipk
./packaging/openwrt/scripts/build-ipk.sh \
    --arch aarch64_cortex-a53 --version 0.1.2 --binary staging/autheap
#   产物：staging/AuthEAP-Client_0.1.2-1_aarch64_cortex-a53.ipk

# 3) 打 apk（24.10）
./packaging/openwrt/scripts/build-apk.sh \
    --arch aarch64 --version 0.1.2 --binary staging/autheap
#   产物：staging/AuthEAP-Client-0.1.2-r1.apk
```

> `--arch` 给 apk 用 `aarch64` / `x86_64` 这类；给 ipk 用 OpenWrt 架构名如
> `aarch64_cortex-a53` / `mipsel_24kc`。安装时请与固件架构一致。

### 设备端安装

```bash
# opkg / 老固件
opkg install AuthEAP-Client_0.1.3-1_<arch>.ipk
# apk / OpenWrt 24.10（自签名/未签名包需放开校验）
apk add --allow-untrusted AuthEAP-Client-0.1.3-r1.apk
# LuCI 界面包同理：luci-app-autheap-client_*.ipk / luci-app-autheap-client-*.apk
```

安装后即得到命令 **`autheap`**，可直接带参数启动（与桌面版一致）：

```bash
# 设备端（root）直接运行：
autheap -i eth0 -u 学号 -p 密码 --plugin rjv3
autheap --help

# 网络测试（诊断，不需要账号，也不占网卡认证）：
autheap --nettest 203.0.113.10        # 目标可写 IP / 主机名 / 主机:端口
autheap --nettest 203.0.113.10:8083 -i eth0
autheap --nettest                    # 留空：只检查本机与默认网关
# 退出码 0 = 未发现问题，1 = 有 ✘；ICMP 需要 root（CAP_NET_RAW）
```

> 服务方式：`/etc/init.d/autheap-client start|stop|restart`（供 LuCI/开机自启用）。

### LuCI 包

`luci-app-autheap-client` 依赖主程序包，安装后：

1. 网页 **网络 → AuthEAP-Client**，配置页按**每个插件/功能各自一个 Tab** 组织：
   - **基本设置**：开关机自启/后台模式、认证网卡、账号密码、后端、重试与超时等；
   - **插件**：启用哪些插件（逐行添加/删除，选项由 `autheap --list-plugins` 动态生成），
     以及 printer 的“打印收到的帧”；
   - **rjv3 锐捷**：锐捷 v3/v4 插件参数（心跳间隔、EAPOL 广播地址、DHCP 方式/脚本、
     服务名、版本串、伪造 DNS/序列号、二次认证等待次数）；
   - **beacon 保活**：目标 IP/端口/间隔/载荷（周期性发送自定义 TCP 数据包）；
   - **日志**：日志文件、大小上限、保留份数、自动清理间隔；
   - 页面下方固定显示**后端读取的配置文件**（可刷新/重新生成）。
2. **开机自启**：配置页的“开机自启”开关（或状态页的按钮）会调用
   `/etc/init.d/autheap-client enable|disable`（生成/移除 `/etc/rc.d` 软链）——
   勾选后随系统启动；未勾选时仍可在状态页随时手动启动/停止/重启；
3. **认证网卡**：接口栏为带系统网卡下拉的输入框（可下拉选择也可手动输入），
   数据来自 `/proc/net/dev`；
4. **其他插件**：除内置的 `rjv3 / printer / stats / report / beacon` 外，可在
   “插件”Tab 里逐行追加你自己写的插件名（需已编译进客户端），会以
   `--plugin <name>` 传给进程；
5. **状态页**只显示运行状态、开机自启、接口/账号、插件、beacon 目标与版本
   （**已移除日志相关内容**，日志统一在「日志」页），并带一个 **网络测试** 区域：
   填入目标 `IP / 主机[:端口]` 后点“开始测试”（留空则只测本机与默认网关），
   后端执行 `autheap --nettest <目标> --interface <认证网卡>`，页面直接展示报告 ——
   本机接口/网卡地址/网关/DNS、目标的域名解析、TCP 连接（默认端口 80）与
   ICMP 连通性，标 ✘ 的行表示该项有问题；
6. **日志页**可实时刷新（增量拉取）并查看历史 `.1/.2…`，支持自动刷新/加载完整历史/
   清空日志。菜单里的 状态/配置/日志 在多数主题下就是页面顶部的三个 Tab。

## 用 OpenWrt SDK / feeds 构建（可选）

如果愿意用官方 SDK（需自行在 SDK 内安装 Rust 工具链并把编译产物路径暴露出来），
把 `autheap-client/`、`luci-app-autheap-client/` 作为两个 package 目录放入 feeds，
或参考 Makefile 内注释。官方 SDK 会根据目标决定产出 `.ipk` 还是 `.apk`。

## 运行时配置（UCI）

安装后默认 `/etc/config/autheap-client`：

```
config autheap-client 'main'
	option enabled '0'           # 开机自启（由 LuCI/init enable|disable 同步；0=不自启）
	option ifname 'eth0'         # 认证网卡
	option username ''
	option password ''
	option backend 'raw'
	option plugin_rjv3 '1'
	option plugin_printer '0'
	option plugin_stats '0'
	option extra_plugins ''      # 其他插件名（空格/逗号分隔，需已编译进客户端）
	option beacon_dst ''          # beacon 插件：目标 IP（填了就自动启用 beacon）
	option beacon_port '8083'     # beacon 插件：目标端口
	option beacon_interval '60'   # beacon 插件：发送间隔（秒；0 = 禁用）
	option beacon_payload ''      # beacon 插件：载荷十六进制（留空 = 内置默认载荷）
	option heartbeat '20'          # 心跳间隔（秒，0 关闭）
	option eap_bcast_addr '0'
	option dhcp_type '2'          # 0=none 1=double 2=after 3=before
	option dhcp_script ''
	option service 'internet'
	option version_str 'RG-SU For Linux V1.0'
	option fake_dns1 ''
	option fake_dns2 ''
	option fake_serial ''
	option max_dhcp_count '3'
	option max_retries '3'
	option max_failures '3'
	option stage_timeout '5'
	option log_file '/var/log/autheap-client.log'   # 日志文件（后端自己写入）
	option log_max_kb '256'       # 单文件上限，超过自动轮转清理
	option log_keep '3'           # 保留历史份数（.1/.2/…）
	option log_rotate_secs '300'  # 检查/自动清理间隔（秒）
```

## 配置文件（后端据此认证）

启动服务时，init 会先调用 `/usr/libexec/autheap-client/gen-config` 把上面的 UCI 渲染成
**`/etc/autheap-client.conf`**（`键 = 值` 文本，权限 0600），再用

```
/usr/bin/autheap --config /etc/autheap-client.conf
```

启动——即“LuCI 的配置做成配置文件，后端调用配置文件认证”。LuCI 保存配置时（rpcd
`set_config`）也会立刻重刷该文件；配置页底部可直接查看/重新生成它。
也可以手动运行：`autheap --config /etc/autheap-client.conf`（命令行参数优先于文件）。

## 日志：实时刷新 + 自动清理

- 后端用 `--log-file`（来自配置文件的 `log_file`）**追加**写日志并逐行 flush，
  因此 LuCI 「日志」页（增量流式拉取）与 `tail -f` 都是**实时**的；`--daemon` 模式下
  也不会因为重定向而丢日志。
- **自动清理**：日志超过 `log_max_kb` 就自动轮转为 `xxx.log.1`，旧的依次后移，
  只保留 `log_keep` 份，超出者删除；`log_rotate_secs` 控制检查间隔（默认 300 秒）。
- LuCI 的「日志」页可选择查看 **当前日志** 或历史 `.1/.2…`，支持 自动刷新 /
  加载完整历史 / 清空日志。

## beacon 插件（周期向指定 IP 发数据包）

在 LuCI 「配置」页的 “beacon 保活” 一栏填 **目标 IP**（即启用）、端口、间隔与载荷即可：

- 源地址 = 运行本机真实地址（内核按路由填写），**不写死任何 IP**；
- 目的地址 = 你填写的 IP/端口；
- 载荷默认内置，留空即用内置默认值（需要更换时填自己抓包 `Data` 的十六进制）；
- 每次发送前重新构造数据包，**时间即发送时刻**；
- 命令行等价：`--plugin beacon --beacon-dst <IP> --beacon-port <PORT> --beacon-interval <SECS>`
  （如需覆盖载荷再加 `--beacon-payload <HEX>`）。


## 排错

- 打不开原始套接字 → 用 root/`procd` 运行并确保进程带 `CAP_NET_RAW`；
- 24.10 装 apk 报签名错误 → 加上 `--allow-untrusted`；
- 路由器没有对应 Rust 目标 → `rustup target add <triple>`（见 `cross-build.sh`）；
- **LuCI 报 `Access denied (-32002)`**（如 `log_files` / `config_file` / `gen_config` /
  `log_clear` / `nettest`）→ 升级 LuCI 包后 **ACL 需要重新加载**：在设备上执行
  `/etc/init.d/rpcd restart`，然后**退出并重新登录 LuCI**（ACL 随会话缓存），最后
  `Ctrl+F5` 强刷页面。新版「日志」页在 `log_files` 不可用时会自动改用 `status`
  里的文件列表，因此即使 ACL 未刷新也能正常看日志；
- **状态页「网络测试」提示 `✘`** → 先看报告里哪一项失败：
  `DNS 解析` 失败说明设备自身 DNS 不通（检查接口/DNS）；
  `TCP 连接` 失败说明目标端口未监听或被防火墙拦（默认端口 80，可用 `目标:端口` 指定）；
  `ICMP ping` 写“跳过（需要 root / CAP_NET_RAW 权限）”说明当前进程没有原始套接字权限，
  服务本身以 root 运行，正常部署下不会跳过；
- **命令行的 `autheap --nettest` 不需要账号/配置**，可在认证失败时用来区分
  “网络本身不通”与“账号/协议问题”。
- LuCI 不显示 → 检查 `/etc/config/rpcd` 是否含 luci 服务、并重启 `rpcd` 与 `luci`。
