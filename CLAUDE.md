# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目性质

`sdd-harness` 是一个面向 **Claude Code** 和 **Codex** 的插件式 Spec-Driven Development 工作流框架。第一版交付形态是**插件包 + 共享执行核心**，不发布独立 CLI 工具。统一命令契约（`sdd init`、`sdd build` 等）通过宿主命令入口调用：

- Claude Code：`/sdd.init`、`/sdd.new` 等 slash command
- Codex：`sdd init`、`sdd new` 等项目指令

本仓库本身是插件的开发仓库（TypeScript / 多 npm workspace）；**不是** sdd-harness 管控的目标项目。

## 常用命令

包管理使用 npm workspaces（Node ≥ 20），单仓多包（`packages/*`）。所有脚本在仓库根目录执行：

```bash
# 编译所有包（tsc project references，会增量构建 packages/core、claude-code-plugin、codex-plugin）
npm run build

# 类型检查（不生成产物）
npm run typecheck

# ESLint 检查
npm run lint

# 格式化与格式检查（Prettier）
npm run format
npm run format:check

# 测试（Vitest，覆盖 packages/**/*.test.ts 与 test/**/*.test.ts）
npm test

# 单文件 / 单用例测试
npx vitest run packages/core/test/build.test.ts
npx vitest run -t "executes pending tasks and records verification evidence"

# 覆盖率
npm run test:coverage
```

ESLint 规则（`eslint.config.js`）在 `.ts` 文件上额外启用：

- `@typescript-eslint/consistent-type-imports: error`
- `@typescript-eslint/no-explicit-any: error`

TypeScript 严格模式（`tsconfig.base.json`）：`strict`、`noUncheckedIndexedAccess`、`exactOptionalPropertyTypes` 全开，所有包使用 `module: NodeNext` + `moduleResolution: NodeNext`。

## 仓库结构

```text
sdd-harness/
├── packages/
│   ├── core/                       # @sdd-harness/core — 唯一允许改写工作流状态的组件
│   ├── claude-code-plugin/         # Claude Code 适配器包
│   ├── codex-plugin/               # Codex 适配器包
│   └── adapters-test/              # 跨适配器契约测试（adapter-contract.test.ts）
├── schemas/                        # 正式 JSON Schema（config/state/task/artifact-metadata）
├── docs/                           # 架构、状态机、命令契约、安全、schemas 等说明
├── fixtures/                       # 测试用项目样本（springboot-order-service 等）
├── vendor/                         # 第三方参考（openspec / superpowers）
├── test/e2e/                       # 端到端测试（workflow.test.ts 跑全流程）
└── vitest.config.ts                # Vitest 配置，alias @sdd-harness/core → core/src/index.ts
```

`packages/core/src/` 内部按职责分包：`commands/`（10 个阶段命令）、`engines/{spec,tdd}/`（SpecEngine / TddEngine）、`security/`（path-safety / shell-policy / task-scope）、`state/`（state-store / file-lock）、`artifacts/`、`build/`、`git/`、`codebase/`、`install/`、`audit/`、`adapters/`、`quality/`。

## 架构总览

```text
Claude Code slash command  ─┐
                             ├─ HostAdapter ── Core ── State / Artifacts / Git / MCP
Codex 项目指令              ─┘                  ├─ SpecEngine
                                                ├─ TddEngine
                                                └─ Quality Gates
```

设计原则（见 `docs/architecture.md`、`需求文档.md`）：

