#!/bin/sh
# CI 用：交叉编译 autheap 并打包 OpenWrt .ipk / .apk；在 tag / 手动发布时自动上传到 Release。
#
# 用法:
#   scripts/ci-build-package.sh <rust-triple> <ipk-arch> <apk-arch>
# 例:
#   scripts/ci-build-package.sh mipsel-unknown-linux-musl mipsel_24kc mipsel_24kc
#
# 版本号来源（优先级）：
#   0) 本地/手动构建：scripts/auto-version.sh 自动 patch+1
#   1) 环境变量 VERSION（多任务流水线里由“版本”任务传入，保证各任务一致；
#      同时会跳过自动递增）
#   2) 工作区文件 .ci-version（同一条流水线的后续步骤共享）
#   3) scripts/ci-version.sh 自动计算（tag 事件取 tag；否则对远端最大版本 +1）
#   CI 设置 AUTHEAP_NO_VERSION_BUMP=1，避免多架构 job 各自递增。
#
# 发布条件：tag / manual 事件，且设置了 RELEASE_TOKEN（或 TOKEN）。
#   GitHub.com Actions 下跳过（由 .github/workflows/release.yml 统一发布）；
#   Forgejo/Codeberg 的 Actions 不跳过（用 CI_SKIP_PUBLISH 控制）。
set -eu

TRIPLE="${1:?用法: ci-build-package.sh <triple> <ipk-arch> <apk-arch>}"
IPK_ARCH="${2:?缺少 ipk 架构名}"
APK_ARCH="${3:?缺少 apk 架构名}"

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR"

# ---------- 版本号 ----------
# 本地/手动构建：每次执行自动 patch+1（CI 发布传入 VERSION，CI 普通构建会
# 设置 AUTHEAP_NO_VERSION_BUMP=1，均跳过这里）。
if [ -z "${VERSION:-}" ] \
   && [ "${AUTHEAP_VERSION_BUMPED:-0}" != 1 ] \
   && [ "${AUTHEAP_NO_VERSION_BUMP:-0}" != 1 ]; then
	VERSION="$(./scripts/auto-version.sh)"
	export VERSION
	export AUTHEAP_VERSION_BUMPED=1
	echo "[*] 自动递增版本号: $VERSION"
fi

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
# 常见 OpenWrt 架构对应的 Rust target / 链接器：
#   x86_64        x86_64-unknown-linux-musl         系统 cc
#   aarch64       aarch64-unknown-linux-musl        aarch64-linux-gnu-gcc
#   armv7         armv7-unknown-linux-musleabihf    arm-linux-gnueabihf-gcc
#   armv6         arm-unknown-linux-musleabihf      arm-linux-gnueabihf-gcc
#   riscv64       riscv64gc-unknown-linux-musl      riscv64-linux-gnu-gcc
#   mipsel        mipsel-unknown-linux-musl         musl.cc mipsel-sf（build-std）
#   mips          mips-unknown-linux-musl           musl.cc mips-sf（build-std）
check_linker() {
	command -v "$1" >/dev/null 2>&1 || {
		echo "[!] 缺少交叉链接器 $1；请安装对应的 gcc-<arch>-linux-gnu 包" >&2
		exit 1
	}
}
case "$TRIPLE" in
	aarch64-unknown-linux-musl)
		export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-aarch64-linux-gnu-gcc}"
		check_linker "$CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"
		;;
	armv7-unknown-linux-musleabihf)
		export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER="${CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER:-arm-linux-gnueabihf-gcc}"
		check_linker "$CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER"
		;;
	arm-unknown-linux-musleabihf)
		export CARGO_TARGET_ARM_UNKNOWN_LINUX_MUSLEABIHF_LINKER="${CARGO_TARGET_ARM_UNKNOWN_LINUX_MUSLEABIHF_LINKER:-arm-linux-gnueabihf-gcc}"
		check_linker "$CARGO_TARGET_ARM_UNKNOWN_LINUX_MUSLEABIHF_LINKER"
		;;
	riscv64gc-unknown-linux-musl)
		export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER:-riscv64-linux-gnu-gcc}"
		check_linker "$CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER"
		# 该 target 默认动态 musl，而 CI 只有 glibc 交叉 gcc（没有 musl Scrt1.o）。
		# 强制静态 musl，并使用 Rust 自带的 musl crt/libc，避免依赖工具链 sysroot。
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
	mips-unknown-linux-musl)
		if [ -z "${MIPS_SF_GCC:-}" ] \
		   && [ ! -x "$REPO_DIR/musl-cross-mips/bin/mips-sf-gcc" ] \
		   && [ ! -x "$HOME/musl-cross-mips/bin/mips-sf-gcc" ]; then
			echo "[*] 下载 mips soft-float 工具链 ..."
			MUSL_CROSS_ARCH=mips ./packaging/openwrt/scripts/fetch-musl-cross.sh "$REPO_DIR/musl-cross-mips"
		fi
		if [ -x "$REPO_DIR/musl-cross-mips/bin/mips-sf-gcc" ]; then
			export MIPS_SF_GCC="${MIPS_SF_GCC:-$REPO_DIR/musl-cross-mips/bin/mips-sf-gcc}"
		fi
		;;
esac

# ---------- 编译 + 打包 ----------
./packaging/openwrt/scripts/cross-build.sh "$TRIPLE"

# riscv64gc musl 默认动态链接；CI 用 glibc 交叉 gcc 时缺少 musl Scrt1.o。
# 上面强制 crt-static 后必须校验产物确实是静态链接，避免“构建成功但链接到
# 错误 libc/解释器”的静默问题。
if [ "$TRIPLE" = "riscv64gc-unknown-linux-musl" ]; then
	if ! file packaging/openwrt/staging/autheap | grep -q 'statically linked'; then
		echo "[!] riscv64 产物不是静态链接：" >&2
		file packaging/openwrt/staging/autheap >&2 || true
		exit 1
	fi
fi

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
	[ "${GITHUB_REF_TYPE:-}" = "tag" ] && return 0
	[ "${GITHUB_EVENT_NAME:-}" = "workflow_dispatch" ] && return 0
	return 1
}

# 仅 GitHub.com 的 Actions 走“GitHub 统一发布”分支；Forgejo/Gitea/Codeberg 的
# Actions 也设置 GITHUB_ACTIONS=true，不能据此跳过发布。
is_github_actions() {
	[ "${GITHUB_ACTIONS:-}" = "true" ] || return 1
	[ -n "${FORGEJO_ACTIONS:-}${GITEA_ACTIONS:-}" ] && return 1
	case "${GITHUB_SERVER_URL:-}" in
		"")
			[ -n "${CI_FORGE_URL:-}" ] && return 1
			return 0
			;;
		https://github.com|https://github.com/) return 0 ;;
		*github.com) return 0 ;;
		*) return 1 ;;
	esac
}

if [ "${CI_SKIP_PUBLISH:-0}" = "1" ]; then
	echo "[*] CI_SKIP_PUBLISH=1：仅构建打包，发布交给单独的 publish 步骤"
	exit 0
fi

if is_github_actions; then
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
