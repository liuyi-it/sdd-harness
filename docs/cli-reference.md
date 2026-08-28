# CLI 命令参考

## 全局参数

| 参数 | 说明 |
| --- | --- |
| `--json` | 输出稳定 JSON，供宿主 Agent 使用 |
| `--cwd <path>` | 指定项目根目录 |
| `--change <id>` | 指定目标 change；多个活动任务时必填 |
| `--timeout <seconds>` | 锁等待或外部命令超时 |

全局参数可放在子命令前后。未知参数会被 CLI 或 Core 拒绝。

## `sdd init`

初始化 Runtime、宿主资产和代码库索引。终端默认 Codex；OMP 宿主内部调用 `--host-adapter omp`。用户无需交互选择宿主。

```bash
sdd init
sdd init --structurePolicy free-design
```

## `sdd status`

只读查看活动任务。可用 `--change` 查看一个已归档或活动 change。多个活动任务且未指定时，状态为 `MULTIPLE_CHANGES`，data 中包含候选，不返回猜测的 next。

```bash
sdd status --json
sdd status --change <id> --json
```

## `sdd new <需求>`

创建新 change 并返回规格阶段行动。需求最长 32768 个 Unicode 字符；可用全局 `--change` 指定新 ID。

```bash
sdd new "实现订单取消功能" --json
sdd new --change order-cancel --result-json '<SpecPhaseResult JSON>' --json
```

每个新需求都创建独立 workflow，因此已有活动任务不会阻止新建。

## `sdd change <新需求>`

修订目标 change。多个活动任务时必须 `--change`；修订开始后重新生成完整规格，完成时作废所有派生制品。

```bash
sdd change "订单取消需要记录操作者" --change order-cancel --json
sdd change --change order-cancel --result-json '<SpecPhaseResult JSON>' --json
```

## `sdd design`

从已批准规格生成技术设计。

```bash
sdd design --change <id> --json
sdd design --change <id> --result-json '<DesignPhaseResult JSON>' --json
```

## `sdd plan`

从规格和设计生成纵向任务计划。Core 校验任务 ID、文件范围、依赖无环、验证命令安全，以及规格需求/场景的精确完整覆盖。

```bash
sdd plan --change <id> --json
sdd plan --change <id> --result-json '<PlanPhaseResult JSON>' --json
```

## `sdd build next|complete`

```bash
sdd build next --change <id> --json
sdd build complete \
  --change <id> \
  --task TASK-001 \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

`next` 返回 `AGENT_TASK_EXECUTION`。`complete` 要求 taskId 匹配 pending 任务、filesChanged 与 Git 事实一致、文件在计划范围内、全部 verification 不重不漏。TDD 任务完成时必须同时包含 expectedFailure=true 的失败证据和最终通过证据。

## `sdd verify`

统一执行可追溯性、任务证据、Git 范围、敏感信息和依赖计划检查。

```bash
sdd verify --change <id> --json
sdd verify --change <id> --result-json '<FixResult JSON>' --json
sdd verify --change <id> --continue --json
```

首次失败返回 `AGENT_FIX_EXECUTION` 并消耗一轮修复预算。修复后自动重新评估；仍失败进入 `QUALITY_BLOCKED`。`--continue` 只允许用户明确授权后的额外一轮，不能与 `--result-json` 同时使用。

## `sdd archive`

仅允许 `QUALITY_READY`。归档前重新核对 Git 指纹和任务完成状态，生成单一 `archive.md`。

```bash
sdd archive --change <id> --json
```

## `sdd codebase`

```bash
sdd codebase status --json
sdd codebase doctor --json
sdd codebase index --json
sdd codebase query "OrderService 取消订单调用链" --intent impact --json
sdd codebase rebuild --json
```

`codebase` 是项目级命令，不选择 change。`query` 必须带非空查询；`intent` 只对 query 有效。

## 已删除命令

v0.6 删除 `sdd auto` 和 `sdd review`，也删除 `--answers`、`--non-interactive` 与计划依赖的 CLI 注入参数。旧命令不会作为兼容别名保留。
