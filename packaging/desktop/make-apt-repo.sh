#!/bin/sh
# 生成本地 apt 仓库（把已构建的 .deb 组织成可被 apt update 的源），
# 之后即可用 apt 安装/升级/卸载，实现真正“通过 apt 管理器”管理。
#
# 依赖：dpkg-scanpackages（apt-utils 提供；Ubuntu/Debian 可 `sudo apt install apt-utils`）
#
# 用法:
#   1) ./build-deb.sh           # 先构建 .deb
#   2) ./make-apt-repo.sh       # 生成本地仓库 out/apt-repo/
#   3) 按提示把源写入 /etc/apt/sources.list.d/ 后 apt update/install
set -eu

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$REPO/packaging/desktop/out"
APTREPO="$OUT/apt-repo"

command -v dpkg-scanpackages >/dev/null 2>&1 || {
	echo "[!] 未找到 dpkg-scanpackages，请先安装 apt-utils" >&2
	exit 1
}

mkdir -p "$APTREPO"
rm -f "$APTREPO"/*.deb "$APTREPO/Packages" "$APTREPO/Packages.gz"
cp "$OUT"/autheap_*.deb "$APTREPO"/ 2>/dev/null || {
	echo "[!] $OUT 下没有 .deb，请先运行 build-deb.sh" >&2
	exit 1
}

ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"

# 平面仓库：同目录放 .deb + Packages.gz
(cd "$APTREPO" && dpkg-scanpackages -m . /dev/null 2>/dev/null | gzip -9c > Packages.gz)

cat > "$APTREPO/Release" <<EOF
Origin: AuthEAP-Client local repo
Label: AuthEAP-Client
Suite: stable
Codename: local
Architectures: $ARCH
Components: main
Description: AuthEAP-Client 桌面 CLI 本地 apt 仓库
EOF

echo "[*] 本地仓库已生成: $APTREPO"
echo
echo "把它注册为 apt 源（trusted=yes 表示跳过签名校验）:"
echo "  echo \"deb [trusted=yes] file:$APTREPO ./\" | sudo tee /etc/apt/sources.list.d/autheap.list"
echo
echo "然后即可用 apt 安装/升级/卸载:"
echo "  sudo apt update"
echo "  sudo apt install autheap             # 安装 / 升级"
echo "  sudo autheap -i eth0 -u 学号 --plugin rjv3   # 直接命令行启动"
echo "  sudo apt remove autheap              # 卸载"
echo "  sudo apt purge  autheap              # 完全清除（含 /etc/autheap.env）"
echo
echo "不再需要时删除源并刷新:"
echo "  sudo rm /etc/apt/sources.list.d/autheap.list && sudo apt update"
