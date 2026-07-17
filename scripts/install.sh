#!/usr/bin/env bash
# sdd-harness 一键全局安装脚本 (macOS/Linux/Git Bash)
# 用法: bash scripts/install.sh
set -euo pipefail

echo "=== sdd-harness 安装 ==="

# 进入项目根目录并加载共享清理逻辑
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/lib/installation.sh
source "$SCRIPT_DIR/lib/installation.sh"

if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
  echo "错误: 安装需要 Node.js 和 npm。" >&2
  exit 1
fi

# 检查 Node.js 版本
NODE_VERSION="$(node -v | cut -d'v' -f2 | cut -d'.' -f1)"
if [ "$NODE_VERSION" -lt 22 ]; then
  echo "错误: sdd-harness 要求 Node.js >= 22，当前版本: $(node -v)"
  echo "请升级 Node.js 后重试: https://nodejs.org/"
  exit 1
fi

cd "$PROJECT_ROOT"

INSTALL_SUCCEEDED=false
rollback_failed_install() {
  local exit_code="$?"
  if [ "$INSTALL_SUCCEEDED" != true ]; then
    echo "安装失败，正在清理未完成的安装产物..." >&2
    sdd_remove_global_cli "$PROJECT_ROOT" || true
    sdd_remove_local_artifacts "$PROJECT_ROOT" || true
    sdd_assert_no_global_cli "$PROJECT_ROOT" || true
    sdd_assert_no_local_artifacts "$PROJECT_ROOT" || true
  fi
  exit "$exit_code"
}
trap rollback_failed_install EXIT

echo "清理旧版安装..."
sdd_remove_global_cli "$PROJECT_ROOT"
sdd_remove_local_artifacts "$PROJECT_ROOT"

# npm ci 会严格使用 lockfile，避免旧依赖残留。
echo "安装依赖..."
npm ci

# 构建所有包
echo "构建..."
npm run build

# 全局 link CLI 包
echo "全局安装 sdd CLI..."
npm link --workspace=packages/cli

# 验证安装
echo "验证安装..."
sdd --version
sdd-harness --version

INSTALL_SUCCEEDED=true
trap - EXIT

echo ""
echo "=== 安装完成 ==="
echo "可用命令: sdd, sdd-harness"
echo "使用 sdd init 初始化项目"
