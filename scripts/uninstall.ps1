Write-Host "卸载 sdd-harness..." -ForegroundColor Yellow
npm unlink --workspace=packages/cli 2>$null
Write-Host "sdd-harness 已卸载" -ForegroundColor Green
