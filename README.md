# sdd-harness

> CLI-first、Agent-agnostic、codebase-memory-powered、verification-gated 的 **Spec-Driven Development Agent Harness**。

---

## 一、目标

`sdd-harness` 解决的核心问题：AI Coding Agent 拿到需求后直接写代码，缺少需求澄清、方案设计、任务拆解、验证和审计——导致修改范围不可控、质量不可追溯。

`sdd-harness` 通过**强制阶段化工作流 + 确定性状态机 + 质量门禁**约束 Agent 的行为，让每一次需求变更都经过完整工程流程，全过程留痕。

`sdd-harness` 是 AI Agent 的工程支架，不是独立工具。CLI 负责状态机、校验、安全边界和审计记录；需求分析、方案设计、任务拆解和代码实现均需要 AI Coding Agent 参与。

```
CLI（确定性）                    AI Agent（智力）
──────────────────────────────────────────────
状态机 / 阶段流转               需求澄清与回答
Schema 校验                     方案设计与选择
Git delta 事实源                 任务拆解与粒度判断
安全边界 / 范围约束              代码实现与测试编写
审计记录 / 归档追踪              规格 / 设计制品生成
```

### 核心原则

1. **CLI 是唯一确定性入口** —— Agent 不直接执行 TypeScript 源码，不绕过 CLI 修改 `.sdd/state.json`
2. **Core 是唯一状态机和门禁执行层** —— 状态流转、Schema 校验、Git delta 验证全部由 Core 统一调度
3. **Agent 只通过 CLI / ActionRequired / TaskResult 与系统交互** —— build 阶段由 Agent 执行，但结果必须通过 CLI 提交验收
4. **内置 codebase-memory-mcp** —— 用户无需手工安装 MCP，CLI 自动托管启动，不可用时降级 fallback-file-scan 并明确提示
5. **不静默降级** —— 降级时必须包含 warning，指向 `sdd codebase doctor`

### 支持的 Agent

| Agent              | 接入方式         | 能力等级   |
| ------------------ | ---------------- | ---------- |
| Claude Code        | Adapter (命令)   | Level 4/5  |
| Codex              | Adapter (Skill)  | Level 4/5  |
| OpenCode           | Adapter (规则)   | Level 4    |
| Kimi Code          | 文档级           | Level 3/4  |
| GitHub Copilot CLI | 文档级           | Level 3/4  |
| 自研 Coding Agent  | Generic Protocol | 由实现决定 |

---

## 二、使用方式

### 前置要求

- Node.js **22 及以上**版本
- Git

### 安装

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness

# macOS / Linux / Windows（Git Bash）
bash scripts/install.sh
```

> 本项目不发布 npm。安装脚本构建并通过 `npm link` 全局注册 `sdd` 和 `sdd-harness`。

### 快速开始

```bash
cd my-project
sdd init
sdd auto "实现订单取消功能"   # 在 AI Agent 上下文中执行
```

Agent 内触发方式：

```text
Claude Code:  /sdd.auto "实现订单取消功能"
Codex:        sdd auto "实现订单取消功能"
OpenCode:     sdd auto "实现订单取消功能"
```

### Agent Build 协议

```bash
# Agent 获取下一个构建任务
sdd build next --json
# → 返回 AGENT_TASK_EXECUTION（含 allowedFiles、contextPack、verification）

