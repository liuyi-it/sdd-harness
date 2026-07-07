# 二期 → 三期迁移指南

## 主要变化

1. **CLI 成为唯一入口**：不再依赖插件宿主执行 TypeScript
2. **Node.js >= 22**：不再支持 Node.js 20
3. **插件 → Adapter**：`claude-code-plugin` → `claude-code-adapter`，`codex-plugin` → `codex-adapter`
4. **内置 codebase-memory-mcp**：无需手动安装 MCP，CLI 自动托管启动
5. **不发布 npm**：通过仓库安装脚本 `scripts/install.sh` 本地全局安装

## 迁移步骤

### 1. 升级 Node.js

```bash
node -v  # 确保 >= 22
```

### 2. 拉取最新代码

```bash
cd sdd-harness
git pull
```

### 3. 重新安装

macOS / Linux / Windows（Git Bash）:

```bash
bash scripts/install.sh
```

### 4. 更新 .sdd/config.yml

在现有配置中添加 `codebase` 配置段（参见 `docs/codebase-memory-mcp.md`）：

```yaml
codebase:
  provider: codebase-memory-mcp
  mode: managed
  version: "0.8.1"
  autoStart: true
  autoIndex: true
  requireAvailable: false
  storageDir: .sdd/index/codebase-memory
  diagnosticsFile: .sdd/adapters/codebase-memory-mcp/diagnostics.json
  capabilitiesFile: .sdd/adapters/codebase-memory-mcp/capabilities.json
  timeoutMs: 30000
  fallback:
    enabled: true
    provider: fallback-file-scan
```

### 5. 重新初始化

```bash
sdd init
```

### 6. 兼容性说明

- `.sdd/` 已有制品（changes、runs 等）应兼容读取
- 如有问题，执行 `sdd codebase doctor` 诊断

## 卸载旧版插件

如果之前安装过 Claude Code / Codex 插件版本：

- **Claude Code**：移除 marketplace 中的 `sdd-harness` 条目
- **Codex**：删除 `~/.codex/plugins/sdd-harness` 目录及对应的 `marketplace.json` 条目
