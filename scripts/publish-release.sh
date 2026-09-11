#!/bin/sh
# 把构建产物（.ipk/.apk/.deb…）作为 Release 附件发布到 Forgejo / Gitea / Codeberg。
# GitHub 请用 .github/workflows/release.yml（softprops/action-gh-release）。
#
# 用法:
#   scripts/publish-release.sh <TAG> <VERSION> <FILE>...
#
# 说明：若该 tag 尚无 Release 会**自动创建**（Forgejo 会同时创建 tag），
#       已存在的同名附件会跳过，因此重复运行是安全的（幂等）。
#
# 环境变量（大多可自动推断）：
#   TOKEN | RELEASE_TOKEN | GITHUB_TOKEN     有 write:repository 权限的令牌（必填）
#   API_BASE | GITHUB_SERVER_URL | CI_FORGE_URL   实例地址，默认 https://codeberg.org
#   REPO_SLUG | GITHUB_REPOSITORY | CI_REPO        owner/repo
#   DRY_RUN=1                                     只打印将执行的动作，不真正上传
set -eu

if [ $# -lt 3 ]; then
	echo "用法: $0 <TAG> <VERSION> <FILE>..." >&2
	exit 2
fi

TAG="$1"
VERSION="$2"
shift 2

SERVER="${API_BASE:-${GITHUB_SERVER_URL:-${CI_FORGE_URL:-https://codeberg.org}}}"
SERVER="${SERVER%/}"
SLUG="${REPO_SLUG:-${GITHUB_REPOSITORY:-${CI_REPO:-}}}"
TOKEN_V="${TOKEN:-${RELEASE_TOKEN:-${GITHUB_TOKEN:-}}}"
DRY_RUN="${DRY_RUN:-0}"

[ -n "$SLUG" ] || { echo "[!] 缺少仓库标识（REPO_SLUG / CI_REPO / GITHUB_REPOSITORY）" >&2; exit 1; }
[ -n "$TOKEN_V" ] || { echo "[!] 缺少访问令牌：请设置 Secret RELEASE_TOKEN（需 write:repository 权限）" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "[!] 需要 curl" >&2; exit 1; }

API="$SERVER/api/v1/repos/$SLUG"
AUTH="Authorization: token $TOKEN_V"
BODY="AuthEAP-Client ${VERSION} —— 由 CI 自动构建并发布。

包含：OpenWrt .ipk/.apk（含 LuCI 界面包）与桌面 Linux .deb。
版本号自动递增；本 Release 由 CI 在 tag/手动触发时创建。"

# 从 JSON 里取第一个 "id": <num>
json_id() {
	tr ',' '\n' | sed -n 's/.*"id"[ \t]*:[ \t]*\([0-9][0-9]*\).*/\1/p' | head -n1
}

echo "[*] 仓库: $SLUG  实例: $SERVER  tag: $TAG  版本: $VERSION"

if [ "$DRY_RUN" = "1" ]; then
	echo "[dry-run] 将创建/复用 Release $TAG 并上传: $*"
	exit 0
fi

# ---------- 1) 取已有 Release ----------
rel_id="$(curl -sS -m 30 -H "$AUTH" "$API/releases/tags/$TAG" 2>/dev/null | json_id || true)"

# ---------- 2) 不存在则创建（Forgejo 会同时建 tag） ----------
if [ -z "$rel_id" ]; then
	echo "[*] Release $TAG 不存在，正在创建 ..."
	resp="$(curl -sS -m 30 -X POST -H "$AUTH" -H 'Content-Type: application/json' \
		-d "{\"tag_name\":\"$TAG\",\"name\":\"$TAG\",\"body\":\"$BODY\"}" \
		"$API/releases" 2>/dev/null || true)"
	rel_id="$(printf '%s' "$resp" | json_id)"
	if [ -z "$rel_id" ]; then
		# 可能被并发任务抢先创建，重试一次查询
		sleep 3
		rel_id="$(curl -sS -m 30 -H "$AUTH" "$API/releases/tags/$TAG" 2>/dev/null | json_id || true)"
	fi
	if [ -z "$rel_id" ]; then
		echo "[!] 无法创建/获取 Release $TAG" >&2
		printf '%s\n' "$resp" >&2
		exit 1
	fi
fi
echo "[*] Release id = $rel_id"

# ---------- 3) 上传附件（同名跳过，幂等） ----------
existing="$(curl -sS -m 30 -H "$AUTH" "$API/releases/$rel_id/assets" 2>/dev/null || true)"
uploaded=0
for f in "$@"; do
	[ -f "$f" ] || { echo "[!] 文件不存在，跳过: $f" >&2; continue; }
	name="$(basename "$f")"
	case "$existing" in
		*"\"name\":\"$name\""*)
			echo "[=] 已存在，跳过: $name"
			continue
			;;
	esac
	echo "[+] 上传: $name"
	curl -sS -m 300 -X POST -H "$AUTH" \
		-F "attachment=@$f" \
		"$API/releases/$rel_id/assets?name=$name" >/dev/null
	uploaded=$((uploaded + 1))
done

echo "[*] 完成：新上传 $uploaded 个附件 → $SERVER/$SLUG/releases/tag/$TAG"
