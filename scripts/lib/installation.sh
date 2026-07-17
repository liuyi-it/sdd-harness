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
    case "$target" in
      *"@sdd-harness/cli"*|*"$project_root/packages/cli"*) return 0 ;;
      *) return 1 ;;
    esac
  fi

  grep -Fq '@sdd-harness/cli' "$path" 2>/dev/null ||
    grep -Fq "$project_root/packages/cli" "$path" 2>/dev/null
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
