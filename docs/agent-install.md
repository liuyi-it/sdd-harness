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
| macOS x86_64（Intel） | `sdd-macos-x64` |
| macOS arm64（Apple Silicon） | `sdd-macos-arm64` |
| Windows x86_64 | `sdd-windows-x64.exe` |

## Linux / macOS 安装

```bash
os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  Darwin-arm64) asset="sdd-macos-arm64" ;;
  Darwin-x86_64) asset="sdd-macos-x64" ;;
  Linux-x86_64) asset="sdd-linux-x64" ;;
  *) echo "不支持的平台: ${os}-${arch}" >&2; exit 1 ;;
esac
install_dir="${PREFIX:-/usr/local/bin}"
curl -fL -o "${install_dir}/sdd" "https://github.com/liuyi-it/sdd-harness/releases/latest/download/${asset}"
chmod +x "${install_dir}/sdd"
sdd --version
```

`/usr/local/bin` 不可写或不在 PATH 时，改用 `PREFIX=$HOME/.local/bin`，并确保该目录在 PATH 中。

## Windows 安装（PowerShell）

```powershell
$installDir = "$env:LOCALAPPDATA\sdd"
New-Item -ItemType Directory -Force $installDir | Out-Null
Invoke-WebRequest -Uri "https://github.com/liuyi-it/sdd-harness/releases/latest/download/sdd-windows-x64.exe" -OutFile "$installDir\sdd.exe"
[Environment]::SetEnvironmentVariable("Path", "$installDir;$([Environment]::GetEnvironmentVariable('Path','User'))", 'User')
$env:Path = "$installDir;$env:Path"
sdd --version
```

## 验证与升级

- 验证安装：`sdd --version`。
- 升级：重新执行对应平台的下载命令即可覆盖旧版本；Windows 升级前先结束运行中的 `sdd.exe`。

## 从源码安装（备选）

需要 Rust 工具链时，可克隆仓库后执行 `bash scripts/install.sh`，具体见 [README](../README.md)。
