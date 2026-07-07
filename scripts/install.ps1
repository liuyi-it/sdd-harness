<#
.SYNOPSIS
  sdd-harness 一键全局安装脚本 (Windows PowerShell)
.DESCRIPTION
  检查 Node.js >= 22，安装依赖，构建，全局 link。
  用法: powershell -ExecutionPolicy Bypass -File scripts/install.ps1
#>

Write-Host "=== sdd-harness 安装 ===" -ForegroundColor Cyan

# 检查 Node.js 版本
$nodeVersion = (node -v) -replace 'v', ''
$majorVersion = [int]($nodeVersion -split '\.')[0]
if ($majorVersion -lt 22) {
    Write-Host "错误: sdd-harness 要求 Node.js >= 22，当前版本: $(node -v)" -ForegroundColor Red
    Write-Host "请升级 Node.js 后重试: https://nodejs.org/"
    exit 1
}

# 进入项目根目录
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Resolve-Path "$ScriptDir\.."
Set-Location $ProjectRoot

# 安装依赖
Write-Host "安装依赖..." -ForegroundColor Yellow
npm install
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# 构建
Write-Host "构建..." -ForegroundColor Yellow
npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# 全局 link
Write-Host "全局安装 sdd CLI..." -ForegroundColor Yellow
npm link --workspace=packages/cli
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# 验证
Write-Host "验证安装..." -ForegroundColor Yellow
sdd --version
sdd-harness --version

Write-Host ""
Write-Host "=== 安装完成 ===" -ForegroundColor Green
Write-Host "可用命令: sdd, sdd-harness"
Write-Host "使用 sdd init 初始化项目"
