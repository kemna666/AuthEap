#!/bin/sh
# 构建前自动递增版本号（默认 patch）。
#
# 用法（一般由构建脚本调用，也可手动）：
#   NEW=$(./scripts/auto-version.sh)          # 0.1.42 -> 0.1.43，并同步 Cargo/默认值
#   AUTHEAP_VERSION_DRY_RUN=1 ./scripts/auto-version.sh   # 只计算，不写文件
#
# 跳过自动递增：
#   VERSION=x.y.z                 使用显式版本（CI 发布/手动指定）
#   AUTHEAP_NO_VERSION_BUMP=1     保持当前版本（CI 普通构建）
#   AUTHEAP_VERSION_BUMPED=1      本次构建已经递增过（父脚本传给子脚本）
#   AUTHEAP_VERSION_DRY_RUN=1     只打印下一个版本，不修改任何文件
#   AUTHEAP_BUMP_LEVEL=minor|major 改变递增级别（默认 patch）
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

current_version() {
	sed -n 's/^version[ 	]*=[ 	]*"\(.*\)".*/\1/p' Cargo.toml | head -n1
}

current="$(current_version)"
[ -n "$current" ] || { echo "无法从 Cargo.toml 读取版本号" >&2; exit 1; }

# 显式版本优先；CI 发布使用 ci-version.sh 算好的版本。
if [ -n "${VERSION:-}" ]; then
	printf '%s\n' "$VERSION"
	exit 0
fi

# 已经递增过/明确禁止递增：保持当前版本。
if [ "${AUTHEAP_VERSION_BUMPED:-0}" = 1 ] || [ "${AUTHEAP_NO_VERSION_BUMP:-0}" = 1 ]; then
	printf '%s\n' "$current"
	exit 0
fi

level="${AUTHEAP_BUMP_LEVEL:-patch}"
case "$level" in
	patch|minor|major) ;;
	*) echo "AUTHEAP_BUMP_LEVEL 只能是 patch/minor/major: $level" >&2; exit 1 ;;
esac

if [ "${AUTHEAP_VERSION_DRY_RUN:-0}" = 1 ]; then
	./scripts/bump-version.sh --from "$current" --level "$level" --quiet --dry-run
	exit 0
fi

# 串行化并发构建，避免两个构建同时读到同一个版本。
LOCK="${TMPDIR:-/tmp}/autheap-build-version.lock"
if command -v flock >/dev/null 2>&1; then
	exec 9>"$LOCK"
	flock 9
else
	LOCKDIR="${LOCK}.d"
	while ! mkdir "$LOCKDIR" 2>/dev/null; do sleep 0.1; done
	trap 'rmdir "$LOCKDIR" 2>/dev/null || true' EXIT
fi

# 拿到锁后重新读取（等待期间可能已被另一个构建递增）。
current="$(current_version)"
new="$(./scripts/bump-version.sh "$level" --quiet)"
[ -n "$new" ] || { echo "自动递增版本号失败" >&2; exit 1; }
[ "$new" != "$current" ] || { echo "版本号未变化: $new" >&2; exit 1; }

printf '%s\n' "$new"
