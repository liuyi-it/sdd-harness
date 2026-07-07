#!/usr/bin/env bash
# sdd-harness 一键全局安装脚本 (macOS/Linux)
# 用法: bash scripts/install.sh
set -euo pipefail

echo "=== sdd-harness 安装 ==="

# 检查 Node.js 版本
NODE_VERSION=$(node -v | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 22 ]; then
  echo "错误: sdd-harness 要求 Node.js >= 22，当前版本: $(node -v)"
  echo "请升级 Node.js 后重试: https://nodejs.org/"
  exit 1
fi

# 进入项目根目录
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# 安装依赖
echo "安装依赖..."
npm install

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

echo ""
echo "=== 安装完成 ==="
echo "可用命令: sdd, sdd-harness"
echo "使用 sdd init 初始化项目"
