# sdd-harness

面向 Codex 与 Oh My Pi（OMP）的轻量规格驱动开发工具。`sdd` Core 只负责确定性状态、Schema、文件范围、证据和质量门禁；宿主 Agent 基于真实代码生成规格、设计、计划并执行任务。

## 设计原则

- 所有软件变更都有规格，但文档深度和任务数量按复杂度调整。
- 只有一条逐阶段流程：`new → design → plan → build → verify → archive`。
- 不提供 `auto`，不单独提供 `review`；验证与审查合并在 `verify`。
- 不依赖 Superpowers、OpenSpec 或 mattpocock-skills，只吸收“先明确需求、再设计、再计划、证据后完成”的理念。
- 同一项目可同时维护多个 change。阶段命令未指定 change 且只有一个活动任务时可继续；存在多个活动任务时返回 `E_CHANGE_SELECTION_REQUIRED`，宿主必须询问用户，绝不猜测最近任务。
- 规格、设计和计划由宿主 Agent 生成；Core 校验结构、拒绝占位内容、渲染 Markdown 并持久化。
- 一个 build 任务是可独立验收的纵向切片，测试、实现、重构和最终验证是任务内部步骤，不拆成四个流程任务。
- `verify` 失败时最多自动派发一轮受控修复；仍失败则停止，只有用户明确授权才能 `--continue`。

## 安装

### 预编译二进制（推荐）

从 [GitHub Releases](https://github.com/liuyi-it/sdd-harness/releases/latest) 下载对应文件并放入 PATH：

| 平台 | 文件 |
| --- | --- |
| Linux x64 | `sdd-linux-x64` |
| Linux x64（musl/Alpine） | `sdd-linux-x64-musl` |
| macOS Intel | `sdd-macos-x64` |
| macOS Apple Silicon | `sdd-macos-arm64` |
| Windows x64 | `sdd-windows-x64.exe` |

macOS Apple Silicon 示例：

```bash
install_dir="$HOME/.local/bin"
mkdir -p "$install_dir"
curl -fL -o "$install_dir/sdd" \
  https://github.com/liuyi-it/sdd-harness/releases/latest/download/sdd-macos-arm64
chmod +x "$install_dir/sdd"
export PATH="$install_dir:$PATH"
sdd --version
```

从源码安装需要 Rust：

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
cd sdd-harness
bash scripts/install.sh
```

卸载 CLI：`bash scripts/uninstall.sh`。卸载不会删除业务项目的 `.sdd/`。

## 初始化与宿主资产

在业务项目根目录执行：

```bash
sdd init
```

终端直接执行时默认安装 Codex 资产。Codex 会得到 `.agents/skills/sdd-harness` 编排 Skill，以及每个命令独立的 Skill：

```text
sdd-init  sdd-status  sdd-new  sdd-change  sdd-design
sdd-plan  sdd-build   sdd-verify  sdd-archive  sdd-codebase
```

OMP 宿主使用内部参数 `sdd init --host-adapter omp`，安装同一组 11 个 Skill，并注册 `/sdd` 和所有 `/sdd.<command>`。用户无需在初始化时交互选择宿主；宿主适配器负责传入自身标识。

初始化同时探测 CodeGraph。CodeGraph 可用时建立或复用 `.codegraph/` 索引；不可用时显式降级为受限文件扫描。预编译 `sdd` 本身不依赖 Node.js 或 Rust。

## 使用

推荐在 Codex 或 OMP 中直接描述软件需求，由安装的 Skill 逐阶段执行。手工查看流程时可运行：

```bash
sdd new "实现订单取消功能" --json
# 宿主根据 actionRequired 生成规格 JSON 并回传
sdd design --change <change-id> --json
sdd plan --change <change-id> --json
sdd build next --change <change-id> --json
sdd verify --change <change-id> --json
sdd archive --change <change-id> --json
```

`new`、`change`、`design`、`plan` 返回 `AGENT_PHASE_EXECUTION`，其中包含阶段上下文和 resultSchema。宿主完成调查或必要提问后，用同一命令的 `--result-json` 回传。

`build next` 返回 `AGENT_TASK_EXECUTION`。宿主只能修改 `allowedFiles`，执行全部 `verification`，然后提交：

```bash
sdd build complete \
  --change <change-id> \
  --task TASK-001 \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

`verify` 统一执行规格/场景覆盖、任务证据、Git 实际范围、敏感信息和依赖计划检查。若返回 `AGENT_FIX_EXECUTION`，宿主修复并以 `sdd verify --result-json` 回传；首轮后仍失败时必须询问用户。

## 多任务选择

```bash
sdd status --json
```

`status` 的 `activeChanges` 会列出所有未归档任务。读取状态不需要选择；`codebase` 也是项目级命令。其他阶段命令在多个活动任务下必须显式传入 `--change <id>`。宿主 Skill 需要先把候选任务的标题和阶段转换为简短中文，再询问用户选择。

## 状态与制品

```text
INDEX_READY
  └─ SPEC_WAITING_AGENT → SPEC_READY
       → DESIGN_WAITING_AGENT → DESIGN_READY
       → PLAN_WAITING_AGENT → PLAN_READY
       → BUILD_WAITING_AGENT ↔ PLAN_READY → BUILD_READY
       → QUALITY_WAITING_FIX | QUALITY_BLOCKED | QUALITY_READY
       → ARCHIVED
```

每个 change 都有独立 workflow、run、任务状态和质量修复轮次。项目级 state 只保存初始化与代码库索引信息。

```text
.sdd/
├── runtime.json
├── runtime.json.sha256
├── runtime.json.bak
├── runtime.json.bak.sha256
└── changes/<change-id>/
    ├── spec.md
    ├── design.md
    ├── plan.md
    ├── tasks.md
    ├── quality-report.md
    └── archive.md       # 归档后仅保留该文件
```

v0.6 不迁移旧 `.sdd`。检测到旧 runtime schema 时会直接返回 `E_STATE_VERSION_UNSUPPORTED`；请先备份并删除旧 `.sdd`，再重新执行 `sdd init`。

## 命令

| 命令 | 作用 |
| --- | --- |
| `sdd init` | 初始化 runtime、代码库索引和宿主资产 |
| `sdd status` | 查看项目状态和所有活动任务 |
| `sdd new <需求>` | 创建 change 并进入规格生成 |
| `sdd change <新需求> --change <id>` | 修订需求并作废派生制品 |
| `sdd design` | 生成技术设计 |
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
| `assets/adapters/codex` | Codex 编排及全命令 Skill |
| `assets/adapters/omp` | OMP 编排、全命令 Skill 与 slash 命令 |
| `assets/policies` | build 阶段下发的受控 Policy |
| `schemas` | runtime、阶段结果、任务、修复和报告 Schema |
| `docs` | 架构、CLI、状态机、安全、Schema 与宿主接入 |

## 文档

- [CLI 命令参考](docs/cli-reference.md)
- [架构](docs/architecture.md)
- [状态机](docs/state-machine.md)
- [安全](docs/security.md)
- [Schema](docs/schemas.md)
- [Agent 接入](docs/adapters.md)
- [AI Agent 自举安装](docs/agent-install.md)
- [第三方工具声明](THIRD_PARTY_NOTICES.md)

## 开发验证

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

MIT
