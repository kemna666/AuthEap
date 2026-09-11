#!/bin/sh
# CI 用：交叉编译 autheap 并打包 OpenWrt .ipk / .apk；在 tag / 手动发布时自动上传到 Release。
#
# 用法:
#   scripts/ci-build-package.sh <rust-triple> <ipk-arch> <apk-arch>
# 例:
#   scripts/ci-build-package.sh mipsel-unknown-linux-musl mipsel_24kc mipsel_24kc
#
# 版本号来源（优先级）：
#   1) 环境变量 VERSION（多任务流水线里由前面的“版本”任务传入，保证各任务一致）
#   2) 工作区文件 .ci-version（同一条流水线的后续步骤共享）
#   3) scripts/ci-version.sh 自动计算（tag 事件取 tag；否则对远端最大版本 +1）
#
# 发布条件：tag / manual 事件，且设置了 RELEASE_TOKEN（或 TOKEN）。
#   GitHub Actions 下跳过（由 .github/workflows/release.yml 统一发布）。
set -eu

TRIPLE="${1:?用法: ci-build-package.sh <triple> <ipk-arch> <apk-arch>}"
IPK_ARCH="${2:?缺少 ipk 架构名}"
APK_ARCH="${3:?缺少 apk 架构名}"

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR"

# ---------- 版本号 ----------
VF=".ci-version"
if [ -n "${VERSION:-}" ]; then
	:
elif [ -s "$VF" ]; then
	VERSION="$(cat "$VF")"
else
	VERSION="$(./scripts/ci-version.sh)"
	printf '%s' "$VERSION" > "$VF"
fi
export VERSION
echo "[*] 版本号: $VERSION（target=$TRIPLE, arch=$IPK_ARCH）"

# 让二进制与安装包的版本号一致（用环境变量覆盖，不改动仓库里的 Cargo.toml）
export AUTHEAP_BUILD_VERSION="$VERSION"

# ---------- 交叉编译环境 ----------
case "$TRIPLE" in
	aarch64-unknown-linux-musl)
		export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-aarch64-linux-gnu-gcc}"
		;;
	mipsel-unknown-linux-musl)
		if [ -z "${MIPSEL_SF_GCC:-}" ] \
		   && [ ! -x "$REPO_DIR/musl-cross/bin/mipsel-sf-gcc" ] \
		   && [ ! -x "$HOME/musl-cross/bin/mipsel-sf-gcc" ]; then
			echo "[*] 下载 mipsel soft-float 工具链 ..."
			./packaging/openwrt/scripts/fetch-musl-cross.sh "$REPO_DIR/musl-cross"
		fi
		if [ -x "$REPO_DIR/musl-cross/bin/mipsel-sf-gcc" ]; then
			export MIPSEL_SF_GCC="${MIPSEL_SF_GCC:-$REPO_DIR/musl-cross/bin/mipsel-sf-gcc}"
		fi
		;;
esac

# ---------- 编译 + 打包 ----------
./packaging/openwrt/scripts/cross-build.sh "$TRIPLE"

./packaging/openwrt/scripts/build-ipk.sh \
	--arch "$IPK_ARCH" --version "$VERSION" \
	--binary packaging/openwrt/staging/autheap
./packaging/openwrt/scripts/build-apk.sh \
	--arch "$APK_ARCH" --version "$VERSION" --release 1 \
	--binary packaging/openwrt/staging/autheap

# ---------- 可选：发布到 Forgejo Release ----------
should_publish() {
	case "${CI_PIPELINE_EVENT:-}" in
		tag|manual|release) return 0 ;;
	esac
	[ -n "${CI_COMMIT_TAG:-}" ] && return 0
	[ "${GITHUB_EVENT_NAME:-}" = "workflow_dispatch" ] && return 0
	return 1
}

if [ "${CI_SKIP_PUBLISH:-0}" = "1" ]; then
	echo "[*] CI_SKIP_PUBLISH=1：仅构建打包，发布交给单独的 publish 步骤"
	exit 0
fi

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
	echo "[*] GitHub Actions：由 release.yml 统一发布，跳过上传"
	exit 0
fi

if should_publish; then
	if [ -n "${RELEASE_TOKEN:-${TOKEN:-}}" ]; then
		# shellcheck disable=SC2086
		./scripts/publish-release.sh "$VERSION" "$VERSION" \
			packaging/openwrt/staging/*.ipk packaging/openwrt/staging/*.apk
	else
		echo "[!] 未配置 RELEASE_TOKEN（Secret），跳过发布；安装包仍在 CI 工作区："
		ls -l packaging/openwrt/staging/*.ipk packaging/openwrt/staging/*.apk
	fi
else
	echo "[*] 事件 ${CI_PIPELINE_EVENT:-${GITHUB_EVENT_NAME:-unknown}}：仅构建，不发布"
fi
