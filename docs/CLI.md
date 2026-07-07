# sdd CLI 命令参考

`sdd` 是 sdd-harness 的 CLI 入口（同时提供 `sdd-harness` 别名）。

## 安装

macOS / Linux / Windows（Git Bash）:

```bash
bash scripts/install.sh
```

## 通用参数

所有主命令支持以下参数：

| 参数                | 说明                                    |
| ------------------- | --------------------------------------- |
| `--json`            | 输出稳定机器可读 JSON                   |
| `--cwd <path>`      | 指定目标项目根目录（默认当前目录）      |
| `--change <id>`     | 指定变更 ID                             |
| `--timeout <s>`     | 限制命令执行时间（秒）                  |
| `--non-interactive` | 禁止交互式澄清，遇到 BLOCKER 时返回失败 |
| `--force`           | 强制执行                                |
| `--verbose`         | 详细输出                                |
| `--help`            | 帮助信息                                |
| `--version`         | 版本号                                  |

## 退出码

| 退出码 | 含义                     |
| ------ | ------------------------ |
| 0      | SUCCESS                  |
| 1      | GENERAL_ERROR            |
| 2      | INVALID_ARGS             |
| 3      | STATE_CONFLICT           |
| 4      | SCHEMA_VALIDATION_FAILED |
| 5      | SECURITY_BLOCKED         |
| 6      | COMPONENT_UNAVAILABLE    |
| 124    | TIMEOUT                  |

CLI 进程退出码始终等于 `CommandResult.exitCode`。

## 主命令

### `sdd init`

初始化项目的 `.sdd/` 目录，建立代码库上下文。

```bash
sdd init --json
```

### `sdd status`

显示当前 SDD 状态。

```bash
sdd status --json
```

### `sdd new <需求>`

创建新的需求变更。

```bash
sdd new "实现订单取消功能" --json
```

### `sdd design`

生成设计制品。

```bash
sdd design --json
```

### `sdd plan`

生成实施计划（任务拆解、Context Pack）。

```bash
sdd plan --json
```

### `sdd build`

构建命令，支持子命令：

```bash
# 获取下一个构建任务
sdd build next --json

# 提交任务结果
sdd build complete --task TASK-001-RED --result result.json --json

# 查看构建状态
sdd build --json
```

### `sdd verify`

验证任务完成度与功能边界。

```bash
sdd verify --json
```

### `sdd review`

审查代码质量。

```bash
sdd review --json
```

### `sdd archive`

归档当前变更。

```bash
sdd archive --json
```

### `sdd auto <需求>`

自动推进完整 SDD 流程。

```bash
sdd auto "实现订单取消功能" --json
```

## codebase 命令

### `sdd codebase status`

显示 codebase 提供者、模式和索引状态。

```bash
sdd codebase status --json
```

### `sdd codebase doctor`

诊断 codebase-memory-mcp 健康状态。

```bash
sdd codebase doctor --json
```

### `sdd codebase index`

手动触发代码库索引。

```bash
sdd codebase index --json
```

### `sdd codebase query <查询>`

结构化代码库查询。

```bash
sdd codebase query "order cancellation" --intent impact --json
```

### `sdd codebase rebuild`

重建代码库索引。

```bash
sdd codebase rebuild --json
```
