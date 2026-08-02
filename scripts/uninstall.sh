#!/usr/bin/env bash
# sdd-harness 完整卸载脚本
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== sdd-harness 卸载 ==="

# 移除所有可能位置注册的全局命令
for prefix in "$HOME/.local/bin" /usr/local/bin "$HOME/bin"; do
  rm -f "$prefix/sdd" "$prefix/sdd-harness" 2>/dev/null || true
done

echo "清理构建产物..."
rm -rf "$PROJECT_ROOT/target"

echo "sdd-harness 已完整卸载"
echo "说明: 业务项目中的 .sdd/ 是用户数据，未自动删除。"
