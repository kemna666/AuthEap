#!/bin/sh
# 生成 luci-app-autheap-client 的 .ipk 与 .apk（架构 all）。
#
# 用法: build-luci.sh [--version 0.1.0]
# 输出: staging/luci-app-autheap-client_*.ipk 与 luci-app-autheap-client-*.apk
set -eu
. "$(dirname "$0")/common.sh"

VERSION="${1:-2026.09.09}"
LUCI_ROOT="$PKG_DIR/luci-app-autheap-client/root"
NAME="luci-app-autheap-client"
ARCH="all"

[ -d "$LUCI_ROOT" ] || die "缺少 $LUCI_ROOT"
mkdir -p "$STAGE"

# ---------------- .ipk ----------------
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/control" "$WORK/data"
(cd "$LUCI_ROOT" && tar cpf - .) | (cd "$WORK/data" && tar xpf -)

cat > "$WORK/control/control" <<EOF
Package: $NAME
Version: $VERSION-1
Depends: AuthEAP-Client, luci-base, rpcd-mod-rpcsys
Section: luci
Priority: optional
Architecture: $ARCH
Maintainer: $DEFAULT_MAINTAINER
Description: LuCI support for AuthEAP-Client (802.1x client)
EOF
# 内层 owner/group 归 0，外层用 tar.gz 包裹（对齐 ImmortalWrt 24.10 新 opkg 格式）
find "$WORK/data" -type d -exec chmod 0755 {} +
(cd "$WORK/control" && tar --owner=0 --group=0 -czf ../control.tar.gz .)
(cd "$WORK/data"    && tar --owner=0 --group=0 -czf ../data.tar.gz .)
printf "2.0\n" > "$WORK/debian-binary"
IPK="$STAGE/${NAME}_${VERSION}-1_all.ipk"
tar --owner=0 --group=0 -czf "$IPK" -C "$WORK" ./debian-binary ./control.tar.gz ./data.tar.gz
info "已生成: $IPK"

# ---------------- .apk ----------------
WORK2="$(mktemp -d)"
trap 'rm -rf "$WORK2" "$WORK"' EXIT
mkdir -p "$WORK2/pkg"
(cd "$LUCI_ROOT" && tar cpf - .) | (cd "$WORK2/pkg" && tar xpf -)
cat > "$WORK2/pkg/.PKGINFO" <<EOF
pkgname = $NAME
pkgver = ${VERSION}-r1
arch = $ARCH
size = $(du -sk "$WORK2/pkg" | cut -f1)
origin = $NAME
pkgdesc = LuCI support for AuthEAP-Client (802.1x client)
url = https://github.com/updateing/minieap
maintainer = $DEFAULT_MAINTAINER
license = GPL-3.0
depend = AuthEAP-Client
EOF
APK="$STAGE/${NAME}-${VERSION}-r1.apk"
(cd "$WORK2/pkg" && tar czf "$APK" .PKGINFO $(ls -A | grep -v '^\.PKGINFO$'))
info "已生成: $APK"
