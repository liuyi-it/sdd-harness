# sdd-harness

面向 AI Coding Agent 的规格驱动开发（SDD）工程支架。它用统一 CLI 管理需求澄清、设计、任务拆解、构建、验证、审查和归档，并通过状态机、Git 变更事实、安全边界与质量门禁约束 Agent。

## 核心能力

- CLI-first：`sdd` 是唯一确定性入口，Core 是唯一状态与门禁执行层。
- Agent-agnostic：内置 Claude Code、Codex、OpenCode Adapter，并提供通用 Agent 协议。
- 规格与 TDD：Requirement/Scenario 规格模型驱动 RED、GREEN、REFACTOR、VERIFY 任务链。
- 代码库理解：自动探测并索引 GitNexus / CodeGraph 知识图谱，按 intent 路由查询；不可用时显式降级到受限文件扫描。
- 安全可追溯：校验路径、命令、文件范围、Git delta、TDD 证据和敏感信息。
- 最小正确实现：按复用、标准库、平台能力和既有依赖的顺序决策；未计划新增依赖会阻断审查。
- 精简制品：目录按需创建，Context Pack 按任务生成，归档最终收敛为三个文件。

## 环境要求

- Rust 工具链（cargo，edition 2021）
- Git
- GitNexus / CodeGraph CLI（可选；`sdd codebase doctor` 可诊断，缺失时自动降级文件扫描）
  - CodeGraph 可使用独立安装包，无需 Node.js。
  - GitNexus 作为外部 npm CLI 使用时，当前要求 Node.js 22 或更高版本；sdd-harness 自身运行时不依赖 Node.js。
- macOS 或 Windows（Git Bash）

## 安装

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

安装脚本会先备份并清理旧版全局命令，再通过 `cargo build --release` 构建并注册全局命令 `sdd` 与 `sdd-harness`。安装完成时会显示实际命令位置并验证其可运行；若 `PREFIX` 不在 PATH 中会给出提示。安装失败时恢复原版本。项目不发布到 crates.io。

卸载：

```bash
bash scripts/uninstall.sh
```

卸载脚本会移除全局 CLI 与构建产物。业务项目中的 `.sdd/` 保存用户规格、任务和归档，不属于安装残留，不会自动删除。

## 快速开始

在需要管理的项目根目录执行：

```bash
sdd init --agent codex
sdd auto "实现订单取消功能"
```

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

# Agent 完成编码并写出 TaskExecutionResult 后提交
sdd build complete \
  --task TASK-001-RED \
  --result .sdd/runs/<run-id>/tasks/TASK-001-RED.result.json \
  --json
```

Core 会验证任务状态、允许/禁止文件、实际 Git delta、TDD evidence 和 verification。Agent 不应直接修改 `.sdd/state.json`。

## 工作流程

```text
init → new → design → plan → build → verify → review → archive

NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
                → BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

主要命令：

| 命令                      | 作用                                        |
| ------------------------- | ------------------------------------------- |
| `sdd init`                | 初始化 `.sdd/`、代码库索引和 Agent 接入文件 |
| `sdd status`              | 查看当前阶段、错误和下一步建议              |
| `sdd new <需求>`          | 澄清需求并生成规格                          |
| `sdd design`              | 生成技术设计                                |
| `sdd plan`                | 生成任务、测试计划和上下文摘要              |
| `sdd build next/complete` | 获取任务或提交 Agent 结果                   |
| `sdd verify`              | 验证规格、任务和证据覆盖                    |
| `sdd review`              | 审查范围、安全、依赖计划、改动规模与债务    |
| `sdd archive`             | 校验并压缩归档，保留简洁性与来源追踪        |
| `sdd auto <需求>`         | 按状态机自动推进流程                        |
| `sdd codebase ...`        | 查看、诊断、查询或重建代码库索引            |

完整参数见 [CLI 命令参考](docs/CLI.md)。

## 代码库理解与降级

`sdd init` 时自动探测 PATH 中的 `gitnexus` 与 `codegraph` 命令，对可用引擎各自索引（`.gitnexus/`、`.codegraph/` 落在业务项目根）。查询按 intent 路由：

| intent | 主路由 | 兜底 |
| ------ | ------ | ---- |
| impact / context / related-files / tests / routes / architecture | GitNexus | CodeGraph → 文件扫描 |
| explore / callers / callees | CodeGraph | GitNexus → 文件扫描 |

两引擎均不可用时，Core 使用 `fallback-file-scan` 受限文件扫描，同时写入诊断并返回 warning；降级不会被静默隐藏。诊断与路由状态可用 `sdd codebase status` / `sdd codebase doctor` 查看。

CodeGraph 当前以 MIT 许可证发布；GitNexus 当前 npm 包使用 PolyForm Noncommercial 许可证。商业场景启用 GitNexus 前需单独完成许可证评估，也可以只安装 CodeGraph，未覆盖的 intent 会按既定降级链处理。

## 制品结构

`.sdd/` 子目录按实际命令惰性创建。一个变更的主要制品为：

```text
.sdd/
├── state.json
├── artifacts.json
├── config.json
├── changes/<change-id>/
│   ├── spec.md
│   ├── spec.json
│   ├── design.md
│   ├── plan.json
│   ├── verify-report.json
│   └── review-report.json
├── context-packs/<task-id>/context.md
├── runs/<run-id>/tasks/<task-id>.result.json
└── index/knowledge.json
```

执行 `sdd archive` 后，变更目录只保留：

```text
.sdd/changes/<change-id>/
├── archive.json   # 规格、计划、质量与归档摘要
├── archive.md     # 人工可读归档报告
└── .archived      # 完整性标记
```

## 项目结构

| 路径                          | 职责                              |
| ----------------------------- | --------------------------------- |
| `crates/sdd-cli`              | 参数解析和命令路由（bin: sdd）    |
| `crates/sdd-core`             | 状态机、制品、Git、安全与质量门禁 |
| `crates/sdd-core/src/knowledge` | GitNexus/CodeGraph 探测、路由与降级 |
| `assets/adapters`             | 各 Agent 的命令、Skill 或规则模板（编译期嵌入） |
| `vendor`                      | 上游快照（openspec/superpowers）  |
| `fixtures`                    | 测试样例项目                       |

## 文档

- [CLI 命令参考](docs/CLI.md)
- [架构说明](docs/architecture.md)
- [命令与制品契约](docs/command-contract.md)
- [状态机](docs/state-machine.md)
- [安全策略](docs/security.md)
- [Schema](docs/schemas.md)
- [Agent 接入](docs/adapters.md)
- [Rust 转换总体设计](docs/superpowers/specs/2026-08-02-rust-migration-design.md)

## 开发与验证

```bash
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

MIT
