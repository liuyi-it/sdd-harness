#!/usr/bin/env bash

# 安装与卸载共享的清理函数。调用方负责启用 set -euo pipefail。

sdd_remove_local_artifacts() {
  local project_root="$1"
  local cleanup_failed=false

  rm -rf "$project_root/node_modules" || cleanup_failed=true
  if [ -d "$project_root/packages" ]; then
    find "$project_root/packages" -type d \( -name node_modules -o -name dist \) \
      -prune -exec rm -rf {} + || cleanup_failed=true
  fi
  find "$project_root" -type f -name '*.tsbuildinfo' -delete || cleanup_failed=true

  if [ "$cleanup_failed" = true ]; then
    echo "错误: 部分本地安装产物清理失败。" >&2
    return 1
  fi
}

sdd_remove_global_cli() {
  local project_root="$1"
  local cleanup_failed=false

  if ! command -v npm >/dev/null 2>&1; then
    echo "警告: 未找到 npm，无法检查全局 CLI；继续清理本地文件。" >&2
    return
  fi

  # 同一台机器可能残留多个 Node/npm 前缀。先清理 PATH 中所有属于
  # sdd-harness 的旧入口，避免新安装完成后仍命中另一套旧版命令。
  sdd_remove_owned_path_shims "$project_root" || cleanup_failed=true

  npm uninstall --global @sdd-harness/cli >/dev/null 2>&1 || true

  local global_root
  if ! global_root="$(npm root --global 2>/dev/null)"; then
    echo "错误: 无法确定 npm 全局模块目录。" >&2
    cleanup_failed=true
    global_root=""
  fi
  if [ -n "$global_root" ]; then
    rm -rf "$global_root/@sdd-harness/cli" || cleanup_failed=true
  fi

  local global_prefix
  if ! global_prefix="$(npm prefix --global 2>/dev/null)"; then
    echo "错误: 无法确定 npm 全局命令目录。" >&2
    cleanup_failed=true
    global_prefix=""
  fi
  if [ -n "$global_prefix" ]; then
    sdd_remove_owned_shim "$global_prefix/bin/sdd" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/bin/sdd-harness" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/sdd" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/sdd.cmd" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/sdd.ps1" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/sdd-harness" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/sdd-harness.cmd" "$project_root" || cleanup_failed=true
    sdd_remove_owned_shim "$global_prefix/sdd-harness.ps1" "$project_root" || cleanup_failed=true
  fi

  if [ "$cleanup_failed" = true ]; then
    echo "错误: 部分全局安装产物清理失败。" >&2
    return 1
  fi

  hash -r 2>/dev/null || true
}

sdd_remove_owned_path_shims() {
  local project_root="$1"
  local cleanup_failed=false
  local command_name
  for command_name in sdd sdd-harness; do
    while IFS= read -r path; do
      [ -n "$path" ] || continue
      sdd_remove_owned_shim_family "$path" "$project_root" || cleanup_failed=true
    done < <(type -a -p "$command_name" 2>/dev/null || true)
  done
  if [ "$cleanup_failed" = true ]; then
    return 1
  fi
}

sdd_remove_owned_shim_family() {
  local path="$1"
  local project_root="$2"
  local base="$path"
  case "$base" in
    *.cmd) base="${base%.cmd}" ;;
    *.ps1) base="${base%.ps1}" ;;
  esac
  sdd_remove_owned_shim "$base" "$project_root"
  sdd_remove_owned_shim "$base.cmd" "$project_root"
  sdd_remove_owned_shim "$base.ps1" "$project_root"
}

sdd_remove_owned_shim() {
  local path="$1"
  local project_root="$2"
  if sdd_is_owned_shim "$path" "$project_root"; then
    rm -f "$path"
  fi
}

sdd_is_owned_shim() {
  local path="$1"
  local project_root="$2"
  if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    return 1
  fi

  if [ -L "$path" ]; then
    local target
    target="$(readlink "$path" 2>/dev/null || true)"
    target="$(printf '%s' "$target" | tr '\\' '/')"
    local normalized_root
    normalized_root="$(printf '%s' "$project_root" | tr '\\' '/')"
    case "$target" in
      *"@sdd-harness/cli"*|*"$normalized_root/packages/cli"*|*"packages/cli"*) return 0 ;;
      *) return 1 ;;
    esac
  fi

  local content
  content="$(tr '\\' '/' <"$path" 2>/dev/null || true)"
  local normalized_root
  normalized_root="$(printf '%s' "$project_root" | tr '\\' '/')"
  case "$content" in
    *"@sdd-harness/cli"*|*"$normalized_root/packages/cli"*|*"packages/cli"*) return 0 ;;
    *) return 1 ;;
  esac
}

