# sdd init 适配器选择与追加安装 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `sdd init` 支持用户选择安装哪些 AI Agent 适配器（不再全部安装），指令文件采用行级去重追加。

**Architecture:** 每个 adapter 包新增 `manifest.json` 自描述安装内容；Core 通过动态导入发现可用适配器；`project-installer.ts` 改为 manifest 驱动的遍历安装；CLI 层在调用 Core 前完成交互式选择。

**Tech Stack:** TypeScript ESM, Node.js 22+, Vitest, YAML (config)

## Global Constraints

- 遵循 Karpathy 风格：先思考再编码、简单优先、手术式修改、目标驱动
- 中文项目：注释和用户可见文本使用中文
- 提交信息使用中文
- 遵循现有文件风格：2 空格缩进、小写短横线命名、`*.test.ts` 测试文件

---

## 文件结构

| 文件                                              | 职责                            | 操作     |
| ------------------------------------------------- | ------------------------------- | -------- |
| `packages/core/src/adapters/types.ts`             | AdapterManifest 接口定义        | **新建** |
| `packages/core/src/adapters/registry.ts`          | 适配器注册表 + 动态发现         | **新建** |
| `packages/core/src/index.ts`                      | 导出新的 adapter 模块           | 修改     |
| `packages/claude-code-adapter/manifest.json`      | Claude Code 适配器安装清单      | **新建** |
| `packages/codex-adapter/manifest.json`            | Codex 适配器安装清单            | **新建** |
| `packages/opencode-adapter/manifest.json`         | OpenCode 适配器安装清单         | **新建** |
| `packages/core/src/install/project-installer.ts`  | manifest 驱动安装逻辑           | 修改     |
| `packages/core/src/commands/init.ts`              | 接收 agent 参数，过滤 manifests | 修改     |
| `packages/cli/src/cli.ts`                         | 新增 `--agent` 参数             | 修改     |
| `packages/cli/src/commands/init.ts`               | 交互式选择逻辑                  | 修改     |
| `packages/core/test/init-agent-selection.test.ts` | 适配器选择相关测试              | **新建** |
| `packages/core/test/init-status.test.ts`          | 适配现有测试                    | 修改     |

---

### Task 1: 创建 AdapterManifest 类型定义

**Files:**

- Create: `packages/core/src/adapters/types.ts`
- Modify: `packages/core/src/index.ts`

**Interfaces:**

- Produces: `AdapterManifest` interface — 供 registry、project-installer、init 使用

- [ ] **Step 1: 创建 types.ts**

```typescript
// packages/core/src/adapters/types.ts

/**
 * 适配器安装清单，由各 adapter 包的 manifest.json 提供。
 * 描述该 agent 需要的集成文件及内容。
 */
export interface AdapterManifest {
  /** 适配器标识，如 "claude"、"codex"、"opencode" */
  agent: string;
  /** 指令文件名，如 "CLAUDE.md"、"AGENTS.md" */
  instructionFile: string;
  /** 追加到指令文件的内容（含 <!-- sdd-harness:managed --> 标记） */
  instructionContent: string;
  /** commands 目录路径，如 ".claude/commands" */
  commandsDir: string;
  /** command 文件模板，{command} 会被替换为具体命令名 */
  commandTemplate: string;
  /** skills 目录路径（可选），如 ".claude/skills/sdd-harness" */
  skillsDir?: string;
  /** SKILL.md 内容（可选） */
  skillContent?: string;
  /** rules 文件列表（可选） */
  rules?: Array<{ file: string; content: string }>;
}
```

- [ ] **Step 2: 从 index.ts 导出新模块**

在 `packages/core/src/index.ts` 末尾追加一行：

```typescript
export type { AdapterManifest } from "./adapters/types.js";
```

- [ ] **Step 3: 运行类型检查验证**

```bash
npm run typecheck
```

