#!/usr/bin/env bash
# sdd 源码安装（macOS/Linux/Git Bash）：新产物验证通过后再替换原命令。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ -z "${PREFIX:-}" ]; then
  if [ -d "$HOME/.local/bin" ] || [ ! -w /usr/local/bin ]; then
    PREFIX="$HOME/.local/bin"
  else
    PREFIX="/usr/local/bin"
  fi
fi
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) EXE_SUFFIX=".exe" ;;
  *) EXE_SUFFIX="" ;;
esac
if [ -d "$PREFIX/sdd${EXE_SUFFIX}" ]; then
  echo "错误: 安装目标是目录: $PREFIX/sdd${EXE_SUFFIX}" >&2
  exit 1
fi

# 优先使用调用者选定的工具链，避免环境脚本重排 PATH 覆盖已有 Cargo。
if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "错误: 需要 Rust 工具链（cargo），或按 docs/agent-install.md 安装预编译版本。" >&2
  exit 1
fi

# 显式指定目录，构建与产物读取使用同一位置；相对值遵循调用者工作目录。
BUILD_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"
case "$BUILD_DIR" in
  /*|[A-Za-z]:/*) ;;
  *) BUILD_DIR="$PWD/$BUILD_DIR" ;;
esac

# 安装本机可执行文件，显式 host 避免工程的交叉编译配置留下陈旧产物被误装。
BUILD_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$BUILD_TARGET" ] || { echo "错误: 无法获取 Rust 本机 target" >&2; exit 1; }
echo "构建..."
cargo build --release --locked --package sdd-cli --manifest-path "$PROJECT_ROOT/Cargo.toml" --target-dir "$BUILD_DIR" --target "$BUILD_TARGET"
BIN="$BUILD_DIR/$BUILD_TARGET/release/sdd${EXE_SUFFIX}"
if [ ! -f "$BIN" ]; then
  echo "错误: 构建产物不存在: $BIN" >&2
  exit 1
fi

mkdir -p "$PREFIX"
STAGED_BIN="$(mktemp "$PREFIX/.sdd-install.XXXXXX${EXE_SUFFIX}")"
cleanup_install() {
  local exit_code="$?"
  if [ -n "$STAGED_BIN" ]; then
    if ! rm -f "$STAGED_BIN"; then
      echo "清理安装暂存失败: $STAGED_BIN" >&2
      exit_code=1
    fi
  fi
  exit "$exit_code"
}
trap cleanup_install EXIT

install -m 0755 "$BIN" "$STAGED_BIN"
"$STAGED_BIN" --version
# 同目录替换不需要删除旧命令或制作备份；此前失败时原安装字节保持不变。
mv -f "$STAGED_BIN" "$PREFIX/sdd${EXE_SUFFIX}"
STAGED_BIN=""

if [ "$(command -v sdd || true)" != "$PREFIX/sdd${EXE_SUFFIX}" ]; then
  echo "请将 $PREFIX 加入 PATH："
  echo "  export PATH=\"$PREFIX:\$PATH\""
fi
echo "安装完成: $PREFIX/sdd${EXE_SUFFIX}"
