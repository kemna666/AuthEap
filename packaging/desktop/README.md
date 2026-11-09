# AuthEAP-Client —— 桌面 Linux「仅 CLI」打包

这里提供面向**桌面 Linux** 的“仅命令行”产物，不含任何 OpenWrt/LuCI 内容
（那部分在 `packaging/openwrt/`，与本目录互不影响）。

安装后提供命令 **`autheap`**，可直接带参数启动：

```bash
sudo autheap -i eth0 -u 学号 -p 密码 --plugin rjv3

# 用配置文件（推荐把一长串参数写进文件）
sudo autheap --config /etc/autheap-client.conf

# 日志：追加写入、实时刷新，并自动轮转清理（超过 256KB 轮转，保留 3 份）
sudo autheap -i eth0 -u 学号 -p 密码 --plugin rjv3 \
     --log-file /var/log/autheap.log --log-max-kb 256 --log-keep 3 --log-rotate-secs 300

# beacon 插件：每 60 秒向指定 IP:端口 发送一段自定义 TCP 载荷
sudo autheap -i eth0 -u 学号 -p 密码 --plugin rjv3 --plugin beacon \
     --beacon-dst ip --beacon-port port --beacon-interval 60
```

支持两种安装方式：

- **apt / dpkg（Debian、Ubuntu 及派生发行版，推荐）**：构建 `.deb` 后用
  `apt install` 安装、`apt remove/purge` 卸载，并可用本地 apt 仓库 `apt update` 管理。
- **源码安装（其它发行版）**：`build-desktop.sh` 编译后用 `install.sh` 装到 `/usr/bin` + systemd。

```
packaging/desktop/
├── build-desktop.sh        # 编译桌面 CLI（默认 raw；--features backend-pcap 可选）
├── build-deb.sh            # 构建 .deb（apt/dpkg 安装，命令 autheap）
├── make-apt-repo.sh        # 生成本地 apt 仓库（可选）
├── install.sh              # 非 apt 发行版的系统安装（/usr/bin + systemd）
├── autheap.service         # systemd 单元（示例）
├── autheap.env             # 密码环境文件示例（AUTHEAP_PASSWORD）
├── README.md
└── out/                    # 构建产物输出（.deb / 二进制 / apt-repo，自动生成，已 gitignore）
```

## A. 通过 apt 安装 / 卸载（Debian/Ubuntu）

```bash
# 1) 构建（默认 raw 后端；需要 libpcap 时加 --features backend-pcap）
./packaging/desktop/build-deb.sh

# 2) 安装（apt 会自动处理依赖；dpkg -i 亦可）
sudo apt install ./packaging/desktop/out/autheap_0.1.3-1_amd64.deb

# 3) 直接命令行启动（raw 后端需 root）
sudo autheap -i eth0 -u 学号 -p 密码 --plugin rjv3
sudo autheap --help

# 4) （可选）编辑密码环境文件（conffile，升级不覆盖）
sudoedit /etc/autheap.env        # AUTHEAP_PASSWORD=你的密码
#    并启用 systemd 服务（先按需改 ExecStart 的 -i/-u）
sudo systemctl enable --now autheap

# 5) 卸载 / 完全清除
sudo apt remove autheap          # 卸载（保留 /etc/autheap.env）
sudo apt purge  autheap          # 完全清除（连同配置一起删）
```

### 可选：发布为本地 apt 仓库

```bash
./packaging/desktop/make-apt-repo.sh
echo "deb [trusted=yes] file:$(pwd)/packaging/desktop/out/apt-repo ./" \
     | sudo tee /etc/apt/sources.list.d/autheap.list
sudo apt update
sudo apt install autheap         # 之后升级/卸载都走 apt
sudo autheap -i eth0 -u 学号 --plugin rjv3
sudo rm /etc/apt/sources.list.d/autheap.list && sudo apt update   # 移除源
```

> `.deb` 依赖 `libc6`；架构由 `dpkg --print-architecture` 自动填。
> 仓库为 `trusted=yes`（未签名），仅建议用于可信本机/内网。

## B. 非 apt 发行版：源码安装

```bash
# 1) 编译（默认 raw 后端，仅 Linux）
#    每次执行会自动把 patch 版本 +1（0.1.42 → 0.1.43），并同步 Cargo.toml/Cargo.lock
./packaging/desktop/build-desktop.sh
#    需要 libpcap 时： ./packaging/desktop/build-desktop.sh --features backend-pcap
#    想固定版本/跳过递增： VERSION=0.1.42 ./packaging/desktop/build-desktop.sh

# 2) 直接试运行（raw 后端需 root）
sudo ./packaging/desktop/out/autheap -i eth0 -u 学号 -p 密码 --plugin rjv3

# 3) 安装为系统服务
sudo ./packaging/desktop/install.sh
sudo vim /etc/autheap.env                       # 填 AUTHEAP_PASSWORD=你的密码
sudo systemctl enable --now autheap
```

> 说明：Rust 工程的库 crate 仍叫 `autheap`，但命令行产物已统一命名为 `autheap`
> （`cargo build --release` 产出 `target/release/autheap`），与 OpenWrt/桌面安装的命令一致。
> 密码环境变量仍为 `AUTHEAP_PASSWORD`（命令行 `-p` 亦可）。