Expected: 通过，无类型错误。

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/adapters/types.ts packages/core/src/index.ts
git commit -m "feat: 新增 AdapterManifest 接口定义"
```

---

### Task 2: 为三个适配器包添加 manifest.json

**Files:**

- Create: `packages/claude-code-adapter/manifest.json`
- Create: `packages/codex-adapter/manifest.json`
- Create: `packages/opencode-adapter/manifest.json`

**Interfaces:**

- Produces: 三个 `manifest.json` 文件，符合 `AdapterManifest` 接口
- Note: `{command}` 在 `commandTemplate` 中是占位符，由 installer 替换为具体命令名

- [ ] **Step 1: 创建 claude-code-adapter/manifest.json**

```json
{
  "agent": "claude",
  "instructionFile": "CLAUDE.md",
  "instructionContent": "<!-- sdd-harness:managed -->\n## sdd-harness\n\n使用 /sdd.auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。verify 或 review 失败后必须停止。\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。",
  "commandsDir": ".claude/commands",
  "commandTemplate": "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n请使用已安装的 ClaudeCodeAdapter 执行 /sdd.{command} $ARGUMENTS，直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
  "skillsDir": ".claude/skills/sdd-harness",
  "skillContent": "---\nname: sdd-harness\ndescription: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n---\n\n# SDD Harness\n\n通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。不得绕过阶段、锁、文件范围、验证、审查或归档门禁。遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\nMCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n"
}
```

- [ ] **Step 2: 创建 codex-adapter/manifest.json**

```json
{
  "agent": "codex",
  "instructionFile": "AGENTS.md",
  "instructionContent": "<!-- sdd-harness:managed -->\n## sdd-harness\n\n使用 sdd auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。verify 或 review 失败后必须停止。\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。",
  "commandsDir": ".codex/commands",
  "commandTemplate": "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n请使用已安装的 CodexAdapter 执行 sdd {command} $ARGUMENTS，直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
  "skillsDir": ".codex/skills/sdd-harness",
  "skillContent": "---\nname: sdd-harness\ndescription: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n---\n\n# SDD Harness\n\n通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。不得绕过阶段、锁、文件范围、验证、审查或归档门禁。遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\nMCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n"
}
```

- [ ] **Step 3: 创建 opencode-adapter/manifest.json**

```json
{
  "agent": "opencode",
  "instructionFile": "AGENTS.md",
  "instructionContent": "<!-- sdd-harness:managed -->\n## sdd-harness\n\n使用 sdd auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。verify 或 review 失败后必须停止。\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。",
  "commandsDir": ".opencode/commands",
  "commandTemplate": "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n请使用已安装的 OpenCodeAdapter 执行 sdd {command} $ARGUMENTS，直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
  "skillsDir": ".opencode/skills/sdd-harness",
  "skillContent": "---\nname: sdd-harness\ndescription: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n---\n\n# SDD Harness\n\n通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。不得绕过阶段、锁、文件范围、验证、审查或归档门禁。遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\nMCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\nKarpathy 风格执行规则：\n1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n"
}
```

- [ ] **Step 4: 更新各 adapter 的 package.json files 字段，包含 manifest.json**

修改 `packages/claude-code-adapter/package.json`，`files` 数组追加 `"manifest.json"`：

```json
{
  "name": "@sdd-harness/claude-code-adapter",
  "version": "0.1.0",
  "type": "module",
  "description": "CLI-first SDD Harness adapter for Claude Code",
  "engines": {
    "node": ">=22"
  },
  "files": ["commands/", "skills/", "AGENTS.md", "manifest.json"]
}
```

修改 `packages/codex-adapter/package.json`：

```json
{
  "name": "@sdd-harness/codex-adapter",
  "version": "0.1.0",
  "type": "module",
  "description": "CLI-first SDD Harness adapter for Codex",
  "engines": {
    "node": ">=22"
  },
  "files": ["skills/", "rules/", "manifest.json"]
}
```

修改 `packages/opencode-adapter/package.json`：

```json
{
  "name": "@sdd-harness/opencode-adapter",
  "version": "0.1.0",
  "type": "module",
  "description": "CLI-first SDD Harness adapter for OpenCode",
  "engines": {
    "node": ">=22"
  },
  "files": ["rules/", "docs/", "AGENT_PROTOCOL.md", "manifest.json"]
}
```

- [ ] **Step 5: 验证 manifest.json 为合法 JSON**

```bash
node -e "JSON.parse(require('fs').readFileSync('packages/claude-code-adapter/manifest.json','utf8'))" && echo "claude OK"
node -e "JSON.parse(require('fs').readFileSync('packages/codex-adapter/manifest.json','utf8'))" && echo "codex OK"
node -e "JSON.parse(require('fs').readFileSync('packages/opencode-adapter/manifest.json','utf8'))" && echo "opencode OK"
```

Expected: 三个 "OK"。

- [ ] **Step 6: 提交**

```bash
git add packages/claude-code-adapter/manifest.json packages/claude-code-adapter/package.json \
        packages/codex-adapter/manifest.json packages/codex-adapter/package.json \
        packages/opencode-adapter/manifest.json packages/opencode-adapter/package.json
git commit -m "feat: 为三个 adapter 包添加 manifest.json，自描述安装内容"
```

---

### Task 3: 创建适配器注册表 + 动态发现

**Files:**

- Create: `packages/core/src/adapters/registry.ts`
- Modify: `packages/core/src/index.ts`

**Interfaces:**

- Produces: `getAvailableAdapters(): Promise<AdapterManifest[]>` — 动态发现所有可用适配器
- 设计说明：优先通过动态 `import()` 加载各适配器包的 `manifest.json`，若加载失败则回退到内置清单。这保证了测试环境和生产环境都能正常工作。

- [ ] **Step 1: 创建 registry.ts**

```typescript
// packages/core/src/adapters/registry.ts

import type { AdapterManifest } from "./types.js";

/**
 * 已知适配器包名注册表。
 * 新增适配器时在此追加一行即可自动被发现。
 */
const ADAPTER_PACKAGES = [
  "@sdd-harness/claude-code-adapter",
  "@sdd-harness/codex-adapter",
  "@sdd-harness/opencode-adapter",
] as const;

