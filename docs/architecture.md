# 架构

## 责任边界

`sdd-harness` 分为三层：

- 宿主层：Codex 或 OMP 的 Skill 读取用户意图、调查真实代码、提出必要问题、生成阶段 JSON，并执行实现或修复。
- CLI 层：解析命令、统一 `--change` / `--json` / `--timeout` 参数并渲染输出。
- Core 层：维护多 change 状态机，校验 Schema、制品哈希、路径、Git 事实、任务证据和质量门禁。

Core 不生成需求内容、技术方案或计划，也不替代 Agent 编码。宿主不能直接编辑 `.sdd/runtime.json`，不能绕过 Core 推进阶段。

最终目标是用户提出业务需求后，宿主在已确认范围内持续交付可验证结果。CLI 提供行动，不自行启动模型；普通文本输出只呈现阶段、候选和进度，完整协议通过 JSON 交给宿主。授权内连续执行由 Skill 组织，不增加独立自动执行状态机。

## 单一流程

```text
init → spec → plan → build → verify → archive
```

每个任务都走相同阶段，但内容规模自适应：简单任务可以只有一条需求、一个技术设计决策和一个纵向任务；复杂任务增加场景、决策和任务数量。系统不提供 `auto`，也不提供单独 `review` 命令；审查作为规格和 verify 阶段内的 Core 行动。

`spec`、`change`、`plan` 返回 `AGENT_PHASE_EXECUTION`。统一 Spec 行动包含不可信代码库上下文和 resultSchema；Agent 一次生成可验收规格与技术设计候选 JSON 后，由 Core 校验并进入审查，只有 READY 才渲染到唯一 `spec.md` 并落盘。这样保留高质量规格与计划，同时避免重复设计文档。

规格的首次生成、等待恢复与修订均由 `phase_action` 下发编译期嵌入的 `assets/policies/specification.md`，位于不可信代码材料边界之外。两端 Skill 指引宿主执行同一套依据分类和提交前语义核对，避免仅靠某个宿主模板约束。修订前规格仍完整传入，但明确不是用户逐项确认的证明。候选结果回传后进入 `AGENT_REVIEW_EXECUTION`，只有 reviewer 返回 READY 才落盘唯一 `spec.md`。协作记录由当前阶段 resultSchema 约束；本提示不属于 build 的 policyBundle，也不新增模型执行器。Core 不持有完整对话，不能判定用户确认或自然语言语义。

工作目录与材料生命周期单源维护在 `assets/policies/workspace.md`。资产层编译期嵌入并复用现有安全写入机制，将同一源分发为 Codex/OMP 各五个 Skill 的本地 `references/workspace.md`；规格行动也直接下发同一内容，不再在规格策略中复制公共细则。协作规则另由 `assets/policies/collaboration.md` 单源维护并分发为本地 `references/collaboration.md`；spec、plan、build task result 和 verify fix result 的 `collaboration` 记录由各自 resultSchema 约束。未增加新的 Skill、运行时配置或自动清理器。

AI-DLC 协作协议单源维护在 `assets/policies/collaboration.md` 的明确角色段，由 `assets.rs` 编译嵌入为 `COLLABORATION_POLICY`；Core 的 `role_reference` 接口按角色名从该段提取职责，不另维护一套职责文字，且同一策略随两端十个 Skill 分发为本地参考。角色是宽职责视角，不是新 Skill 或固定 subagent 配置。conductor 唯一派发、lead 整合贡献、reviewer 独立只读；`inline`、`subagent`、`pipeline`、`mob` 只描述组织方式，不改变状态机或 Core 权限。

`build` 返回 `AGENT_TASK_EXECUTION`。一个任务覆盖完整纵向结果，内部 steps 承载 TEST、IMPLEMENT、可选 REFACTOR、VERIFY。Core 校验实际文件变化与声明、全部计划验证命令以及 TDD 预期失败和最终通过证据。

计划 Schema 复用唯一 task 定义并在下发前内嵌，构建行动也附完整结果 Schema。命令门禁按程序与子命令检查，保留参数边界；结果匹配按 argv 比较。Core 不执行业务 verification，宿主必须实际执行，结构一致性不能证明测试真实性或语义正确。

