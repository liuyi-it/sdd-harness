#!/usr/bin/env bash
# sdd-harness 完整卸载脚本
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LEGACY_NPM_PACKAGES=("@sdd-harness/cli" "sdd-harness")

echo "=== sdd-harness 卸载 ==="

# PREFIX 指定时仅卸载该位置；否则清理默认安装位置。
if [ -n "${PREFIX:-}" ]; then
  prefixes=("$PREFIX")
else
  prefixes=("$HOME/.local/bin" /usr/local/bin "$HOME/bin")
fi
for prefix in "${prefixes[@]}"; do
  rm -f "$prefix/sdd" "$prefix/sdd-harness" "$prefix/sdd.exe" "$prefix/sdd-harness.exe" 2>/dev/null || true
done

if [ -z "${PREFIX:-}" ] && command -v npm >/dev/null 2>&1; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) NPM_BIN="$(npm prefix -g)" ;;
    *) NPM_BIN="$(npm prefix -g)/bin" ;;
  esac
  NPM_ROOT="$(npm root -g)"
  for package_name in "${LEGACY_NPM_PACKAGES[@]}"; do
    npm uninstall -g "$package_name" >/dev/null 2>&1 || true
  done
  rmdir "$NPM_ROOT/@sdd-harness" 2>/dev/null || true
  rm -f "$NPM_BIN/sdd" "$NPM_BIN/sdd-harness" \
    "$NPM_BIN/sdd.exe" "$NPM_BIN/sdd-harness.exe" \
    "$NPM_BIN/sdd.cmd" "$NPM_BIN/sdd-harness.cmd" \
    "$NPM_BIN/sdd.ps1" "$NPM_BIN/sdd-harness.ps1" 2>/dev/null || true
fi

echo "清理构建产物..."
rm -rf "$PROJECT_ROOT/target"

echo "sdd-harness 已完整卸载"
echo "说明: 业务项目中的 .sdd/ 是用户数据，未自动删除。"
