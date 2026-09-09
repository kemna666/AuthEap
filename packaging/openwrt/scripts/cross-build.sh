#!/bin/sh
# 在宿主机交叉编译 autheap-client 二进制（musl 目标，适配 OpenWrt）。
#
# 用法:
#   cross-build.sh [TRIPLE] [--features backend-pcap]
# 默认 TRIPLE = aarch64-unknown-linux-musl
# 常见:
#   x86_64-unknown-linux-musl
#   aarch64-unknown-linux-musl
#   mipsel-unknown-linux-musl
#   arm-unknown-linux-musleabi (mipsel_24kc / ar71xx 等)
# 产物: packaging/openwrt/staging/autheap
set -eu
. "$(dirname "$0")/common.sh"

TRIPLE="${1:-aarch64-unknown-linux-musl}"
shift 2>/dev/null || true
FEATURES=""

for arg in "$@"; do
  case "$arg" in
    --features=*) FEATURES="--features ${arg#*=}" ;;
    --features) FEATURES="--features backend-pcap" ;;
  esac
done

info "交叉编译目标: $TRIPLE  feature: ${FEATURES:-（无，默认 raw 后端）}"

# 确保 rustup 安装了目标
if command -v rustup >/dev/null 2>&1; then
  rustup target add "$TRIPLE" >/dev/null 2>&1 || \
    die "无法安装目标 $TRIPLE，请手动执行: rustup target add $TRIPLE"
fi

# 需要静态 libc (musl) 目标通常已静态链接，无需额外系统依赖
(cd "$REPO" && cargo build --release --target "$TRIPLE" $FEATURES) \
  || die "cargo 构建失败"

mkdir -p "$STAGE"
BIN="$REPO/target/$TRIPLE/release/autheap"
require_binary "$BIN"
cp "$BIN" "$STAGE/autheap"
chmod +x "$STAGE/autheap"

info "完成：$STAGE/autheap"
info "接下来可执行 build-ipk.sh / build-apk.sh"
