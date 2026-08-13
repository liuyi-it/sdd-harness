# Agent 接入

sdd-harness 让 Agent 负责对话和编码，Core 负责状态、文件范围、验证、审查和归档。当前只支持 OMP 与 OpenCode；其他 AI Agent 不会被 `sdd init` 接受。日常使用不需要阅读 `.sdd` 内部制品。

## OMP Adapter

OMP 接入内容位于 `.omp/skills/`、`.omp/commands/` 和 `.omp/agents/`；入口是直接描述需求、`/sdd 需求`，或显式的 `/sdd.<command>` 命令。

```bash
sdd init
```

终端运行时会交互选择 OMP 或 OpenCode；不提供 `--agent` 参数。进入 OMP 后使用 `/sdd.init`，宿主会自动注入 OMP；OpenCode 使用 `/sdd-init`，宿主会自动注入 OpenCode。

## OMP 行为

`sdd init` 写入三类短资源：

- `sdd-harness` Skill：自然语言静默触发 SDD；只在必要时向用户提问。
- `/sdd`：自然语言显式调用入口。
- 6 个 `/sdd.<command>`：面向用户的阶段控制入口。
- `sdd-worker-simple`、`sdd-worker`、`sdd-worker-complex`：项目级 subagent profile，分别使用 `@smol`、`@task`、`@slow`；模型与思考强度由项目 `.omp/config.yml` 配置，最终可用性由 OMP 解析。

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

主 Agent 根据任务复杂度自主选择三个 profile；共享文件、架构、安全、不可逆外部副作用和最终验收仍由主 Agent 负责。主 Agent 必须检查 subagent 的 diff、文件范围和验证结果，再提交 Core 的任务结果并执行最终 `verify` / `review`。

项目通过 `.omp/config.yml` 提供 subagent 角色的默认模型与思考强度，OMP 仍负责模型注册、凭据和可用性解析，不安装 SDK、RPC 或 Superpowers 运行时。不要直接修改 `.sdd/runtime.json`，不要把 JSON、Context Pack 或 Policy Bundle 原样展示给用户。`vendor/superpowers/` 仅是审计快照；其原始运行时提示词不会被安装或注入。

## OpenCode Adapter

OpenCode 使用原生项目目录：`.opencode/skills/`、`.opencode/commands/` 和 `.opencode/agents/`，不会写入用户已有的 OpenCode 配置。

```bash
sdd init
# 交互选择 OpenCode
```

OpenCode 命令使用连字符命名：`/sdd`、`/sdd-init`、`/sdd-new`、`/sdd-change`、`/sdd-status`、`/sdd-plan`、`/sdd-verify`、`/sdd-review`、`/sdd-archive`。三个 `sdd-worker-*` 是 `mode: subagent` 的原生 worker，禁止继续派发子 Agent；它们不锁定 provider/model，由 OpenCode 当前配置决定，避免把 OMP 的角色别名或不可用模型硬编码到 OpenCode。

终端选择结果只生成对应的一套接入文件；宿主 Agent 内部通过适配器上下文选择自身目录，用户不需要再次选择。
