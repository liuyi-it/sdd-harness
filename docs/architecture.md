# 架构说明

```text
Claude Code 命令 ─┐
                  ├─ HostAdapter ─ Core ─ State / Artifacts / Git / MCP
Codex Skill ──────┘                 ├─ SpecEngine
                                   ├─ TddEngine
                                   └─ Quality Gates
```

`@sdd-harness/core` 是唯一允许修改工作流状态的组件。平台适配器只负责解析宿主命令格式，并原样返回 Core 输出的 `CommandResult`。宿主环境通过依赖注入提供 `McpTransport` 和 `TaskExecutor`，这样外部工具调用边界清晰、便于测试。

二期 A 之后，Core 还维护两类额外事实源：

- `.sdd/project/conventions.json`：已有项目的目录结构规范画像，或空项目的初始化约定。
- `.sdd/loop/`：`auto` Loop 规格与运行历史，记录当前运行、恢复、重启和逐步审计信息。
- `.sdd/index/mcp-capabilities.json` / `codebase-diagnostics.json`：固定版 MCP 的能力发现和连接诊断。

`build next` 只为即将执行的任务把项目规则、目录规范、代码摘要和变更制品打包为 Context Pack；只要规则哈希、目录规范哈希或源码输入哈希变化，Context Pack 就会自动刷新。

Core 会把所有流程事实写入 `.sdd/`，并按命令实际需要惰性创建子目录。制品摘要集中记录在 `.sdd/artifacts.json`，不再为每个制品生成 sidecar 文件。状态文件更新采用临时文件写入、落盘、重命名加备份恢复策略；所有写命令都必须先获取 `.sdd/lock`。

## 上游能力内置边界

`vendor/openspec/upstream/` 和 `vendor/superpowers/upstream/` 保存固定 commit 的完整审计快照，但不会作为外部 CLI 执行。发布校验根据 `VERSION.json` 和 `MANIFEST.sha256` 检查文件类型、符号链接目标、内容摘要和许可证。

运行时通过两层内部适配实现上游语义：

```text
OpenSpec 快照 ──> openspec model/parser/validator/renderer ──> SpecEngine
Superpowers 快照 ──> protocol/planner/project-commands ──────> TddEngine
```

SpecEngine 生成 Requirement、Scenario 和 delta，统一写入 `spec.json`；TddEngine 根据真实代码路径生成 RED、GREEN、REFACTOR、VERIFY 原子任务，统一写入 `plan.json`。上游默认目录和宿主脚本不会成为事实源，最终制品仍只写入 `.sdd/`。

## 质量与归档数据流

```text
spec.json
  -> Requirement / Scenario
  -> 四阶段 Task 链
  -> Context Pack + 项目规则 / 目录规范
  -> TaskExecutor v2 / v1 normalize
  -> Git delta 裁决后的任务结果
  -> verifyGate / reviewGate / drift
  -> archive.json / archive.md / .archived
  -> ARCHIVED state
```

任务结果在进入质量闸门前进行深层结构校验。TaskExecutor 仍可返回 v1 结果，但 Core 会统一归一化为 1.2.0 运行级制品，并仅允许结构化 `{ command, args }` 命令证据进入归档链路。归档会在同一写锁内重新验证报告摘要、Git 快照、文件范围和追踪闭环，再将全部规格、设计、计划、证据与快照压缩到 `archive.json`，将归档报告与追踪矩阵合并到 `archive.md`，最终仅保留这两个文件和 `.archived`。如果 marker 已写而状态更新失败，下一次 `archive` 会根据有效 marker 收敛状态。

二期 B 在这条链路上增加了三层安全/质量边界：

- MCP 查询统一走 `McpTransportV2`，输出 `McpQueryResult`；缺工具或失败时降级为 `fallback-file-scan`，并保留 `reason` / `confidence`。
- `verify` / `review` 同时写 Markdown 和 `.v1.2.json`，后者作为机器可读质量门禁事实源。
- 不可信仓库内容、README 和 MCP 输出必须包裹到 `UNTRUSTED_REPOSITORY_CONTENT` / `UNTRUSTED_MCP_OUTPUT` 边界中；`review` 还会对 current-run diff 做 secrets 扫描，命中时生成 `SECRET_LEAK` 阻断归档。

二期 C 增加 Git 隔离工作区边界：

- `workflow.gitIsolation` 可声明 `createBranch`、`createWorktree`、`branchPattern` 和 `worktreeDir`。
- `GitIsolationManager` 负责创建或安全复用 `sdd/<change-id>` 分支与 `.sdd/worktrees/<change-id>`；遇到脏 worktree、基线漂移或注册不一致时直接阻断。
- `build` / `verify` / `review` 读取 `state.workspace`，把业务目录切到 worktree；`.sdd/` 制品和状态仍只写 controlRoot。
- `archive.md` 会额外记录 `branchName`、`worktreePath` 和业务目录最终 `HEAD`，但不会自动 merge、push 或删除 worktree。

# 第五期 Policy 层

Core/LoopEngine 仍是唯一流程编排者。每个现有命令通过 `PhasePolicyRegistry` 解析 Policy ID，由 `@sdd-harness/agent-policies` 编译为 `PolicyBundle`；Adapter 只声明宿主 capability 和安装位置。依赖方向固定为 `core → agent-policies → agent-protocol types`，Policy 包不得依赖 StateStore、Git writer 或 worktree manager。

Context Pack v2 只引用仓库内的 `spec.json`、`design.md`、`plan.json` 和 codebase 制品，并记录任务范围、verification、Policy refs 与 digest。它在 `build next` 时按任务生成，不在 `plan` 阶段批量落盘。`verify`/`review` 的可恢复失败追加 `REPAIR` 任务，仍由现有 Build 协议执行；失败签名预算耗尽或需要扩大范围时暂停等待用户决策。
