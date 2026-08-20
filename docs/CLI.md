# CLI 命令参考

`sdd` 是 sdd-harness 唯一的命令行入口。支持 macOS、Windows（Git Bash）和 Linux。预编译二进制从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载，运行时不需要 Rust；从源码构建才需要 Rust 工具链。可选的 CodeGraph 是 npm CLI（需要 Node.js），`sdd` 本身不依赖 Node.js。

## 安装

预编译二进制从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载对应平台文件（Linux x64：`sdd-linux-x64`；macOS Intel：`sdd-macos-x64`；macOS Apple Silicon：`sdd-macos-arm64`；Windows x64：`sdd-windows-x64.exe`），放入 PATH 即可运行。

从源码安装：

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

重复安装会先备份并清除已有全局 `sdd`，再通过 `cargo build --release` 构建并注册命令；安装后会验证命令可运行。失败安装会恢复原版本。可用 `PREFIX=/path bash scripts/install.sh` 指定安装目录。`bash scripts/uninstall.sh` 执行完整卸载，但不会删除业务项目中的 `.sdd/` 用户数据。

在业务项目中重新执行 `sdd init` 会默认刷新 Codex 的 Skill、subagent 和代码库索引；工作流状态、变更、运行、归档与有效用户配置会保留。OMP 宿主会通过内部标记生成自己的原生资产；OpenCode 不再受支持。

所有工作流状态和制品都写入目标项目的 `.sdd/`。

## 通用参数

| 参数                | 说明                                                                            |
| ------------------- | ------------------------------------------------------------------------------- |
| `--json`            | 输出稳定的 `CommandResult` JSON                                                 |
| `--cwd <path>`      | 指定项目根目录，默认当前目录                                                    |
| `--change <id>`     | 新建时指定变更 ID；后续命令必须与当前活动变更一致                               |
| `--timeout <s>`     | 锁等待与子进程执行超时（秒）                                                  |
| `--non-interactive` | 仅用于允许需求不完整时直接失败的无人值守流程；遇到未回答的 BLOCKER 返回退出码 6 |
| `--verbose`         | 输出详细信息                                                                    |
| `--help`            | 显示帮助                                                                        |
| `--version`         | 显示版本                                                                        |

进程退出码始终等于 `CommandResult.exitCode`。常见值为：`0` 成功、`1` 状态损坏或一般错误、`2` 参数错误、`3` 状态冲突、`4` 缺少或无效制品、`5` 组件不可用或引擎生成失败、`6` 非交互模式下存在未回答的 BLOCKER、`7` 验证/TDD 失败、`8` 审查失败、`9` 并发锁冲突、`10` 安全阻断、`124` 超时、`130` 中断。

## 工作流命令

### `sdd init`

初始化 `.sdd/`、配置、代码库索引和 Codex 原生接入文件；默认不需要交互选择，不能通过 `--agent` 参数选择。
空项目可用 `--structurePolicy free-design|user-defined` 固化目录结构策略；未指定时初始化继续完成并返回 `W_EMPTY_PROJECT`。

```bash
sdd init
sdd init --structurePolicy free-design
```

### `sdd status`

显示当前阶段、活动变更、错误和下一步建议。

```bash
sdd status
sdd status --loop --json
```

### `sdd new <需求>`

创建变更并生成供人工审核的 `spec.md`，以及写入 `.sdd/runtime.json` 的机器规格模型。首次在 `INDEX_READY` 调用必须传入非空需求；信息不足时进入 `CLARIFYING`，此时应收集用户回答，而不是重试空命令或默认改用 `--non-interactive`。若进程在 `NEW_STARTED` 中断，Core 会复用当前 `changeId`/`runId`，`sdd new --answers` 只恢复该变更，不会创建新变更。当前变更已进入 `SPEC_READY` 时需要修改需求时，请使用 `sdd change`，直接 `sdd new` 会返回 `E_ACTIVE_CHANGE_EXISTS`。

```bash
sdd new "实现订单取消功能"
# 收到 CLARIFYING 的问题并向用户确认后继续
sdd new --answers '{"Q-ACTOR":"仅订单创建人可取消待处理订单"}' --json
# 仅无人值守且接受需求不完整时直接失败的场景使用
sdd new "为待处理订单提供取消 API，包含权限、冲突响应、审计和测试" --non-interactive
```

