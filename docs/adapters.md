# Agent 接入

sdd-harness 让 Agent 负责对话和编码，Core 负责状态、文件范围、验证、审查和归档。`sdd init` 默认接入 Codex；OMP 作为另一原生宿主由其 Skill 注入 `hostAdapter=omp`。OpenCode 已不再受支持。

## Codex Adapter（默认）

执行 `sdd init` 后，项目会生成：

- `.agents/skills/sdd-harness/SKILL.md`：符合 Codex 仓库级 Skill 发现路径的工作流说明，可通过 `$sdd-harness` 显式调用，也可由任务描述自动匹配。
- `.codex/agents/sdd-explorer.toml`：只读调用链与约束探索员。
- `.codex/agents/sdd-worker.toml`：拥有工作区写权限的单任务实现者。
- `.codex/agents/sdd-reviewer.toml`：只读的独立复核者。

主 Agent 保留需求澄清、架构决定、`.sdd` 状态推进和最终验收。它可以并行派发相互独立的探索、日志分析或审查任务；写入任务必须限定在不重叠的允许文件内，默认串行。每个 subagent 都必须返回结论、证据位置、涉及文件、验证结果和未决风险；主 Agent 必须重新检查 diff、文件范围和验证输出。

Codex 子 Agent 的模型与权限按角色固化：探索和审查使用只读 `gpt-5.6-terra`，实现使用 `workspace-write` 的 `gpt-5.6`。项目不写入 `.codex/config.toml`，避免覆盖用户自己的全局 Agent 并发、权限或 MCP 配置。

## OMP Adapter

OMP 接入内容位于 `.omp/skills/`、`.omp/commands/` 和 `.omp/agents/`；入口是直接描述需求、`/sdd 需求`，或显式的 `/sdd.<command>` 命令。

`sdd init` 写入三类短资源：

- `sdd-harness` Skill：自然语言静默触发 SDD；只在必要时向用户提问。
- `/sdd`：自然语言显式调用入口，以及 8 个 `/sdd.<command>` 阶段控制入口。
- `sdd-worker-simple`、`sdd-worker`、`sdd-worker-complex`：项目级 subagent profile，分别使用 `@smol`、`@task`、`@slow`；模型与思考强度由项目 `.omp/config.yml` 配置，最终可用性由 OMP 解析。

主 Agent 必须检查 subagent 的 diff、文件范围和验证结果，再提交 Core 的任务结果并执行最终 `verify` / `review`。不要直接修改 `.sdd/runtime.json`，不要把 JSON、Context Pack 或 Policy Bundle 原样展示给用户。`vendor/superpowers/` 仅是审计快照；其原始运行时提示词不会被安装或注入。
