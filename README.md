# sdd-harness

面向 AI Coding Agent 的规格驱动开发（SDD）工程支架。它用统一 CLI 管理需求澄清、设计、任务拆解、构建、验证、审查和归档，并通过状态机、Git 变更事实、安全边界与质量门禁约束 Agent。

## 核心能力

- CLI-first：`sdd` 是唯一确定性入口，Core 是唯一状态与门禁执行层。
- Agent 原生接入：默认生成 Codex 仓库级 Skill 与专用 subagent，另保留 OMP 的原生命令与角色配置。
- 规格与 TDD：Requirement/Scenario 规格模型驱动 RED、GREEN、REFACTOR、VERIFY 任务链。
- 代码库理解：自动探测并索引 CodeGraph 知识图谱，按 intent 查询；不可用时显式降级到受限文件扫描。
- 安全可追溯：受管文件原子写入且拒绝符号链接，严格校验路径、命令、Git delta、TDD 证据和敏感信息。
- 最小正确实现：按复用、标准库、平台能力和既有依赖的顺序决策；未计划新增依赖会阻断审查。
- 精简制品：活动需求只保留人工审核的 `spec.md`、可选 `proposal.md`、`design.md`、`plan.md`、`tasks.md` 与质量报告，机器状态写入统一 runtime；归档后整合为一个 `archive.md`。

## 环境要求

- 预编译二进制运行不需要 Rust；从源码构建才需要 Rust 工具链（cargo，edition 2021）
- Git
- CodeGraph CLI（可选；`sdd codebase doctor` 可诊断，缺失时自动降级文件扫描）
  - CodeGraph 是 npm CLI，需要 Node.js；`sdd` 预编译二进制本身不依赖 Node.js。
- Codex（默认；`sdd init` 写入 `.agents/skills` 和 `.codex/agents`）
- Oh My Pi（OMP；宿主初始化时写入项目级 Skill、精简 slash 命令集、subagent profiles 和角色模型配置）
- macOS、Windows（Git Bash）或 Linux

## 安装

### 预编译二进制（推荐）

从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载对应平台的 `sdd` 二进制并放入 PATH：

| 平台 | 文件 |
| --- | --- |
| Linux x64 | `sdd-linux-x64` |
| Linux x64（musl/Alpine） | `sdd-linux-x64-musl` |
| macOS Intel | `sdd-macos-x64` |
| macOS Apple Silicon | `sdd-macos-arm64` |
| Windows x64 | `sdd-windows-x64.exe` |

macOS 示例：

```bash
install_dir="$HOME/.local/bin"
mkdir -p "$install_dir"
curl -fL -o "$install_dir/sdd" https://github.com/liuyi-it/sdd-harness/releases/latest/download/sdd-macos-arm64
chmod +x "$install_dir/sdd"
export PATH="$install_dir:$PATH"
sdd --version
```

### 从源码安装

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

安装脚本会先备份已有的全局 `sdd`，再通过 `cargo build --release` 构建并注册全局命令 `sdd`。安装完成时会显示实际命令位置并验证其可运行；若 `PREFIX` 不在 PATH 中会给出提示。安装失败时恢复原版本。项目通过 GitHub Releases 分发预编译二进制，不发布到 crates.io。

卸载：

```bash
bash scripts/uninstall.sh
```

卸载脚本会移除全局 CLI 与构建产物。业务项目中的 `.sdd/` 保存用户规格、任务和归档，不属于安装残留，不会自动删除。

## 快速开始

在需要管理的项目根目录执行：

```bash
sdd init
sdd auto "实现订单取消功能"
```

在 Codex 中可用 `$sdd-harness` 显式启用，也可直接描述软件任务让 Skill 自动匹配；主 Agent 会把只读调用链探索交给 `sdd-explorer`，按复杂度把单个 build 任务交给 `sdd-worker` 或 `sdd-worker-complex`，将高风险架构与并发问题交给 `sdd-architect`，再用 `sdd-reviewer` 独立复核。OMP 中仍可直接描述需求静默触发，或使用 `/sdd 需求` 显式调用。

也可以逐阶段推进：

```bash
sdd new "实现订单取消功能"
sdd design
sdd plan
sdd build next --json
sdd verify
sdd review
sdd archive
```

`sdd auto` 在需要 Agent 编码或用户澄清时暂停，不会绕过交互边界自动修改代码。

首次调用 `sdd new` 或 `sdd auto` 必须携带非空需求；没有需求时，Agent 应先询问用户。不要默认添加 `--non-interactive`，它仅适用于允许需求不完整时直接失败的无人值守流程。若命令进入澄清状态，收集用户回答后使用 `sdd new --answers '<JSON answers>' --json` 继续。

## Agent 构建协议

