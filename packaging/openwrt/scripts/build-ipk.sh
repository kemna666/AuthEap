#!/bin/sh
# 生成 OpenWrt .ipk 包（opkg 传统格式）。
#
# 用法:
#   build-ipk.sh --arch aarch64_cortex-a53 \
#                [--version 0.1.43] [--release 1] \
#                [--binary staging/autheap]
# 输出: staging/AuthEAP-Client_<version>-<release>_<arch>.ipk
set -eu
. "$(dirname "$0")/common.sh"

ARCH=""
VERSION="$(sed -n 's/^version[ \t]*=[ \t]*"\(.*\)".*/\1/p' "$REPO/Cargo.toml" | head -n1)"
RELEASE="$DEFAULT_RELEASE"
BINARY=""

while [ $# -gt 0 ]; do
  case "$1" in
    --arch)   ARCH="$2";    shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --release) RELEASE="$2"; shift 2 ;;
    --binary) BINARY="$2";  shift 2 ;;
    *) die "未知参数: $1" ;;
  esac
done

[ -n "$ARCH" ] || die "需要 --arch（OpenWrt 架构名，如 aarch64_cortex-a53 / mipsel_24kc）"
[ -n "$BINARY" ] || BINARY="$STAGE/autheap"
require_binary "$BINARY"
mkdir -p "$STAGE"

NAME="AuthEAP-Client"
PKG="${NAME}_${VERSION}-${RELEASE}_${ARCH}.ipk"
OUT="$STAGE/$PKG"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/control" "$WORK/data"
mkdir -p "$WORK/data/usr/bin" "$WORK/data/etc/init.d" "$WORK/data/etc/config"
mkdir -p "$WORK/data/usr/libexec/autheap-client"

cp "$BINARY" "$WORK/data/usr/bin/autheap"
cp "$FILES_ROOT/etc/init.d/autheap-client" "$WORK/data/etc/init.d/autheap-client"
cp "$FILES_ROOT/etc/config/autheap-client"  "$WORK/data/etc/config/autheap-client"
# UCI → 后端配置文件 渲染脚本（init.d / LuCI 调用）
cp "$FILES_ROOT/usr/libexec/autheap-client/gen-config" \
   "$WORK/data/usr/libexec/autheap-client/gen-config"
cp "$FILES_ROOT/usr/libexec/autheap-client/mline-setup" \
   "$WORK/data/usr/libexec/autheap-client/mline-setup"

# 权限
chmod 0755 "$WORK/data/usr/bin/autheap"
chmod 0755 "$WORK/data/etc/init.d/autheap-client"
chmod 0755 "$WORK/data/usr/libexec/autheap-client/gen-config"
chmod 0755 "$WORK/data/usr/libexec/autheap-client/mline-setup"
chmod 0600 "$WORK/data/etc/config/autheap-client"  # 含密码，收紧权限

# control
cat > "$WORK/control/control" <<EOF
Package: $NAME
Version: $VERSION-$RELEASE
Depends: libc, libgcc
Section: net
Priority: optional
Architecture: $ARCH
Maintainer: $DEFAULT_MAINTAINER
Description: $DEFAULT_DESC
EOF
echo "/etc/config/autheap-client" > "$WORK/control/conffiles"

# 打内层压缩包（owner/group 归 0，对齐官方包）
find "$WORK/data" -type d -exec chmod 0755 {} +
(cd "$WORK/control" && tar --owner=0 --group=0 -czf ../control.tar.gz .)
(cd "$WORK/data"    && tar --owner=0 --group=0 -czf ../data.tar.gz .)
printf "2.0\n" > "$WORK/debian-binary"

# 外层用 tar.gz 包裹（ImmortalWrt 24.10/OpenWrt 新 opkg 认 tar 包，不认 ar）：
#   官方 ipk = gzip tar，内含 ./debian-binary ./data.tar.gz ./control.tar.gz
tar --owner=0 --group=0 -czf "$OUT" -C "$WORK" ./debian-binary ./control.tar.gz ./data.tar.gz

info "已生成: $OUT"
