# sdd-harness Rust 转换总体设计与规划

日期：2026-08-02
状态：已批准（用户逐轮确认：三个关键决策 + 方案 C）
范围：整体转换为 Rust 项目，移除 codebase-memory-mcp，改用 GitNexus 与 CodeGraph

## 1. 背景与北极星

sdd-harness 是面向 AI Coding Agent 的规格驱动开发（SDD）工程支架：以统一 CLI 管理需求澄清、设计、任务拆解、构建、验证、审查和归档，通过状态机、Git 变更事实、安全边界与质量门禁约束 Agent。

北极星（长期不变）：**CLI-first 的确定性入口 + 唯一状态与门禁执行层 + Agent-agnostic**。Rust 转换不改变北极星，只改变实现载体。

## 2. 转换目标（固定规划）

1. **载体转换**：Node.js workspaces（9 包，约 3 万行 TS）整体转换为 Rust Cargo workspace；Node 代码与工具链完全移除，仓库变为纯 Rust 项目。
2. **代码库理解替换**：移除 codebase-memory-mcp 相关全部内容（托管生命周期、pinned dependency、探测安装、schema、文档引用），改用 GitNexus 与 CodeGraph 双引擎按 intent 路由，均不可用时降级受限文件扫描。
3. **契约稳定**：对外命令集、子命令、`--json` 输出结构、退出码映射、`E_*` 错误码体系保持稳定（适配器模板与 Agent 协议依赖它们）；内部存储格式允许重构。
4. **能力对齐**：状态机、质量门禁、安全校验、引擎（spec/openspec/tdd/superpowers）、Git 检查与 worktree 隔离、loop 自动流程全部平移至 Rust。
5. **质量不降**：单元测试 + fixtures 集成测试覆盖对齐原 Vitest 测试；`npm run typecheck/lint/test` 等价物为 `cargo check` / `cargo clippy` / `cargo test`。
6. **审核与交付**：整体完成后使用 open-code-review-delegate 审核代码，问题修复后提交 git，**不推送 GitHub**。

## 3. 已确认决策

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | Node 版处置 | 完全移除（纯 Rust 项目；适配器模板与 vendor 快照作为静态资产保留在仓库） |
| 2 | GitNexus/CodeGraph 分工 | 双引擎按 intent 路由；初始化探测并索引两者；均不可用时降级受限文件扫描 |
| 3 | 存储兼容性 | 允许重构 `.sdd/` 目录与 JSON schema（仅保证状态机语义一致），CLI 对外契约保持稳定 |
| 4 | 项目形态 | 方案 C：Cargo workspace 双 crate（sdd-core lib + sdd-cli bin）+ assets/ 资产目录 |

## 4. 目标架构

```text
Agent Adapter ──> sdd-cli (bin, clap) ──> sdd-core (lib)
                                             ├── commands   11 个命令实现
                                             ├── state      状态机 + 存储 + 文件锁
                                             ├── quality    质量门禁（verify/review/tdd/minimality…）
                                             ├── security   路径安全/密钥扫描/任务范围/不可信内容
                                             ├── git        git 检查 + worktree 隔离
                                             ├── engines    spec / openspec / tdd / superpowers
                                             ├── protocol   Agent Task Protocol 类型与校验
                                             ├── policies   策略解析（compiler/resolver/digest）
                                             └── knowledge  GitNexus + CodeGraph 适配（新）
```

```text
sdd-harness/
├── Cargo.toml            # workspace（成员：crates/sdd-core、crates/sdd-cli）
├── crates/
│   ├── sdd-core/         # lib：全部领域逻辑，模块见上
│   └── sdd-cli/          # bin：clap 参数解析 + CommandResult 渲染
├── assets/               # adapter 模板（claude/codex/opencode/generic）、vendor 快照
├── fixtures/             # 测试样例项目（原样保留）
├── schemas/              # 精简后的 JSON schema（5 个）
├── tests/                # 集成测试
└── docs/                 # 架构、命令契约、安全说明（更新为 Rust 版）
```

### 原 9 包映射

| 原包 | 去向 |
|------|------|
| @sdd-harness/core | crates/sdd-core（commands/state/quality/security/git/engines） |
| @sdd-harness/cli | crates/sdd-cli |
| @sdd-harness/agent-protocol | crates/sdd-core/src/protocol/ |
| @sdd-harness/agent-policies | crates/sdd-core/src/policies/ |
| @sdd-harness/codebase-memory | crates/sdd-core/src/knowledge/（重写） |
| @sdd-harness/core 的 codebase/ | crates/sdd-core/src/knowledge/（重写） |
| claude-code-adapter / codex-adapter / opencode-adapter / generic-agent-adapter | assets/（原样保留模板文件） |

## 5. 技术选型

- CLI 解析：`clap`（derive）
- JSON：`serde` + `serde_json`
- 错误：统一 `Result<T, SddError>`，`SddError { code: E_*，message，suggestion }`，退出码映射与 Node 版一致
- 无异步运行时：`std::process::Command` 调用 gitnexus/codegraph/git，`std::fs` 文件操作（原 MCP 通信整个移除，tokio 无收益）
- 测试：crate 内 `#[cfg(test)]` 单元测试 + `tests/` 集成测试（fixtures 驱动完整工作流）
- 跨平台：macOS + Windows（Git Bash）支持保留
- 不发布 crates.io（与"不发布 npm"一致）

## 6. 知识图谱适配层（knowledge/）

```rust
trait KnowledgeProvider {
    fn probe(&self) -> ProbeResult;               // PATH 探测命令可用性
    fn index(&self, root) -> IndexResult;         // gitnexus analyze / codegraph init
    fn query(&self, root, intent) -> QueryResult; // 按 intent 路由
}
```

