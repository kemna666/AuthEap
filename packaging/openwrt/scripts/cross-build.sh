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

# ---------- Tier-3 目标（mipsel 等）：需要 build-std + soft-float 工具链 ----------
USE_BUILD_STD=0
case "$TRIPLE" in
  mipsel-unknown-linux-musl|mips-unknown-linux-musl|mipsel-unknown-linux-gnu)
    USE_BUILD_STD=1
    ;;
esac

if [ "$USE_BUILD_STD" = 1 ]; then
  # mipsel rust target 无预编译 std，必须 -Z build-std（rust-src 组件）。
  # 链接器必须是 **soft-float** musl 工具链（MT7621 无 FPU）。
  LINKER="${MIPSEL_SF_GCC:-}"
  if [ -z "$LINKER" ]; then
    for cand in "$REPO/musl-cross/bin/mipsel-sf-gcc" \
                "$HOME/musl-cross/bin/mipsel-sf-gcc" \
                "$(command -v mipsel-linux-muslsf-gcc 2>/dev/null || true)"; do
      if [ -n "$cand" ] && [ -x "$cand" ]; then LINKER="$cand"; break; fi
    done
  fi
  [ -n "$LINKER" ] || die "缺少 mipsel soft-float 工具链；先运行 scripts/fetch-musl-cross.sh，或用 MIPSEL_SF_GCC=<path> 指定"

  info "soft-float 链接器: $LINKER"
  export CARGO_TARGET_MIPSEL_UNKNOWN_LINUX_MUSL_LINKER="$LINKER"
  export RUSTFLAGS="${RUSTFLAGS:-} -C panic=abort -C link-self-contained=no"
  export CARGO_PROFILE_RELEASE_LTO=false

  if command -v rustup >/dev/null 2>&1; then
    rustup component add rust-src >/dev/null 2>&1 || info "提示: 无法自动安装 rust-src，请手动 rustup component add rust-src"
  fi

  info "cargo build --release --target $TRIPLE -Z build-std=std,panic_abort"
  (cd "$REPO" && RUSTC_BOOTSTRAP=1 cargo build --release --target "$TRIPLE" \
      -Z build-std=std,panic_abort $FEATURES) || die "cargo 构建失败"
else
  # 确保 rustup 安装了目标
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "$TRIPLE" >/dev/null 2>&1 || \
      die "无法安装目标 $TRIPLE，请手动执行: rustup target add $TRIPLE"
  fi

  # 需要静态 libc (musl) 目标通常已静态链接，无需额外系统依赖
  (cd "$REPO" && cargo build --release --target "$TRIPLE" $FEATURES) \
    || die "cargo 构建失败"
fi

mkdir -p "$STAGE"
BIN="$REPO/target/$TRIPLE/release/autheap"
require_binary "$BIN"
cp "$BIN" "$STAGE/autheap"
chmod +x "$STAGE/autheap"

info "完成：$STAGE/autheap"
info "接下来可执行 build-ipk.sh / build-apk.sh"