sdd_assert_no_local_artifacts() {
  local project_root="$1"
  local residuals=()

  if [ -e "$project_root/node_modules" ]; then
    residuals+=("node_modules")
  fi
  while IFS= read -r path; do
    residuals+=("${path#"$project_root/"}")
  done < <(
    find "$project_root/packages" \
      \( -type d \( -name node_modules -o -name dist \) -o -type f -name '*.tsbuildinfo' \) \
      -print 2>/dev/null
  )
  while IFS= read -r path; do
    residuals+=("${path#"$project_root/"}")
  done < <(find "$project_root" -maxdepth 1 -type f -name '*.tsbuildinfo' -print)

  if [ "${#residuals[@]}" -gt 0 ]; then
    echo "错误: 清理后仍存在安装产物: ${residuals[*]}" >&2
    return 1
  fi
}

sdd_assert_no_global_cli() {
  local project_root="$1"
  if ! command -v npm >/dev/null 2>&1; then
    echo "错误: 未找到 npm，无法确认全局 CLI 已清理。" >&2
    return 1
  fi

  local residuals=()
  local global_root
  if ! global_root="$(npm root --global 2>/dev/null)"; then
    echo "错误: 无法检查 npm 全局模块目录。" >&2
    return 1
  fi
  if [ -n "$global_root" ] && [ -e "$global_root/@sdd-harness/cli" ]; then
    residuals+=("$global_root/@sdd-harness/cli")
  fi

  local global_prefix
  if ! global_prefix="$(npm prefix --global 2>/dev/null)"; then
    echo "错误: 无法检查 npm 全局命令目录。" >&2
    return 1
  fi
  if [ -n "$global_prefix" ]; then
    local path
    for path in \
      "$global_prefix/bin/sdd" \
      "$global_prefix/bin/sdd-harness" \
      "$global_prefix/sdd" \
      "$global_prefix/sdd.cmd" \
      "$global_prefix/sdd.ps1" \
      "$global_prefix/sdd-harness" \
      "$global_prefix/sdd-harness.cmd" \
      "$global_prefix/sdd-harness.ps1"; do
      if sdd_is_owned_shim "$path" "$project_root"; then
        residuals+=("$path")
      fi
    done
  fi

  if [ "${#residuals[@]}" -gt 0 ]; then
    echo "错误: 清理后仍存在全局安装产物: ${residuals[*]}" >&2
    return 1
  fi
}

sdd_verify_global_cli() {
  local project_root="$1"
  local global_root
  local global_prefix
  if ! global_root="$(npm root --global 2>/dev/null)"; then
    echo "错误: 无法确定 npm 全局模块目录。" >&2
    return 1
  fi
  if ! global_prefix="$(npm prefix --global 2>/dev/null)"; then
    echo "错误: 无法确定 npm 全局命令目录。" >&2
    return 1
  fi

  local linked_cli="$global_root/@sdd-harness/cli"
  local expected_cli="$project_root/packages/cli"
  local linked_target
  local expected_target
  linked_target="$(cd "$linked_cli" 2>/dev/null && pwd -P)" || {
    echo "错误: 全局 CLI 链接不存在: $linked_cli" >&2
    return 1
  }
  expected_target="$(cd "$expected_cli" 2>/dev/null && pwd -P)" || return 1
  if [ "$(sdd_normalize_path "$linked_target")" != "$(sdd_normalize_path "$expected_target")" ]; then
    echo "错误: 全局 CLI 未链接到当前仓库。" >&2
    echo "实际: $linked_target" >&2
    echo "期望: $expected_target" >&2
    return 1
  fi

  hash -r 2>/dev/null || true
  local command_name
  for command_name in sdd sdd-harness; do
    local resolved
    resolved="$(command -v "$command_name" 2>/dev/null || true)"
    if [ -z "$resolved" ]; then
      echo "错误: 安装后找不到命令 $command_name。" >&2
      return 1
    fi
    if ! sdd_is_global_command_path "$resolved" "$global_prefix" "$command_name"; then
      echo "错误: $command_name 被 PATH 中的其他命令遮蔽。" >&2
      echo "当前命中: $resolved" >&2
      echo "本次安装目录: $global_prefix" >&2
      echo "请清理上述冲突入口或调整 PATH 后重新安装。" >&2
      return 1
    fi
    echo "$command_name 命令位置: $resolved"
    "$resolved" --version
  done
}

sdd_is_global_command_path() {
  local path
  local prefix
  local command_name="$3"
  path="$(sdd_normalize_path "$1")"
  prefix="$(sdd_normalize_path "$2")"
  case "$path" in
    "$prefix/$command_name"|"$prefix/$command_name.cmd"|"$prefix/$command_name.ps1"|\
    "$prefix/bin/$command_name"|"$prefix/bin/$command_name.cmd"|"$prefix/bin/$command_name.ps1") return 0 ;;
    *) return 1 ;;
  esac
}

sdd_normalize_path() {
  local value
  value="$(printf '%s' "$1" | tr '\\' '/')"
  if command -v cygpath >/dev/null 2>&1 && [[ "$value" =~ ^[A-Za-z]:/ ]]; then
    value="$(cygpath -u "$value")"
  fi
  if [ "${OS:-}" = "Windows_NT" ]; then
    value="$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')"
  fi
  printf '%s' "$value" | sed 's:/*$::'
}