# Agent 完成编码后提交结果
sdd build complete --task T001 --result result.json --json
# → Core 校验 Git delta、TDD evidence、文件范围，更新任务状态
```

### 命令参考

| 命令                       | 作用                             |
| -------------------------- | -------------------------------- |
| `sdd init`                 | 初始化项目，建立代码库上下文     |
| `sdd status`               | 查看当前 SDD 状态与下一步建议    |
| `sdd new <需求>`            | 创建需求变更，需求分析与规格生成 |
| `sdd design`               | 基于规格生成设计方案             |
| `sdd plan`                 | 基于设计拆解开发任务             |
| `sdd build next`           | 获取下一个构建任务（Agent 使用）  |
| `sdd build complete`       | 提交构建结果（Agent 使用）        |
| `sdd verify`               | 验证任务完成度与功能边界         |
| `sdd review`               | 审查代码质量与实现合理性         |
| `sdd archive`              | 归档当前需求变更                 |
| `sdd auto <需求>`           | 自动推进完整 SDD 流程            |
| `sdd codebase status`      | 显示 codebase 提供者与状态       |
| `sdd codebase doctor`      | 诊断 codebase-memory-mcp 健康    |
| `sdd codebase query <q>`   | 结构化代码库查询                 |

通用参数：`--json` `--cwd <path>` `--change <id>` `--timeout <s>` `--non-interactive` `--force` `--verbose` `--help` `--version`

### 卸载

```bash
bash scripts/uninstall.sh
```

---

## 三、技术方案

### 架构总览

```text
@sdd-harness/cli                       CLI 唯一入口，参数解析，命令路由
  ├── @sdd-harness/core                状态机、Schema 校验、Git delta 事实源、质量门禁
  ├── @sdd-harness/agent-protocol      AgentActionRequired / AgentTaskResult 类型与校验
  ├── @sdd-harness/codebase-memory     内置托管 codebase-memory-mcp，降级 fallback-file-scan
  └── Agent Adapters                   命令模板，翻译宿主指令为 CLI 调用
        ├── claude-code-adapter         /sdd.* slash commands
        ├── codex-adapter               Skill 规则
        ├── opencode-adapter            规则文件
        └── generic-agent-adapter       通用 Agent Prompt + Loop
```

### 包结构

```
packages/
├── core/                   @sdd-harness/core          状态机 + 命令实现 + 质量门禁
├── cli/                    @sdd-harness/cli            sdd / sdd-harness bin
├── agent-protocol/         @sdd-harness/agent-protocol Agent 交互协议
├── codebase-memory/        @sdd-harness/codebase-memory MCP 托管 + 降级
├── claude-code-adapter/    @sdd-harness/claude-code-adapter
├── codex-adapter/          @sdd-harness/codex-adapter
├── opencode-adapter/       @sdd-harness/opencode-adapter
└── generic-agent-adapter/  @sdd-harness/generic-agent-adapter
```

### 工作流程与状态机

```
init → new → design → plan → build → verify → review → archive

NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
                → BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

允许的运行中状态：`INITIALIZING` `INDEXING` `NEW_STARTED` `DESIGNING` `PLANNING` `BUILDING` `VERIFYING` `REVIEWING` `ARCHIVING` `CLARIFYING` `FAILED` `PAUSED`

### Agent 交互模型

```
sdd build next --json
  → Core 读取 tasks.json → 找到下一个待执行任务
  → 返回 AgentActionRequired {
      type: "AGENT_TASK_EXECUTION",
      taskId, changeId, contextPack,
      allowedFiles, forbiddenFiles,
      verification, resultFile
    }

Agent 执行：
  1. 读取 contextPack
  2. 修改 allowedFiles 内的文件
  3. 运行 verification 命令
  4. 写入 TaskExecutionResult 到 resultFile
  5. 调用 sdd build complete --task <id> --result <path>

sdd build complete
  → Core 校验 result schema (1.2.0)
  → Git delta 作为文件变更事实源
  → 校验文件范围、TDD evidence、verification
  → 更新 task 状态，全部完成进入 BUILD_READY
```

### codebase-memory-mcp 降级策略

```
sdd init
  → CodebaseMemoryManager.initialize()
    → npx codebase-memory-mcp@0.8.1 启动
      → 成功 → provider=codebase-memory-mcp, degraded=false
      → 失败 → fallback-file-scan, degraded=true
              → W_CODEBASE_MEMORY_UNAVAILABLE warning
              → next: sdd codebase doctor
              → 写入 diagnostics.json
```

### 生成的制品

所有 SDD 制品统一存放在 `.sdd/` 目录：需求说明与澄清问题、OpenSpec delta、设计方案、任务拆解与 Context Pack、运行级任务结果与 TDD 证据、verify/review/archive 报告、Loop 编排审计记录。

### 开发

```bash
npm install
npm run build
npm run format:check
npm run lint
npm run typecheck
npm test
```

---

## License

MIT
