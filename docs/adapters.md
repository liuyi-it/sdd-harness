# OMP 接入

sdd-harness 让 Agent 负责对话和编码，Core 负责状态、文件范围、验证、审查和归档。日常使用不需要阅读 `.sdd` 内部制品。

## OMP Adapter

项目只支持 Oh My Pi（OMP），接入内容位于 `.omp/skills/`、`.omp/commands/` 和 `.omp/agents/`；入口是直接描述需求、`/sdd 需求`，或显式的 `/sdd.<command>` 命令。

```bash
sdd init
```

## OMP 行为

`sdd init` 写入三类短资源：

- `sdd-harness` Skill：自然语言静默触发 SDD；只在必要时向用户提问。
- `/sdd`：自然语言显式调用入口。
- 6 个 `/sdd.<command>`：面向用户的阶段控制入口。
- `sdd-worker`：项目级 subagent，按 `@smol` → `@task` 解析当前可用模型。

已注册的显式命令：

| OMP 命令 | 对应 CLI | 用途 |
| --- | --- | --- |
| `/sdd.init` | `sdd init` | 初始化项目 |
| `/sdd.status` | `sdd status` | 查看状态 |
| `/sdd.plan` | `sdd plan` | 生成计划和任务 |
| `/sdd.verify` | `sdd verify` | 验证需求和证据 |
| `/sdd.review` | `sdd review` | 审查改动 |
| `/sdd.archive` | `sdd archive` | 归档需求 |

需求创建、设计、构建、自动推进和代码库查询由 `/sdd` 在内部编排；完整 CLI 子命令仍保留给终端、CI 和排障，不再全部暴露为 slash 命令。

小而独立的任务可以交给 `sdd-worker`；复杂、共享文件、架构、安全或外部副作用任务由主 Agent 执行。主 Agent 必须检查 subagent 的 diff、文件范围和验证结果，再提交 Core 的任务结果并执行最终 `verify` / `review`。

OMP 自己负责模型注册、凭据和可用性解析，项目不硬编码具体模型名，也不安装 SDK、RPC 或 Superpowers 运行时。不要直接修改 `.sdd/state.json`，不要把 JSON、Context Pack 或 Policy Bundle 原样展示给用户。`vendor/superpowers/` 仅是审计快照；其原始运行时提示词不会被安装或注入。
