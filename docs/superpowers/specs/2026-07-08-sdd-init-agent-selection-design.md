# sdd init 适配器选择与追加安装设计

## 背景

当前 `sdd init` 始终同时安装 Claude Code 和 Codex 的集成文件（CLAUDE.md、AGENTS.md、`.claude/`、`.codex/`），无法按需选择。此外指令文件采用覆盖写而非追加写，已有内容可能丢失。

## 目标

1. `sdd init` 支持选择安装哪些 AI Agent 适配器，不再全部安装
2. 适配器列表动态发现，新增适配器包可自动适配
3. 对 CLAUDE.md / AGENTS.md 采用行级去重追加，已有行不重复写入

## 设计

### 1. AdapterManifest 接口

定义在 `packages/core/src/adapters/types.ts`：

```typescript
interface AdapterManifest {
  agent: string; // "claude" | "codex" | "opencode"
  instructionFile: string; // "CLAUDE.md" | "AGENTS.md"
  instructionContent: string; // 追加的指令块（含 <!-- sdd-harness:managed -->）
  commandsDir: string; // ".claude/commands" | ".codex/commands" 等
  skillsDir?: string; // ".claude/skills/sdd-harness" 等
  skillContent?: string; // SKILL.md 内容
  rules?: Array<{ file: string; content: string }>;
}
```

各适配器包统一导出 `manifest` 对象。commands 不在 manifest 中逐条列出，所有适配器的 sdd 命令内容相同，按 `commandsDir` 统一下发。

### 2. 适配器动态发现

`packages/core/src/adapters/registry.ts` 维护包名注册表：

```typescript
const ADAPTER_PACKAGES = [
  "@sdd-harness/claude-code-adapter",
  "@sdd-harness/codex-adapter",
  "@sdd-harness/opencode-adapter",
];
```

`getAvailableAdapters(): AdapterManifest[]` 遍历注册表执行 `import(pkg)`：

- 成功 → 取 `.manifest` 加入可用列表
- 失败（包未安装或导出不合规）→ 跳过，不阻断

新增适配器仅需：创建包并导出 `manifest` + 注册表中追加一行包名。

### 3. CLI 参数与交互

CLI `cli.ts` 新增 `--agent` 参数：

```bash
sdd init --agent claude,opencode   # 跳过交互，安装指定适配器
sdd init                            # 进入交互模式
```

**非交互模式**：解析逗号分隔列表，校验是否在可用列表中，不在则报错。

**交互模式**：

```
检测到以下可用的 AI Agent 适配器，请选择要安装的（输入编号，多选用逗号分隔）：
  1. claude   - Claude Code
  2. codex    - OpenAI Codex CLI
  3. opencode - OpenCode

请输入编号（至少选择一个）：
```

- 输入如 `1,3` → 选中 claude 和 opencode
- 无效编号 → 重新输入；空输入 → 提示至少选一个
- `--non-interactive` 且无 `--agent` → 报错退出
- 默认全不选，用户必须明确选择

交互式 IO 在 CLI 层的 `runInit()` 中完成，结果写入 `args.agent` 字符串数组传给 Core。

### 4. 安装逻辑

#### project-installer.ts 改造

`installProjectIntegration` 签名变更：

```typescript
// 旧
installProjectIntegration(root: string, options: { force?: boolean })
// 新
installProjectIntegration(root: string, manifests: AdapterManifest[], options: { force?: boolean })
```

遍历 `manifests`，对每个执行：

1. 追加指令文件 → `installInstructions(path, manifest.instructionContent, options)`（行级去重逻辑不变，见第 5 节）
2. 写入 commands → 按 `manifest.commandsDir` 生成所有 sdd 命令文件
3. 写入 skills → 若 `manifest.skillsDir` 存在，写入 SKILL.md
4. 写入 rules → 若 `manifest.rules` 存在，逐条写入

`installSchemas` 保持不变（与 agent 无关，始终生成）。

#### init.ts 改造

- `runInit` 接收 `args.agent: string[]`
- `defaultConfig()` 的 `plugins` 仅包含选中的适配器（如选了 claude，则 `plugins: { claudeCode: { enabled: true } }`；未选的不出现）

### 5. 追加去重逻辑

现有实现位于 `project-installer.ts` 的 `installInstructions`（第 51-54 行），本次不改动：

- 文件不存在 → 创建完整内容
- 文件存在 + 有新行 → 追加到文件末尾
- 文件存在 + 无新行 → 仅补 `.meta.json`，不修改文件
- `force=true` → 全量覆盖

## 调用链

```
sdd init --agent claude,opencode
  → CLI: 解析 --agent → args.agent = ["claude", "opencode"]
  → CLI runInit(): 调用 Core
  → Core runInit(): getAvailableAdapters() → 过滤出 claude + opencode manifest
  → installProjectIntegration(root, [claudeManifest, opencodeManifest])
  → 逐适配器追加指令 + 写入 commands/skills/rules
  → 写入 config.yml（plugins 仅含选中的）
  → 完成
```

## 涉及文件

| 文件                                             | 改动                                                                                                                                                                    |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `packages/core/src/adapters/types.ts`            | **新增** AdapterManifest 接口                                                                                                                                           |
| `packages/core/src/adapters/registry.ts`         | **新增** 注册表 + getAvailableAdapters()                                                                                                                                |
| `packages/cli/src/cli.ts`                        | 新增 --agent 参数解析                                                                                                                                                   |
| `packages/cli/src/commands/init.ts`              | 交互式选择逻辑                                                                                                                                                          |
| `packages/core/src/commands/init.ts`             | 接收 args.agent，过滤 manifests；defaultConfig 仅写选中 agent                                                                                                           |
| `packages/core/src/install/project-installer.ts` | 签名改为接收 manifests[]，遍历安装；移除 `claudeInstructions()`、`codexInstructions()`、`installClaudeCommands()`、`installSkills()` 等硬编码函数，统一由 manifest 驱动 |
| `packages/claude-code-adapter/src/index.ts`      | **新增** manifest 导出                                                                                                                                                  |
| `packages/codex-adapter/src/index.ts`            | **新增** manifest 导出                                                                                                                                                  |
| `packages/opencode-adapter/src/index.ts`         | **新增** manifest 导出                                                                                                                                                  |

## 测试要点

1. `--agent claude` → 仅安装 CLAUDE.md + `.claude/commands/`，不生成 AGENTS.md 和 `.codex/`
2. `--agent claude,codex,opencode` → 安装全部三个
3. 交互模式输入 `1,2` → 安装 claude + codex
4. `--agent unknown` → 报错退出
5. CLAUDE.md 已有相同行 → 追加后不重复
6. CLAUDE.md 部分行已存在 → 仅追加新行
7. `--non-interactive` 无 `--agent` → 报错退出
8. 新建 adapter 包 + 注册表追加 → 自动出现在可用列表
