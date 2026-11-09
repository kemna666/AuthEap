#!/bin/sh
# 公共工具：路径与输出函数。被 build-ipk.sh / build-apk.sh / cross-build.sh 使用。
set -eu

REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
PKG_DIR="$REPO/packaging/openwrt"
STAGE="$PKG_DIR/staging"
FILES_ROOT="$PKG_DIR/autheap-client/root"

DEFAULT_VERSION="0.1.43"
DEFAULT_RELEASE="1"
DEFAULT_MAINTAINER="AuthEAP-Client contributors"
DEFAULT_DESC="可扩展的 802.1x (EAP-MD5) 客户端（锐捷 v3/v4 插件，Rust 版）"

info() { echo "[*] $*"; }
die()  { echo "[!] $*" >&2; exit 1; }

require_binary() {
    [ -n "${1:-}" ] && [ -f "$1" ] && [ -x "$1" ] && return 0
    die "缺少可执行文件: ${1:-（未设置 --binary）}，请先运行 cross-build.sh"
}
