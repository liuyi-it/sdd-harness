# CLI 命令与协议参考

`sdd` 是 sdd-harness 唯一的命令行入口。支持 macOS、Windows（Git Bash）和 Linux。安装方式见 [README](../README.md#安装)；AI Agent 自行安装或更新时见[自举安装说明](agent-install.md)。所有工作流状态和制品都写入目标项目的 `.sdd/`。

## 通用参数

| 参数                | 说明                                                                            |
| ------------------- | ------------------------------------------------------------------------------- |
| `--json`            | 输出稳定的 `CommandResult` JSON                                                 |
| `--cwd <path>`      | 指定项目根目录，默认当前目录                                                    |
| `--change <id>`     | `new` 时指定变更 ID，或为无位置 ID 的后续命令声明当前变更；`change` 使用位置 ID |
| `--timeout <s>`     | 锁等待与子进程执行超时（秒）                                                  |
| `--help`            | 显示帮助                                                                        |
| `--version`         | 显示版本                                                                        |

进程退出码始终等于 `CommandResult.exitCode`。常见值为：`0` 成功、`1` 状态损坏或一般错误、`2` 参数错误、`3` 状态冲突、`4` 缺少或无效制品、`5` 组件不可用或引擎生成失败、`6` 存在未回答的阻塞问题、`7` 验证/TDD 失败、`8` 审查失败、`9` 并发锁冲突、`10` 安全阻断、`124` 超时。

## JSON 与 Agent 任务协议

`--json` 输出稳定的 `CommandResult`：`ok`、`state`、`exitCode` 必填；`changeId`、`next`、`data`、`warnings`、`actionRequired`、`error` 按结果出现。CLI、Adapter 与 Core API 共用这一契约。

`build next` 返回 `actionRequired.type="AGENT_TASK_EXECUTION"`，其中包含任务和变更 ID、内联 Context Pack、允许/期望新增/禁止文件、结构化 verification、代码库状态和可选 Policy Bundle；`resultTransport` 固定为 `inline-json`。Agent 只能通过 `build complete --task <id> --result-json '<JSON>'` 提交结果，不能直接修改 `.sdd/`。

`TaskExecutionResult` 必须与当前任务身份一致，并满足以下规则：

- `--result-json` 上限为 4 MiB；`message`、命令和单条输出有独立长度上限。
- RED 必须包含预期失败证据；GREEN、REFACTOR 和 VERIFY 的阶段证据必须通过。
- verification 必须不重不漏地覆盖计划命令；evidence 最多 64 条，verification 最多 32 条，`filesChanged` 最多 500 条。
- 实际文件范围以 Git delta 为事实源，Agent 声明不能扩大任务权限。

完整字段和结构约束以根目录 `schemas/` 及 [Schema 说明](schemas.md)为准。

## 工作流命令

### `sdd init`

初始化 `.sdd/`、配置、代码库索引和 Codex 原生接入文件。Codex 是默认宿主；OMP 的 Skill 使用内部 `hostAdapter=omp` 标记生成自己的原生资产。
空项目可用 `--structurePolicy free-design|user-defined` 固化目录结构策略；未指定时初始化继续完成并返回 `W_EMPTY_PROJECT`。
重复执行会刷新所选宿主资产和代码库索引，并更新当前宿主/目录策略配置；既有变更、运行、任务、报告和归档状态保持不变。

```bash
sdd init
sdd init --structurePolicy free-design
```

### `sdd status`

显示当前阶段、活动变更、错误和下一步建议。

```bash
sdd status
```

### `sdd new <需求>`

创建变更并生成供人工审核的 `spec.md`，以及写入 `.sdd/runtime.json` 的机器规格模型。首次在 `INDEX_READY` 调用必须传入非空需求；`new`、`change` 的需求正文统一限制为 32768 个 Unicode 字符。信息不足时进入 `CLARIFYING`，此时应收集用户回答，而不是重试空命令或默认改用 `--non-interactive`。`--non-interactive` 只属于 `new`/`auto`，用于允许需求不完整时直接失败的无人值守流程。若进程在 `NEW_STARTED` 中断，Core 会复用当前 `changeId`/`runId`，`sdd new --answers` 只恢复该变更，不会创建新变更。当前变更已进入 `SPEC_READY` 时需要修改需求时，请使用 `sdd change`，直接 `sdd new` 会返回 `E_ACTIVE_CHANGE_EXISTS`。

```bash
sdd new "实现订单取消功能"
# 收到 CLARIFYING 的问题并向用户确认后继续
sdd new --answers '{"Q-GOAL":"订单创建人取消待处理订单后看到已取消状态","Q-SCOPE":"仅修改订单服务取消接口，不改支付和库存流程","Q-ACCEPTANCE":"取消成功返回已取消状态；非待处理订单返回冲突且状态不变"}' --json
# 仅无人值守且接受需求不完整时直接失败的场景使用
sdd new "为待处理订单提供取消 API，包含权限、冲突响应、审计和测试" --non-interactive
```

### `sdd change <change-id> <新需求>`

修改当前活动且未归档的变更。命令直接重写 `spec.md` 和 `proposal.md`，删除旧需求派生的 `design.md`、`plan.md`、`tasks.md`，并把工作流退回 `SPEC_READY`，因此修改后必须重新执行 `design` 和 `plan`。不生成需求级备份或修订历史；runtime 的崩溃恢复备份不属于需求历史，Git 是唯一需求历史来源。

变更 ID 只允许由位置参数提供；同时传入全局 `--change` 会返回参数冲突，不会静默覆盖任一值。

```bash
sdd change cancel-pending-order "授权用户通过 PATCH /orders/{id} 更新待处理订单，返回 status 和 error_code" --json
sdd change cancel-pending-order "..." --answers '{"Q-ACTOR":"授权用户"}' --json
```
成功结果的 `data` 只包含当前文档和被删除的派生文档；不会返回 revision、diff 或 snapshot 路径。

### `sdd design`

根据 `spec.md` 和代码库影响生成技术方案，写入 `.sdd/runtime.json` 的 `changes.<changeId>.design`，同时生成供人工审核的 `design.md`。

```bash
sdd design --change add-order-cancel
```

### `sdd plan`

生成写入 `.sdd/runtime.json` 的机器计划、人工审核计划 `plan.md` 和可勾选任务清单 `tasks.md`。此阶段不会批量创建 Context Pack。

```bash
sdd plan --change add-order-cancel
sdd plan --dependencies '[{"name":"serde","manifest":"Cargo.toml","action":"ADD","reason":"序列化协议","requirements":["REQ-001"]}]'
```

### `sdd build`

不带子命令时等价于 `build next`；Agent 集成使用 `next/complete` 协议。

```bash
# 获取下一个任务，并为该任务按需生成 Context Pack
sdd build next --json

# 以内联 JSON 提交 Agent 的 TaskExecutionResult
sdd build complete \
  --task TASK-001-RED \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

`--result-json` 上限为 4 MiB；verification 必须不重不漏地覆盖 Context Pack 声明的全部验证命令。

### `sdd verify`

检查规格、任务状态、TDD 链、任务结果、Git 快照和场景证据覆盖。

```bash
sdd verify --json
```

### `sdd review`

执行确定性范围复核、敏感信息扫描、依赖计划校验，并记录改动规模和显式债务标记。最小正确实现原则已在 build Policy 中下发；新增 Cargo 依赖未在计划中以 `ADD` 声明时以 `E_UNPLANNED_DEPENDENCY` 阻断。失败后保留报告并回到可重新验证或审查的阶段。

确定性审查先执行：若被安全、范围或阶段门禁阻断，不会启动 OCR。确定性扫描通过且存在变更文件时，才按配置 `quality.ocr.mode` 决定是否调用可选的 Alibaba Open Code Review（`quality.ocr.command`，默认 `ocr`）：

- `auto`（默认）：找不到 `ocr` 命令时仅返回 `W_OCR_NOT_FOUND` 警告并保留确定性审查结论，不阻断；
- `off`：不启动 OCR；
- `required`：找不到 `ocr` 命令时返回 `E_REVIEW_BACKEND_UNAVAILABLE` 硬失败。

Core 固定以 `ocr review --format json --audience agent` 和 `OCR_NO_UPDATE=1` 启动后端。只接受当前 `ocr.run-manifest/v1` 完整输出；旧版 `success` / `session_id`、未知字段、覆盖统计不一致、失败 coverage、越界路径或非法行号都会拒绝。OCR 已启动后的超时、非零退出、失败状态或非法 JSON/finding 一律硬失败并持久化 `passed=false` 报告，使用稳定错误码：`E_REVIEW_BACKEND_TIMEOUT`（超时）、`E_REVIEW_BACKEND_FAILED`（非零退出或失败状态）、`E_REVIEW_BACKEND_INVALID_OUTPUT`（非法 JSON/finding）、`E_REVIEW_BACKEND_UNAVAILABLE`（启动失败）。OCR 子进程默认 120 秒超时，可用 `--timeout` 调整。OCR 的 prompt、thinking、API key 与完整 stderr 不会被持久化。

```bash
sdd review --json
```

### `sdd archive`

重新验证质量报告、任务结果、制品哈希和 Git 漂移，然后将 `spec.md`、`design.md`、`plan.md`、`tasks.md` 与验证/审查结果整合为完整 `archive.md`；机器归档、状态、配置、制品、任务结果、loop 和索引均保留在 `.sdd/runtime.json`。

```bash
sdd archive --json
```

## 自动流程

### `sdd auto <需求>`

根据状态机连续执行可确定的阶段。仅首次处于 `INDEX_READY` 时必须传入非空需求；在 `CLARIFYING`、`NEW_STARTED`、Agent 编码、失败或归档完成时收敛；auto 步骤失败或 `--stop` 后进入 `PAUSED`，用 `sdd auto --resume` 恢复。归档完成后携带新需求再次调用 `sdd auto "<需求>"` 会开启新变更。`--resume`、`--restart`、`--stop`、`--events` 和 `--loop-status` 控制已有 loop；`--tail` 必须与 `--events` 一起使用；`--answers` 将澄清答案透传给 `new`，不传需求文本。

```bash
sdd auto "实现订单取消功能"
# 收到 CLARIFYING 后
sdd auto --resume --answers '{"Q-GOAL":"订单创建人取消待处理订单后看到已取消状态","Q-SCOPE":"仅修改订单服务取消接口，不改支付和库存流程","Q-ACCEPTANCE":"取消成功返回已取消状态；非待处理订单返回冲突且状态不变"}'
# 若进程中断并停在 NEW_STARTED
sdd auto --resume
sdd auto --resume --run <run-id>
sdd auto --restart
sdd auto --stop
sdd auto --events --tail 20 --json
sdd auto --loop-status --json
```

## codebase 命令

| 命令                        | 作用                        |
| --------------------------- | --------------------------- |
| `sdd codebase status`       | 显示提供者、模式和索引状态  |
| `sdd codebase doctor`       | 诊断 CodeGraph 安装、索引状态和降级原因 |
| `sdd codebase index`        | 触发代码库索引              |
| `sdd codebase query <查询>` | 执行结构化代码库查询        |
| `sdd codebase rebuild`      | 重建索引                    |

```bash
sdd codebase query "order cancellation" --intent impact --json
```

CodeGraph 不可用、尚未建立真实 `.codegraph` 索引、输出为空/非 UTF-8 或命令失败时，会返回显式 warning 并降级到 `fallback-file-scan`；查询不会为缺失索引启动外部进程。`index`/`rebuild` 即使退出码为 0，也必须验证索引目录存在。使用 `sdd codebase doctor` 查看原因。
