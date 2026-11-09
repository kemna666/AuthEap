#!/bin/sh
# 带自动版本号的 Cargo 构建入口。
#
# 每执行一次，默认把 patch 版本 +1（0.1.42 → 0.1.43），同步 Cargo.toml /
# Cargo.lock 及各打包脚本默认值，然后执行 cargo build。
#
# 用法:
#   ./scripts/build.sh                 # cargo build --release
#   ./scripts/build.sh --release --features backend-pcap
#   ./scripts/build.sh --debug         # 不传 --release，编译 debug
#
# 跳过自动递增：VERSION=x.y.z、AUTHEAP_NO_VERSION_BUMP=1。
# 直接 `cargo build` 不会改版本号；需要自动版本时使用本脚本。
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

if [ "${1:-}" = "--debug" ]; then
	shift
	set -- "$@"
else
	case " $* " in
		*" --release "*) ;;
		*) set -- --release "$@" ;;
	esac
fi

VERSION="$(./scripts/auto-version.sh)"
export VERSION
export AUTHEAP_BUILD_VERSION="$VERSION"
export AUTHEAP_VERSION_BUMPED=1
echo "[*] 本次构建版本号: $VERSION" >&2

exec cargo build "$@"