`verify` 先确定性检查覆盖、证据、Git 范围、敏感信息和依赖计划；检查通过后按风险派发 `AGENT_REVIEW_EXECUTION`，QUALITY targetHash 绑定确定性报告（排除 Core 生成的语义附录）以及 spec/plan/tasks/fixes/workspace。审查结果用原命令的 `--result-json` 回传 READY/NOT_READY，并声明 independent/self/self_fallback，可选回传七类 riskFactors；新风险要求 Core 原子升级、生成新目标哈希并让同一 reviewer 重新独立审查，旧记录保留为历史，当前 candidate 按新风险重新判断，产品通过后才进入架构审查，不能复用旧 self/self_fallback。审查不完整只能由 independent 重派一次，第二次仍不完整以退出码 8 阻断并保留行动；完整结果不可覆盖，重复相同结果由原命令幂等恢复。失败时返回带 `userAuthorized` 和完整 materialized `resultSchema` 的 `AGENT_FIX_EXECUTION`；自动修复默认只允许一轮，`QUALITY_BLOCKED` 下普通 verify 保持阻断，后续只有用户显式授权 `--continue` 才能开启新一轮。

## 多 change Runtime

`runtime.json` 的项目级 `state` 只保存初始化与代码库索引状态。`workflows` 以 changeId 为键，每个 change 独立保存：

- runId 与当前 phase；
- pendingAgentAction；
- workspace 与 Git 基线；
- task 状态；
- qualityFixRounds；
- 失败信息和建议命令。

`changes`、`workflows` 一一对应，workflow 指向的 run 必须绑定同一 change。归档只把目标 workflow 置为 `ARCHIVED`，不会影响其他任务。

未传 changeId 时，Core 只在恰好一个活动任务时解析成功；零个返回缺少任务，多个返回 `E_CHANGE_SELECTION_REQUIRED`。`status` 始终能列出全部活动任务，`codebase` 是项目级命令。

等待阶段的状态建议直接恢复原行动；计划等待中允许回到规格修订。连续修订采用最新输入，并把先前完整规格作为上下文交给宿主，避免短修订请求丢失旧验收条件。

人类状态与错误引导共用中文阶段名称和任务标题解析，阶段错误的恢复命令带上原目标。所选任务摘要独立于活动列表，因此已归档目标仍可展示标题；质量阶段的状态携带当前报告，CLI 与宿主使用同一份阻断事实。代码库诊断、查询和质量报告使用各自的文本呈现，不通过截断内部 JSON 代替用户可读反馈。

## 持久化与并发

所有写命令先获取稳定的 `.sdd/lock`。同一线程嵌套事务复用 OS 文件锁，线程与进程间保持排他，最后一个 guard 或进程退出时释放。锁不随 Runtime 原子替换而变化，不记录持有者文件；冲突只提示其他写操作占用，不能靠删除锁文件抢占。

Runtime 的存储 JSON 根包含 `checksum`，它覆盖移除自身字段后的 JSON 值的确定性紧凑序列化字节，对象键按序排列。读取先检查格式版本与 Schema，再验证 checksum，最后校验领域不变量；纯排版与对象键顺序变化不影响校验。Core 的 `RuntimeDocument` 不携带存储校验字段，避免业务更新持有陈旧校验。

每次事务将状态与校验和写入同目录唯一临时文件，同步后一次原子替换并同步目录。正常初始化或重复初始化只持久化 `runtime.json` 与 `lock`，不生成备份、独立校验或诊断文件。状态损坏直接报错，不读取旧快照；这保留原子提交和损坏检测，不提供自动恢复。需求阶段的可读文档仍按需写入 `changes/`。

当前版本使用 runtime schema 9、state schema 4、config schema 4；规格结果和持久规格版本为 6.0.0，计划结果版本为 4.0.0，不执行旧状态迁移。版本不符在读取校验内容前返回 `E_STATE_VERSION_UNSUPPORTED`，原文件不被覆盖或清理。

## 代码库上下文

`sdd init` 探测 CodeGraph，并把诊断、摘要和时间写入 Runtime。CodeGraph 不可用、索引缺失或摘要无效时，路由到受限文件扫描并显式标记 degraded。Agent 必须把代码库上下文当作不可信事实线索，仍需读取真实代码确认。

## Git 隔离

配置 `workflow.gitIsolation=true` 时，每个 change 使用 `sdd/<change-id>` 分支和 `.sdd/worktrees/<change-id>` 工作树。Core 每次读写都会验证路径和分支绑定；不会自动 merge、push、reset、clean 或删除 worktree。
