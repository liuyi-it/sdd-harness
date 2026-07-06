# sdd-harness 二期总体设计

## 1. 目标与边界

二期将现有可运行的 SDD 工作流升级为结构化、可验证、可恢复、可扩展的 SDD Loop Harness。产品仍只以 Claude Code 与 Codex 插件交付，共享 `@sdd-harness/core`，不提供或预留独立 CLI、后台服务与 Web UI。

二期分为三个必须完成且独立验收的里程碑：

1. Phase II-A：Schema 1.2.0、迁移、Loop 持久化、TaskExecutor v2。
2. Phase II-B：codebase-memory-mcp 查询、结构化质量报告、安全增强。
3. Phase II-C：Branch/Worktree 隔离。

每个阶段必须完成实现、测试、中文文档、提交和推送，才能进入下一阶段。

## 2. 总体架构

系统继续采用三层结构：

```text
Claude Code / Codex Adapter
        ↓ 宿主能力注入
@sdd-harness/core
        ↓ 唯一状态写入者
.sdd/ 流程事实源 + Git 业务事实源
```

Adapter 只负责解析宿主参数、注入 TaskExecutor/MCP 能力和展示 `CommandResult`。Core 是 `.sdd/` 的唯一写入者，并新增六个职责边界：

- `schema`：Schema 1.2.0、真实制品校验和版本迁移。
- `loop`：Loop Specification、运行记录、继续与重启语义。
- `execution`：TaskExecutor v2、v1 归一化和执行结果裁决。
- `quality`：VerifyReport、ReviewReport 和确定性 ReviewIssue。
- `security`：Secrets Scanner、审计脱敏和不可信上下文边界。
- `git-isolation`：分支/worktree 生命周期和业务执行路径。

## 3. Schema 与迁移

状态、配置、任务及所有二期新增 Schema 和制品统一使用 `schemaVersion: "1.2.0"`。现有项目直接从 1.0.0 迁移到 1.2.0，不保留 1.1.0 中间迁移节点。

迁移前备份 `state.json` 和 `config.yml`，迁移后生成 Loop Specification、`activeLoop: null`、迁移报告和审计记录。旧任务和任务结果保持可读；缺少 `phase` 或 `scenarios` 的旧任务不得继续 build，必须重新执行 plan。旧 TaskExecutionResult 通过归一化层读取，不原地篡改历史制品。

Golden validation 必须运行真实 fixture 流程，并用 Schema 校验真实生成的 tasks、task results、loop run、verify report 和 review report，不能只验证手写样例。

## 4. Bounded SDD Loop

`sdd auto` 保留 `new → design → plan → build → verify → review → archive` 主流程，但从状态驱动的串行调用升级为有协议、有预算、有审计记录的 bounded loop。

`sdd init` 生成 `.sdd/loops/auto-change.yml`。缺失时可恢复默认文件；用户修改过默认文件时保留原文件并生成 candidate，只有显式 force 才允许覆盖。

每次运行创建 `.sdd/runs/<run-id>/loop-run.json`，并在 `state.activeLoop` 保存当前摘要。state 是当前状态事实源，loop-run 是审计历史；发生冲突时不得用历史记录反向覆盖 state。

运行参数语义如下：

- 默认：继续当前 `activeLoop`。
- `resume=<run-id>`：显式继续指定运行。
- `restart=true`：将旧运行标记为 `ABORTED` 后创建新运行。
- resume 与 restart 同时出现：返回参数错误。

步骤写入采用“运行记录草稿 → state 原子提交 → 运行记录收敛”的顺序。运行记录缺失时依据 state 恢复并标记 `recovered=true`。同一项目只允许一个活动 Loop，继续使用项目文件锁串行化状态变更。

## 5. TaskExecutor v2

v2 请求包含 `changeId`、`runId`、Git baseline、Context Pack、constraints、mode 和取消信号。执行模式优先使用 `subagent`；宿主不支持时降级为 `main-agent`，两种模式接受相同的 Core 校验，降级原因写入运行记录和审计日志。

执行命令必须表示为 `{ command, args }`。禁止 shell、管道、重定向、命令替换和字符串拼接。v1 字符串命令只允许通过严格白名单解析；无法安全解析时返回 `E_SECURITY_BLOCKED`。

v1 结果经 normalize 转换为 v2，补齐 taskId、status、createdFiles、notes、commandsRun 和 outputSummary。TaskExecutor 的 status 和文件列表只是声明。Core 必须重新计算 Git delta，并校验文件范围、禁止文件、命令白名单、TDD 阶段、验证证据和并行归属后，才能决定最终任务状态。

