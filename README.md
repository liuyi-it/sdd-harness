# sdd-harness

面向 AI Coding Agent 的规格驱动开发（SDD）工程支架。它用统一 CLI 管理需求澄清、设计、任务拆解、构建、验证、审查和归档，并通过状态机、Git 变更事实、安全边界与质量门禁约束 Agent。

## 核心能力

- CLI-first：`sdd` 是唯一确定性入口，Core 是唯一状态与门禁执行层。
- Agent-agnostic：内置 Claude Code、Codex、OpenCode Adapter，并提供通用 Agent 协议。
- 规格与 TDD：Requirement/Scenario 规格模型驱动 RED、GREEN、REFACTOR、VERIFY 任务链。
- 代码库理解：自动托管 `codebase-memory-mcp`，不可用时显式降级到文件扫描。
- 安全可追溯：校验路径、命令、文件范围、Git delta、TDD 证据和敏感信息。
- 最小正确实现：按复用、标准库、平台能力和既有依赖的顺序决策；未计划新增依赖会阻断审查。
- 精简制品：目录按需创建，Context Pack 按任务生成，归档最终收敛为三个文件。

## 环境要求

- Node.js 22 或更高版本
- Git
- macOS 或 Windows（Git Bash）

## 安装

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

安装脚本会先移除当前 npm 前缀及 `PATH` 中属于本项目的旧版 CLI、依赖和构建产物，再通过 lockfile 全新安装、构建并注册全局命令 `sdd` 和 `sdd-harness`。安装完成时会显示实际命令位置并验证它们指向当前仓库；若同名命令被其他目录遮蔽，安装会明确失败，不再出现“安装成功但运行旧版”的情况。安装失败时会自动回滚未完成的安装产物。项目不发布到 npm。

Windows 下启动 codebase-memory-mcp 时，会依次检查项目本地 npm 包、npm 全局包中的真实 `.exe`、`CODEBASE_MEMORY_MCP_PATH`、`%LOCALAPPDATA%\Programs\codebase-memory-mcp\codebase-memory-mcp.exe` 和 `PATH`，最后才使用 `npx`。npm wrapper 的真实二进制缺失时不会再误判为可用安装。

卸载：

```bash
bash scripts/uninstall.sh
```

卸载脚本会移除全局 CLI、本仓库的 `node_modules`、各 workspace 的 `node_modules` / `dist` 以及 TypeScript 构建缓存。业务项目中的 `.sdd/` 保存用户规格、任务和归档，不属于安装残留，不会自动删除。

## 快速开始

在需要管理的项目根目录执行：

```bash
sdd init --agent codex
sdd auto "实现订单取消功能"
```

也可以逐阶段推进：

```bash
sdd new "实现订单取消功能"
sdd design
sdd plan
sdd build next --json
sdd verify
sdd review
sdd archive
```

`sdd auto` 在需要 Agent 编码或用户澄清时暂停，不会绕过交互边界自动修改代码。

首次调用 `sdd new` 或 `sdd auto` 必须携带非空需求；没有需求时，Agent 应先询问用户。不要默认添加 `--non-interactive`，它仅适用于允许需求不完整时直接失败的无人值守流程。若命令进入澄清状态，收集用户回答后使用 `sdd new --answers '<JSON answers>' --json` 继续。

## Agent 构建协议

```bash
# 获取下一个任务及其按需生成的 Context Pack
sdd build next --json

# Agent 完成编码并写出 TaskExecutionResult 后提交
sdd build complete \
  --task TASK-001-RED \
  --result .sdd/runs/<run-id>/tasks/TASK-001-RED.result.json \
  --json
```

Core 会验证任务状态、允许/禁止文件、实际 Git delta、TDD evidence 和 verification。Agent 不应直接修改 `.sdd/state.json`。

## 工作流程

```text
init → new → design → plan → build → verify → review → archive

NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
                → BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

主要命令：

| 命令                      | 作用                                        |
| ------------------------- | ------------------------------------------- |
| `sdd init`                | 初始化 `.sdd/`、代码库索引和 Agent 接入文件 |
| `sdd status`              | 查看当前阶段、错误和下一步建议              |
| `sdd new <需求>`          | 澄清需求并生成规格                          |
| `sdd design`              | 生成技术设计                                |
| `sdd plan`                | 生成任务、测试计划和上下文摘要              |
| `sdd build next/complete` | 获取任务或提交 Agent 结果                   |
| `sdd verify`              | 验证规格、任务和证据覆盖                    |
| `sdd review`              | 审查范围、安全、依赖计划、改动规模与债务    |
| `sdd archive`             | 校验并压缩归档，保留简洁性与来源追踪        |
| `sdd auto <需求>`         | 按状态机自动推进流程                        |
| `sdd codebase ...`        | 查看、诊断、查询或重建代码库索引            |

完整参数见 [CLI 命令参考](docs/CLI.md)。

## 制品结构

`.sdd/` 子目录按实际命令惰性创建。一个变更的主要制品为：

```text
.sdd/
├── state.json
├── artifacts.json
├── changes/<change-id>/
│   ├── spec.md
│   ├── spec.json
│   ├── design.md
│   └── plan.json
├── context-packs/<change-id>/<task-id>.md
└── runs/<run-id>/tasks/<task-id>.result.json
```

执行 `sdd archive` 后，变更目录只保留：

```text
.sdd/changes/<change-id>/
├── archive.json   # 规格、计划、质量、简洁性指标、债务与 Git 快照
├── archive.md     # 人工可读归档报告、简洁性摘要与追踪矩阵
└── .archived      # 完整性标记
```

## 项目结构

| 包                             | 职责                              |
| ------------------------------ | --------------------------------- |
| `@sdd-harness/cli`             | 参数解析和命令路由                |
| `@sdd-harness/core`            | 状态机、制品、Git、安全与质量门禁 |
| `@sdd-harness/agent-protocol`  | Agent Action/Result 类型与校验    |
| `@sdd-harness/agent-policies`  | 分阶段工程策略及摘要              |
| `@sdd-harness/codebase-memory` | MCP 托管、诊断与降级              |
| `packages/*-adapter`           | 各 Agent 的命令、Skill 或规则模板 |

## 文档

- [CLI 命令参考](docs/CLI.md)
- [架构说明](docs/architecture.md)
- [命令与制品契约](docs/command-contract.md)
- [状态机](docs/state-machine.md)
- [安全策略](docs/security.md)
- [Schema](docs/schemas.md)
- [Agent 接入](docs/adapters.md)

## 开发与验证

```bash
npm install
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build
npm run validate:schemas
npm run validate:release
```

## License

MIT
