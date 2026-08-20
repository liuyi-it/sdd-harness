#!/usr/bin/env bash
# sdd 完整卸载脚本
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== sdd 卸载 ==="

# PREFIX 指定时仅卸载该位置；否则清理默认安装位置。
if [ -n "${PREFIX:-}" ]; then
  prefixes=("$PREFIX")
else
  prefixes=("$HOME/.local/bin" /usr/local/bin "$HOME/bin")
fi
for prefix in "${prefixes[@]}"; do
  rm -f "$prefix/sdd" "$prefix/sdd.exe" 2>/dev/null || true
done

echo "清理构建产物..."
rm -rf "$PROJECT_ROOT/target"

echo "sdd 已完整卸载"
echo "说明: 业务项目中的 .sdd/ 是用户数据，未自动删除。"
