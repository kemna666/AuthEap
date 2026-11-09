#!/bin/sh
# 在宿主机交叉编译 autheap-client 二进制（musl 目标，适配 OpenWrt）。
#
# 用法:
#   cross-build.sh [TRIPLE] [--features backend-pcap]
# 默认 TRIPLE = aarch64-unknown-linux-musl
# 常见:
#   x86_64-unknown-linux-musl        (x86_64)
#   aarch64-unknown-linux-musl       (aarch64_cortex-a53/a72)
#   armv7-unknown-linux-musleabihf   (arm_cortex-a7/a9/a15)
#   arm-unknown-linux-musleabihf     (arm_arm1176jzf-s_vfp 等 armv6)
#   riscv64gc-unknown-linux-musl     (riscv64_riscv64)
#   mipsel-unknown-linux-musl        (mipsel_24kc / MT7621，软浮点)
#   mips-unknown-linux-musl          (mips_24kc 等大端 MIPS，软浮点)
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

# ---------- 自动版本号 ----------
# 直接运行 cross-build.sh 时视为一次新构建，自动 patch+1；
# ci-build-package.sh / build-deb.sh 等父脚本会通过 VERSION 或
# AUTHEAP_VERSION_BUMPED=1 传入已递增的版本，避免同一次构建重复递增。
if [ -z "${AUTHEAP_BUILD_VERSION:-}" ]; then
  if [ -z "${VERSION:-}" ] \
     && [ "${AUTHEAP_VERSION_BUMPED:-0}" != 1 ] \
     && [ "${AUTHEAP_NO_VERSION_BUMP:-0}" != 1 ]; then
    VERSION="$("$REPO/scripts/auto-version.sh")"
    export VERSION
    export AUTHEAP_VERSION_BUMPED=1
    info "自动版本号: $VERSION"
  fi
fi
# 显式 VERSION 优先于外部可能残留的 AUTHEAP_BUILD_VERSION。
[ -n "${VERSION:-}" ] && export AUTHEAP_BUILD_VERSION="$VERSION"

info "交叉编译目标: $TRIPLE  feature: ${FEATURES:-（无，默认 raw 后端）}"

# ---------- Tier-3 目标（mipsel / mips）：需要 build-std + soft-float 工具链 ----------
USE_BUILD_STD=0
case "$TRIPLE" in
  mipsel-unknown-linux-musl|mips-unknown-linux-musl|mipsel-unknown-linux-gnu)
    USE_BUILD_STD=1
    ;;
esac

if [ "$USE_BUILD_STD" = 1 ]; then
  # mipsel/mips 的 rust target 无预编译 std，必须 -Z build-std（rust-src 组件）。
  # 链接器必须是 **soft-float** musl 工具链（MT7621 等 MIPS 路由器无 FPU）。
  LINKER=""; LINKER_ENV=""
  case "$TRIPLE" in
    mipsel-*)
      LINKER="${MIPSEL_SF_GCC:-}"
      LINKER_ENV=CARGO_TARGET_MIPSEL_UNKNOWN_LINUX_MUSL_LINKER
      CANDIDATES="$REPO/musl-cross/bin/mipsel-sf-gcc
$HOME/musl-cross/bin/mipsel-sf-gcc
$(command -v mipsel-linux-muslsf-gcc 2>/dev/null || true)"
      TOOL_ENV='MIPSEL_SF_GCC'
      ;;
    mips-*)
      LINKER="${MIPS_SF_GCC:-}"
      LINKER_ENV=CARGO_TARGET_MIPS_UNKNOWN_LINUX_MUSL_LINKER
      CANDIDATES="$REPO/musl-cross-mips/bin/mips-sf-gcc
$HOME/musl-cross-mips/bin/mips-sf-gcc
$(command -v mips-linux-muslsf-gcc 2>/dev/null || true)"
      TOOL_ENV='MIPS_SF_GCC'
      ;;
  esac
  if [ -z "$LINKER" ]; then
    printf '%s\n' "$CANDIDATES" | while IFS= read -r cand; do
      [ -n "$cand" ] && [ -x "$cand" ] && printf '%s\n' "$cand"
    done > /tmp/autheap-linker.$$
    LINKER="$(head -n1 /tmp/autheap-linker.$$ 2>/dev/null || true)"
    rm -f /tmp/autheap-linker.$$
  fi
  [ -n "$LINKER" ] || die "缺少 MIPS soft-float 工具链；先运行 packaging/openwrt/scripts/fetch-musl-cross.sh（mipsel）或 MUSL_CROSS_ARCH=mips 版本，或用 $TOOL_ENV=<path> 指定"

  info "soft-float 链接器: $LINKER"
  eval "export $LINKER_ENV=\"$LINKER\""
  export RUSTFLAGS="${RUSTFLAGS:-} -C panic=abort -C link-self-contained=no"
  export CARGO_PROFILE_RELEASE_LTO=false

  if command -v rustup >/dev/null 2>&1; then
    rustup component add rust-src >/dev/null 2>&1 || info "提示: 无法自动安装 rust-src，请手动 rustup component add rust-src"
  fi

  info "cargo build --release --target $TRIPLE -Z build-std=std,panic_abort"
  (cd "$REPO" && RUSTC_BOOTSTRAP=1 cargo build --release --target "$TRIPLE" \
      -Z build-std=std,panic_abort $FEATURES) || die "cargo 构建失败"
else
  # 常见交叉目标的默认链接器（也可用 CARGO_TARGET_*_LINKER 覆盖）。
  LINKER_ENV=""; LINKER=""
  case "$TRIPLE" in
    aarch64-unknown-linux-musl)
      LINKER_ENV=CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER
      eval "LINKER=\${$LINKER_ENV:-aarch64-linux-gnu-gcc}"
      ;;
    armv7-unknown-linux-musleabihf)
      LINKER_ENV=CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER
      eval "LINKER=\${$LINKER_ENV:-arm-linux-gnueabihf-gcc}"
      ;;
    arm-unknown-linux-musleabihf)
      LINKER_ENV=CARGO_TARGET_ARM_UNKNOWN_LINUX_MUSLEABIHF_LINKER
      eval "LINKER=\${$LINKER_ENV:-arm-linux-gnueabihf-gcc}"
      ;;
    riscv64gc-unknown-linux-musl)
      LINKER_ENV=CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER
      eval "LINKER=\${$LINKER_ENV:-riscv64-linux-gnu-gcc}"
      # 该 target 默认动态 musl；若链接器是 glibc gcc，会找不到 musl Scrt1.o。
      # 强制静态 musl + Rust 自带 crt/libc，保证本地与 CI 行为一致。
      _rv_flags="${CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS:-}"
      case " $_rv_flags " in
        *"+crt-static"*) ;;
        *) _rv_flags="$_rv_flags -C target-feature=+crt-static" ;;
      esac
      case " $_rv_flags " in
        *"link-self-contained=yes"*) ;;
        *) _rv_flags="$_rv_flags -C link-self-contained=yes" ;;
      esac
      export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$_rv_flags"
      ;;
  esac
  if [ -n "$LINKER_ENV" ]; then
    command -v "$LINKER" >/dev/null 2>&1 || \
      die "缺少交叉链接器 $LINKER；请安装对应 gcc-<arch>-linux-gnu 包，或用 $LINKER_ENV=<path> 指定"
    info "链接器: $LINKER"
    eval "export $LINKER_ENV=\"$LINKER\""
  fi

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