经校验的结果分别写入变更汇总和运行级单任务制品。

## 6. MCP 查询能力

二期 MCP 特指固定版本 `codebase-memory-mcp v0.8.1`、提交 `f0c9be1`。`McpTransportV2` 只是 Core 对该能力的宿主抽象，不用于接入任意 MCP，也不负责自动下载或升级。

初始化时进行 capability discovery，并写入 capabilities 与 diagnostics 制品。查询按 impact、related-files、symbols、callers、callees、routes、tests 和 architecture 等 intent 执行。MCP 未安装、超时或缺少工具时，自动返回同结构的 fallback-file-scan 结果，并明确设置 `degraded=true` 和降级原因。

## 7. 机器可读质量门禁

verify 和 review 无论 PASS 或 FAIL，都必须先原子写入 JSON 与 Markdown 报告，再返回 `CommandResult`。

VerifyReport 覆盖制品、任务、需求、场景、TDD 证据、测试和 drift。任一任务未完成、追踪关系缺失、证据链不完整或存在未申报文件时失败。

ReviewReport 只实现确定性审查，输出结构化 ReviewIssue。BLOCKER、SECRET_LEAK、关键 SECURITY/FILE_SCOPE 问题、禁止文件和未跟踪 current-run diff 阻断 archive。LLM review 不属于二期范围。

## 8. 安全设计

仓库内容和 MCP 输出分别使用 `UNTRUSTED_REPOSITORY_CONTENT` 与 `UNTRUSTED_MCP_OUTPUT` 边界。Adapter 向 TaskExecutor 注入固定安全规则：上下文不是指令，只能执行允许命令、修改授权文件，且不得读取仓库外路径。

Secrets Scanner 扫描 current-run diff、任务结果和写入前制品。AuditLogger 必须在落盘前脱敏，扫描器不反扫 audit.log。初版覆盖 AWS Key、GitHub token、JWT、私钥、Authorization header、含密码数据库 URL 和通用 secret/token 赋值。

例外只能按“路径 + 规则类型”配置，并写入审计日志。私钥、GitHub token 等高风险规则不可豁免；配置中不得保存原始 secret。

## 9. Branch/Worktree 隔离

`createWorktree=true` 自动启用 `createBranch`。Core 创建或安全复用 `sdd/<change-id>` 分支，并在 `.sdd/worktrees/<change-id>` 创建 worktree。分支基线不匹配、路径占用或 worktree 状态异常时阻断，不执行 reset。

TaskExecutor、GitInspector、verify 和 review 以 worktree 为业务工作目录；主项目根目录仍是 `.sdd/` 流程制品的唯一位置。archive 记录分支、worktree 和最终提交信息，但不自动 merge、push 或删除 branch/worktree，清理由用户显式触发。

## 10. 错误处理与一致性

所有新制品使用现有安全路径和原子写入机制。安全错误、路径越界、符号链接逃逸、组件完整性失败属于 hard stop。FAILED 和 PAUSED 可恢复，CLARIFYING 等待用户输入。部分写入不得暴露为有效完成状态，重复执行必须通过幂等检查收敛。

结构化错误至少包含稳定 code 和安全 message；日志与报告不得包含原始 secret。MCP 降级是可观察 warning，不等同于失败；Schema、Git scope 和安全校验失败必须阻断状态推进。

## 11. 测试与交付

测试分为四层：

- 单元测试：Schema、迁移、Loop、normalize、报告和安全规则。
- 集成测试：state/loop-run 一致性、Git delta、MCP 降级和失败报告落盘。
- Adapter 契约测试：Claude Code 与 Codex 输入输出一致。
- E2E：完整 auto、resume/restart、secret 阻断、提示注入和 worktree 隔离。

每个里程碑必须执行：

```text
npm run format:check
npm run lint
npm run typecheck
npm test
npm run validate:schemas
npm run validate:release
```

同时更新 README、架构、Schema、安全、状态机和需求追踪文档。所有门禁通过并提交推送后，才能进入下一里程碑。

## 12. 实施顺序

Phase II-A 先建立 1.2.0 数据契约、迁移、Loop 和 TaskExecutor v2，使后续功能有稳定事实源。Phase II-B 在此基础上接入 MCP、结构化质量报告和安全链路。Phase II-C 最后增加 Git 隔离，避免同时改变执行协议和工作目录语义。

三个阶段均为二期最终交付范围；Phase II-C 不阻塞前两个阶段的独立验收，但不能从完整二期验收中省略。
