# AI Agent 自举安装 sdd

本文面向需要自行安装或更新 `sdd` 的 AI Agent。项目通过 GitHub Releases 发布预编译二进制，安装不需要 Rust 工具链。

## 下载最新 Release

GitHub 最新 Release 的下载地址（`latest` 会自动指向最新版本）：

```text
https://github.com/liuyi-it/sdd-harness/releases/latest/download/<asset>
```

按平台选择资产：

| 平台 | 资产 |
| --- | --- |
| Linux x86_64 | `sdd-linux-x64` |
| Linux x86_64（musl/Alpine） | `sdd-linux-x64-musl` |
| macOS x86_64（Intel） | `sdd-macos-x64` |
| macOS arm64（Apple Silicon） | `sdd-macos-arm64` |
| Windows x86_64 | `sdd-windows-x64.exe` |

## Linux / macOS 安装与升级

下面在独立 Bash 中运行，下载、校验或版本验证失败均保留原命令；成功后只清理本次暂存。可用 `PREFIX` 指定安装目录。

```bash
bash <<'SH'
set -euo pipefail
os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  Darwin-arm64) asset="sdd-macos-arm64" ;;
  Darwin-x86_64) asset="sdd-macos-x64" ;;
  Linux-x86_64)
    if ldd --version 2>&1 | grep -qi musl; then
      asset="sdd-linux-x64-musl"
    else
      asset="sdd-linux-x64"
    fi
    ;;
  *) echo "不支持的平台: ${os}-${arch}" >&2; exit 1 ;;
esac
if [ -n "${PREFIX:-}" ]; then
  install_dir="$PREFIX"
elif [ -d "$HOME/.local/bin" ] || [ ! -w /usr/local/bin ]; then
  install_dir="$HOME/.local/bin"
else
  install_dir="/usr/local/bin"
fi
download_dir="$(mktemp -d)"
staged_bin=""
cleanup() {
  rm -rf "$download_dir"
  if [ -n "$staged_bin" ]; then rm -f "$staged_bin"; fi
}
trap cleanup EXIT
base="https://github.com/liuyi-it/sdd-harness/releases/latest/download"
curl -fL -o "$download_dir/$asset" "$base/$asset"
curl -fL -o "$download_dir/$asset.sha256" "$base/$asset.sha256"
expected="$(awk 'NR == 1 {print $1}' "$download_dir/$asset.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$download_dir/$asset" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$download_dir/$asset" | awk '{print $1}')"
fi
[ "$actual" = "$expected" ] || { echo "SHA-256 校验失败" >&2; exit 1; }
mkdir -p "$install_dir"
[ ! -d "$install_dir/sdd" ] || { echo "安装目标是目录" >&2; exit 1; }
staged_bin="$(mktemp "$install_dir/.sdd-install.XXXXXX")"
install -m 0755 "$download_dir/$asset" "$staged_bin"
"$staged_bin" --version
mv -f "$staged_bin" "$install_dir/sdd"
staged_bin=""
echo "安装完成: $install_dir/sdd"
SH
```

若实际安装目录不在 PATH 中，将它加入当前 shell 和持久化配置后再使用 `sdd`。脚本不修改其他 PATH 位置的文件；发现重复安装时先逐项核对 `type -a sdd` 的路径与来源，再按用户授权清理。

## Windows 安装与升级（PowerShell）

升级前先结束运行中的 `sdd.exe`。文件占用、下载、校验、版本验证或权限错误会停止替换；已有文件使用 `File.Replace`，没有旧安装时使用 `File.Move`。不要先删除原命令。

```powershell
$ErrorActionPreference = 'Stop'
$installDir = "$env:LOCALAPPDATA\sdd"
$downloadDir = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
$stagedPath = $null
New-Item -ItemType Directory $downloadDir | Out-Null
try {
    $asset = 'sdd-windows-x64.exe'
    $base = 'https://github.com/liuyi-it/sdd-harness/releases/latest/download'
    $download = Join-Path $downloadDir $asset
    $checksum = "$download.sha256"
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $download
    Invoke-WebRequest -Uri "$base/$asset.sha256" -OutFile $checksum
    $expected = ((Get-Content $checksum).Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash $download -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) { throw 'SHA-256 校验失败' }
    New-Item -ItemType Directory -Force $installDir | Out-Null
    $stagedPath = Join-Path $installDir ('.sdd-install-' + [IO.Path]::GetRandomFileName() + '.exe')
    Copy-Item $download $stagedPath
    & $stagedPath --version
    if ($LASTEXITCODE -ne 0) { throw '新二进制版本验证失败' }
    $destination = Join-Path $installDir 'sdd.exe'
    if (Test-Path $destination -PathType Leaf) {
        [IO.File]::Replace($stagedPath, $destination, $null)
    } else {
        [IO.File]::Move($stagedPath, $destination)
    }
    $stagedPath = $null
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($installDir -notin ($userPath -split ';')) {
        [Environment]::SetEnvironmentVariable('Path', "$installDir;$userPath", 'User')
    }
    if ($installDir -notin ($env:Path -split ';')) { $env:Path = "$installDir;$env:Path" }
    Write-Output "安装完成: $destination"
} finally {
    if ($stagedPath -and (Test-Path $stagedPath)) { Remove-Item $stagedPath -Force }
    Remove-Item $downloadDir -Recurse -Force
}
```

此 PowerShell 示例经过静态复核，尚未在本轮 Windows 实机运行；Rust 的 Windows CI 不能代替该安装与文件占用验证。

## 验证与升级边界

- 验证安装：`sdd --version`，并核对 PATH 中实际命令位置。
- 最新 Release 二进制与对应 `.sha256` 在替换前一起校验。`latest` 更新期间若两次下载不一致，校验失败会保留原安装；重新运行即可。
- 以上命令仅操作本次目标安装文件及自身暂存；不扫描或删除其他项目的宿主资产、状态、历史归档或业务文件。
- 已授权清理旧安装时，先核实具体二进制、备份和构建目录的来源，再清理；无需制作或保留旧版兼容副本。共享 Rust/Maven 缓存和其他工具不属于旧 `sdd` 安装。

`v0.7.0` 使用 Runtime schema 9，规格结果和持久规格版本为 6.0.0，计划结果版本为 4.0.0：初始化后 `.sdd/` 只有 `runtime.json` 和 `lock`，校验和内嵌，不生成自动备份或独立诊断文件。版本号未必随每次源码变更递增，安装源码版时还应核对构建提交。

不迁移旧项目的 `.sdd`。升级 CLI 后若返回 `E_STATE_VERSION_UNSUPPORTED`，先用匹配版本读取并保留需要的结果，用户确认旧状态无需保留后再删除该目录并执行 `sdd init`。不要让 Agent 自动删除用户的旧状态；也不要只删除旧校验文件或让不同格式的 CLI 交替操作同一目录。

## 从源码安装（备选）

需要 Rust 工具链时，可克隆仓库后执行 `bash scripts/install.sh`，具体见 [README](../README.md)。脚本使用 `--locked` 与本机 target，`CARGO_TARGET_DIR` 的相对路径以调用时工作目录为基准；构建期间不删除旧命令，验证成功后才同目录替换。`PREFIX` 指定安装位置，安装后只清理本次暂存，不保留旧二进制备份。卸载脚本删除失败时返回非零并显示错误，业务项目 `.sdd/` 始终保留。
