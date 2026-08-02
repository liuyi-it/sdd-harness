#!/usr/bin/env bash
# sdd-harness 一键全局安装脚本 (macOS/Linux/Git Bash)
# 用法: bash scripts/install.sh
set -euo pipefail

echo "=== sdd-harness 安装 ==="

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# 选择安装前缀：优先 ~/.local/bin，其次 /usr/local/bin（不可写时回退）
if [ -d "$HOME/.local/bin" ] || [ ! -w /usr/local/bin ]; then
  PREFIX="$HOME/.local/bin"
else
  PREFIX="/usr/local/bin"
fi
mkdir -p "$PREFIX"

# 检查 Rust 工具链
if ! command -v cargo >/dev/null 2>&1; then
  echo "错误: 需要 Rust 工具链（cargo）。"
  echo "请先安装 rustup: https://rustup.rs（国内可配置镜像后安装）"
  exit 1
fi

INSTALL_SUCCEEDED=false
rollback_failed_install() {
  local exit_code="$?"
  if [ "$INSTALL_SUCCEEDED" != true ]; then
    echo "安装失败，正在清理未完成的安装产物..." >&2
    rm -f "$PREFIX/sdd" "$PREFIX/sdd-harness" || true
  fi
  exit "$exit_code"
}
trap rollback_failed_install EXIT

echo "清理旧版安装..."
rm -f "$PREFIX/sdd" "$PREFIX/sdd-harness"

# 构建 release 二进制
echo "构建..."
cargo build --release --manifest-path "$PROJECT_ROOT/Cargo.toml"

BIN="$PROJECT_ROOT/target/release/sdd"
if [ ! -f "$BIN" ]; then
  echo "错误: 构建产物不存在: $BIN" >&2
  exit 1
fi

# 注册全局命令
echo "注册全局命令到 $PREFIX ..."
install -m 0755 "$BIN" "$PREFIX/sdd"
ln -sf "$PREFIX/sdd" "$PREFIX/sdd-harness"

# 验证安装
if [ "$(command -v sdd || true)" != "$PREFIX/sdd" ]; then
  echo "警告: $PREFIX 不在 PATH 中，请将以下行加入 shell 配置（~/.zshrc / ~/.bashrc）："
  echo "  export PATH=\"$PREFIX:\$PATH\""
fi
"$PREFIX/sdd" --version >/dev/null 2>&1 || {
  echo "错误: 安装验证失败，sdd 无法运行" >&2
  exit 1
}

INSTALL_SUCCEEDED=true
trap - EXIT

echo ""
echo "=== 安装完成 ==="
echo "命令位置: $PREFIX/sdd"
echo "可用命令: sdd, sdd-harness"
echo "使用 sdd init 初始化项目"