// ---------------------------------------------------------------------------
// 内置 fallback 清单 — 当动态 import manifest.json 失败时使用。
// 每个 adapter 包的 manifest.json 是权威来源，此处为同等内容的冗余副本。
// 新增适配器时必须同时添加 fallback 条目。
// ---------------------------------------------------------------------------

function builtinClaudeManifest(): AdapterManifest {
  return {
    agent: "claude",
    instructionFile: "CLAUDE.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\n" +
      "使用 /sdd.auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。" +
      "build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。" +
      "verify 或 review 失败后必须停止。\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。",
    commandsDir: ".claude/commands",
    commandTemplate:
      "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n" +
      "请使用已安装的 ClaudeCodeAdapter 执行 /sdd.{command} $ARGUMENTS，" +
      "直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
    skillsDir: ".claude/skills/sdd-harness",
    skillContent:
      "---\nname: sdd-harness\n" +
      "description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n" +
      "---\n\n# SDD Harness\n\n" +
      "通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。" +
      "不得绕过阶段、锁、文件范围、验证、审查或归档门禁。" +
      "遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\n" +
      "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
  };
}

function builtinCodexManifest(): AdapterManifest {
  return {
    agent: "codex",
    instructionFile: "AGENTS.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\n" +
      "使用 sdd auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。" +
      "build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。" +
      "verify 或 review 失败后必须停止。\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。",
    commandsDir: ".codex/commands",
    commandTemplate:
      "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n" +
      "请使用已安装的 CodexAdapter 执行 sdd {command} $ARGUMENTS，" +
      "直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
    skillsDir: ".codex/skills/sdd-harness",
    skillContent:
      "---\nname: sdd-harness\n" +
      "description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n" +
      "---\n\n# SDD Harness\n\n" +
      "通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。" +
      "不得绕过阶段、锁、文件范围、验证、审查或归档门禁。" +
      "遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\n" +
      "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
  };
}

function builtinOpenCodeManifest(): AdapterManifest {
  return {
    agent: "opencode",
    instructionFile: "AGENTS.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\n" +
      "使用 sdd auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。" +
      "build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。" +
      "verify 或 review 失败后必须停止。\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。",
    commandsDir: ".opencode/commands",
    commandTemplate:
      "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n" +
      "请使用已安装的 OpenCodeAdapter 执行 sdd {command} $ARGUMENTS，" +
      "直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
    skillsDir: ".opencode/skills/sdd-harness",
    skillContent:
      "---\nname: sdd-harness\n" +
      "description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n" +
      "---\n\n# SDD Harness\n\n" +
      "通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。" +
      "不得绕过阶段、锁、文件范围、验证、审查或归档门禁。" +
      "遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\n" +
      "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\n" +
      "Karpathy 风格执行规则：\n" +
      "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
      "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
      "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
      "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
  };
}

/** 包名 → fallback manifest 映射，键名需与 ADAPTER_PACKAGES 一一对应 */
const BUILTIN_FALLBACK: Record<string, () => AdapterManifest> = {
  "@sdd-harness/claude-code-adapter": builtinClaudeManifest,
  "@sdd-harness/codex-adapter": builtinCodexManifest,
  "@sdd-harness/opencode-adapter": builtinOpenCodeManifest,
};

/**
 * 动态发现所有可用的适配器。
 * 优先通过动态 import 加载各适配器包的 manifest.json；
 * 若加载失败则回退到内置 fallback 清单。
 * 新增适配器时需同时：1) 在 ADAPTER_PACKAGES 追加包名
 * 2) 在 BUILTIN_FALLBACK 追加对应 fallback 条目。
 */
export async function getAvailableAdapters(): Promise<AdapterManifest[]> {
  const manifests: AdapterManifest[] = [];
  await Promise.all(
    ADAPTER_PACKAGES.map(async (pkg) => {
      try {
        const mod = await import(`${pkg}/manifest.json`, {
          with: { type: "json" },
        });
        const manifest = mod.default as AdapterManifest;
        if (isValidManifest(manifest)) {
          manifests.push(manifest);
          return;
        }
      } catch {
        // 动态导入失败，回退到内置清单
      }
      const fallback = BUILTIN_FALLBACK[pkg];
      if (fallback) manifests.push(fallback());
    }),
  );
  return manifests;
}

function isValidManifest(value: unknown): value is AdapterManifest {
  if (value === null || typeof value !== "object") return false;
  const m = value as Record<string, unknown>;
  return (
    typeof m.agent === "string" &&
    m.agent.length > 0 &&
    typeof m.instructionFile === "string" &&
    typeof m.instructionContent === "string" &&
    typeof m.commandsDir === "string" &&
    typeof m.commandTemplate === "string"
  );
}
```

- [ ] **Step 2: 从 index.ts 导出**

在 `packages/core/src/index.ts` 末尾追加：

```typescript
export { getAvailableAdapters } from "./adapters/registry.js";
```

- [ ] **Step 3: 运行类型检查**

```bash
npm run typecheck
```

Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/adapters/registry.ts packages/core/src/index.ts
git commit -m "feat: 适配器注册表动态发现 getAvailableAdapters"
```

---

### Task 4: 重构 project-installer.ts — manifest 驱动安装

**Files:**