```bash
# 获取下一个任务及其按需生成的 Context Pack
sdd build next --json

# Agent 完成编码后以内联 JSON 提交 TaskExecutionResult
sdd build complete \
  --task TASK-001-RED \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

Core 会验证任务状态、允许/禁止文件、实际 Git delta、TDD evidence、verification 是否不重不漏、用户可见结果、验收条件和测试接缝；内联任务结果上限为 4 MiB。Agent 不应直接修改 `.sdd/runtime.json`。

## 工作流程

```text
NOT_INITIALIZED → INITIALIZING → INDEX_READY → NEW_STARTED → [CLARIFYING]
→ SPEC_READY → DESIGN_READY → PLAN_READY → BUILD_WAITING_AGENT
↔ BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

主要命令：

| 命令                      | 作用                                        |
| ------------------------- | ------------------------------------------- |
| `sdd init`                | 默认接入 Codex，初始化 `.sdd`、代码库索引和接入文件 |
| `sdd status`              | 查看当前阶段、错误和下一步建议              |
| `sdd new <需求>`          | 澄清需求并生成规格                          |
| `sdd change <id> <需求>`  | 修订当前需求并清除旧的派生设计与计划        |
| `sdd design`              | 生成技术设计                                |
| `sdd plan`                | 生成任务、测试计划和上下文摘要              |
| `sdd build next/complete` | 获取任务或提交 Agent 结果                   |
| `sdd verify`              | 验证规格、任务和证据覆盖                    |
| `sdd review`              | 审查范围、安全、依赖计划、改动规模与债务    |
| `sdd archive`             | 校验并压缩归档，保留简洁性与来源追踪        |
| `sdd auto <需求>`         | 按状态机自动推进流程                        |
| `sdd codebase ...`        | 查看、诊断、查询或重建代码库索引            |

完整参数见 [CLI 命令参考](docs/cli-reference.md)。

## 代码库理解与降级

`sdd init` 时自动探测 PATH 中的 `codegraph` 命令并索引 `.codegraph/`。所有 intent 都通过 CodeGraph 查询：

| intent | 主路由 | 兜底 |
| ------ | ------ | ---- |
| 全部 intent | CodeGraph | 文件扫描 |

CodeGraph 不可用、未建立真实索引或返回空/非法文本时，Core 使用 `fallback-file-scan` 受限文件扫描，同时写入诊断并返回 warning；查询不会在索引缺失时启动外部进程，索引成功还要通过目录后置条件。降级不会被静默隐藏。诊断与路由状态可用 `sdd codebase status` / `sdd codebase doctor` 查看。

## 制品结构

`.sdd/` 只保留统一机器事实源和人工审核文档：

```text
.sdd/
├── runtime.json       # 状态、配置、制品、规格、计划、报告、结果、loop、索引和归档
├── runtime.json.bak   # runtime 崩溃恢复备份（不是需求修订历史）
├── runtime.json.sha256 # runtime 损坏检测校验和
├── runtime.json.bak.sha256 # 备份的校验和
└── changes/<change-id>/
    ├── spec.md
    ├── proposal.md    # sdd change 后保留
    ├── design.md
    ├── plan.md
    ├── tasks.md
    ├── verify-report.md
    ├── review-report.md
    └── archive.md     # archive 后仅保留
```

首次 `sdd new` 会从需求文本生成可读的需求词组 change ID；同名变更在已有目录后追加序号。英文词组使用 kebab-case，中文词组保留原文可读性。`sdd change` 直接更新当前 `spec.md`/`proposal.md`，不生成需求级备份或修订目录，Git 是历史来源；位置 change ID 与全局 `--change` 不得同时出现。`build next` 先验证全部 Context Pack 引用制品，再返回内联内容；`build complete --result-json` 以内联 JSON 提交任务结果，不生成结果文件路径。

## 项目结构

| 路径                          | 职责                              |
| ----------------------------- | --------------------------------- |
| `crates/sdd-cli`              | 参数解析和命令路由（bin: sdd）    |
| `crates/sdd-core`             | 状态机、制品、Git、安全与质量门禁 |
| `crates/sdd-core/src/knowledge` | CodeGraph 探测、路由与降级 |
| `assets/adapters/omp` / `codex` | OMP / Codex 的原生 Skill、命令和 subagent 模板（编译期嵌入） |
| `assets/policies`             | 构建阶段按需下发的受控 Policy |
| `schemas`                     | Runtime、状态、规格、任务和报告的 JSON Schema |
| `docs`                        | 当前架构、CLI、状态机、安全与接入文档 |
| `fixtures`                    | 测试样例项目                       |

## 文档

- [CLI 命令参考](docs/cli-reference.md)
- [架构说明](docs/architecture.md)
- [状态机](docs/state-machine.md)
- [安全策略](docs/security.md)
- [Schema](docs/schemas.md)
- [Agent 接入](docs/adapters.md)
- [AI Agent 自举安装](docs/agent-install.md)
- [第三方来源声明](THIRD_PARTY_NOTICES.md)

## 开发与验证

```bash
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

MIT
