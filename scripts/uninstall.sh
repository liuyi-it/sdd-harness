#!/usr/bin/env bash
# sdd-harness 完整卸载脚本
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/lib/installation.sh
source "$SCRIPT_DIR/lib/installation.sh"

echo "=== sdd-harness 卸载 ==="
echo "移除全局 CLI..."
sdd_remove_global_cli "$PROJECT_ROOT" || true

echo "清理依赖与构建产物..."
sdd_remove_local_artifacts "$PROJECT_ROOT" || true

echo "校验清理结果..."
sdd_assert_no_global_cli "$PROJECT_ROOT"
sdd_assert_no_local_artifacts "$PROJECT_ROOT"

echo "sdd-harness 已完整卸载"
echo "说明: 业务项目中的 .sdd/ 是用户数据，未自动删除。"
