#!/usr/bin/env bash
# sdd-harness 卸载脚本
set -euo pipefail

echo "卸载 sdd-harness..."
npm unlink --workspace=packages/cli 2>/dev/null || true
echo "sdd-harness 已卸载"