- Modify: `packages/core/src/install/project-installer.ts`

**Interfaces:**

- Consumes: `AdapterManifest` from Task 1, `COMMANDS` from contracts
- Produces: `installProjectIntegration(root, manifests, options)` — 新签名，按 manifest 遍历安装

- [ ] **Step 1: 重写 project-installer.ts**

将文件完整替换为以下内容：

```typescript
import { access, mkdir, readFile } from "node:fs/promises";
import { basename, join } from "node:path";

import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import type { AdapterManifest } from "../adapters/types.js";
import { COMMANDS } from "../contracts.js";
import { CANONICAL_SCHEMAS } from "./canonical-schemas.js";

export interface ProjectIntegrationResult {
  candidateFiles: string[];
}

/**
 * 按选定的适配器清单安装项目集成文件。
 * 每个适配器独立安装其指令文件、commands、skills 和 rules。
 * schemas 与适配器无关，始终安装。
 */
export async function installProjectIntegration(
  root: string,
  manifests: AdapterManifest[],
  options: { force?: boolean } = {},
): Promise<ProjectIntegrationResult> {
  const results = await Promise.all([
    ...manifests.flatMap((manifest) => [
      installInstructions(
        join(root, manifest.instructionFile),
        manifest.instructionContent,
        options,
      ),
      installCommands(root, manifest, options),
      ...(manifest.skillsDir !== undefined &&
      manifest.skillContent !== undefined
        ? [installSkill(root, manifest, options)]
        : []),
      ...(manifest.rules !== undefined
        ? manifest.rules.map((rule) =>
            installRule(root, rule.file, rule.content, options),
          )
        : []),
    ]),
    installSchemas(root, options),
  ]);
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

/**
 * 行级去重追加指令文件内容。
 * 文件不存在 → 创建；文件存在 → 仅追加不存在的行。
 * force=true 时全量覆盖。
 */
async function installInstructions(
  path: string,
  managedContent: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const writer = new ArtifactWriter();
  let existing = "";
  try {
    existing = await readFile(path, "utf8");
  } catch {
    // 文件不存在是预期情况（首次 init）
  }
  if (existing === "") {
    await writer.write(path, managedContent, managedInputs(managedContent));
    return { candidateFiles: [] };
  }

  // 逐行比较：只追加 existing 中不存在的行，避免重复内容
  const existingLines = new Set(
    existing.split("\n").map((line) => line.trimEnd()),
  );
  const newLines = managedContent
    .split("\n")
    .filter((line) => !existingLines.has(line.trimEnd()));

  if (newLines.length === 0) {
    await ensureMetadata(path, managedInputs(managedContent));
    return { candidateFiles: [] };
  }

  if (options.force === true) {
    await writer.write(path, managedContent, managedInputs(managedContent));
    return { candidateFiles: [] };
  }

  // 追加新行到文件末尾
  const appendContent = `\n${newLines.join("\n")}`;
  await writer.write(
    path,
    existing + appendContent,
    managedInputs(managedContent),
  );
  return { candidateFiles: [] };
}

/**
 * 按 manifest 的 commandsDir 和 commandTemplate 生成所有 sdd 命令文件。
 */
async function installCommands(
  root: string,
  manifest: AdapterManifest,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const directory = join(root, manifest.commandsDir);
  await mkdir(directory, { recursive: true });
  const results = await Promise.all(
    COMMANDS.map((command) =>
      writeManagedFile(
        join(directory, `sdd.${command}.md`),
        manifest.commandTemplate.replaceAll("{command}", command),
        options,
      ),
    ),
  );
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

/**
 * 按 manifest 的 skillsDir 安装 SKILL.md。
 */
async function installSkill(
  root: string,
  manifest: AdapterManifest,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const skillPath = join(root, manifest.skillsDir!, "SKILL.md");
  await mkdir(join(skillPath, ".."), { recursive: true });
  return writeManagedFile(skillPath, manifest.skillContent!, options);
}

/**
 * 安装单个 rule 文件。
 */
async function installRule(
  root: string,
  ruleFile: string,
  content: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const rulePath = join(root, ruleFile);
  await mkdir(join(rulePath, ".."), { recursive: true });
  return writeManagedFile(rulePath, content, options);
}

/**
 * 安装 JSON Schema 文件（与适配器无关，始终安装）。
 */
async function installSchemas(
  root: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const directory = join(root, ".sdd", "schemas");
  await mkdir(directory, { recursive: true });
  const results = await Promise.all(
    Object.entries(CANONICAL_SCHEMAS).map(async ([name, content]) =>
      writeManagedFile(join(directory, name), content, options),
    ),
  );
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

async function writeManagedFile(
  path: string,
  content: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const writer = new ArtifactWriter();
  try {
    const existing = await readFile(path, "utf8");
    if (existing === content) {
      await ensureMetadata(path, managedInputs(content));
      return { candidateFiles: [] };
    }
    if (options.force === true) {
      await writer.write(path, content, managedInputs(content));
      return { candidateFiles: [] };
    }
    const candidatePath = `${path}.candidate.md`;
    await writer.write(candidatePath, content, managedInputs(content));
    return { candidateFiles: [basename(candidatePath)] };
  } catch {
    await writer.write(path, content, managedInputs(content));
    return { candidateFiles: [] };
  }
}

function managedInputs(content: string): Record<string, unknown> {
  return {
    generatedBy: "sdd-harness",
    content,
  };
}

async function ensureMetadata(
  path: string,
  inputs: Record<string, unknown>,
): Promise<void> {
  try {
    await access(`${path}.meta.json`);
  } catch {
    await new ArtifactWriter().write(
      path,
      await readFile(path, "utf8"),
      inputs,
    );
  }
}
```

