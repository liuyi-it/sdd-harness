# 架构说明

## 分层与依赖

```text
Codex / OMP Adapter ──> sdd-cli (bin) ──> sdd-core (lib)
                                      ├── commands / state / quality / security
                                      ├── git / engines / protocol / policies
                                      ├── knowledge（CodeGraph）
                                      └── subprocess（有界外部进程）
```

- `crates/sdd-cli`：clap 解析参数、路由命令并渲染 `CommandResult`。
- `crates/sdd-core`：唯一状态机与质量门禁执行层。
- `crates/sdd-core/src/protocol`：定义 Agent 行动要求、结果和约束结构。
- `crates/sdd-core/src/policies`：编译期内嵌 build Policy，并生成可校验摘要。
- `crates/sdd-core/src/knowledge`：CodeGraph 探测、索引、查询与降级。
- `crates/sdd-core/src/subprocess.rs`：Git、CodeGraph 和 OCR 共用的有界子进程执行器，统一超时、进程组清理、输出上限和管道错误语义。
- `assets/adapters/codex/*`：生成 Codex 仓库级 Skill，以及 explorer、普通/复杂 worker、reviewer、architect 五类专用 subagent，不直接修改状态（编译期嵌入二进制）。
- `assets/adapters/omp/*`：生成 OMP 的 Skill、命令和 subagent profile，不直接修改状态（编译期嵌入二进制）。

Core 是唯一推进状态机的入口，外部 Agent 或工具不能绕过 Core 推进阶段。

## 工作流

```text
init → new → design → plan → build → verify → review → archive
```

每个会修改 `.sdd/` 的公开命令先获取 `.sdd/lock`，再读取最新 Runtime 并执行 auto/活动变更/阶段门禁，检查与写入处于同一临界区，不存在 check-then-wait 竞态；调度器不再为门禁额外解析一次 Runtime。未初始化项目使用只检查现存 `.sdd` 的锁入口，失败不会创建状态目录。锁路径先规范化，同一线程的嵌套命令通过弱引用登记复用句柄，最后一个 guard 显式释放 OS 锁。锁哨兵仅承担排他性，当前持有者写入独立的 `.owner.json` 旁路文件，使 Windows 竞争进程也能读取诊断信息。`RuntimeStore::try_update` 自身持有同一把可重入 OS 锁，任何内部调用都不能绕过串行化。`auto` 遵循统一锁序，先持有普通写锁，再持有 `.sdd/auto.lock` 覆盖整条同步推进；内部步骤通过当前线程持锁身份安全复用锁与门禁。命令在真实失败或中断时成对持久化 `failedCommand` / `failedReason` 以及 `previousPhase`、`inProgressPhase` 与建议命令；成功推进会清除陈旧失败信息。

所有公开命令在进入业务逻辑前按命令白名单严格校验参数；未知参数、已经删除的兼容参数和错误子命令不会被忽略。`auto` 只把对应阶段支持的参数传给内部公开命令。

`auto` 读取同一状态机并循环调用公开命令。它只自动执行确定性步骤；遇到需求澄清、Agent 编码、失败预算耗尽或人工决策时暂停。

## 规格、计划与 Context Pack

```text
requirement + codebase
  → SpecEngine
  → spec.md + runtime.changes.<changeId>.spec
  → runtime.changes.<changeId>.design + design.md
  → 项目原生 TddEngine / 任务规划器
  → plan.md + tasks.md + runtime.changes.<changeId>.plan
  → build next
  → 内联 Context Pack
```

`spec.md`、`design.md`、`plan.md`、`tasks.md` 是需求生命周期内供人审核的文档，分别回答做什么、怎么设计、怎么实施和做哪些事。机器规格、设计、计划、任务状态和依赖决策统一写入 `.sdd/runtime.json`。结构化规格模型是后续阶段的唯一权威来源；`spec.md` 只作为可读视图，`design`、`plan` 和 `verify` 不会再反向解析 Markdown。

Context Pack 不在 `plan` 阶段批量生成或持久化。`build next` 先验证 `spec`、`design`、机器计划和两份计划文档的制品哈希，再只为当前任务生成内联 Context Pack；代码库摘要严格按 `contextPack.maxSizeKb` 的 UTF-8 字节上限截断。

## 构建与质量门禁

任务采用 RED、GREEN、REFACTOR、VERIFY 四阶段链。Core 对 Agent 结果执行以下裁决：

