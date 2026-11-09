#!/bin/sh
# 构建「仅 CLI」版本（目标：桌面 Linux 宿主机）。
#
# 产物是纯命令行可执行文件（不含 OpenWrt/LuCI 任何内容），按内部标识
# autheap-client 命名输出到 packaging/desktop/out/。
#
# 用法:
#   ./build-desktop.sh                 # 默认 raw 后端（AF_PACKET，Linux，需 root）
#   ./build-desktop.sh --features backend-pcap   # 使用 libpcap 后端（需先装 libpcap-dev）
set -eu

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$REPO/packaging/desktop/out"
FEATURES=""

for arg in "$@"; do
	case "$arg" in
		--features=*) FEATURES="--features ${arg#*=}" ;;
		--features)   FEATURES="--features backend-pcap" ;;
		*) echo "[!] 未知参数: $arg" >&2; echo "    用法: build-desktop.sh [--features backend-pcap]" >&2; exit 1 ;;
	esac
done

# 每次直接编译桌面版时自动递增版本号；build-deb.sh 会传入 VERSION 并跳过。
if [ -z "${VERSION:-}" ] \
   && [ "${AUTHEAP_VERSION_BUMPED:-0}" != 1 ] \
   && [ "${AUTHEAP_NO_VERSION_BUMP:-0}" != 1 ]; then
	VERSION="$("$REPO/scripts/auto-version.sh")"
	export VERSION
	export AUTHEAP_VERSION_BUMPED=1
	echo "[*] 自动版本号: $VERSION"
fi
[ -n "${VERSION:-}" ] && export AUTHEAP_BUILD_VERSION="$VERSION"

echo "[*] 编译桌面 Linux 版 CLI（autheap）${FEATURES:+（$FEATURES）}..."
(cd "$REPO" && cargo build --release $FEATURES)

mkdir -p "$OUT"
cp "$REPO/target/release/autheap" "$OUT/autheap"
chmod +x "$OUT/autheap"

echo "[*] 完成: $OUT/autheap"
echo "[*] 快速试运行（raw 后端需 root）:"
echo "    sudo $OUT/autheap --help"
echo "[*] 安装到系统: sudo packaging/desktop/install.sh（或 build-deb.sh 打 .deb）"
