# CLI 命令参考

`sdd` 是 sdd-harness 的命令行入口，`sdd-harness` 是等价别名。支持 macOS 和 Windows（Git Bash），运行时要求 Node.js 22+。

## 安装

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

重复安装会先清除当前 npm 前缀和 `PATH` 中属于本项目的旧版全局 CLI、本仓库依赖、workspace 构建目录和 TypeScript 构建缓存；安装后会验证实际命中的命令确实来自当前仓库，被其他同名命令遮蔽时直接报错。失败安装会自动回滚。`bash scripts/uninstall.sh` 执行完整卸载，但不会删除业务项目中的 `.sdd/` 用户数据。

在业务项目中重新执行 `sdd init` 会刷新命令、Skill、Schema、Adapter 元数据和代码库索引；工作流状态、变更、运行、归档、有效用户配置和自定义 loop 配置会保留。`CLAUDE.md` / `AGENTS.md` 仅替换 sdd-harness 受管区块。Windows 会优先复用 npm 包中的真实二进制或 `%LOCALAPPDATA%\Programs\codebase-memory-mcp\codebase-memory-mcp.exe`；也可通过 `CODEBASE_MEMORY_MCP_PATH` 显式指定。

所有工作流状态和制品都写入目标项目的 `.sdd/`。

## 通用参数

| 参数                | 说明                            |
| ------------------- | ------------------------------- |
| `--json`            | 输出稳定的 `CommandResult` JSON |
| `--cwd <path>`      | 指定项目根目录，默认当前目录    |
| `--change <id>`     | 指定变更 ID                     |
| `--timeout <s>`     | 设置命令超时秒数                |
| `--non-interactive` | 禁止交互式澄清                  |
| `--force`           | 覆盖允许强制重建的制品          |
| `--verbose`         | 输出详细信息                    |
| `--help`            | 显示帮助                        |
| `--version`         | 显示版本                        |

进程退出码始终等于 `CommandResult.exitCode`。常见值为：`0` 成功、`1` 状态损坏或一般错误、`2` 参数错误、`3` 状态冲突、`4` 缺少或无效制品、`7` 验证/TDD 失败、`8` 审查失败、`9` 并发锁冲突、`10` 安全阻断、`124` 超时、`130` 中断。

## 工作流命令

### `sdd init`

初始化 `.sdd/`、配置、Schema、代码库索引和 Agent 接入文件。未指定 `--agent` 时，Core API 默认安装所有内置 Adapter；CLI 可显式选择一个或多个 Agent。

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

创建变更并生成 `spec.md`、`spec.json`。信息不足时进入 `CLARIFYING`。

```bash
sdd new "实现订单取消功能"
sdd new "实现订单取消功能" --change add-order-cancel --non-interactive
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
```

### `sdd build`

不带子命令时由注入的 TaskExecutor 执行可运行任务；Agent 集成通常使用 `next/complete` 协议。

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

执行确定性代码审查、范围复核、敏感信息扫描和最小正确实现审查。新增 `package.json` 依赖未在计划中声明时以 `E_UNPLANNED_DEPENDENCY` 阻断；代码规模、依赖升级和 `sdd-debt` 只记录为非阻断 finding。可恢复失败会生成 REPAIR 任务或暂停等待用户决策。

```bash
sdd review --json
```

### `sdd archive`

重新验证质量报告、Git 快照、漂移和追踪闭环，并把改动规模、依赖 delta、`sdd-debt` 与 Policy 来源写入归档，然后将变更目录压缩为 `archive.json`、`archive.md`、`.archived`。

```bash
sdd archive --json
```

## 自动流程

### `sdd auto <需求>`

根据状态机连续执行可确定的阶段，在需求澄清、Agent 编码、失败或归档完成时收敛。

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
| `sdd codebase doctor`       | 诊断 MCP 健康状态和降级原因 |
| `sdd codebase index`        | 触发代码库索引              |
| `sdd codebase query <查询>` | 执行结构化代码库查询        |
| `sdd codebase rebuild`      | 重建索引                    |

```bash
sdd codebase query "order cancellation" --intent impact --json
```

`codebase-memory-mcp` 不可用时，命令会返回显式 warning 并降级到 `fallback-file-scan`；使用 `sdd codebase doctor` 查看原因。
