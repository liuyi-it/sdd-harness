# sdd-harness

> CLI-first、Agent-agnostic、codebase-memory-powered、verification-gated 的 **Spec-Driven Development Agent Harness**。

`sdd-harness` 是一个通用 CLI Harness，通过统一 CLI、`.sdd/` 事实源、内置托管的 codebase-memory-mcp、Generic Agent Protocol 和质量门禁，为多种 AI Coding Agent 提供统一的软件需求交付流程。

`sdd-harness` 不再是某个 Agent 的插件——CLI 是唯一确定性执行入口，不同 Agent 通过 Adapter 接入同一套 CLI 和协议。

**重要：`sdd-harness` 是 AI Agent 的工程支架，不是独立工具。** CLI 负责状态机、校验、安全边界和审计记录，但需求分析、方案设计、任务拆解和代码实现均需要 AI Coding Agent 参与。脱离 AI Agent，CLI 只能完成 `sdd init` 和 `sdd status`。

```
CLI（确定性）                    AI Agent（智力）
──────────────────────────────────────────────
状态机 / 阶段流转               需求澄清与回答
Schema 校验                     方案设计与选择
Git delta 事实源                 任务拆解与粒度判断
安全边界 / 范围约束              代码实现与测试编写
审计记录 / 归档追踪              规格 / 设计制品生成
```

---

## 支持的 Agent

| Agent              | 接入方式         | 能力等级   |
| ------------------ | ---------------- | ---------- |
| Claude Code        | Adapter (命令)   | Level 4/5  |
| Codex              | Adapter (Skill)  | Level 4/5  |
| OpenCode           | Adapter (规则)   | Level 4    |
| Kimi Code          | 文档级           | Level 3/4  |
| GitHub Copilot CLI | 文档级           | Level 3/4  |
| 自研 Coding Agent  | Generic Protocol | 由实现决定 |

---

## 快速开始

### 前置要求

- Node.js **22 及以上**版本
- Git
- 可选：`codebase-memory-mcp`（CLI 自动通过 npx 托管启动，无需手工安装）

### 安装

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness

# macOS / Linux / Windows（Git Bash）
bash scripts/install.sh
```

> 本项目不发布 npm。安装脚本会在本地构建并通过 `npm link` 全局注册 `sdd` 和 `sdd-harness` 两个命令。

### 使用

```bash
cd my-project
sdd init
sdd auto "实现订单取消功能"
```

---

## 工作流程

```text
初始化项目 (init) → 创建需求 (new) → 设计方案 (design) → 拆解任务 (plan)
                  → 实现代码 (build) → 验证 (verify) → 审查 (review) → 归档 (archive)
```

状态机主路径：

```text
NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
                → BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

---

## 命令说明

| 命令      | 作用                             |
| --------- | -------------------------------- |
| `init`    | 初始化项目，建立代码库上下文     |
| `auto`    | 自动执行完整 SDD 流程            |
| `new`     | 创建需求变更，需求分析与规格生成 |
| `design`  | 基于规格生成设计方案             |
| `plan`    | 基于设计拆解开发任务             |
| `build`   | 根据任务计划实现代码             |
| `verify`  | 验证任务完成度与功能边界         |
| `review`  | 审查代码质量与实现合理性         |
| `archive` | 归档当前需求变更                 |
| `status`  | 查看当前 SDD 状态与下一步建议    |

**通用参数**：`--json`、`--cwd <path>`、`--change <id>`、`--timeout <s>`、`--non-interactive`、`--force`、`--verbose`、`--help`、`--version`

### codebase 命令

| 命令                     | 作用                          |
| ------------------------ | ----------------------------- |
| `sdd codebase status`    | 显示 codebase 提供者与状态    |
| `sdd codebase doctor`    | 诊断 codebase-memory-mcp 健康 |
| `sdd codebase index`     | 手动触发代码库索引            |
| `sdd codebase query <q>` | 结构化代码库查询              |
| `sdd codebase rebuild`   | 重建代码库索引                |

---

## 内置 codebase-memory-mcp

默认情况下，用户无需手工安装 MCP。`sdd init` 会自动通过 `npx` 启动托管的 `codebase-memory-mcp`。

MCP 不可用时会自动降级为 fallback-file-scan，但系统会明确提示用户执行 `sdd codebase doctor` 诊断。**绝不静默降级。**

---

## 卸载

```bash
# macOS / Linux / Windows（Git Bash）
bash scripts/uninstall.sh
```

---

## 生成的制品

所有 SDD 制品统一存放在项目的 `.sdd/` 目录：

- 需求说明与澄清问题
- OpenSpec delta、需求规格
- 设计方案与任务拆解
- 每个任务的 Context Pack
- 运行级任务结果与 TDD 证据
- verify / review / archive 报告
- Loop 自动编排审计记录

---

## License

MIT
