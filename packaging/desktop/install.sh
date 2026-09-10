#!/bin/sh
# 安装桌面「仅 CLI」版到系统（非 apt 发行版）：
#   /usr/bin/autheap  +  /etc/systemd/system/autheap.service
#   /etc/autheap.env（密码环境文件，600）
#
# Debian/Ubuntu 请优先用 build-deb.sh 打 .deb 后用 apt 安装。
# 用法: sudo packaging/desktop/install.sh
set -eu

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$REPO/packaging/desktop/out/autheap"

[ -x "$BIN" ] || {
	echo "[!] 缺少产物 $BIN" >&2
	echo "    请先运行 packaging/desktop/build-desktop.sh" >&2
	exit 1
}

install -m 0755 "$BIN" /usr/bin/autheap
install -D -m 0644 "$REPO/packaging/desktop/autheap.service" \
	/etc/systemd/system/autheap.service

if [ ! -f /etc/autheap.env ]; then
	install -D -m 0600 "$REPO/packaging/desktop/autheap.env" /etc/autheap.env
fi

systemctl daemon-reload

echo "[*] 已安装：/usr/bin/autheap"
echo "[*] 请编辑 /etc/autheap.env 填好 AUTHEAP_PASSWORD（或去掉环境文件改用 --password）"
echo "    并把 /etc/systemd/system/autheap.service 里的 -i eth0 / -u <USER> 改为你的网卡和账号"
echo "[*] 然后: systemctl enable --now autheap"
echo "[*] 直接命令行: sudo autheap -i eth0 -u 学号 --plugin rjv3"