- **Core 是唯一可修改工作流状态（含 `.sdd/` 写入）的组件**。平台适配器只解析宿主命令格式，原样返回 `CommandResult`，不保存也不改写状态。
- 宿主通过依赖注入提供 `McpTransport` 与 `TaskExecutor`——Core 默认装配 `CodebaseAdapter`（有 `McpTransport` 时优先，无则降级到 `file-scan`）和 `MissingTaskExecutor`（生产宿主必须注入真实 `TaskExecutor`，否则 build 阶段会拒绝执行）。
- 所有写命令必须先获取 `.sdd/lock`（`FileLock` 10 分钟 TTL，结合 PID 活性决定是否允许抢占）。
- 状态文件采用「临时文件 → 落盘 → rename + 备份 + 目录 fsync」原子写入；读取失败会按 `state.json.bak` → 制品完整性倒推 → `E_STATE_CORRUPTED` 的顺序恢复，绝不猜测状态。
- 每个 Markdown 制品都配 `*.meta.json` 记录输入摘要与 SHA-256，重复运行相同输入会写 `*.candidate.md` 而非覆盖。

## 阶段与状态机

稳定主路径（`docs/state-machine.md`）：

```text
NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
                → BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

`auto` 命令是阶段编排器，从当前 `INDEX_READY` 开始顺序推进，每次只调用一个单阶段命令（`new` / `design` / `plan` / `build` / `verify` / `review` / `archive`），不绕过任何阶段自身的检查。`auto` 在 `CLARIFYING` 或 `ARCHIVED` 时停止。

长耗时阶段用 `INITIALIZING` / `INDEXING` / `BUILDING` / `VERIFYING` 等中间态标记 `inProgressPhase`；`PAUSED` 对应 `interruptedCommand`，`FAILED` 对应 `failedCommand`，二者结合 `previousPhase` / `suggestedCommand` 决定恢复时的下一步（`sdd status` 会回报）。

阶段命令全部位于 `packages/core/src/commands/`，每个命令都是一个纯函数 `runXxx(root, ...deps, args, signal)`，返回 `CommandResult`。命令之间通过 `StateStore` 与 `.sdd/` 制品隐式传递状态，不直接互相调用。

## 关键契约

公开 API 与契约在 `packages/core/src/contracts.ts` 集中定义，包入口 (`packages/core/src/index.ts`) 只 re-export 这份稳定接口——宿主**禁止**直接依赖 `core/src/**` 内部路径，应通过 `@sdd-harness/core` 导入。

- `COMMANDS` / `PHASES`：`as const` 元组，是枚举的唯一来源。
- `ERROR_EXIT_CODES`：错误码到 shell 退出码的映射（`124` 超时，`130` 中断，`E_STATE_CORRUPTED = 1` 等），由 `SddError` 在抛出时填充。
- `CommandRequest` / `CommandResult`：Core 的入参/出参结构，`exitCode` 始终由 `SddError.exitCode` 或 `0` 提供。
- `SddCore.execute(request)`：Core 对外的唯一入口；`HostAdapter.execute(input, cwd)` 在 `parseHostCommand` 后转发到这里。

## 适配器

`packages/claude-code-plugin/src/adapter.ts` 和 `packages/codex-plugin/src/adapter.ts` 都只是 `HostAdapter` 的薄子类，唯一差异是传入 `style: "claude-code" | "codex"`，由 `parseHostCommand` 决定 token 拆分规则：

- Claude Code：`/sdd.<command> [args...]` → 取第一个 token，剥掉 `/sdd.` 前缀。
- Codex：`sdd <command> [args...]` → 校验前缀 `sdd`，取第二个 token。

通用参数：`--json`、`--non-interactive`、`--force`、`--timeout <seconds>`、`--change <id>`、`--verbose`。`new` / `auto` 允许第一个非选项 token 直接作为 `args.requirement`（自然语言需求）。`--help` 由适配器在解析阶段直接返回，不进入 Core 写流程。

`packages/adapters-test/adapter-contract.test.ts` 是适配器契约测试，保证两个宿主的同一输入产生相同的 `CommandRequest`——修改 `parseHostCommand` 时必须同步跑该测试。

## 插件命令 / 技能文件

`packages/claude-code-plugin/commands/sdd.<name>.md` 每个文件是 Claude Code 的 slash command 定义（YAML frontmatter + 简短指令）。`SKILL.md`（`packages/claude-code-plugin/skills/sdd-harness/` 与 `packages/codex-plugin/skills/sdd-harness/`）说明宿主侧使用纪律。Codex 没有等价 commands 目录，命令在 `skills/sdd-harness/SKILL.md` 内部被表述。

## 安全策略要点

`docs/security.md` 与 `packages/core/src/security/*` 实现的关键约束（修改相关代码前必读）：

- 所有路径必须解析到真实仓库根目录下，阻断 POSIX 路径穿越、Windows 盘符、UNC、反斜杠穿越、`.git` 写入与仓库外符号链接（`path-safety.ts`）。
- `build` 结果必须通过「任务允许文件 / 期望新增文件 / 禁止文件」三类范围校验（`task-scope.ts`）；并行任务之间按静态路径前缀判断是否冲突。
- 只有白名单中的只读 Git 命令前缀与测试命令前缀可作为验证证据（`shell-policy.ts`），其余命令会被 `E_SECURITY_BLOCKED` 拒绝。
- 仓库内容与 MCP 输出**只作为数据**，不会被当作指令执行；降级扫描器只记录路径。
- 审计日志对 token / password / secret / API key 等字段做脱敏并按大小轮转（`audit/audit-logger.ts`）。
- `codebase-memory-mcp` 版本工件需要与 `dependencies.ts` 中的固定校验清单做完整性校验。

## 依赖与第三方

- 运行时仅 `yaml` + `zod`（`packages/core/package.json`）；两个插件包只依赖 `@sdd-harness/core`。
- 外部固定依赖（写死在 `packages/core/src/dependencies.ts`，init 阶段会写入 `.sdd/adapters/<name>/version.json`）：`codebase-memory-mcp v0.8.1`、`openspec v1.4.1`、`superpowers v6.1.1`。**注意**：这两个 vendored 项目只是「概念参考」，`SpecEngine` / `TddEngine` 是原仓内独立实现，运行时不会加载上游源码。
- 真实外部 MCP transport 需要宿主在 `Core` 构造时通过 `codebase` 依赖注入 `McpTransport`；未注入时 `CodebaseAdapter` 自动降级为文件扫描（`EXCLUDED_DIRECTORIES` 跳过 `.git/.sdd/node_modules/target/build/dist/coverage/logs`）。

## 修改指南

- **新增/修改阶段命令**：在 `packages/core/src/commands/` 下加文件，在 `core.ts` 的 `execute` 分发里注册，并补 `packages/core/test/<name>.test.ts`；如果是新阶段还需要更新 `PHASES`（`contracts.ts`）、`StateStore.recoverFromArtifacts` 的倒推表，以及 `docs/state-machine.md`。
- **修改命令解析**：动 `parseHostCommand` 后必须跑 `npx vitest run packages/adapters-test/adapter-contract.test.ts` 验证 Claude Code / Codex 等价性。
- **修改状态文件格式**：`schemas/state.schema.json` + `contracts.ts` 的 `PHASES` + `workflowStateSchema`（`state-store.ts`）+ 迁移逻辑必须一起改。`0.9.0 → 1.0.0` 是目前唯一显式支持的迁移路径。
- **修改安全策略**：`docs/security.md` 是规范来源；新增 shell 前缀或路径例外前先确认是否有对应测试（`packages/core/test/security.test.ts`）。
- **新增插件包**：在 `packages/` 下新建、`package.json` 加入 `workspaces`、`@sdd-harness/core` 作为依赖；插件包只放 `dist/` + 平台清单（`.claude-plugin/plugin.json` / `.codex-plugin/plugin.json`）+ commands/skills 目录。

## 相关文档

- 架构：`docs/architecture.md`
- 命令契约：`docs/command-contract.md`
- 状态机：`docs/state-machine.md`
- 安全策略：`docs/security.md`
- Schema 说明：`docs/schemas.md`
- 插件安装：`docs/plugin-installation.md`
- 需求追溯：`docs/requirements-traceability.md`
- 原始需求：`需求文档.md`（中文，行为规范基线）