- [ ] **Step 2: 运行类型检查**

```bash
npm run typecheck
```

Expected: 通过。

- [ ] **Step 3: 提交**

```bash
git add packages/core/src/install/project-installer.ts
git commit -m "refactor: project-installer 改为 manifest 驱动，支持按适配器选择性安装"
```

---

### Task 5: 更新 init.ts — 接收 agent 参数并过滤 manifests

**Files:**

- Modify: `packages/core/src/commands/init.ts`

**Interfaces:**

- Consumes: `getAvailableAdapters()` from Task 3, `installProjectIntegration(manifests)` from Task 4
- Produces: `runInit` 支持 `args.agent` 参数；`defaultConfig` 仅写入选中 agent

- [ ] **Step 1: 修改 init.ts**

在 `packages/core/src/commands/init.ts` 中做以下修改：

**修改1 — 在文件顶部 import 区域新增导入：**

```typescript
import { getAvailableAdapters } from "../adapters/registry.js";
import type { AdapterManifest } from "../adapters/types.js";
```

**修改2 — 修改 `runInit` 中调用 `installProjectIntegration` 的部分（原第 123 行附近）：**

将：

```typescript
const integration = await installProjectIntegration(root, {
  force: args?.force === true,
});
```

替换为：

```typescript
const selectedAgents = normalizeAgentArg(args?.agent);
const allManifests = await getAvailableAdapters();
const manifests = filterManifests(allManifests, selectedAgents);
const integration = await installProjectIntegration(root, manifests, {
  force: args?.force === true,
});
```

**修改3 — 在 `runInit` 函数之前（`lockOptions` 函数之后）新增辅助函数：**

```typescript
/**
 * 将 args.agent 规范化为字符串数组。
 * 支持逗号分隔字符串 "claude,codex" 或 string[]。
 * 未提供时返回 undefined（表示安装全部可用适配器，保持向后兼容）。
 */
function normalizeAgentArg(raw: unknown): string[] | undefined {
  if (raw === undefined || raw === null) return undefined;
  if (Array.isArray(raw))
    return raw.filter(
      (a): a is string => typeof a === "string" && a.length > 0,
    );
  if (typeof raw === "string")
    return raw
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  return undefined;
}

/**
 * 按用户选择的 agent 列表过滤 manifest。
 * selectedAgents 为 undefined 时返回全部（向后兼容直接调用 Core API）。
 */
function filterManifests(
  all: AdapterManifest[],
  selected: string[] | undefined,
): AdapterManifest[] {
  if (selected === undefined) return all;
  const set = new Set(selected);
  return all.filter((m) => set.has(m.agent));
}
```

**修改4 — 修改 `defaultConfig` 函数的 plugins 部分（原第 275 行）：**

将：

```typescript
plugins: { claudeCode: { enabled: true }, codex: { enabled: true } },
```

替换为动态生成。先给 `defaultConfig` 增加 `agentNames` 参数：

```typescript
function defaultConfig(
  root: string,
  agentNames?: string[],
): Record<string, unknown> {
  const plugins: Record<string, unknown> = {};
  if (agentNames === undefined || agentNames.length === 0) {
    // 向后兼容默认值
    plugins.claudeCode = { enabled: true };
    plugins.codex = { enabled: true };
  } else {
    for (const name of agentNames) {
      if (name === "claude") plugins.claudeCode = { enabled: true };
      else if (name === "codex") plugins.codex = { enabled: true };
      else if (name === "opencode") plugins.openCode = { enabled: true };
      else plugins[name] = { enabled: true };
    }
  }
  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    project: {
      name: root.split(/[\\/]/).filter(Boolean).at(-1) ?? "auto-detect",
    },
    plugins,
    // ... 其余保持不变
  };
}
```

**修改5 — 修改 `defaultConfig` 的调用点（原第 118 行附近）：**

将：

```typescript
await writeIfMissing(
  join(sddRoot, "config.yml"),
  stringify(defaultConfig(root)),
);
```

替换为：

```typescript
await writeIfMissing(
  join(sddRoot, "config.yml"),
  stringify(
    defaultConfig(
      root,
      manifests.map((m) => m.agent),
    ),
  ),
);
```

- [ ] **Step 2: 运行类型检查**

```bash
npm run typecheck
```

Expected: 通过。

- [ ] **Step 3: 提交**

```bash
git add packages/core/src/commands/init.ts
git commit -m "feat: init 接收 --agent 参数，按选中的适配器安装集成并写入 config"
```