### `sdd change <change-id> <新需求>`

修改当前活动且未归档的变更。命令直接重写 `spec.md` 和 `proposal.md`，删除旧需求派生的 `design.md`、`plan.md`、`tasks.md`，并把工作流退回 `SPEC_READY`，因此修改后必须重新执行 `design` 和 `plan`。不生成需求级备份或修订历史；runtime 的崩溃恢复备份不属于需求历史，Git 是唯一需求历史来源。

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

# 提交 Agent 写出的 TaskExecutionResult；支持结果文件或内联 JSON
sdd build complete \
  --task TASK-001-RED \
  --result /tmp/task-result.json \
  --json

sdd build complete \
  --task TASK-001-RED \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

### `sdd verify`

检查规格、任务状态、TDD 链、任务结果、Git 快照和场景证据覆盖。

```bash
sdd verify --json
```

### `sdd review`

执行确定性代码审查、范围复核、敏感信息扫描和最小正确实现审查。新增 Cargo 依赖未在计划中以 `ADD` 声明时以 `E_UNPLANNED_DEPENDENCY` 阻断；改动规模和显式债务标记只记录为非阻断 finding。失败后保留报告并回到可重新验证或审查的阶段。

确定性审查先执行：若被安全、范围或阶段门禁阻断，不会启动 OCR。确定性扫描通过且存在变更文件时，才按配置 `quality.ocr.mode` 决定是否调用可选的 Alibaba Open Code Review（`quality.ocr.command`，默认 `ocr`）：

- `auto`（默认）：找不到 `ocr` 命令时仅返回 `W_OCR_NOT_FOUND` 警告并保留确定性审查结论，不阻断；
- `off`：不启动 OCR；
- `required`：找不到 `ocr` 命令时返回 `E_REVIEW_BACKEND_UNAVAILABLE` 硬失败。

OCR 已启动后的超时、非零退出、失败状态或非法 JSON/finding 一律硬失败并持久化 `passed=false` 报告，使用稳定错误码：`E_REVIEW_BACKEND_TIMEOUT`（超时）、`E_REVIEW_BACKEND_FAILED`（非零退出或失败状态）、`E_REVIEW_BACKEND_INVALID_OUTPUT`（非法 JSON/finding）、`E_REVIEW_BACKEND_UNAVAILABLE`（启动失败）。OCR 子进程默认 120 秒超时，可用 `--timeout` 调整。OCR 的 prompt、thinking、API key 与完整 stderr 不会被持久化。

```bash
sdd review --json
```

### `sdd archive`

重新验证质量报告、任务结果、制品哈希和 Git 漂移，然后将 `spec.md`、`design.md`、`plan.md`、`tasks.md` 与验证/审查结果整合为完整 `archive.md`；机器归档、状态、配置、制品、任务结果、Context Pack、loop 和索引均保留在 `.sdd/runtime.json`。

```bash
sdd archive --json
```

## 自动流程

### `sdd auto <需求>`

根据状态机连续执行可确定的阶段。仅首次处于 `INDEX_READY` 时必须传入非空需求；在 `CLARIFYING`、`NEW_STARTED`、Agent 编码、失败或归档完成时收敛；auto 步骤失败或 `--stop` 后进入 `PAUSED`，用 `sdd auto --resume` 恢复。归档完成后携带新需求再次调用 `sdd auto "<需求>"` 会开启新变更。`--resume`、`--restart`、`--stop`、`--events` 和 `--loop-status` 控制已有 loop；`--tail` 必须与 `--events` 一起使用；`--answers` 将澄清答案透传给 `new`，不传需求文本。

```bash
sdd auto "实现订单取消功能"
# 收到 CLARIFYING 后
sdd auto --resume --answers '{"Q-ACTOR":"授权用户","Q-ACTION":"取消待处理订单"}'
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

CodeGraph 不可用时，命令会返回显式 warning 并降级到 `fallback-file-scan`；使用 `sdd codebase doctor` 查看原因。
