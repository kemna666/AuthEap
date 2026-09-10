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
│       └── etc/config/autheap-client
├── luci-app-autheap-client/  # LuCI 界面包
│   ├── Makefile
│   └── root/usr/share/{luci,rpcd}/...  及 www/luci-static/resources/view/...
├── scripts/
│   ├── common.sh             # 公共变量与函数
│   ├── cross-build.sh        # 交叉编译出 autheap 二进制
│   ├── build-ipk.sh          # 用产物构建 .ipk
│   ├── build-apk.sh          # 用产物构建 .apk
│   └── build-luci.sh         # 构建 LuCI 界面 .ipk / .apk
└── staging/                  # 运行脚本时生成的中间/产物目录（自动创建）
```

## 快速开始（不依赖完整 OpenWrt SDK）

在**宿主机**（x86_64 Linux 即可）执行：

```bash
# 1) 交叉编译。ARCH 用你设备的目标三/二元组，例如：
./packaging/openwrt/scripts/cross-build.sh aarch64-unknown-linux-musl
#   产物在 packaging/openwrt/staging/autheap

# 2) 打 ipk
./packaging/openwrt/scripts/build-ipk.sh \
    --arch aarch64_cortex-a53 --version 0.1.0 --binary staging/autheap
#   产物：staging/AuthEAP-Client_0.1.0-1_aarch64_cortex-a53.ipk

# 3) 打 apk（24.10）
./packaging/openwrt/scripts/build-apk.sh \
    --arch aarch64 --version 0.1.0 --binary staging/autheap
#   产物：staging/AuthEAP-Client-0.1.0-r1.apk
```

> `--arch` 给 apk 用 `aarch64` / `x86_64` 这类；给 ipk 用 OpenWrt 架构名如
> `aarch64_cortex-a53` / `mipsel_24kc`。安装时请与固件架构一致。

### 设备端安装

```bash
# opkg / 老固件
opkg install AuthEAP-Client_0.1.0-1_<arch>.ipk
# apk / OpenWrt 24.10（自签名/未签名包需放开校验）
apk add --allow-untrusted AuthEAP-Client-0.1.0-r1.apk
# LuCI 界面包同理：luci-app-autheap-client_*.ipk / luci-app-autheap-client-*.apk
```

安装后即得到命令 **`autheap`**，可直接带参数启动（与桌面版一致）：

```bash
# 设备端（root）直接运行：
autheap -i eth0 -u 学号 -p 密码 --plugin rjv3
autheap --help
```

> 服务方式：`/etc/init.d/autheap-client start|stop|restart`（供 LuCI/开机自启用）。

### LuCI 包

`luci-app-autheap-client` 依赖主程序包，安装后：

1. 网页 **网络 → AuthEAP-Client** 填写接口、用户名/密码、锐捷选项；
2. **开机自启**：配置页的“开机自启”开关（或状态页的按钮）会调用
   `/etc/init.d/autheap-client enable|disable`（生成/移除 `/etc/rc.d` 软链）——
   勾选后随系统启动；未勾选时仍可在状态页随时手动启动/停止/重启；
3. **认证网卡**：接口栏为带系统网卡下拉的输入框（可下拉选择也可手动输入），
   数据来自 `/proc/net/dev`；
4. **其他插件**：除内置的 `rjv3 / printer / stats` 勾选外，可在“其他插件”里
   追加你自己写的插件名（空格/逗号分隔，需已编译进客户端），会以
   `--plugin <name>` 传给进程；
5. 状态页可看到运行状态、开机自启状态与最近日志。

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
```


## 排错

- 打不开原始套接字 → 用 root/`procd` 运行并确保进程带 `CAP_NET_RAW`；
- 24.10 装 apk 报签名错误 → 加上 `--allow-untrusted`；
- 路由器没有对应 Rust 目标 → `rustup target add <triple>`（见 `cross-build.sh`）；
- LuCI 不显示 → 检查 `/etc/config/rpcd` 是否含 luci 服务、并重启 `rpcd` 与 `luci`。
