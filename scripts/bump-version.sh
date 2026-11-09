#!/bin/sh
# 版本号自增 / 设置工具（Cargo.toml、Cargo.lock 与各构建脚本默认值同步修改）。
#
# 版本号格式：点分数字（如 0.1.1）。递增规则：
#   patch（默认）= 最后一段 +1           0.1.1  → 0.1.2
#   minor        = 倒数第二段 +1，其后清零 0.1.1  → 0.2.0
#   major        = 第一段 +1，其后清零    0.1.1  → 1.0.0
#
# 用法:
#   scripts/bump-version.sh [patch|minor|major] [选项]
#
# 选项:
#   --level <patch|minor|major>   指定递增级别（等价于第一个位置参数）
#   --set <X.Y.Z>                 直接设置为指定版本（不做递增）
#   -n, --dry-run                 只打印递增后的版本号，不修改任何文件
#   -q, --quiet                   不输出提示信息（只把新版本号打印到 stdout）
#       --commit                  提交 Cargo.toml / Cargo.lock
#       --tag                     打 git tag（默认 tag 名 = 版本号本身）
#       --tag-prefix <P>          tag 加前缀，如 --tag-prefix v → v0.1.2
#       --push                    git push 当前分支与 tag（需已配置凭据）
#   -h, --help                    显示帮助
#
# 例：
#   NEW=$(./scripts/bump-version.sh -n)          # 取下一个版本号（不改文件）
#   ./scripts/bump-version.sh patch --commit --tag --push   # 发布新版本
#   ./scripts/bump-version.sh --set 2026.9.11 --quiet       # 指定版本
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
TOML="Cargo.toml"
LOCK="Cargo.lock"

LEVEL="patch"
SETVER=""
FROMVER=""
DRY=0
QUIET=0
DO_COMMIT=0
DO_TAG=0
DO_PUSH=0
TAG_PREFIX="${TAG_PREFIX:-}"

usage() {
	cat <<'EOF'
版本号自增 / 设置：scripts/bump-version.sh [patch|minor|major] [选项]

  --level <patch|minor|major>   递增级别（默认 patch）
  --set <X.Y.Z>                 直接设置为指定版本
  --from <X.Y.Z>                以该版本为基准递增（不读 Cargo.toml；CI 用）
  -n, --dry-run                 只打印新版本号，不改文件
  -q, --quiet                   不输出提示
      --commit                  提交 Cargo.toml / Cargo.lock
      --tag                     打 git tag（默认 tag 名 = 版本号）
      --tag-prefix <P>          tag 前缀（如 v）
      --push                    git push 分支与 tag
  -h, --help                    显示帮助
EOF
}

die() { echo "[!] $*" >&2; exit 1; }
info() { [ "$QUIET" = 1 ] || echo "[*] $*" >&2; }

while [ $# -gt 0 ]; do
	case "$1" in
		patch|minor|major) LEVEL="$1"; shift ;;
		--level)   LEVEL="${2:?--level 需要参数}"; shift 2 ;;
		--set)     SETVER="${2:?--set 需要参数}"; shift 2 ;;
		--from)    FROMVER="${2:?--from 需要参数}"; shift 2 ;;
		-n|--dry-run) DRY=1; shift ;;
		-q|--quiet)   QUIET=1; shift ;;
		--commit)  DO_COMMIT=1; shift ;;
		--tag)     DO_TAG=1; shift ;;
		--push)    DO_PUSH=1; shift ;;
		--tag-prefix) TAG_PREFIX="${2:?--tag-prefix 需要参数}"; shift 2 ;;
		-h|--help) usage; exit 0 ;;
		*) die "未知参数: $1（-h 查看用法）" ;;
	esac
done

# ---------- 读当前版本 ----------
current="$(sed -n 's/^version[ 	]*=[ 	]*"\(.*\)".*/\1/p' "$TOML" | head -n1)"
[ -n "$current" ] || die "无法从 $TOML 读取版本号"
[ -f "$LOCK" ] || die "缺少 $LOCK"

is_version() {
	case "$1" in
		''|*[!0-9.]*) return 1 ;;
	esac
	# 不允许连续点 / 首尾点
	case "$1" in
		.*|*.|*..*) return 1 ;;
	esac
	return 0
}

# ---------- 递增 ----------
next_version() {
	cur="$1"; lvl="$2"
	is_version "$cur" || die "当前版本号格式不支持自增: $cur（可用 --set 指定）"
	segs=''
	oldifs="$IFS"; IFS='.'
	for p in $cur; do
		segs="$segs $p"
	done
	IFS="$oldifs"
	n=0
	for p in $segs; do
		n=$((n + 1))
	done
	case "$lvl" in
		major) idx=1 ;;
		minor) if [ "$n" -ge 2 ]; then idx=$((n - 1)); else idx=1; fi ;;
		*)     idx=$n ;;
	esac
	i=1; out=''
	for p in $segs; do
		if [ "$i" = "$idx" ]; then
			out="$out${out:+.}$((p + 1))"
		elif [ "$i" -gt "$idx" ]; then
			out="$out${out:+.}0"
		else
			out="$out${out:+.}$p"
		fi
		i=$((i + 1))
	done
	printf '%s' "$out"
}

