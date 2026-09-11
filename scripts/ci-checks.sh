#!/bin/sh
# CI 共用静态检查：shell 语法（sh -n）+ LuCI JSON 合法性。
# 用法: scripts/ci-checks.sh
set -eu

fail=0
echo "[*] shell 语法检查"
for f in packaging/openwrt/scripts/*.sh scripts/*.sh; do
	[ -f "$f" ] || continue
	if sh -n "$f"; then echo "  OK  $f"; else echo "  ERR $f"; fail=1; fi
done
for f in \
	packaging/openwrt/autheap-client/root/etc/init.d/autheap-client \
	packaging/openwrt/autheap-client/root/usr/libexec/autheap-client/gen-config \
	packaging/openwrt/luci-app-autheap-client/root/usr/libexec/rpcd/luci.autheap_client
do
	if sh -n "$f"; then echo "  OK  $f"; else echo "  ERR $f"; fail=1; fi
done

if command -v python3 >/dev/null 2>&1; then
	echo "[*] JSON 检查"
	find packaging/openwrt/luci-app-autheap-client/root -name '*.json' \
		-exec python3 -c 'import json,sys
for p in sys.argv[1:]:
    json.load(open(p))
    print("  OK  " + p)' {} +
else
	echo "[!] 未找到 python3，跳过 JSON 检查"
fi

# ---------- CI 流水线 YAML 合法性（有 pyyaml 才检查）----------
YAML_FILES="$(ls .woodpecker.yml .github/workflows/*.yml .forgejo/workflows/*.yml 2>/dev/null || true)"
if [ -n "$YAML_FILES" ]; then
	echo "[*] YAML 检查"
	python3 -c '
import sys
try:
    import yaml
except ImportError:
    print("  [i] 没有 pyyaml，跳过 YAML 检查")
    sys.exit(0)
bad = 0
for p in sys.argv[1:]:
    try:
        list(yaml.safe_load_all(open(p)))
        print("  OK  " + p)
    except Exception as e:
        print("  ERR " + p + ": " + str(e))
        bad = 1
sys.exit(bad)
' $YAML_FILES || fail=1
fi

# ---------- LuCI 视图 JS 语法 ----------
# LuCI 视图是 CommonJS 风格（允许顶层 return），因此包一层函数再解析。
JS_FILES="$(find packaging/openwrt/luci-app-autheap-client/root/www -name '*.js' 2>/dev/null | sort)"
if [ -n "$JS_FILES" ]; then
	echo "[*] JS 语法检查"
	if command -v node >/dev/null 2>&1; then
		for f in $JS_FILES; do
			tmp="$(mktemp "${TMPDIR:-/tmp}/autheap-js.XXXXXX.js")"
			{ printf '(function(){\n'; cat "$f"; printf '\n})\n'; } > "$tmp"
			if node --check "$tmp" >/dev/null 2>&1; then echo "  OK  $f"; else echo "  ERR $f"; fail=1; fi
			rm -f "$tmp"
		done
	elif python3 -c 'import esprima' >/dev/null 2>&1; then
		for f in $JS_FILES; do
			if python3 -c 'import esprima,sys
esprima.parseScript("(function(){\n" + open(sys.argv[1], encoding="utf-8").read() + "\n})")' "$f" 2>/dev/null; then
				echo "  OK  $f"
			else
				echo "  ERR $f"
				fail=1
			fi
		done
	else
		echo "  （跳过：未找到 node，也未安装 python esprima）"
	fi
fi

[ "$fail" = 0 ] || { echo "[!] 静态检查失败" >&2; exit 1; }
echo "[*] 静态检查通过"
