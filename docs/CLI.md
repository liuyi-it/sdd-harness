# CLI 命令参考

`sdd` 是 sdd-harness 的命令行入口，`sdd-harness` 是等价别名。支持 macOS、Windows（Git Bash）和 Linux。预编译二进制从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载，运行时不需要 Rust；从源码构建才需要 Rust 工具链。可选的 CodeGraph 独立 CLI 不要求 Node.js；通过 npm 使用 GitNexus 时还需 Node.js 22 或更高版本。

## 安装

预编译二进制从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载对应平台文件（Linux x64：`sdd-linux-x64`；macOS Intel：`sdd-macos-x64`；macOS Apple Silicon：`sdd-macos-arm64`；Windows x64：`sdd-windows-x64.exe`），放入 PATH 即可运行。

从源码安装：

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

重复安装会先备份并清除旧版全局 CLI，再通过 `cargo build --release` 构建并注册命令；安装后会验证命令可运行。失败安装会恢复原版本。可用 `PREFIX=/path bash scripts/install.sh` 指定安装目录。`bash scripts/uninstall.sh` 执行完整卸载，但不会删除业务项目中的 `.sdd/` 用户数据。

在业务项目中重新执行 `sdd init` 会刷新所选 Adapter 文件和代码库索引；工作流状态、变更、运行、归档与有效用户配置会保留。`AGENTS.md` 只替换 sdd-harness 受管区块。

所有工作流状态和制品都写入目标项目的 `.sdd/`。

## 通用参数

| 参数                | 说明                                                                            |
| ------------------- | ------------------------------------------------------------------------------- |
| `--json`            | 输出稳定的 `CommandResult` JSON                                                 |
| `--cwd <path>`      | 指定项目根目录，默认当前目录                                                    |
| `--change <id>`     | 新建时指定变更 ID；后续命令必须与当前活动变更一致                               |
| `--timeout <s>`     | 设置命令超时秒数                                                                |
| `--non-interactive` | 仅用于允许需求不完整时直接失败的无人值守流程；遇到未回答的 BLOCKER 返回退出码 6 |
| `--force`           | 覆盖允许强制重建的制品                                                          |
| `--verbose`         | 输出详细信息                                                                    |
| `--help`            | 显示帮助                                                                        |
| `--version`         | 显示版本                                                                        |

进程退出码始终等于 `CommandResult.exitCode`。常见值为：`0` 成功、`1` 状态损坏或一般错误、`2` 参数错误、`3` 状态冲突、`4` 缺少或无效制品、`6` 非交互模式下存在未回答的 BLOCKER、`7` 验证/TDD 失败、`8` 审查失败、`9` 并发锁冲突、`10` 安全阻断、`124` 超时、`130` 中断。

## 工作流命令

### `sdd init`

初始化 `.sdd/`、配置、代码库索引和 Agent 接入文件。未指定 `--agent` 时安装全部内置 Adapter；CLI 可显式选择一个或多个 Agent。
空项目可用 `--structurePolicy free-design|user-defined` 固化目录结构策略；未指定时初始化继续完成并返回 `W_EMPTY_PROJECT`。

```bash
sdd init --agent codex
sdd init --agent claude,codex
sdd init --agent opencode --structurePolicy free-design
```

### `sdd status`

显示当前阶段、活动变更、错误和下一步建议。

```bash
sdd status
sdd status --loop --json
```

### `sdd new <需求>`

创建变更并生成 `spec.md`、`spec.json`。首次调用必须传入非空需求；信息不足时进入 `CLARIFYING`，此时应收集用户回答，而不是重试空命令或默认改用 `--non-interactive`。

```bash
sdd new "实现订单取消功能"
# 收到 CLARIFYING 的问题并向用户确认后继续
sdd new --answers '{"Q-001":"仅订单创建人可取消待处理订单"}' --json
# 仅无人值守且接受需求不完整时直接失败的场景使用
sdd new "为待处理订单提供取消 API，包含权限、冲突响应、审计和测试" --non-interactive
```

### `sdd design`

根据规格和代码库影响生成 `design.md`。

```bash
sdd design --change add-order-cancel
```

### `sdd plan`

生成 `plan.json`，其中包含任务、可读计划、测试计划、上下文摘要和可选依赖决策。此阶段不会批量创建 Context Pack。

```bash
sdd plan --change add-order-cancel
sdd plan --dependencies '[{"name":"serde","manifest":"Cargo.toml","action":"ADD","reason":"序列化协议","requirements":["REQ-001"]}]'
```

### `sdd build`

不带子命令时等价于 `build next`；Agent 集成使用 `next/complete` 协议。

```bash
# 获取下一个任务，并为该任务按需生成 Context Pack
sdd build next --json

# 提交 Agent 写出的 TaskExecutionResult
sdd build complete \
  --task TASK-001-RED \
  --result .sdd/runs/<run-id>/tasks/TASK-001-RED.result.json \
  --json
```

### `sdd verify`

检查规格、任务状态、TDD 链、任务结果、Git 快照和场景证据覆盖。

```bash
sdd verify --json
```

### `sdd review`

执行确定性代码审查、范围复核、敏感信息扫描和最小正确实现审查。新增 Cargo 依赖未在计划中以 `ADD` 声明时以 `E_UNPLANNED_DEPENDENCY` 阻断；改动规模和显式债务标记只记录为非阻断 finding。失败后保留报告并回到可重新验证或审查的阶段。

```bash
sdd review --json
```

### `sdd archive`

重新验证质量报告、任务结果、制品哈希和 Git 漂移，然后将完整计划与报告写入归档，并把变更目录压缩为 `archive.json`、`archive.md`、`.archived`。

```bash
sdd archive --json
```

## 自动流程

### `sdd auto <需求>`

根据状态机连续执行可确定的阶段。首次调用必须传入非空需求；在需求澄清、Agent 编码、失败或归档完成时收敛。`--resume`、`--restart`、`--stop`、`--events` 和 `--loop-status` 则是控制已有 loop 的命令，不传需求。

```bash
sdd auto "实现订单取消功能"
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
| `sdd codebase doctor`       | 诊断双引擎安装、索引状态和降级原因 |
| `sdd codebase index`        | 触发代码库索引              |
| `sdd codebase query <查询>` | 执行结构化代码库查询        |
| `sdd codebase rebuild`      | 重建索引                    |

```bash
sdd codebase query "order cancellation" --intent impact --json
```

GitNexus / CodeGraph 均不可用时，命令会返回显式 warning 并降级到 `fallback-file-scan`；使用 `sdd codebase doctor` 查看原因。