---

### Task 6: CLI 层 — --agent 参数解析 + 交互式选择

**Files:**

- Modify: `packages/cli/src/cli.ts`
- Modify: `packages/cli/src/commands/init.ts`

**Interfaces:**

- Consumes: `getAvailableAdapters()` from core
- Produces: CLI `--agent` 参数；交互式 agent 选择

- [ ] **Step 1: 修改 cli.ts — 新增 --agent 参数**

在 `packages/cli/src/cli.ts` 的 `parseArgs` options 中新增 `agent`：

```typescript
agent: { type: "string" },
```

在 `extraArgs` 组装区域（约第 126 行附近），新增对 `--agent` 的处理：

```typescript
if (values.agent) extraArgs.agent = values.agent;
```

- [ ] **Step 2: 重写 cli/src/commands/init.ts — 加入交互式选择和 agent 校验**

将 `packages/cli/src/commands/init.ts` 完整替换为：

```typescript
import { createInterface } from "node:readline/promises";
import type { SddCore, CommandResult } from "@sdd-harness/core";
import { getAvailableAdapters } from "@sdd-harness/core";

/**
 * CLI 层 init 命令：
 * 1. 如果指定了 --agent，校验是否都在可用列表中，不在则报错
 * 2. 如果未指定 --agent，进入交互式选择
 * 3. --non-interactive 且无 --agent 时报错
 * 4. 将选中的 agent 列表写入 args.agent 后调用 Core
 */
export async function runInit(
  core: SddCore,
  cwd: string,
  args: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const adapters = await getAvailableAdapters();

  if (args.agent !== undefined) {
    // --agent 指定模式：校验参数有效性
    const requestedAgents =
      typeof args.agent === "string"
        ? args.agent
            .split(",")
            .map((s) => s.trim())
            .filter((s) => s.length > 0)
        : Array.isArray(args.agent)
          ? args.agent.filter((a): a is string => typeof a === "string")
          : [];
    const availableNames = new Set(adapters.map((a) => a.agent));
    const invalid = requestedAgents.filter((a) => !availableNames.has(a));
    if (invalid.length > 0) {
      return {
        ok: false,
        state: "NOT_INITIALIZED",
        exitCode: 2,
        error: {
          code: "E_NOT_INITIALIZED",
          message: `未知适配器: ${invalid.join(", ")}。可用: ${[...availableNames].join(", ")}`,
        },
      };
    }
    args = { ...args, agent: requestedAgents };
  } else {
    // 交互模式
    const nonInteractive =
      args["non-interactive"] === true || args.nonInteractive === true;
    if (nonInteractive) {
      return {
        ok: false,
        state: "NOT_INITIALIZED",
        exitCode: 2,
        error: {
          code: "E_NOT_INITIALIZED",
          message: "非交互模式下必须通过 --agent 指定要安装的适配器",
        },
      };
    }

    if (adapters.length === 0) {
      return {
        ok: false,
        state: "NOT_INITIALIZED",
        exitCode: 6,
        error: {
          code: "E_COMPONENT_UNAVAILABLE",
          message: "未检测到任何可用的 AI Agent 适配器",
        },
      };
    }

    console.log(
      "检测到以下可用的 AI Agent 适配器，请选择要安装的（输入编号，多选用逗号分隔）：",
    );
    adapters.forEach((adapter, index) => {
      console.log(`  ${index + 1}. ${adapter.agent}`);
    });
    console.log("");

    const rl = createInterface({
      input: process.stdin,
      output: process.stdout,
    });
    let selected: string[] = [];
    while (selected.length === 0) {
      const answer = await rl.question("请输入编号（至少选择一个）：");
      selected = parseSelection(
        answer,
        adapters.map((a) => a.agent),
      );
      if (selected.length === 0) {
        console.log("输入无效或为空，请至少选择一个适配器。");
      }
    }
    rl.close();

    args = { ...args, agent: selected };
  }

  const request: Parameters<SddCore["execute"]>[0] = {
    command: "init",
    cwd,
    args,
  };
  if (signal) request.signal = signal;
  return core.execute(request);
}

/**
 * 解析用户输入的编号字符串为 agent 名称数组。
 * 支持 "1"、"1,3"、"1, 3" 等格式。
 * 无效编号被忽略，返回空数组表示需要重新输入。
 */
function parseSelection(input: string, available: string[]): string[] {
  const trimmed = input.trim();
  if (trimmed === "") return [];

  const indices = trimmed
    .split(/[,，]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map((s) => {
      const n = Number(s);
      return Number.isInteger(n) && n >= 1 && n <= available.length
        ? n - 1
        : -1;
    })
    .filter((i) => i >= 0);

  // 去重
  const unique = [...new Set(indices)];
  return unique.map((i) => available[i]!);
}
```

- [ ] **Step 3: 检查 CLI 包的类型**