- 探测：在 PATH 中查找 `gitnexus` / `codegraph` 可执行文件
- 索引：`sdd init` 时对可用引擎各自索引（`gitnexus analyze`、`codegraph init`）；失败只写诊断，不阻断初始化
- 路由表（intent 沿用现有 8 个）：

| intent | 主路由 | 兜底 |
|---|---|---|
| impact 影响面 | `gitnexus impact` | `codegraph impact` → 文件扫描 |
| context 符号 360° | `gitnexus context` | `codegraph explore` → 文件扫描 |
| explore 符号源码+调用路径 | `codegraph explore` | `gitnexus query` → 文件扫描 |
| callers / callees | `codegraph callers/callees` | `gitnexus query` → 文件扫描 |
| related-files / architecture / tests / routes | `gitnexus query` / `codegraph query` | 文件扫描 |

- 降级链：GitNexus → CodeGraph → 受限文件扫描（保留 fallback-file-scan 语义，`degraded=true` 显式暴露，诊断不静默）
- `sdd codebase status/doctor/index/query/rebuild` 子命令保留，输出两引擎 installed/indexed 状态
- 索引产物 `.gitnexus/`、`.codegraph/` 落在业务项目根；诊断写入 `.sdd/index/`

## 7. 状态存储重构与契约边界

### 存储（`.sdd/` 保留，JSON 格式，schema 从 11 个精简为 5 个）

```text
.sdd/
├── state.json          # 状态机事实源（phase、change、failedCommand、恢复信息）
├── artifacts.json      # 制品输入摘要与内容哈希
├── changes/<change-id>/ # spec.json、design.md、plan.json、report 等
├── runs/<run-id>/tasks/ # 任务结果
├── loop/               # 自动流程规格与运行记录
└── index/              # 两引擎诊断 + capabilities
```

5 个 schema：`state`、`artifact`、`task`、`task-result`、`report`（review/verify 报告合并；loop 并入 state/artifact）。

原子写入语义保留：临时文件 + rename + 备份恢复；`.sdd/lock` 文件锁保留。

### 对外契约（保持稳定，不随存储重构）

- 命令全集：`init / auto / new / design / plan / build / verify / review / archive / status / codebase`
- 子命令：`sdd codebase status/doctor/index/query/rebuild`
- `--json` 输出结构、退出码映射、`E_*` 错误码体系
- 阶段枚举：22 个 phase（NOT_INITIALIZED → ARCHIVED）

## 8. 错误处理、测试与安装

### 错误处理

统一 `Result<T, SddError>`；错误码 → 退出码映射与 Node 版一致（如 E_SECURITY_BLOCKED=10、E_VERIFY_FAILED=7、E_LOCK_TIMEOUT=9 等）；错误路径同步实现，不允许 panic 逃逸到 CLI；写命令仍走 `.sdd/lock` 锁。

### 测试

- 单元测试：状态机、质量门禁、安全校验、schema 校验、协议校验
- 集成测试：fixtures 跑完整工作流（init → new → design → plan → build → verify → review → archive），对齐原 validate-schemas.mjs 的 e2e 覆盖
- 知识图谱：mock provider（伪造 CLI stdout 解析）单元测试；真实命令探测的可选 ignore 测试
- 契约测试：CLI 输出快照

### 安装

`cargo build --release` 产出 `sdd` 二进制；`install.sh` 改为检测/注册二进制（macOS/Windows 双平台）；卸载脚本 `uninstall.sh` 保留；不发布 crates.io。

## 9. 迁移路线图

1. **骨架**：Cargo workspace + 双 crate + clap 命令解析 + 契约框架（错误码/退出码/命令注册）
2. **状态层**：state.json、文件锁、原子写入、schema 校验
3. **基础命令**：init / status / new（spec engine 澄清链）
4. **知识图谱**：knowledge 模块（探测/索引/路由/降级）+ `sdd codebase` 命令
5. **规划链**：design / plan（openspec 解析、tdd-engine、context-pack）
6. **构建链**：build（RED/GREEN/REFACTOR/VERIFY）、task-executor、git inspector
7. **质量链**：verify / review / archive（tdd-evidence、minimality、traceability）
8. **引擎与循环**：auto、superpowers planner、loop 引擎
9. **资产与写入**：adapter 模板复制（init 写入项目）、vendor 快照保留
10. **清理与验证**：移除全部 Node 残留（package.json/tsconfig/node_modules/scripts/*.mjs、codebase-memory-mcp 全部引用）、更新 README/docs/AGENTS.md/CLAUDE.md、全量 `cargo test` + clippy
11. **审核与交付**：open-code-review-delegate 审核 → 修复问题 → git 提交（中文信息）→ 不推送

## 10. 完成标准

- [ ] 仓库无 Node 残留：无 package.json/tsconfig.json/node_modules/JS/TS 源码
- [ ] `cargo build --release`、`cargo test`、`cargo clippy` 全绿
- [ ] 11 个命令 + codebase 子命令可用，退出码/错误码对齐原契约
- [ ] codebase-memory-mcp 全部引用移除；GitNexus/CodeGraph 探测、索引、intent 路由、降级链实现并测试
- [ ] fixtures 完整工作流集成测试通过
- [ ] 文档（README/docs/AGENTS.md/CLAUDE.md）更新为 Rust 项目表述
- [ ] open-code-review-delegate 审核通过（无未修复问题）
- [ ] git 提交完成，未推送 GitHub

## 11. 非目标

- 不做 MCP server 模式（codebase-memory-mcp 托管形态整体移除，知识图谱经子进程 CLI 调用）
- 不发布 crates.io / npm
- 不改变 Agent Task Protocol 的语义（类型定义平移，契约不变）
- 不迁移 vendor 快照内容（openspec/superpowers/mattpocock-skills 原样保留）
