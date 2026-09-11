#!/bin/sh
# 计算本次 CI 构建应使用的版本号（“每发布一个版本依次递增”）。
#
# 版本号规则：`0.1.1` 形式（主.次.修），每次发布默认递增最后一位：
#   0.1.1 → 0.1.2 → 0.1.3 …（需要时可用 minor/major 跳级）
#
# 规则（依次尝试）：
#   1) tag 触发（CI_COMMIT_TAG / GITHUB_REF_TYPE=tag）→ 直接用该 tag（去掉 v 前缀）；
#   2) 发布事件（手动 / release）→ 取**与 Cargo.toml 同一主版本号**的远端 tag/release 中
#      最大的那个，按级别递增；若 Cargo.toml 里的版本还没发布过，则直接发布它
#      （因此把它改成 0.1.1 后，第一个 release 就是 0.1.1，之后依次 0.1.2…）；
#   3) 普通 push / PR → 用 Cargo.toml 里的当前版本（只构建，不发布）。
#
# 注：只认与 Cargo.toml 同主版本号的版本，因此换命名规则时（例如 2026.9.9 → 0.1.1）
#     会自动只参考新规则的版本，不会被旧 tag 干扰。
#
# 用法: scripts/ci-version.sh [patch|minor|major]
#
# 环境变量（自动推断，一般无需设置）：
#   API_BASE / GITHUB_SERVER_URL / CI_FORGE_URL   实例地址，默认 https://codeberg.org
#   REPO_SLUG / GITHUB_REPOSITORY / CI_REPO       owner/repo
#   TOKEN / RELEASE_TOKEN / GITHUB_TOKEN          私有仓库访问令牌（可选）
#   GITHUB_ACTIONS=true                           在 GitHub Actions 中时用 GitHub API
set -eu

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR"

LEVEL="${1:-${BUMP_LEVEL:-patch}}"

# ---------- 1) tag 触发 ----------
TAG="${CI_COMMIT_TAG:-}"
if [ -z "$TAG" ] && [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
	TAG="${GITHUB_REF_NAME:-}"
fi
if [ -n "$TAG" ]; then
	printf '%s\n' "${TAG#v}"
	exit 0
fi

# ---------- 1b) 非发布事件（push / PR）：用仓库里的当前版本，不递增 ----------
is_release_event() {
	case "${CI_PIPELINE_EVENT:-}" in
		tag|manual|release) return 0 ;;
	esac
	[ -n "${CI_COMMIT_TAG:-}" ] && return 0
	[ "${GITHUB_EVENT_NAME:-}" = "workflow_dispatch" ] && return 0
	return 1
}
# Cargo.toml 里的当前版本：既是“版本线”基准（同主版本号），也是无远端记录时的起点
cur="$(sed -n 's/^version[ 	]*=[ 	]*"\(.*\)".*/\1/p' Cargo.toml | head -n1)"
if ! is_release_event; then
	if [ -n "$cur" ]; then
		printf '%s\n' "$cur"
		exit 0
	fi
fi

# ---------- 2) 远端已有 tag / release ----------
SLUG="${REPO_SLUG:-${GITHUB_REPOSITORY:-${CI_REPO:-}}}"
BASE="${API_BASE:-${GITHUB_SERVER_URL:-${CI_FORGE_URL:-https://codeberg.org}}}"
BASE="${BASE%/}"
TOKEN_V="${TOKEN:-${RELEASE_TOKEN:-${GITHUB_TOKEN:-}}}"

# 从 JSON 里抽取版本样式的字符串
json_versions() {
	tr ',' '\n' < "$1" 2>/dev/null \
		| sed -n 's/.*"\(name\|tag_name\)"[ ]*:[ ]*"\([^"]*\)".*/\2/p' \
		| sed 's/^v//' \
		| grep -E '^[0-9]+(\.[0-9]+)*$' || true
}

remote_versions() {
	[ -n "$SLUG" ] || return 1
	if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
		# GitHub API（tag / release 二选一，取到即用）
		if [ -n "$TOKEN_V" ]; then
			curl -fsS -m 20 -H "Authorization: Bearer $TOKEN_V" \
				"https://api.github.com/repos/$SLUG/tags?per_page=100" 2>/dev/null \
				|| curl -fsS -m 20 -H "Authorization: Bearer $TOKEN_V" \
					"https://api.github.com/repos/$SLUG/releases?per_page=100" 2>/dev/null
		else
			curl -fsS -m 20 "https://api.github.com/repos/$SLUG/tags?per_page=100" 2>/dev/null \
				|| curl -fsS -m 20 "https://api.github.com/repos/$SLUG/releases?per_page=100" 2>/dev/null
		fi
		return 0
	fi
	# Forgejo / Gitea / Codeberg API
	if [ -n "$TOKEN_V" ]; then
		curl -fsS -m 20 -H "Authorization: token $TOKEN_V" \
			"$BASE/api/v1/repos/$SLUG/tags?limit=100" 2>/dev/null \
			|| curl -fsS -m 20 -H "Authorization: token $TOKEN_V" \
				"$BASE/api/v1/repos/$SLUG/releases?limit=100" 2>/dev/null
	else
		curl -fsS -m 20 "$BASE/api/v1/repos/$SLUG/tags?limit=100" 2>/dev/null \
			|| curl -fsS -m 20 "$BASE/api/v1/repos/$SLUG/releases?limit=100" 2>/dev/null
	fi
}

# 只保留与 Cargo.toml 同一主版本号的版本（换命名规则时自然切换，例如 2026.9.9 → 0.1.1）
major="$(printf '%s' "$cur" | cut -d. -f1)"
same_line() {
	if [ -n "$major" ]; then
		grep -E "^${major}\\.[0-9]+(\\.[0-9]+)*$" || true
	else
		cat
	fi
}

best=""
if command -v curl >/dev/null 2>&1; then
	tmp="$(mktemp "${TMPDIR:-/tmp}/autheap-tags.XXXXXX")"
	if remote_versions > "$tmp" 2>/dev/null && [ -s "$tmp" ]; then
		best="$(json_versions "$tmp" | same_line | sort -V | tail -n1)"
	fi
	rm -f "$tmp"
fi

# 远端查不到时，退回本地 git tag（同样只认同一主版本号）
if [ -z "$best" ] && command -v git >/dev/null 2>&1 && [ -d .git ]; then
	best="$(git tag -l 2>/dev/null | sed 's/^v//' | grep -E '^[0-9]+(\.[0-9]+)*$' \
		| same_line | sort -V | tail -n1 || true)"
fi

# ---------- 3) 决定本次发布的版本 ----------
# base = max(Cargo.toml 版本, 远端同版本线的最大版本)
base="$cur"
if [ -n "$best" ] && [ "$(printf '%s\n%s\n' "$cur" "$best" | sort -V | tail -n1)" != "$cur" ]; then
	base="$best"
fi

# Cargo.toml 里的版本还没发布过（且未要求跳级）→ 直接发布该版本。
# 这样把 Cargo.toml 改成 0.1.1 后，第一个 release 就是 0.1.1，之后 0.1.2、0.1.3…
if [ "$base" = "$cur" ] && [ "$best" != "$cur" ] && [ "$LEVEL" = "patch" ]; then
	printf '%s\n' "$cur"
	exit 0
fi

if [ -n "$base" ]; then
	./scripts/bump-version.sh --from "$base" --level "$LEVEL" --quiet --dry-run
else
	./scripts/bump-version.sh --level "$LEVEL" --quiet --dry-run
fi