if [ -n "$SETVER" ]; then
	new="$SETVER"
	is_version "$new" || die "--set 的版本号不合法: $new"
	base="$current"
elif [ -n "$FROMVER" ]; then
	new="$(next_version "$FROMVER" "$LEVEL")"
	base="$FROMVER"
else
	new="$(next_version "$current" "$LEVEL")"
	base="$current"
fi

if [ "$DRY" = 1 ]; then
	printf '%s\n' "$new"
	exit 0
fi

# ---------- 写文件 ----------
file_mode() {
	stat -c '%a' "$1" 2>/dev/null || echo 644
}

write_toml() {
	tmp="$(mktemp "${TMPDIR:-/tmp}/autheap-toml.XXXXXX")"
	awk -v v="$1" '
		BEGIN { done = 0 }
		/^version[ \t]*=/ && done == 0 {
			print "version = \"" v "\""
			done = 1
			next
		}
		{ print }
	' "$TOML" > "$tmp" || { rm -f "$tmp"; die "写入 $TOML 失败"; }
	mode="$(file_mode "$TOML")"
	mv "$tmp" "$TOML"
	chmod "$mode" "$TOML"
}

write_lock() {
	tmp="$(mktemp "${TMPDIR:-/tmp}/autheap-lock.XXXXXX")"
	awk -v v="$1" '
		/^name = "autheap"$/ { found = 1 }
		found == 1 && /^version[ \t]*=/ && done == 0 {
			print "version = \"" v "\""
			done = 1
			found = 0
			next
		}
		{ print }
	' "$LOCK" > "$tmp" || { rm -f "$tmp"; die "写入 $LOCK 失败"; }
	mode="$(file_mode "$LOCK")"
	mv "$tmp" "$LOCK"
	chmod "$mode" "$LOCK"
}

# 同步各构建脚本里的静态默认版本号，避免 Cargo.toml 与打包默认值脱节。
replace_default() {
	file="$1"; old="$2"; newv="$3"
	[ -f "$file" ] || return 0
	# 用 awk index() 做字面替换，避免版本号里的 . 或注释里的 [] 被当成正则。
	tmp="$(mktemp "${TMPDIR:-/tmp}/autheap-ver.XXXXXX")"
	awk -v old="$old" -v newv="$newv" '
		{
			line = $0
			out = ""
			while ((i = index(line, old)) > 0) {
				out = out substr(line, 1, i - 1) newv
				line = substr(line, i + length(old))
			}
			print out line
		}
	' "$file" > "$tmp" || { rm -f "$tmp"; return 1; }
	mode="$(file_mode "$file")"
	mv "$tmp" "$file"
	chmod "$mode" "$file"
}

write_defaults() {
	v="$1"
	replace_default "packaging/openwrt/scripts/common.sh" \
		'DEFAULT_VERSION="'"$current"'"' 'DEFAULT_VERSION="'"$v"'"'
	replace_default "packaging/openwrt/scripts/build-luci.sh" \
		"（默认 $current / 1）" "（默认 $v / 1）"
	replace_default "packaging/desktop/build-deb.sh" \
		"#   VERSION=$current ./build-deb.sh" "#   VERSION=$v ./build-deb.sh"
	replace_default "packaging/openwrt/autheap-client/Makefile" \
		"PKG_VERSION:=$current" "PKG_VERSION:=$v"
	replace_default "packaging/openwrt/scripts/build-ipk.sh" \
		"[--version $current]" "[--version $v]"
	replace_default "packaging/openwrt/scripts/build-apk.sh" \
		"[--version $current]" "[--version $v]"
}

write_toml "$new"
write_lock "$new"
write_defaults "$new"
info "版本号: $base → $new"

# ---------- 可选 git 操作 ----------
TAG="${TAG_PREFIX}${new}"
if [ "$DO_COMMIT" = 1 ]; then
	git add "$TOML" "$LOCK" \
		packaging/openwrt/scripts/common.sh \
		packaging/openwrt/scripts/build-luci.sh \
		packaging/desktop/build-deb.sh \
		packaging/openwrt/autheap-client/Makefile \
		packaging/openwrt/scripts/build-ipk.sh \
		packaging/openwrt/scripts/build-apk.sh
	if git diff --cached --quiet; then
		info "没有变化，跳过提交"
	else
		git commit -m "chore(release): $new" >/dev/null
		info "已提交: chore(release): $new"
	fi
fi
if [ "$DO_TAG" = 1 ]; then
	git tag -f "$TAG" >/dev/null
	info "已打 tag: $TAG"
fi
if [ "$DO_PUSH" = 1 ]; then
	git push origin HEAD
	[ "$DO_TAG" = 1 ] && git push -f origin "$TAG"
	info "已推送"
fi

printf '%s\n' "$new"
