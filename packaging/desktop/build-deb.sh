#!/bin/sh
# 构建桌面 Linux 的 .deb 包（可用 apt/dpkg 安装与卸载；安装后命令为 autheap）。
#
# 依赖：dpkg-deb（Ubuntu/Debian 自带）+ Rust 工具链（编译 CLI）。
#
# 用法:
#   ./build-deb.sh                         # 默认 raw 后端
#   ./build-deb.sh --features backend-pcap
#   VERSION=0.1.42 ./build-deb.sh
#
# 产物: packaging/desktop/out/autheap_<version>-<release>_<arch>.deb
set -eu

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
PKGDIR="$REPO/packaging/desktop"
OUT="$PKGDIR/out"

command -v dpkg-deb >/dev/null 2>&1 || {
	echo "[!] 未找到 dpkg-deb，请使用 Debian/Ubuntu 系发行版" >&2
	exit 1
}

# 每次直接执行 build-deb 视为一次构建：自动 patch+1；CI 会传 VERSION 跳过。
VERSION="${VERSION:-}"
if [ -z "$VERSION" ] \
   && [ "${AUTHEAP_VERSION_BUMPED:-0}" != 1 ] \
   && [ "${AUTHEAP_NO_VERSION_BUMP:-0}" != 1 ]; then
	VERSION="$("$REPO/scripts/auto-version.sh")"
	export VERSION
	export AUTHEAP_VERSION_BUMPED=1
	echo "[*] 自动版本号: $VERSION"
fi
VERSION="${VERSION:-$(sed -n 's/^version[ 	]*=[ 	]*"\(.*\)".*/\1/p' "$REPO/Cargo.toml" | head -n1)}"
export VERSION
export AUTHEAP_BUILD_VERSION="$VERSION"
RELEASE="${RELEASE:-1}"
NAME="autheap"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"

# 0) 确保 CLI 已编译到 out/autheap，且版本与本次 deb 一致
if [ ! -x "$OUT/$NAME" ] \
   || ! "$OUT/$NAME" --version 2>/dev/null | grep -Fqx "autheap $VERSION"; then
	echo "[*] 未找到匹配版本 $VERSION 的 $OUT/$NAME，先调用 build-desktop.sh ..."
	"$PKGDIR/build-desktop.sh" "$@"
fi
BIN="$OUT/$NAME"
[ -x "$BIN" ] || { echo "[!] 缺少二进制 $BIN" >&2; exit 1; }

mkdir -p "$OUT"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PKG="$WORK/pkg"

# ---------- data（$PKG 的根即安装根） ----------
mkdir -p \
	"$PKG/usr/bin" \
	"$PKG/etc" \
	"$PKG/lib/systemd/system" \
	"$PKG/usr/share/doc/$NAME"

install -m 0755 "$BIN" "$PKG/usr/bin/$NAME"
install -m 0600 "$PKGDIR/autheap.env" "$PKG/etc/autheap.env"
install -m 0644 "$PKGDIR/autheap.service" \
	"$PKG/lib/systemd/system/autheap.service"
install -m 0644 "$PKGDIR/README.md" "$PKG/usr/share/doc/$NAME/README.md"
cat > "$PKG/usr/share/doc/$NAME/copyright" <<EOF
AuthEAP-Client (Rust 版 802.1x/EAP-MD5 客户端，锐捷 v3/v4 插件)

License: GPL-3.0-or-later
Upstream: https://github.com/updateing/minieap
EOF

# ---------- DEBIAN/control 与维护脚本 ----------
mkdir -p "$PKG/DEBIAN"

SIZE_KB=$(du -sk "$PKG" | cut -f1)
cat > "$PKG/DEBIAN/control" <<EOF
Package: $NAME
Version: $VERSION-$RELEASE
Section: net
Priority: optional
Architecture: $ARCH
Depends: libc6 (>= 2.34)
Installed-Size: $SIZE_KB
Maintainer: AuthEAP-Client contributors
Homepage: https://github.com/updateing/minieap
Description: AuthEAP-Client 802.1x client (EAP-MD5, Ruijie v3/v4), Rust CLI
 可扩展的 802.1x (EAP-MD5) 认证客户端（锐捷 v3/v4 算法插件），桌面 Linux 仅 CLI 版。
 .
 安装后编辑 /etc/autheap.env 填入密码，再以 root 直接运行：
   autheap -i eth0 -u <学号> --plugin rjv3
 或配置 systemd 单元后：systemctl start autheap
EOF

cat > "$PKG/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1; then
	systemctl daemon-reload >/dev/null 2>&1 || true
fi
exit 0
EOF
cat > "$PKG/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1 && \
   systemctl is-active --quiet autheap.service 2>/dev/null; then
	systemctl stop autheap.service >/dev/null 2>&1 || true
fi
exit 0
EOF
cat > "$PKG/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1; then
	systemctl daemon-reload >/dev/null 2>&1 || true
fi
exit 0
EOF
chmod 0755 "$PKG/DEBIAN/postinst" "$PKG/DEBIAN/prerm" "$PKG/DEBIAN/postrm"

# conffile 声明（apt remove 保留 / purge 删除）
echo "/etc/autheap.env" > "$PKG/DEBIAN/conffiles"

# ---------- 组装 .deb ----------
(cd "$PKG" && find usr etc lib -type f -print0 | xargs -0 md5sum \
	> "$PKG/DEBIAN/md5sums")
DEB="$OUT/${NAME}_${VERSION}-${RELEASE}_${ARCH}.deb"
rm -f "$DEB"
dpkg-deb --build --root-owner-group "$PKG" "$DEB" >/dev/null

echo "[*] 已生成: $DEB"
echo "[*] 安装后直接命令行启动:"
echo "    sudo autheap -i eth0 -u 学号 -p 密码 --plugin rjv3"
echo "[*] 安装:   sudo apt install $DEB        （或 sudo dpkg -i $DEB）"
echo "[*] 卸载:   sudo apt remove autheap   （完全清除: sudo apt purge autheap）"
echo "[*] 本地仓库: 运行 make-apt-repo.sh 后用 apt update 安装"
