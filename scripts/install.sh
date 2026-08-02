#!/usr/bin/env bash
# sdd-harness 一键全局安装脚本 (macOS/Linux/Git Bash)
# 用法: bash scripts/install.sh
set -euo pipefail

echo "=== sdd-harness 安装 ==="

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# 选择安装前缀：允许 PREFIX 覆盖，便于受控安装与 CI 验收。
CUSTOM_PREFIX=false
if [ -z "${PREFIX:-}" ]; then
  if [ -d "$HOME/.local/bin" ] || [ ! -w /usr/local/bin ]; then
    PREFIX="$HOME/.local/bin"
  else
    PREFIX="/usr/local/bin"
  fi
else
  CUSTOM_PREFIX=true
fi
mkdir -p "$PREFIX"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) EXE_SUFFIX=".exe" ;;
  *) EXE_SUFFIX="" ;;
esac
COMMANDS=("sdd${EXE_SUFFIX}" "sdd-harness${EXE_SUFFIX}")

# 加载 cargo 环境（rustup 默认安装位置）
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

# 检查 Rust 工具链
if ! command -v cargo >/dev/null 2>&1; then
  echo "错误: 需要 Rust 工具链（cargo）。"
  echo "请先安装 rustup: https://rustup.rs（国内可配置镜像后安装）"
  exit 1
fi

INSTALL_SUCCEEDED=false
BACKUP_DIR="$(mktemp -d)"
for command_name in "${COMMANDS[@]}"; do
  if [ -f "$PREFIX/$command_name" ]; then
    cp "$PREFIX/$command_name" "$BACKUP_DIR/$command_name"
  fi
done
rollback_failed_install() {
  local exit_code="$?"
  if [ "$INSTALL_SUCCEEDED" != true ]; then
    echo "安装失败，正在恢复原安装..." >&2
    rm -f "${COMMANDS[@]/#/$PREFIX/}" || true
    for command_name in "${COMMANDS[@]}"; do
      if [ -f "$BACKUP_DIR/$command_name" ]; then
        cp "$BACKUP_DIR/$command_name" "$PREFIX/$command_name"
      fi
    done
  fi
  rm -rf "$BACKUP_DIR"
  exit "$exit_code"
}
trap rollback_failed_install EXIT

echo "清理旧版安装..."
rm -f "$PREFIX/sdd" "$PREFIX/sdd-harness" "$PREFIX/sdd.exe" "$PREFIX/sdd-harness.exe"

# 构建 release 二进制
echo "构建..."
cargo build --release --manifest-path "$PROJECT_ROOT/Cargo.toml"

BIN="$PROJECT_ROOT/target/release/sdd${EXE_SUFFIX}"
if [ ! -f "$BIN" ]; then
  echo "错误: 构建产物不存在: $BIN" >&2
  exit 1
fi

# 注册全局命令
echo "注册全局命令到 $PREFIX ..."
install -m 0755 "$BIN" "$PREFIX/sdd${EXE_SUFFIX}"
install -m 0755 "$BIN" "$PREFIX/sdd-harness${EXE_SUFFIX}"

# 验证安装
if [ "$(command -v sdd || true)" != "$PREFIX/sdd${EXE_SUFFIX}" ]; then
  echo "警告: $PREFIX 不在 PATH 中，请将以下行加入 shell 配置（~/.zshrc / ~/.bashrc）："
  echo "  export PATH=\"$PREFIX:\$PATH\""
fi
"$PREFIX/sdd${EXE_SUFFIX}" --version >/dev/null 2>&1 || {
  echo "错误: 安装验证失败，sdd 无法运行" >&2
  exit 1
}

# 新二进制验证成功后再清理 npm 全局 link，避免失败回滚时丢失旧版 CLI。
if [ "$CUSTOM_PREFIX" = false ] && command -v npm >/dev/null 2>&1; then
  npm unlink -g sdd-harness >/dev/null 2>&1 || true
  NPM_PREFIX="$(npm prefix -g)"
  if [ -n "$EXE_SUFFIX" ]; then
    NPM_BIN="$NPM_PREFIX"
  else
    NPM_BIN="$NPM_PREFIX/bin"
  fi
  rm -f "$NPM_BIN/sdd" "$NPM_BIN/sdd-harness" \
    "$NPM_BIN/sdd.cmd" "$NPM_BIN/sdd-harness.cmd" \
    "$NPM_BIN/sdd.ps1" "$NPM_BIN/sdd-harness.ps1" 2>/dev/null || true
fi

INSTALL_SUCCEEDED=true
rm -rf "$BACKUP_DIR"
trap - EXIT

echo ""
echo "=== 安装完成 ==="
echo "命令位置: $PREFIX/sdd${EXE_SUFFIX}"
echo "可用命令: sdd, sdd-harness"
echo "使用 sdd init 初始化项目"
