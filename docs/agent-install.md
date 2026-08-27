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

## Linux / macOS 安装

```bash
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
mkdir -p "$install_dir"
curl -fL -o "${install_dir}/sdd" "https://github.com/liuyi-it/sdd-harness/releases/latest/download/${asset}"
chmod +x "${install_dir}/sdd"
"${install_dir}/sdd" --version
```

若实际安装目录不在 PATH 中，将它加入当前 shell 和持久化配置后再使用 `sdd`。

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
- 校验下载完整性（macOS/Linux）：

```bash
asset="sdd-macos-arm64" # 按平台替换
checksum_file="/tmp/$asset.sha256"
sdd_path="$(command -v sdd)"
[ -n "$sdd_path" ] || { echo "PATH 中未找到 sdd" >&2; exit 1; }
curl -fL -o "$checksum_file" "https://github.com/liuyi-it/sdd-harness/releases/latest/download/$asset.sha256"
expected="$(awk '{print $1}' "$checksum_file")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$sdd_path" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$sdd_path" | awk '{print $1}')"
fi
[ "$actual" = "$expected" ] || { echo "SHA-256 校验失败" >&2; exit 1; }
```

Windows（PowerShell）：

```powershell
$checksumFile = "$env:TEMP\sdd-windows-x64.exe.sha256"
Invoke-WebRequest -Uri "https://github.com/liuyi-it/sdd-harness/releases/latest/download/sdd-windows-x64.exe.sha256" -OutFile $checksumFile
$expected = ((Get-Content $checksumFile).Trim() -split '\s+')[0].ToLower()
$actual = (Get-FileHash "$installDir\sdd.exe" -Algorithm SHA256).Hash.ToLower()
if ($actual -ne $expected) { throw "SHA-256 校验失败" }
```

- 升级：重新执行对应平台的下载命令即可覆盖旧版本；Windows 升级前先结束运行中的 `sdd.exe`。

## 从源码安装（备选）

需要 Rust 工具链时，可克隆仓库后执行 `bash scripts/install.sh`，具体见 [README](../README.md)。
