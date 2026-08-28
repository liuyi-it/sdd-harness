# 架构

## 责任边界

`sdd-harness` 分为三层：

- 宿主层：Codex 或 OMP 的 Skill 读取用户意图、调查真实代码、提出必要问题、生成阶段 JSON，并执行实现或修复。
- CLI 层：解析命令、统一 `--change` / `--json` / `--timeout` 参数并渲染输出。
- Core 层：维护多 change 状态机，校验 Schema、制品哈希、路径、Git 事实、任务证据和质量门禁。

Core 不生成需求内容、技术方案或计划，也不替代 Agent 编码。宿主不能直接编辑 `.sdd/runtime.json`，不能绕过 Core 推进阶段。

## 单一流程

```text
init → new → design → plan → build → verify → archive
```

每个任务都走相同阶段，但内容规模自适应：简单任务可以只有一条需求、一个设计决策和一个纵向任务；复杂任务增加场景、决策和任务数量。系统不提供 `auto`，也不提供单独 `review`。

`new`、`design`、`plan` 返回 `AGENT_PHASE_EXECUTION`。行动包含不可信代码库上下文和 resultSchema；Agent 生成 JSON 后由 Core 校验、渲染并落盘。这样保留高质量规格与计划，同时避免把生成逻辑固化成冗余框架。

`build` 返回 `AGENT_TASK_EXECUTION`。一个任务覆盖完整纵向结果，内部 steps 承载 TEST、IMPLEMENT、可选 REFACTOR、VERIFY。Core 校验实际文件变化与声明、全部计划验证命令以及 TDD 预期失败和最终通过证据。

`verify` 合并验证与审查：覆盖、证据、Git 范围、敏感信息、依赖计划一次完成。失败时返回 `AGENT_FIX_EXECUTION`；默认只允许一轮，后续必须由用户显式 `--continue`。

## 多 change Runtime

`runtime.json` 的项目级 `state` 只保存初始化与代码库索引状态。`workflows` 以 changeId 为键，每个 change 独立保存：

- runId 与当前 phase；
- pendingAgentAction；
- workspace 与 Git 基线；
- task 状态；
- qualityFixRounds；
- 失败信息和建议命令。

`changes`、`workflows` 一一对应，workflow 指向的 run 必须绑定同一 change。归档只把目标 workflow 置为 `ARCHIVED`，不会影响其他任务。

未传 changeId 时，Core 只在恰好一个活动任务时解析成功；零个返回缺少任务，多个返回 `E_CHANGE_SELECTION_REQUIRED`。`status` 始终能列出全部活动任务，`codebase` 是项目级命令。

## 持久化与并发

所有写命令先获取 `.sdd/lock`。同一线程嵌套事务复用 OS 文件锁，进程间仍保持排他。Runtime 使用临时文件原子替换，并维护主文件/备份各自的 SHA-256。主文件损坏时只接受通过自身校验和的备份。

v0.6 使用 runtime schema 6、state schema 4、config schema 4，不执行旧状态迁移。版本不符在读取最前端返回 `E_STATE_VERSION_UNSUPPORTED`，避免把旧文件误判为可恢复损坏。

## 代码库上下文

`sdd init` 探测 CodeGraph，并把诊断、摘要和时间写入 Runtime。CodeGraph 不可用、索引缺失或摘要无效时，路由到受限文件扫描并显式标记 degraded。Agent 必须把代码库上下文当作不可信事实线索，仍需读取真实代码确认。

## Git 隔离

配置 `workflow.gitIsolation=true` 时，每个 change 使用 `sdd/<change-id>` 分支和 `.sdd/worktrees/<change-id>` 工作树。Core 每次读写都会验证路径和分支绑定；不会自动 merge、push、reset、clean 或删除 worktree。