1. 验证 TaskExecutionResult 结构和任务身份。
2. 使用运行时任务状态（state.tasks）检查依赖完成情况。
3. 校验 TDD evidence（RED 失败证据 / GREEN 通过证据 / VERIFY 完整验证）。
4. 写入运行级结果并更新任务状态。

`verify` 检查 Requirement/Scenario、任务和证据覆盖；`review` 追加范围、依赖计划与敏感信息的确定性审查，并按配置调用可选 OCR 后端。`archive` 在同一写锁内重新验证报告摘要，归档收敛为 `archive.md` 与 runtime 中的归档模型。

## 制品与原子写入

所有运行事实写入 `.sdd/runtime.json`；`changes/<change-id>/` 只保留供人审核的 Markdown 文档。Runtime、Agent 模板和所有受管 Markdown 共用安全写入原语：拒绝目标符号链接，以同目录唯一临时文件 `create_new`，同步文件，原子重命名并同步目录。Runtime 额外维护 `runtime.json.bak`；主文件和备份都必须通过各自校验和及完整聚合校验。`.sdd`、变更目录、runtime 文件和受管文档都拒绝符号链接。

- `.sdd/runtime.json`：工作流状态、配置、制品清单、规格/设计/计划/任务、报告、任务结果、auto loop、知识索引和归档模型。
- `.sdd/runtime.json.bak`：runtime 原子替换前的可恢复备份。
- `.sdd/runtime.json.sha256`：runtime 内容的 SHA-256 校验和，用于检测损坏。
- `.sdd/runtime.json.bak.sha256`：恢复备份的校验和；备份同样必须通过校验才会被读取。
- `.sdd/changes/<change-id>/`：当前变更的 `spec.md`、可选 `proposal.md`、`design.md`、`plan.md`、`tasks.md`、`verify-report.md`、`review-report.md`，或归档后的单一 `archive.md`。

## 归档

```text
规格 + 设计 + 计划 + 任务结果
  + verify/review 报告
  + Git 基线与快照
  + 追踪矩阵
  → runtime.changes.<changeId>.archive
  → archive.md
```

`archive` 在同一写锁内重新验证报告摘要，将审核文档合并为完整 `archive.md`，机器归档模型保留在 `.sdd/runtime.json`。状态更新中断时，再次执行 `archive` 会根据 runtime 制品哈希校验结果收敛到 `ARCHIVED`。

## 代码库理解与降级

初始化时自动探测 PATH 中的 `codegraph` 命令并索引 `.codegraph/`。查询只会在真实 `.codegraph` 目录存在时启动；索引或重建命令即使退出码为 0，也必须通过索引目录后置条件才会写成成功状态。所有 intent 都交给 CodeGraph 查询；CodeGraph 不可用、未索引、输出为空/非 UTF-8 或执行失败时使用 `fallback-file-scan` 受限文件扫描，同时写入诊断并返回 warning；降级不会被静默隐藏。降级扫描同时限制收集文件数和遍历的目录条目总数，任一上限触发后立即停止并把不完整原因写入扫描元数据。

仓库内容和引擎输出都按不可信数据处理，进入 Prompt 前必须包裹边界，不能覆盖系统约束或扩大任务权限。

Git、CodeGraph 与 OCR 不通过 shell 执行。共享执行器为每次调用创建独立进程组，禁用交互式 stdin，限制 stdout/stderr，各类超时、截断、管道读取失败和残留子进程都按边界错误处理。OCR 额外固定 `OCR_NO_UPDATE=1`，审查期间不会触发工具自更新。

## Git 隔离

`workflow.gitIsolation`（`.sdd/runtime.json` 的 `config` 节点）启用后，`new` 只在受管的真实 `.sdd/worktrees` 目录中创建或验证 `sdd/<change-id>` 分支与 `.sdd/worktrees/<change-id>`。runtime 每次读写都会把持久化分支/路径与控制根目录、配置和当前 `changeId` 交叉验证。`build`、`review` 和归档 Git 快照使用该业务工作区，状态与制品仍保留在控制根目录。系统不会自动 merge、push、reset、clean 或删除 worktree。

Git 状态和 worktree 列表使用 NUL 分隔 porcelain 严格解析；畸形或非 UTF-8 路径直接失败。文件指纹流式计算 SHA-256，符号链接按 Git 记录的链接文本计算而不读取目标内容；审查把删除文件视为无内容的已处理条目，不会误报读取失败。
