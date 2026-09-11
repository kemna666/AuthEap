#!/bin/sh
# 下载并解压 musl.cc 的 mipsel **soft-float** 交叉工具链。
#
# 为什么需要它：MT7621 等 mipsel_24kc 目标没有硬件浮点，OpenWrt 用户态用的是
# soft-float musl（解释器 /lib/ld-musl-mipsel-sf.so.1），因此交叉链接必须用
# `mipsel-linux-muslsf-gcc`，不能用默认的 hard-float `mipsel-linux-musl-gcc`。
#
# 用法:
#   fetch-musl-cross.sh [目标目录]          # 默认 <仓库>/musl-cross
# 可用环境变量覆盖下载地址:
#   MUSL_CROSS_URL=https://musl.cc/mipsel-linux-muslsf-cross.tgz
#
# 完成后会得到 <目标目录>/bin/mipsel-sf-gcc（透传给 muslsf-gcc 的包装脚本），
# 供 packaging/openwrt/scripts/cross-build.sh 使用。
set -eu
. "$(dirname "$0")/common.sh"

DEST="${1:-$REPO/musl-cross}"
URL="${MUSL_CROSS_URL:-https://musl.cc/mipsel-linux-muslsf-cross.tgz}"

if [ -x "$DEST/bin/mipsel-sf-gcc" ]; then
	info "工具链已存在: $DEST/bin/mipsel-sf-gcc"
	exit 0
fi

mkdir -p "$DEST"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

info "下载 musl soft-float 工具链: $URL"
if command -v curl >/dev/null 2>&1; then
	curl -fL --retry 15 --retry-all-errors -C - -o "$TMP/toolchain.tgz" "$URL" \
		|| die "下载失败（网络？可设 MUSL_CROSS_URL 用镜像）"
elif command -v wget >/dev/null 2>&1; then
	wget -O "$TMP/toolchain.tgz" "$URL" || die "下载失败（网络？）"
else
	die "需要 curl 或 wget"
fi

info "解压 ..."
tar xzf "$TMP/toolchain.tgz" -C "$TMP"

SRC="$(find "$TMP" -maxdepth 2 -type d -name 'mipsel-linux-muslsf-cross' | head -n1)"
[ -n "$SRC" ] || die "解压后未找到 mipsel-linux-muslsf-cross 目录"
cp -a "$SRC/." "$DEST/"

# 透传包装脚本（记忆/文档里用到的名字）：等价于直接调用 muslsf-gcc
cat > "$DEST/bin/mipsel-sf-gcc" <<'EOF'
#!/bin/sh
exec "$(dirname "$0")/mipsel-linux-muslsf-gcc" "$@"
EOF
chmod +x "$DEST/bin/mipsel-sf-gcc"

info "完成: $DEST/bin/mipsel-sf-gcc"
info "交叉编译示例: MIPSEL_SF_GCC=$DEST/bin/mipsel-sf-gcc ./packaging/openwrt/scripts/cross-build.sh mipsel-unknown-linux-musl"
