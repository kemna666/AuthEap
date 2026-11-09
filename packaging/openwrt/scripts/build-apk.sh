#!/bin/sh
# 生成 OpenWrt 24.10+ 的 .apk 包（apk 工具格式，未签名）。
#
# 用法:
#   build-apk.sh --arch aarch64 \
#                [--version 0.1.43] [--release 1] \
#                [--binary staging/autheap]
# 输出: staging/AuthEAP-Client-<version>-r<release>_<arch>.apk
#
# 设备端安装（未签名包）:
#   apk add --allow-untrusted AuthEAP-Client-0.1.42-r1_<arch>.apk
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

[ -n "$ARCH" ] || die "需要 --arch（如 aarch64 / x86_64 / mipsel）"
[ -n "$BINARY" ] || BINARY="$STAGE/autheap"
require_binary "$BINARY"
mkdir -p "$STAGE"

NAME="AuthEAP-Client"
PKG="${NAME}-${VERSION}-r${RELEASE}"
# apk 的包名不带架构，但 7 个架构的产物会放在同一个 staging 目录，文件名必须带架构，
# 否则后构建的包会覆盖前面的包。
OUT="$STAGE/${PKG}_${ARCH}.apk"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 组装数据文件（与 ipk 相同的安装布局）
mkdir -p "$WORK/pkg/usr/bin" "$WORK/pkg/etc/init.d" "$WORK/pkg/etc/config" \
         "$WORK/pkg/usr/libexec/autheap-client"
cp "$BINARY" "$WORK/pkg/usr/bin/autheap"
cp "$FILES_ROOT/etc/init.d/autheap-client" "$WORK/pkg/etc/init.d/autheap-client"
cp "$FILES_ROOT/etc/config/autheap-client"  "$WORK/pkg/etc/config/autheap-client"
cp "$FILES_ROOT/usr/libexec/autheap-client/gen-config" \
   "$WORK/pkg/usr/libexec/autheap-client/gen-config"
cp "$FILES_ROOT/usr/libexec/autheap-client/mline-setup" \
   "$WORK/pkg/usr/libexec/autheap-client/mline-setup"
chmod 0755 "$WORK/pkg/usr/bin/autheap"
chmod 0755 "$WORK/pkg/etc/init.d/autheap-client"
chmod 0755 "$WORK/pkg/usr/libexec/autheap-client/gen-config"
chmod 0755 "$WORK/pkg/usr/libexec/autheap-client/mline-setup"
chmod 0600 "$WORK/pkg/etc/config/autheap-client"

# OpenWrt apk 的 libc 依赖使用 so:libc.musl-<soname>.so.1。
# 注意：软件包架构名与解释器 soname 并不总是一致（armv6/armv7 都是 armhf，
# mipsel/mips 是软浮点 mipsel-sf/mips-sf），不能直接用 $ARCH。
case "$ARCH" in
  x86_64*)  LIBC_SONAME=x86_64 ;;
  i386*|i486*|i586*|i686*) LIBC_SONAME=i386 ;;
  aarch64*) LIBC_SONAME=aarch64 ;;
  riscv64*) LIBC_SONAME=riscv64 ;;
  arm*)     LIBC_SONAME=armhf ;;
  mipsel*)  LIBC_SONAME=mipsel-sf ;;
  mips*)    LIBC_SONAME=mips-sf ;;
  *)        LIBC_SONAME="$ARCH" ;;
esac

# .PKGINFO（apk v2 格式；加 depend 可在此处追加多行）
cat > "$WORK/pkg/.PKGINFO" <<EOF
pkgname = $NAME
pkgver = ${VERSION}-r${RELEASE}
arch = $ARCH
size = $(du -sk "$WORK/pkg" | cut -f1)
origin = $NAME
pkgdesc = $DEFAULT_DESC
url = https://github.com/updateing/minieap
commit =
maintainer = $DEFAULT_MAINTAINER
license = GPL-3.0
replaces =
provides =
depend = so:libc.musl-$LIBC_SONAME.so.1
EOF

# tar.gz 打包（apk 即 gzip 的 tar，内部 .PKGINFO + 文件）
(cd "$WORK/pkg" && tar czf "$OUT" .PKGINFO usr etc)

info "已生成（未签名）: $OUT"
info "安装: apk add --allow-untrusted $OUT"