```bash
npm run typecheck
```

Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add packages/cli/src/cli.ts packages/cli/src/commands/init.ts
git commit -m "feat: CLI 新增 --agent 参数和交互式适配器选择"
```

---

### Task 7: 编写测试

**Files:**

- Create: `packages/core/test/init-agent-selection.test.ts`
- Modify: `packages/core/test/init-status.test.ts`

**Interfaces:**

- Tests: 适配器选择、manifest 过滤、CLI 参数解析

- [ ] **Step 1: 创建 init-agent-selection.test.ts**

```typescript
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import type { AdapterManifest } from "../src/adapters/types.js";
import { installProjectIntegration } from "../src/install/project-installer.js";

const roots: string[] = [];

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-agent-sel-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Fixture\n", "utf8");
  await writeFile(join(root, "package.json"), '{"name":"fixture"}\n', "utf8");
  return root;
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

function makeClaudeManifest(): AdapterManifest {
  return {
    agent: "claude",
    instructionFile: "CLAUDE.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\nClaude 指令内容\n",
    commandsDir: ".claude/commands",
    commandTemplate:
      "---\ndescription: sdd {command}\n---\n\n执行 /sdd.{command}\n",
    skillsDir: ".claude/skills/sdd-harness",
    skillContent: "---\nname: sdd-harness\n---\n\nClaude skill\n",
  };
}

function makeCodexManifest(): AdapterManifest {
  return {
    agent: "codex",
    instructionFile: "AGENTS.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\nCodex 指令内容\n",
    commandsDir: ".codex/commands",
    commandTemplate:
      "---\ndescription: sdd {command}\n---\n\n执行 sdd {command}\n",
    skillsDir: ".codex/skills/sdd-harness",
    skillContent: "---\nname: sdd-harness\n---\n\nCodex skill\n",
  };
}

