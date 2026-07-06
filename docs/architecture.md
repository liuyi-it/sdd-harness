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

`build` 前会把项目规则、目录规范、代码摘要和变更制品重新打包为 Context Pack；只要规则哈希、目录规范哈希或源码输入哈希变化，Context Pack 就会自动刷新。

Core 会把所有流程事实写入 `.sdd/`。每个 Markdown 制品都配套元数据文件，记录输入摘要与制品 SHA-256。状态文件更新采用临时文件写入、落盘、重命名加备份恢复策略；所有写命令都必须先获取 `.sdd/lock`。

## 上游能力内置边界

`vendor/openspec/upstream/` 和 `vendor/superpowers/upstream/` 保存固定 commit 的完整审计快照，但不会作为外部 CLI 执行。发布校验根据 `VERSION.json` 和 `MANIFEST.sha256` 检查文件类型、符号链接目标、内容摘要和许可证。

运行时通过两层内部适配实现上游语义：

```text
OpenSpec 快照 ──> openspec model/parser/validator/renderer ──> SpecEngine
Superpowers 快照 ──> protocol/planner/project-commands ──────> TddEngine
```

SpecEngine 生成 Requirement、Scenario、delta 和 `spec.model.json`；TddEngine 根据真实代码路径生成 RED、GREEN、REFACTOR、VERIFY 原子任务。上游默认目录和宿主脚本不会成为事实源，最终制品仍只写入 `.sdd/`。

## 质量与归档数据流

```text
Spec model
  -> Requirement / Scenario
  -> 四阶段 Task 链
  -> Context Pack + 项目规则 / 目录规范
  -> TaskExecutor v2 / v1 normalize
  -> Git delta 裁决后的任务结果
  -> verifyGate / reviewGate / drift
  -> traceability.md / archive-report.md
  -> .archived + ARCHIVED state
```

任务结果在进入质量闸门前进行深层结构校验。TaskExecutor 仍可返回 v1 结果，但 Core 会统一归一化为 1.2.0 运行级制品，并仅允许结构化 `{ command, args }` 命令证据进入归档链路。归档会在同一写锁内重新验证报告 metadata、Git 快照、文件范围和追踪闭环。追踪与归档报告使用临时文件、fsync、备份和 rename 组提交；如果 marker 已写而状态更新失败，下一次 `archive` 会根据有效 marker 收敛状态，避免 `.archived` 与 `state.json` 分裂。

二期 B 在这条链路上增加了三层安全/质量边界：

- MCP 查询统一走 `McpTransportV2`，输出 `McpQueryResult`；缺工具或失败时降级为 `fallback-file-scan`，并保留 `reason` / `confidence`。
- `verify` / `review` 同时写 Markdown 和 `.v1.2.json`，后者作为机器可读质量门禁事实源。
- 不可信仓库内容、README 和 MCP 输出必须包裹到 `UNTRUSTED_REPOSITORY_CONTENT` / `UNTRUSTED_MCP_OUTPUT` 边界中；`review` 还会对 current-run diff 做 secrets 扫描，命中时生成 `SECRET_LEAK` 阻断归档。
