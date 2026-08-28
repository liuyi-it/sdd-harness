# Agent 接入

## Codex

`sdd init` 默认写入 `.agents/skills/`：

- `sdd-harness`：自然语言软件变更的编排入口；
- `sdd-init`、`sdd-status`、`sdd-new`、`sdd-change`、`sdd-design`、`sdd-plan`、`sdd-build`、`sdd-verify`、`sdd-archive`、`sdd-codebase`：每个公共命令一个聚焦 Skill。

每个 Skill 都有独立 name/description，因此 Codex 可以按用户意图发现，也可以通过 `$sdd-...` 显式调用。本项目不再安装专用 subagent 配置；是否使用通用 subagent 由宿主和用户决定，小任务不会被强制拆给多个角色。

## OMP

OMP 宿主运行 `sdd init --host-adapter omp`，写入：

- `.omp/skills/` 下与 Codex 同名的 11 个 Skill；
- `.omp/commands/sdd.md` 自然语言入口；
- `/sdd.init`、`/sdd.status`、`/sdd.new`、`/sdd.change`、`/sdd.design`、`/sdd.plan`、`/sdd.build`、`/sdd.verify`、`/sdd.archive`、`/sdd.codebase` 全部显式命令。

终端默认 Codex，OMP 自己传入隐藏宿主标识；用户无需在 `sdd init` 时选择。

## 多任务交互契约

所有需要 change 的阶段 Skill 在执行前运行 `sdd status --json`：

1. 用户明确 changeId：使用该目标。
2. 只有一个活动 change：可以继续唯一任务。
3. 存在多个活动 change 且用户未明确：展示候选标题与阶段并询问；不得运行写命令，不得选择最近任务。
4. `status` 和 `codebase` 是项目级入口，不需要选择。

Core 同时执行相同规则，防止 Skill 提示遗漏时写错任务。

## 行动协议

- `AGENT_PHASE_EXECUTION`：调查真实代码，必要时询问用户，生成 resultSchema JSON；禁止修改业务文件。
- `AGENT_TASK_EXECUTION`：只修改 allowedFiles，执行任务内部 steps 和全部 verification，提交 TaskExecutionResult。
- `AGENT_FIX_EXECUTION`：只修复质量报告阻断项，提交 FixResult；自动轮次耗尽后必须获得用户授权。

宿主只向用户解释目标、决策、修改、验证、风险和选择问题。CLI JSON、Context Pack、Policy Bundle、runtime 路径、change/run/task 标识仅供内部处理，除非用户明确要求排障原始信息。
