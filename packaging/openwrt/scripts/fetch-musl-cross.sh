#!/bin/sh
# 下载并解压 musl.cc 的 MIPS **soft-float** 交叉工具链。
#
# 为什么需要它：MT7621 等 mipsel_24kc 目标没有硬件浮点，OpenWrt 用户态用的是
# soft-float musl（解释器 /lib/ld-musl-mipsel-sf.so.1），因此交叉链接必须用
# `mipsel-linux-muslsf-gcc`，不能用默认的 hard-float `mipsel-linux-musl-gcc`。
# 大端 MIPS（mips_24kc）同理，使用 mips-linux-muslsf。
#
# 用法:
#   fetch-musl-cross.sh [目标目录]                 # 默认 mipsel → <仓库>/musl-cross
#   MUSL_CROSS_ARCH=mips fetch-musl-cross.sh [目录] # 大端 MIPS → <仓库>/musl-cross-mips
#
# 可用环境变量覆盖下载地址:
#   MUSL_CROSS_URL=https://musl.cc/mipsel-linux-muslsf-cross.tgz
#   MUSL_CROSS_ARCH=mipsel|mips
#
# 完成后会得到 <目标目录>/bin/mipsel-sf-gcc 或 <目标目录>/bin/mips-sf-gcc
# （透传给真实 muslsf-gcc 的包装脚本），供 cross-build.sh / ci-build-package.sh 使用。
set -eu
. "$(dirname "$0")/common.sh"

TARGET="${MUSL_CROSS_ARCH:-mipsel}"
DEST_ARG="${1:-}"

case "$TARGET" in
	mipsel)
		URL="${MUSL_CROSS_URL:-https://musl.cc/mipsel-linux-muslsf-cross.tgz}"
		PKG_DIR='mipsel-linux-muslsf-cross'
		REAL='mipsel-linux-muslsf-gcc'
		WRAPPER='mipsel-sf-gcc'
		DEFAULT_DEST="$REPO/musl-cross"
		;;
	mips)
		URL="${MUSL_CROSS_URL:-https://musl.cc/mips-linux-muslsf-cross.tgz}"
		PKG_DIR='mips-linux-muslsf-cross'
		REAL='mips-linux-muslsf-gcc'
		WRAPPER='mips-sf-gcc'
		DEFAULT_DEST="$REPO/musl-cross-mips"
		;;
	*)
		die "不支持的 MUSL_CROSS_ARCH=$TARGET（可用 mipsel / mips）"
		;;
esac

DEST="${DEST_ARG:-$DEFAULT_DEST}"
if [ -x "$DEST/bin/$WRAPPER" ]; then
	info "工具链已存在: $DEST/bin/$WRAPPER"
	exit 0
fi

mkdir -p "$DEST"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

info "下载 $TARGET soft-float 工具链: $URL"

# musl.cc 单连接速度可能很慢；支持分段并发下载（CI 里很重要）。
# 可用 MUSL_CROSS_PARTS 调整并发数（默认 16）。
parallel_download() {
	command -v curl >/dev/null 2>&1 || return 1
	command -v xargs >/dev/null 2>&1 || return 1
	command -v seq >/dev/null 2>&1 || return 1

	size="$(curl -fsSLI "$URL" 2>/dev/null | tr -d '\r' \
		| awk 'tolower($1) == "content-length:" { v = $2 } END { print v }')"
	case "$size" in ''|*[!0-9]*) return 1 ;; esac
	[ "$size" -gt 8388608 ] || return 1

	N="${MUSL_CROSS_PARTS:-16}"
	case "$N" in ''|*[!0-9]*|0) N=16 ;; esac
	[ "$N" -le 64 ] || N=64
	CHUNK=$(( (size + N - 1) / N ))
	PARTS="$(mktemp -d "${TMPDIR:-/tmp}/autheap-musl-parts.XXXXXX")"

	export URL size N CHUNK PARTS
	if ! seq 0 $((N - 1)) | xargs -P "$N" -I{} sh -c '
		i={}
		start=$((i * CHUNK))
		end=$((start + CHUNK - 1))
		[ "$end" -ge "$size" ] && end=$((size - 1))
		curl -fsSL --retry 5 --retry-all-errors \
			-r "$start-$end" -o "$PARTS/part-$i" "$URL"
	'; then
		rm -rf "$PARTS"
		return 1
	fi
	for i in $(seq 0 $((N - 1))); do
		cat "$PARTS/part-$i" || return 1
	done > "$TMP/toolchain.tgz"
	rm -rf "$PARTS"
	got="$(wc -c < "$TMP/toolchain.tgz" | tr -dc '0-9')"
	[ "$got" = "$size" ] || return 1
	return 0
}

if command -v curl >/dev/null 2>&1; then
	if parallel_download; then
		info "分段下载完成（$(( $(wc -c < "$TMP/toolchain.tgz") / 1024 / 1024 )) MB）"
	else
		info "分段下载不可用，回退单连接下载 ..."
		curl -fL --retry 15 --retry-all-errors -C - -o "$TMP/toolchain.tgz" "$URL" \
			|| die "下载失败（网络？可设 MUSL_CROSS_URL 用镜像）"
	fi
elif command -v wget >/dev/null 2>&1; then
	wget -O "$TMP/toolchain.tgz" "$URL" || die "下载失败（网络？）"
else
	die "需要 curl 或 wget"
fi

info "解压 ..."
tar xzf "$TMP/toolchain.tgz" -C "$TMP"

SRC="$(find "$TMP" -maxdepth 2 -type d -name "$PKG_DIR" | head -n1)"
[ -n "$SRC" ] || die "解压后未找到 $PKG_DIR 目录"
cp -a "$SRC/." "$DEST/"

# 透传包装脚本（记忆/文档里用到的名字）：等价于直接调用相应 muslsf-gcc
cat > "$DEST/bin/$WRAPPER" <<WRAP
#!/bin/sh
exec "\$(dirname "\$0")/$REAL" "\$@"
WRAP
chmod +x "$DEST/bin/$WRAPPER"

info "完成: $DEST/bin/$WRAPPER"
info "交叉编译示例:"
if [ "$TARGET" = mips ]; then
	info "  MIPS_SF_GCC=$DEST/bin/$WRAPPER ./packaging/openwrt/scripts/cross-build.sh mips-unknown-linux-musl"
else
	info "  MIPSEL_SF_GCC=$DEST/bin/$WRAPPER ./packaging/openwrt/scripts/cross-build.sh mipsel-unknown-linux-musl"
fi