describe("installProjectIntegration agent selection", () => {
  it("仅安装选中的适配器 — claude", async () => {
    const root = await project();
    const result = await installProjectIntegration(root, [
      makeClaudeManifest(),
    ]);

    expect(result.candidateFiles).toEqual([]);
    await expect(access(join(root, "CLAUDE.md"))).resolves.toBeUndefined();
    await expect(access(join(root, "AGENTS.md"))).rejects.toThrow();
    await expect(
      access(join(root, ".claude/commands/sdd.init.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".codex/commands/sdd.init.md")),
    ).rejects.toThrow();
    await expect(
      access(join(root, ".claude/skills/sdd-harness/SKILL.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".codex/skills/sdd-harness/SKILL.md")),
    ).rejects.toThrow();
  });

  it("同时安装 claude 和 codex", async () => {
    const root = await project();
    const result = await installProjectIntegration(root, [
      makeClaudeManifest(),
      makeCodexManifest(),
    ]);

    expect(result.candidateFiles).toEqual([]);
    await expect(access(join(root, "CLAUDE.md"))).resolves.toBeUndefined();
    await expect(access(join(root, "AGENTS.md"))).resolves.toBeUndefined();
    await expect(
      access(join(root, ".claude/commands/sdd.init.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".codex/commands/sdd.init.md")),
    ).resolves.toBeUndefined();
  });

  it("空 manifests 数组不创建任何指令文件", async () => {
    const root = await project();
    const result = await installProjectIntegration(root, []);

    expect(result.candidateFiles).toEqual([]);
    await expect(access(join(root, "CLAUDE.md"))).rejects.toThrow();
    await expect(access(join(root, "AGENTS.md"))).rejects.toThrow();
    // schemas 始终安装
    await expect(
      access(join(root, ".sdd/schemas/config.schema.json")),
    ).resolves.toBeUndefined();
  });

  it("追加模式下已存在的行不会重复", async () => {
    const root = await project();
    // 先写入包含部分相同行的文件
    await writeFile(
      join(root, "CLAUDE.md"),
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\nClaude 指令内容\n",
      "utf8",
    );

    const result = await installProjectIntegration(root, [
      makeClaudeManifest(),
    ]);

    expect(result.candidateFiles).toEqual([]);
    const content = await readFile(join(root, "CLAUDE.md"), "utf8");
    // 不应有重复的 "Claude 指令内容"
    const lines = content.split("\n");
    const instructionLines = lines.filter((l) => l === "Claude 指令内容");
    expect(instructionLines.length).toBe(1);
  });

  it("追加模式：部分行已存在时仅追加新行", async () => {
    const root = await project();
    await writeFile(
      join(root, "CLAUDE.md"),
      "<!-- sdd-harness:managed -->\n## sdd-harness\n",
      "utf8",
    );

    await installProjectIntegration(root, [makeClaudeManifest()]);

    const content = await readFile(join(root, "CLAUDE.md"), "utf8");
    expect(content).toContain("## sdd-harness");
    expect(content).toContain("Claude 指令内容");
    // 不应有重复的 "## sdd-harness"
    const h2Count = content
      .split("\n")
      .filter((l) => l === "## sdd-harness").length;
    expect(h2Count).toBe(1);
  });

  it("有 rules 的 manifest 会安装 rule 文件", async () => {
    const root = await project();
    const manifestWithRules: AdapterManifest = {
      ...makeClaudeManifest(),
      rules: [{ file: ".claude/rules/sdd.md", content: "# SDD Rules\n" }],
    };

    await installProjectIntegration(root, [manifestWithRules]);

    const ruleContent = await readFile(
      join(root, ".claude/rules/sdd.md"),
      "utf8",
    );
    expect(ruleContent).toContain("# SDD Rules");
  });

  it("force 模式覆盖已有指令文件", async () => {
    const root = await project();
    await writeFile(join(root, "CLAUDE.md"), "旧内容\n", "utf8");

    await installProjectIntegration(root, [makeClaudeManifest()], {
      force: true,
    });

    const content = await readFile(join(root, "CLAUDE.md"), "utf8");
    expect(content).toContain("<!-- sdd-harness:managed -->");
    expect(content).toContain("Claude 指令内容");
    expect(content).not.toContain("旧内容");
  });
});

describe("parseSelection", () => {
  // 测试 CLI 层 parseSelection 函数
  function parseSelection(input: string, available: string[]): string[] {
    const trimmed = input.trim();
    if (trimmed === "") return [];

    const indices = trimmed
      .split(/[,，]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
      .map((s) => {
        const n = Number(s);
        return Number.isInteger(n) && n >= 1 && n <= available.length
          ? n - 1
          : -1;
      })
      .filter((i) => i >= 0);

    const unique = [...new Set(indices)];
    return unique.map((i) => available[i]!);
  }

  it('"1" 选中第一个', () => {
    expect(parseSelection("1", ["claude", "codex", "opencode"])).toEqual([
      "claude",
    ]);
  });

  it('"1,3" 选中第一个和第三个', () => {
    expect(parseSelection("1,3", ["claude", "codex", "opencode"])).toEqual([
      "claude",
      "opencode",
    ]);
  });

  it('"1, 3" 带空格正常解析', () => {
    expect(parseSelection("1, 3", ["claude", "codex", "opencode"])).toEqual([
      "claude",
      "opencode",
    ]);
  });

  it("空输入返回空数组", () => {
    expect(parseSelection("", ["claude", "codex"])).toEqual([]);
  });

  it("无效编号被忽略返回空数组", () => {
    expect(parseSelection("5", ["claude", "codex"])).toEqual([]);
  });

  it("去重：输入 1,1 只返回一个", () => {
    expect(parseSelection("1,1", ["claude", "codex"])).toEqual(["claude"]);
  });
});
```

- [ ] **Step 2: 修改 init-status.test.ts — 适配现有测试**

在 `packages/core/test/init-status.test.ts` 中，将所有检查 `CLAUDE.md`、`AGENTS.md`、`.claude/`、`.codex/` 文件和目录存在的断言更新。

**修改点 A** — `initializes every required directory` 测试（约第 179-346 行）：

在检查生成文件的断言中（第 198-228 行的 `for (const path of [...])` 循环），将路径列表更新为包含所有三个适配器的文件（因为测试不带 agent 参数时默认安装全部）：

在现有路径列表中追加 opencode 相关路径：

```
"CLAUDE.md",
"CLAUDE.md.meta.json",
"AGENTS.md",
"AGENTS.md.meta.json",
".claude/commands/sdd.init.md",
".claude/commands/sdd.init.md.meta.json",
".claude/commands/sdd.status.md",
".claude/skills/sdd-harness/SKILL.md",
".claude/skills/sdd-harness/SKILL.md.meta.json",
".codex/commands/sdd.init.md",
".codex/commands/sdd.init.md.meta.json",
".codex/commands/sdd.status.md",
".codex/skills/sdd-harness/SKILL.md",
".codex/skills/sdd-harness/SKILL.md.meta.json",
".opencode/commands/sdd.init.md",
".opencode/commands/sdd.init.md.meta.json",
".opencode/commands/sdd.status.md",
".opencode/skills/sdd-harness/SKILL.md",
".opencode/skills/sdd-harness/SKILL.md.meta.json",
```

**修改点 B** — Karpathy 规则检查（第 333-345 行）：

将检查的文件路径列表扩展，追加 opencode 相关路径：

```typescript
for (const path of [
  "CLAUDE.md",
  "AGENTS.md",
  ".claude/commands/sdd.init.md",
  ".claude/skills/sdd-harness/SKILL.md",
  ".codex/commands/sdd.init.md",
  ".codex/skills/sdd-harness/SKILL.md",
  ".opencode/commands/sdd.init.md",
  ".opencode/skills/sdd-harness/SKILL.md",
]) {
```

- [ ] **Step 3: 运行全部测试**

```bash
npm test
```

Expected: 全部通过（包括新增的 init-agent-selection 测试和修改后的 init-status 测试）。

- [ ] **Step 4: 提交**

```bash
git add packages/core/test/init-agent-selection.test.ts packages/core/test/init-status.test.ts
git commit -m "test: 适配器选择测试，更新 init 测试适配新安装逻辑"
```

---

### Task 8: 最终验证

- [ ] **Step 1: 运行完整检查**

```bash
npm run format:check && npm run lint && npm run typecheck && npm test
```

Expected: 全部通过。

- [ ] **Step 2: 提交最终调整（如有）**

```bash
git add -A
git commit -m "chore: 最终格式化和 lint 修复"
```
