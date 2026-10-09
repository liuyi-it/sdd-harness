# sdd-harness

面向 Codex 与 Oh My Pi（OMP）的轻量规格驱动开发工具。`sdd` Core 只负责确定性状态、Schema、文件范围、证据和质量门禁；宿主 Agent 基于真实代码生成统一规格（含技术设计）、计划并执行任务。

最终目标是让用户描述需求、参与必要决策，Agent 在已有授权内持续交付经过验证的业务结果。产品约束和后续优化标准见 [仓库协作指南](AGENTS.md#项目最终目标)，可复跑的真实 CLI 示例见 [可用性试用与回归](docs/usability.md)。

Spring Boot 的 HTTP、并发创建与进程重启验收样例见 [工单试用工程](fixtures/springboot/README.md)，可使用 Maven 和 Java 17 独立复跑。

## 设计原则

- 所有软件变更都有规格，但文档深度和任务数量按复杂度调整。
- 只有一条逐阶段流程：`spec → plan → build → verify → archive`。
- 不提供 `auto`，不单独提供 `review` 命令；规格和 verify 的审查作为 Core 行动嵌入现有阶段。
- 不依赖 Superpowers、OpenSpec 或 mattpocock-skills，只吸收“先明确需求、再设计、再计划、证据后完成”的理念。
- 同一项目可同时维护多个 change。阶段命令未指定 change 且只有一个活动任务时可继续；存在多个活动任务时返回 `E_CHANGE_SELECTION_REQUIRED`，宿主必须询问用户，绝不猜测最近任务。
- 规格、技术设计和计划由宿主 Agent 生成；其中规格与技术设计共同写入唯一的 `spec.md`。Core 校验结构、拒绝占位内容、渲染 Markdown 并持久化。
- 规格生成须区分用户确认、代码事实、设计建议和待确认事项；提交前核对需求覆盖、规则依据与修订一致性。复用旧实现不自动继承其业务限制，结构通过不代表语义正确，详见 [规格依据与语义审查](docs/adapters.md#规格依据与语义审查)。
- 协作采用 AI-DLC 的六类宽职责角色和 `inline`、`subagent`、`pipeline`、`mob` 四种拓扑；它们共享同一协作参考，不新增 Skill 或固定 subagent 配置。conductor 唯一派发，lead 整合贡献，reviewer 独立只读。
- 一个 build 任务是可独立验收的纵向切片，测试、实现、重构和最终验证是任务内部步骤，不拆成四个流程任务。
- `verify` 失败时最多自动派发一轮受控修复；仍失败进入 `QUALITY_BLOCKED`，普通 `verify` 保持阻断，只有用户明确授权才能 `--continue` 开启额外一轮。

五个阶段均须明确目标项目并显式设置工作目录；正式流程文档由该项目的 CLI 生成，不默认另存到会话目录。中间草稿优先内存，必要时使用独立系统临时目录并在不再需要时安全清理；业务交付物、必要验证证据、归档材料及用户要求保留的文件不属于清理范围。详见 [工作目录与临时材料](docs/adapters.md#工作目录与临时材料)。

## 安装

当前版本为 `v0.7.0`，新增按需多 Agent 协作、成果版本绑定审查，以及 Maven 参数质量门禁修复。Runtime schema 升为 9，旧项目状态不自动迁移；升级前请阅读 [安装与升级说明](docs/agent-install.md#验证与升级)。

### 预编译二进制（推荐）

从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载对应文件并放入 PATH：

| 平台 | 文件 |
| --- | --- |
| Linux x64 | `sdd-linux-x64` |
| Linux x64（musl/Alpine） | `sdd-linux-x64-musl` |
| macOS Intel | `sdd-macos-x64` |
| macOS Apple Silicon | `sdd-macos-arm64` |
| Windows x64 | `sdd-windows-x64.exe` |

包括 macOS Apple Silicon 在内的安装与升级命令见 [安装指南](docs/agent-install.md)。二进制与校验文件先下载到独立临时目录，校验 SHA-256 和新版本可执行性，再在安装目录暂存并替换；失败时保留原安装。

从源码安装需要 Rust：

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

卸载 CLI：`bash scripts/uninstall.sh`。卸载失败返回非零并显示错误，不报告完成；不会删除业务项目的 `.sdd/`。源码安装支持 `PREFIX` 与 `CARGO_TARGET_DIR`，优先使用 PATH 中已有的 Cargo，仅缺少时加载现有 Rust 环境；锁定依赖并显式构建本机 target，验证后才替换命令。

## 初始化与宿主资产

在业务项目根目录执行：

```bash
sdd init
```

终端直接执行时默认安装 Codex 资产。Codex 只会得到五个阶段 Skill：

```text
sdd-spec  sdd-plan  sdd-build  sdd-verify  sdd-archive
```

每个阶段同时安装 `references/workspace.md` 和 `references/collaboration.md`；它们是共享参考，不计入 Skill 数量。OMP 安装同样的五个 Skill 和两份参考，并注册当前有效的十个 slash command。

OMP 宿主使用内部参数 `sdd init --host-adapter omp`，安装同一组五个 Skill，并注册 `/sdd` 与当前九个 `/sdd.<command>` 快捷入口（清单见 [宿主接入](docs/adapters.md#omp)）。快捷入口不是额外 Skill：`/sdd` 和 `/sdd.change` 会转入统一 Spec 流程；已退役的 `new`、`design` 入口不再分发。初始化只刷新当前清单，保留清单外已有文件，旧资产的删除须按具体项目授权处理。用户无需在初始化时交互选择宿主；宿主适配器负责传入自身标识。

初始化同时探测 CodeGraph。CodeGraph 可用时建立或复用 `.codegraph/` 索引；不可用时显式降级为受限文件扫描。预编译 `sdd` 本身不依赖 Node.js 或 Rust。

## 使用

推荐在 Codex 或 OMP 中直接描述软件需求，由安装的 Skill 逐阶段执行。完整实现请求会在确认范围后持续推进到交付；只要求规格或计划时停在相应阶段。已有回答不重复询问。CLI 本身不会启动 AI，返回的行动需由宿主 Agent 执行。手工查看流程时可运行：

```bash
sdd spec "实现订单取消功能" --json
# 宿主按关键歧义持续多轮澄清，结合选择题与开放问题，明确后再回传
sdd plan --change <change-id> --json
sdd build next --change <change-id> --json
sdd verify --change <change-id> --json
sdd archive --change <change-id> --json
```

`spec`、`change`、`plan` 返回 `AGENT_PHASE_EXECUTION`，其中包含阶段上下文和 resultSchema。`spec` 与 `change` 的结果同时包含可验收需求、场景、技术设计和必填 `reviewPolicy`；spec、plan、build task result 和 verify fix result 还必须按当前 Schema 记录 `collaboration`（拓扑、lead、贡献及限制）。规格候选回传后先进入 `AGENT_REVIEW_EXECUTION`；审查为 READY 后 Core 才写入唯一 `spec.md`。审查结果可报告同一七类枚举中的新风险，Core 原子追加风险、保留旧审查记录并按新风险重新判断当前 candidate，要求同一 reviewer 重新独立审查；不完整 independent 审查最多重派一次，第二次以退出码 8 保留行动。宿主对阶段行动和审查行动都使用原命令的 `--result-json` 回传。

`build next` 返回 `AGENT_TASK_EXECUTION`。宿主只能修改 `allowedFiles`，执行全部 `verification`，然后提交：

```bash
sdd build complete \
  --change <change-id> \
  --task TASK-001 \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

`verify` 先确定性执行规格/场景覆盖、任务证据、Git 实际范围、敏感信息和依赖计划检查，再按风险进入 `AGENT_REVIEW_EXECUTION`。若返回 `AGENT_FIX_EXECUTION`，行动带 `userAuthorized` 和完整 materialized resultSchema，宿主修复并以 `sdd verify --result-json` 回传；质量自动修复仅一轮，首轮后仍失败时进入 `QUALITY_BLOCKED`，普通 verify 不开启新轮次，只有用户明确授权后才能 `sdd verify --continue`。

## 多任务选择

```bash
sdd status --json
```

`status` 的 `activeChanges` 会列出所有未归档任务。读取状态不需要选择；`codebase` 也是项目级命令。其他阶段命令在多个活动任务下必须显式传入 `--change <id>`。宿主 Skill 需要先把候选任务的标题和阶段转换为简短中文，再询问用户选择。

普通终端输出直接显示候选标题、阶段、标识和任务进度。中断后，状态中的下一步会重新取得等待中的行动，无需用户拼装结果 JSON；计划等待期间也可以修订需求，连续修订采用最新输入并携带修订前规格。

阶段用错时，错误提示给出中文原因及带目标标识的恢复命令；多任务冲突直接列出业务标题。质量失败会展示具体问题；修复轮次耗尽后，即使手动修复也须明确授权并使用 `--continue` 重入修复验证链，否则普通 verify 保持阻断。`sdd --help` 提供首次使用示例，代码库诊断和查询直接显示可读结果。

## 状态与制品

```text
INDEX_READY
  └─ SPEC_WAITING_AGENT → SPEC_WAITING_REVIEW → SPEC_READY
       → PLAN_WAITING_AGENT → PLAN_READY
       → BUILD_WAITING_AGENT ↔ PLAN_READY → BUILD_READY
       → QUALITY_WAITING_FIX | QUALITY_WAITING_REVIEW | QUALITY_BLOCKED | QUALITY_READY
       → ARCHIVED
```

每个 change 都有独立 workflow、run、任务状态和质量修复轮次。项目级 state 只保存初始化与代码库索引信息。

```text
.sdd/
├── runtime.json
├── lock
└── changes/<change-id>/
    ├── spec.md
    ├── plan.md
    ├── tasks.md
    ├── quality-report.md
    └── archive.md       # 归档后仅保留该文件
```

初始化后 `.sdd/` 只包含 `runtime.json` 与 `lock`，进入需求阶段后才生成 `changes/` 下的可读文档。状态内嵌 SHA-256，与数据一次原子写入；不生成备份、校验边车或锁诊断文件。检测到损坏就报错停止，不自动回退。锁文件保持稳定，文件存在不表示进程仍在运行。

当前 Runtime 格式为 schema 9，规格结果和持久规格版本为 6.0.0，计划结果版本为 4.0.0，不迁移旧 `.sdd`。检测到旧格式时会直接返回 `E_STATE_VERSION_UNSUPPORTED`；先用匹配版本读取并保留需要的结果，用户确认旧状态无需保留后再清理并执行 `sdd init`。Agent 不自动删除已有状态。

## 命令

| 命令 | 作用 |
| --- | --- |
| `sdd init` | 初始化 runtime、代码库索引和宿主资产 |
| `sdd status` | 查看项目状态和所有活动任务 |
| `sdd spec <需求>` | 创建 change 并统一生成规格与技术设计 |
| `sdd change <新需求> --change <id>` | 修订需求并作废派生制品 |
| `sdd plan` | 生成纵向任务计划 |
| `sdd build next/complete` | 派发任务或提交证据 |
| `sdd verify` | 统一验证、审查和受控修复 |
| `sdd archive` | 归档已通过质量门禁的 change |
| `sdd codebase ...` | 管理或查询代码库上下文 |

完整参数见 [CLI 命令参考](docs/cli-reference.md)。

## 项目结构

| 路径 | 职责 |
| --- | --- |
| `crates/sdd-cli` | CLI 参数与输出 |
| `crates/sdd-core` | 多 change 状态机、Schema、制品、Git 与质量门禁 |
| `assets/adapters/codex` | Codex 的五个阶段 Skill |
| `assets/adapters/omp` | OMP 的五个阶段 Skill 与全部 slash 快捷命令 |
| `assets/policies` | 阶段共享的工作目录、协作、规格和 build 受控 Policy |
| `schemas` | runtime、阶段结果、任务、修复和报告 Schema |
| `docs` | 架构、CLI、状态机、安全、Schema 与宿主接入 |

## 文档

- [CLI 命令参考](docs/cli-reference.md)
- [架构](docs/architecture.md)
- [状态机](docs/state-machine.md)
- [安全](docs/security.md)
- [Schema](docs/schemas.md)
- [Agent 接入](docs/adapters.md)
- [可用性试用与回归](docs/usability.md)
- [AI Agent 自举安装](docs/agent-install.md)
- [第三方工具声明](THIRD_PARTY_NOTICES.md)

## 开发验证

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CLI 可用性回归会创建临时 Git 项目，并使用 Python 标准库真实运行失败与通过测试，需要 PATH 中有 Git 和 Python（Unix 为 `python3`，Windows 为 `python`）；不会访问远端或安装第三方包。

## License

MIT
