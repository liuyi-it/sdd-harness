# sdd-harness Rust 转换实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Node.js workspaces 项目（9 包，~3 万行 TS）整体转换为 Rust Cargo workspace（sdd-core lib + sdd-cli bin），移除 codebase-memory-mcp，改用 GitNexus + CodeGraph 双引擎按 intent 路由，保持 CLI 对外契约稳定，最终经 open-code-review-delegate 审核后提交（不推送 GitHub）。

**Architecture:** 双 crate workspace：`sdd-core`（lib，全部领域逻辑：commands/state/quality/security/git/engines/protocol/policies/knowledge）+ `sdd-cli`（bin，clap 解析 + 渲染）。知识图谱经 `std::process::Command` 子进程调用 gitnexus/codegraph CLI，无异步运行时。`.sdd/` 目录保留，schema 精简为 5 个。适配器模板与 vendor 快照保留为 assets/ 文件。

**Tech Stack:** Rust（edition 2021）、clap（derive）、serde + serde_json、std::process（子进程）、std::fs（文件操作）；dev 依赖：tempfile（测试临时目录）。

**执行方式：** 转换期间 Node 版 TS 源码**保留**在仓库中作为翻译事实来源（每个任务的"翻译自"指明对应源文件），全部转换完成后（T22）统一删除。每个任务独立可测、独立 commit。

## Global Constraints

- Rust edition 2021，2 空格缩进，`cargo fmt` / `cargo clippy` 零告警
- 运行时依赖仅限：clap、serde、serde_json；dev 依赖仅限：tempfile、serde_json（测试用）
- 全项目中文化：注释、用户可见消息用中文；标识符/错误码（`E_*`）/命令字面量保持英文
- 对外契约保持稳定：11 个命令（init/auto/new/design/plan/build/verify/review/archive/status/codebase）、codebase 5 子命令、`--json` 输出结构、退出码映射、`E_*` 错误码
- **契约变更点（唯一允许）**：`AgentActionRequired.codebase.provider` 枚举值由 `"codebase-memory-mcp"` 改为 `"gitnexus" | "codegraph" | "fallback-file-scan"`（本计划 T13）
- git commit 信息使用中文
- 每个任务结束：`cargo test` 通过 + git commit

---

### Task 1: Cargo workspace 骨架与命令框架

**Files:**
- Create: `Cargo.toml`（workspace）
- Create: `crates/sdd-core/Cargo.toml`、`crates/sdd-core/src/lib.rs`
- Create: `crates/sdd-cli/Cargo.toml`、`crates/sdd-cli/src/main.rs`
- Create: `crates/sdd-core/src/contracts.rs`、`crates/sdd-core/src/error.rs`
- Create: `.gitignore`（追加 target/）
- Test: `crates/sdd-core/tests/smoke.rs`、`crates/sdd-cli/tests/cli_smoke.rs`

**Interfaces:**
- Produces: `crates/sdd-core::contracts::{CommandName, CommandRequest, CommandResult, PHASES}`；`crates/sdd-core::error::{SddError, ErrorCode, error_exit_codes()}`；`sdd-core::run(request) -> CommandResult`（后续所有命令经此分发）

- [ ] **Step 1: 创建 workspace 与 crate 骨架**

根 `Cargo.toml`：
```toml
[workspace]
resolver = "2"
members = ["crates/sdd-core", "crates/sdd-cli"]

[workspace.package]
version = "0.2.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
```

`crates/sdd-core/Cargo.toml`：
```toml
[package]
name = "sdd-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

`crates/sdd-cli/Cargo.toml`：
```toml
[package]
name = "sdd-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "sdd"
path = "src/main.rs"

[dependencies]
clap = { workspace = true }
serde_json = { workspace = true }
sdd-core = { path = "../sdd-core" }
```

- [ ] **Step 2: 契约层 contracts.rs**

翻译自 `packages/core/src/contracts.ts`（COMMANDS/PHASES/ERROR_EXIT_CODES/CommandRequest/CommandResult/ExitCode 保留原值）。关键结构：
```rust
pub const COMMANDS: [&str; 11] = [
    "init", "auto", "new", "design", "plan", "build",
    "verify", "review", "archive", "status", "codebase",
];

pub const PHASES: [&str; 22] = [
    "NOT_INITIALIZED", "INITIALIZING", "INDEXING", "INDEX_READY",
    "NEW_STARTED", "CLARIFYING", "SPEC_READY", "DESIGNING", "DESIGN_READY",
    "PLANNING", "PLAN_READY", "BUILDING", "BUILD_WAITING_AGENT", "BUILD_READY",
    "VERIFYING", "VERIFY_READY", "REVIEWING", "REVIEW_READY",
    "ARCHIVING", "ARCHIVED", "FAILED", "PAUSED",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandRequest {
    pub command: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CommandResult {
    pub ok: bool,
    pub state: String,
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}
```
`CliWarning`、`AgentActionRequired`、`PolicyBundle` 在 T13 完整化（build 命令任务），此处定义 `AgentActionRequired` 占位结构以保持 lib.rs 可编译。

`error.rs` 翻译自 `packages/core/src/errors.ts` + contracts.ts 的 `ERROR_EXIT_CODES`（30+ 错误码与退出码值逐字保留）：
```rust
use crate::contracts::CommandError;

pub struct SddError {
    pub code: String,
    pub message: String,
    pub next: Option<String>,
    pub exit_code: i32,
}

impl SddError {
    pub fn new(code: &str, message: &str) -> Self {
        Self { code: code.to_string(), message: message.to_string(),
               next: None, exit_code: error_exit_codes().get(code).copied().unwrap_or(1) }
    }
    pub fn with_next(mut self, next: &str) -> Self { self.next = Some(next.to_string()); self }
    pub fn to_command_error(&self) -> CommandError { /* code/message/next */ }
}

impl std::fmt::Display for SddError { /* 输出 message */ }
impl std::error::Error for SddError {}

pub fn error_exit_codes() -> std::collections::HashMap<&'static str, i32> {
    [("E_NOT_INITIALIZED", 3), ("E_INVALID_PHASE_COMMAND", 3), ("E_ACTIVE_CHANGE_EXISTS", 3),
     ("E_MISSING_CHANGE", 4), ("E_MISSING_ARTIFACT", 4), ("E_INDEX_NOT_READY", 5),
     ("E_COMPONENT_UNAVAILABLE", 5), ("E_COMPONENT_INTEGRITY_FAILED", 10),
     ("E_DEGRADED_MODE", 0), ("E_UNRESOLVED_BLOCKER", 6), ("E_VERIFY_REQUIRED", 3),
     ("E_REVIEW_REQUIRED", 3), ("E_VERIFY_FAILED", 7), ("E_TDD_EVIDENCE_REQUIRED", 7),
     ("E_AGENT_TASK_FAILED", 7), ("E_UNDECLARED_FILE_CHANGE", 10), ("E_REVIEW_FAILED", 8),
     ("E_UNPLANNED_DEPENDENCY", 8), ("E_ARCHIVED_READONLY", 3), ("E_CONCURRENT_RUN", 9),
     ("E_LOCK_TIMEOUT", 9), ("E_TIMEOUT", 124), ("E_INTERRUPTED", 130),
     ("E_STATE_CORRUPTED", 1), ("E_SECURITY_BLOCKED", 10), ("E_PATH_OUTSIDE_REPO", 10),
     ("E_SYMLINK_BLOCKED", 10), ("E_PARALLEL_FILE_CONFLICT", 3)]
    .into_iter().collect()
}
```
（对照 `packages/core/src/contracts.ts` 的 ERROR_EXIT_CODES 全表逐一核对，包括 `E_STATE_CORRUPTED` 等全部条目，遗漏=契约违反。）

- [ ] **Step 3: lib.rs 分发入口（翻译自 core.ts 的 Core.execute 骨架，仅 init/status 之外抛 E_NOT_INITIALIZED/E_INVALID_PHASE_COMMAND）**

```rust
pub mod contracts;
pub mod error;

use contracts::{CommandRequest, CommandResult};
use error::SddError;

pub fn run(request: &CommandRequest) -> Result<CommandResult, SddError> {
    match request.command.as_str() {
        "status" => Err(SddError::new("E_NOT_INITIALIZED", "状态命令将在 Task 5 实现")),
        _ => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("命令 {} 在状态 NOT_INITIALIZED 下不可用", request.command),
        )),
    }
}
```

- [ ] **Step 4: 失败测试——CLI 集成测试**

`crates/sdd-cli/tests/cli_smoke.rs`：
```rust
use std::process::Command;

fn sdd() -> Command { Command::new(env!("CARGO_BIN_EXE_sdd")) }

#[test]
fn unknown_command_exits_with_code_2() {
    let out = sdd().arg("not-a-command").output().unwrap();
    assert_eq!(out.status.code(), Some(2)); // clap 默认 invalid args
}

#[test]
fn build_next_on_uninitialized_reports_code() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd().current_dir(dir.path()).args(["build", "next"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    // 骨架阶段允许 stderr 提示未实现，但退出码必须非 0
    assert_ne!(out.status.code(), Some(0));
    let _ = stderr;
}
```

- [ ] **Step 5: 实现 CLI main.rs（clap 命令注册 + 分发到 sdd-core::run + 退出码）**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sdd", about = "面向 AI Coding Agent 的规格驱动开发（SDD）工程支架")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init, Auto, New { requirement: Option<String> }, Design, Plan,
    Build { sub: Option<String> }, Verify, Review, Archive, Status, Codebase,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let request = sdd_core::contracts::CommandRequest {
        command: cli_command_name(&cli.command).to_string(),
        cwd: std::env::current_dir().unwrap().to_string_lossy().to_string(),
        args: None,
    };
    match sdd_core::run(&request) {
        Ok(result) => std::process::ExitCode::from(result.exit_code as u8),
        Err(e) => { eprintln!("{}", e.message); std::process::ExitCode::from(e.exit_code as u8) }
    }
}

fn cli_command_name(c: &Command) -> &'static str {
    match c {
        Command::Init => "init", Command::Auto => "auto", Command::New { .. } => "new",
        Command::Design => "design", Command::Plan => "plan", Command::Build { .. } => "build",
        Command::Verify => "verify", Command::Review => "review", Command::Archive => "archive",
        Command::Status => "status", Command::Codebase => "codebase",
    }
}
```

- [ ] **Step 6: 运行验证**

```bash
cargo build --workspace && cargo test --workspace
```
Expected: 编译通过，`unknown_command_exits_with_code_2` PASS（退出码 2），`build_next_on_uninitialized_reports_code` PASS。

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/ .gitignore
git commit -m "feat: 搭建 Rust workspace 骨架（sdd-core lib + sdd-cli bin）"
```

---

### Task 2: 状态存储层（state.json + 文件锁 + 原子写入）

**Files:**
- Create: `crates/sdd-core/src/state/mod.rs`、`crates/sdd-core/src/state/state_store.rs`、`crates/sdd-core/src/state/file_lock.rs`
- Test: `crates/sdd-core/tests/state_store.rs`

**Interfaces:**
- Consumes: T1 `contracts::{PHASES}`、`error::SddError`
- Produces: `state::WorkflowState`（serde 结构）、`state::StateStore::new(cwd) -> Self`、`StateStore::read() -> Result<WorkflowState, SddError>`、`StateStore::write(&self, &WorkflowState)`、`state::lock_sdd(cwd) -> Result<SddLockGuard, SddError>`（写命令入口调用）

**翻译自：** `packages/core/src/state/state-store.ts`（WorkflowState 字段：phase/change/activeChange/failedCommand/previousPhase/inProgressPhase/next/updatedAt 等，见该文件 `WorkflowState` 接口与 `packages/core/src/state/schema-migration.ts`）、`packages/core/src/state/file-lock.ts`。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/state_store.rs`：
```rust
use sdd_core::state::{StateStore, WorkflowState, lock_sdd};

#[test]
fn read_missing_state_returns_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let state = store.read().unwrap();
    assert_eq!(state.phase, "NOT_INITIALIZED");
}

#[test]
fn write_then_read_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    store.write(&WorkflowState { phase: "INDEX_READY".into(), change_id: None }).unwrap();
    let state = store.read().unwrap();
    assert_eq!(state.phase, "INDEX_READY");
}

#[test]
fn lock_is_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    let _guard = lock_sdd(&path).unwrap();
    assert!(lock_sdd(&path).is_err()); // 第二次获取必须失败
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test state_store
```
Expected: FAIL（模块不存在编译错误）。

- [ ] **Step 3: 实现 WorkflowState 与 StateStore**

`state/mod.rs`：
```rust
pub mod file_lock;
pub mod state_store;

pub use file_lock::{lock_sdd, SddLockGuard};
pub use state_store::{StateStore, WorkflowState};
```

`state/state_store.rs`（翻译自 state-store.ts 的 read/write 语义；原子写入=临时文件+rename）：
```rust
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::SddError;

pub const SDD_DIR: &str = ".sdd";
pub const STATE_FILE: &str = "state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WorkflowState {
    #[serde(default = "default_phase")]
    pub phase: String,
    #[serde(default)]
    pub change_id: Option<String>,
}

fn default_phase() -> String { "NOT_INITIALIZED".to_string() }

pub struct StateStore {
    root: PathBuf,
}

impl StateStore {
    pub fn new(cwd: String) -> Self { Self { root: PathBuf::from(cwd) } }
    pub fn state_path(&self) -> PathBuf { self.root.join(SDD_DIR).join(STATE_FILE) }

    pub fn read(&self) -> Result<WorkflowState, SddError> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(WorkflowState::default());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取状态文件失败：{e}")))?;
        serde_json::from_str(&raw)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("状态文件 JSON 解析失败：{e}")))
    }

    pub fn write(&self, state: &WorkflowState) -> Result<(), SddError> {
        let dir = self.root.join(SDD_DIR);
        fs::create_dir_all(&dir)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{e}")))?;
        let path = self.state_path();
        let tmp = dir.join("state.json.tmp");
        let content = serde_json::to_string_pretty(state).unwrap();
        fs::write(&tmp, content)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入临时状态失败：{e}")))?;
        fs::rename(&tmp, &path)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("提交状态文件失败：{e}")))
    }
}
```

`state/file_lock.rs`（翻译自 file-lock.ts：`.sdd/lock` 文件 + 原子创建；Node 版用同步文件描述符 + 超时，Rust 版用 `OpenOptions::create_new`）：
```rust
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use crate::error::SddError;

const LOCK_FILE: &str = "lock";
const LOCK_TIMEOUT_MS: u64 = 10_000;

pub struct SddLockGuard { path: PathBuf }

pub fn lock_sdd(cwd: &str) -> Result<SddLockGuard, SddError> {
    let dir = PathBuf::from(cwd).join(".sdd");
    fs::create_dir_all(&dir).map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{e}")))?;
    let path = dir.join(LOCK_FILE);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(LOCK_TIMEOUT_MS);
    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(SddLockGuard { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if std::time::Instant::now() > deadline {
                    return Err(SddError::new("E_LOCK_TIMEOUT", "等待 .sdd/lock 超时，可能有其他命令正在运行"));
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(SddError::new("E_STATE_CORRUPTED", &format!("获取锁失败：{e}"))),
        }
    }
}

impl Drop for SddLockGuard {
    fn drop(&mut self) { let _ = fs::remove_file(&self.path); }
}
```

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test state_store
```
Expected: 3 个测试全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/state/ crates/sdd-core/tests/state_store.rs
git commit -m "feat: 实现状态存储层（state.json 原子写入与文件锁）"
```

---

### Task 3: JSON Schema 校验与 5 个 schema 精简

**Files:**
- Create: `schemas/state.schema.json`、`schemas/task.schema.json`、`schemas/task-result.schema.json`、`schemas/report.schema.json`、`schemas/artifact.schema.json`
- Create: `crates/sdd-core/src/schema/mod.rs`、`crates/sdd-core/src/schema/validator.rs`
- Test: `crates/sdd-core/tests/schema_validator.rs`
- Delete: 原 `schemas/*.schema.json` 11 个（由本任务 5 个替代）

**Interfaces:**
- Consumes: T2 `WorkflowState`
- Produces: `schema::validate_json(&str, &serde_json::Value) -> Result<(), SddError>`（校验失败返回 E_STATE_CORRUPTED）、`schema::SCHEMAS: [(&str, &str); 5]`（名称→内嵌 schema JSON）

**翻译自：** 原 `schemas/` 11 个 schema（state/task/task-execution-result/artifact-metadata/review-report/verify-report/loop/loop-run/config/mcp-query-result/review-issue）——合并原则：review-report+verify-report+review-issue→report；loop+loop-run 并入 state/artifact；task-execution-result→task-result；mcp-query-result 删除（MCP 移除，改由 knowledge 诊断结构替代）；config 并入 state。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/schema_validator.rs`：
```rust
use serde_json::json;
use sdd_core::schema::validate_json;

#[test]
fn valid_state_passes() {
    let doc = json!({ "phase": "INDEX_READY", "changeId": null });
    assert!(validate_json("state", &doc).is_ok());
}

#[test]
fn invalid_state_rejected() {
    // phase 必须是枚举值之一
    let doc = json!({ "phase": "NOT_A_PHASE" });
    assert!(validate_json("state", &doc).is_err());
}

#[test]
fn all_five_schemas_registered() {
    assert_eq!(sdd_core::schema::SCHEMAS.len(), 5);
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test schema_validator
```
Expected: FAIL（模块不存在）。

- [ ] **Step 3: 写 5 个 schema 与内嵌校验器**

`schemas/state.schema.json`（精简自原 state.schema.json；`phase` 用 PHASES 枚举 + `default:"NOT_INITIALIZED"`）：
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://sdd-harness.dev/schemas/state.schema.json",
  "title": "SDD 状态事实源",
  "type": "object",
  "required": ["phase"],
  "properties": {
    "phase": {
      "type": "string",
      "enum": ["NOT_INITIALIZED","INITIALIZING","INDEXING","INDEX_READY","NEW_STARTED","CLARIFYING","SPEC_READY","DESIGNING","DESIGN_READY","PLANNING","PLAN_READY","BUILDING","BUILD_WAITING_AGENT","BUILD_READY","VERIFYING","VERIFY_READY","REVIEWING","REVIEW_READY","ARCHIVING","ARCHIVED","FAILED","PAUSED"]
    },
    "changeId": { "type": ["string", "null"] }
  },
  "additionalProperties": true
}
```
`task.schema.json`（task id/标题/阶段 RED|GREEN|REFACTOR|VERIFY/状态/允许文件/禁止文件）、`task-result.schema.json`（taskId/status/evidence/verification/filesChanged）、`report.schema.json`（kind: verify|review、summary、issues[]、passed 布尔）、`artifact.schema.json`（type/hash/contentPath）——各自参照原 schema 的必填与类型，字段名沿用原 camelCase。

`crates/sdd-core/src/schema/validator.rs`（内嵌最小校验器——不引入外部 schema 校验 crate，检查：type、required、enum、properties；翻译自 `scripts/validate-schemas.mjs` 中内联 validator 的语义）：
```rust
pub const SCHEMAS: [(&str, &str); 5] = [
    ("state", include_str!("../../../schemas/state.schema.json")),
    ("task", include_str!("../../../schemas/task.schema.json")),
    ("task-result", include_str!("../../../schemas/task-result.schema.json")),
    ("report", include_str!("../../../schemas/report.schema.json")),
    ("artifact", include_str!("../../../schemas/artifact.schema.json")),
];

pub fn validate_json(name: &str, doc: &serde_json::Value) -> Result<(), SddError> {
    let (_, raw) = SCHEMAS.iter().find(|(n, _)| *n == name)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("未知 schema：{name}")))?;
    let schema: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("schema 解析失败：{e}")))?;
    // 检查 required + enum（type 由调用方保证）：
    // 1. schema.required 中的字段必须存在于 doc
    // 2. 对 properties 中带 enum 的字段，doc 对应值必须命中枚举
    // 3. properties 中 type=object 的字段递归校验（最多 3 层）
    // 完整实现见 validate-schemas.mjs 的 validateAgainstSchema 函数语义
    todo_check_against_schema(&schema, doc, name)
}

fn todo_check_against_schema(_s: &serde_json::Value, _d: &serde_json::Value, _n: &str) -> Result<(), SddError> {
    Ok(())
}
```
> 注意：`todo_check_against_schema` 需要按 validate-schemas.mjs 的内联 validator 实现 required/enum/type 检查后替换；本任务实现到 `invalid_state_rejected` 可通过即可（enum 检查是核心，必须真实实现，禁止保留 todo）。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test schema_validator
```
Expected: 3 个测试 PASS（enum 检查真实实现）。

- [ ] **Step 5: 删除原 11 个 schema 并提交**

```bash
git rm schemas/artifact-metadata.schema.json schemas/config.schema.json schemas/loop-run.schema.json schemas/loop.schema.json schemas/mcp-query-result.schema.json schemas/review-issue.schema.json schemas/review-report.schema.json schemas/state.schema.json schemas/task-execution-result.schema.json schemas/task.schema.json schemas/verify-report.schema.json
git add schemas/ crates/sdd-core/src/schema/ crates/sdd-core/tests/schema_validator.rs
git commit -m "feat: 精简 JSON schema 为 5 个并实现内嵌校验器"
```

---

### Task 4: init 与 status 命令（含 .sdd 初始化）

**Files:**
- Create: `crates/sdd-core/src/commands/mod.rs`、`crates/sdd-core/src/commands/init.rs`、`crates/sdd-core/src/commands/status.rs`
- Modify: `crates/sdd-core/src/lib.rs`（注册 commands 模块）
- Test: `crates/sdd-core/tests/init_status.rs`

**Interfaces:**
- Consumes: T2 `StateStore`/`lock_sdd`、T1 `CommandResult`
- Produces: `commands::init::run_init(cwd) -> Result<CommandResult, SddError>`（创建 .sdd/、写 state.json phase=INDEX_READY——知识图谱索引部分 T7 接入）、`commands::status::run_status(cwd) -> Result<StatusInfo, SddError>`（`StatusInfo { state: String, next: Option<String> }`）
- `lib.rs` 的 `run()` 分发：`"init"` → run_init、`"status"` → run_status 返回 CommandResult{ok:true,state,exit_code:0}

**翻译自：** `packages/core/src/commands/init.ts`（不含 codebase 部分）、`packages/core/src/commands/status.ts`、`packages/core/src/state/schema-migration.ts`（迁移/恢复语义简化：Rust 版不做历史版本迁移，读到旧格式返回 E_STATE_CORRUPTED）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/init_status.rs`：
```rust
use sdd_core::run;
use sdd_core::contracts::CommandRequest;

fn req(dir: &std::path::Path, command: &str) -> CommandRequest {
    CommandRequest { command: command.into(), cwd: dir.to_string_lossy().to_string(), args: None }
}

#[test]
fn init_creates_sdd_and_index_ready() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&req(dir.path(), "init")).unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "INDEX_READY");
    assert!(dir.path().join(".sdd/state.json").exists());
}

#[test]
fn status_after_init_reports_index_ready() {
    let dir = tempfile::tempdir().unwrap();
    run(&req(dir.path(), "init")).unwrap();
    let result = run(&req(dir.path(), "status")).unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "INDEX_READY");
}

#[test]
fn status_before_init_is_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&req(dir.path(), "status")).unwrap();
    assert_eq!(result.state, "NOT_INITIALIZED");
}

#[test]
fn init_twice_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    run(&req(dir.path(), "init")).unwrap();
    let result = run(&req(dir.path(), "init")).unwrap();
    assert!(result.ok);
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test init_status
```
Expected: FAIL（run 分发未实现 init/status）。

- [ ] **Step 3: 实现 init 与 status**

`commands/init.rs`：
```rust
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::state::{lock_sdd, StateStore, WorkflowState};

pub fn run_init(cwd: &str) -> Result<CommandResult, SddError> {
    let _guard = lock_sdd(cwd)?;
    let store = StateStore::new(cwd.to_string());
    let state = WorkflowState { phase: "INDEX_READY".to_string(), change_id: None };
    store.write(&state)?;
    Ok(CommandResult { ok: true, state: state.phase, exit_code: 0, change_id: None,
        next: Some("sdd new <需求>".to_string()), data: None, warnings: None, error: None })
}
```

`commands/status.rs`（翻译自 status.ts：只读，无锁；返回 state 与建议 next）：
```rust
pub struct StatusInfo { pub state: String, pub next: Option<String> }

pub fn run_status(cwd: &str) -> Result<StatusInfo, SddError> {
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let next = match state.phase.as_str() {
        "NOT_INITIALIZED" => Some("sdd init".to_string()),
        "INDEX_READY" => Some("sdd new <需求>".to_string()),
        _ => None,
    };
    Ok(StatusInfo { state: state.phase, next })
}
```

`commands/mod.rs`：`pub mod init; pub mod status;`

`lib.rs` 分发更新（`run()` 中）：
```rust
match request.command.as_str() {
    "init" => commands::init::run_init(&request.cwd).map(ok_result),
    "status" => {
        let info = commands::status::run_status(&request.cwd)?;
        Ok(CommandResult { ok: true, state: info.state, exit_code: 0, change_id: None,
            next: info.next, data: None, warnings: None, error: None })
    }
    "build" => Err(SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
        .with_next("sdd init")),
    _ => Err(SddError::new("E_INVALID_PHASE_COMMAND", "...")),
}
```
（注意：Node 版 core.execute 对非 init/status 命令先查状态再分发；Rust 版在命令分发前置统一检查——见 Task 8 细化；本任务仅保证 init/status 可用。）

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test init_status && cargo test --workspace
```
Expected: 4 个新测试 PASS，旧测试不回归。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/commands/ crates/sdd-core/src/lib.rs crates/sdd-core/tests/init_status.rs
git commit -m "feat: 实现 init 与 status 命令（.sdd 初始化与状态查询）"
```

---

### Task 5: knowledge 模块——Provider trait、GitNexus/CodeGraph 封装与探测

**Files:**
- Create: `crates/sdd-core/src/knowledge/mod.rs`、`crates/sdd-core/src/knowledge/provider.rs`、`crates/sdd-core/src/knowledge/gitnexus.rs`、`crates/sdd-core/src/knowledge/codegraph.rs`
- Test: `crates/sdd-core/tests/knowledge.rs`

**Interfaces:**
- Produces:
```rust
pub enum KnowledgeIntent { Impact, Context, Explore, Callers, Callees, RelatedFiles, Tests, Routes, Architecture }
pub struct ProbeResult { pub available: bool, pub version: Option<String>, pub message: Option<String> }
pub struct IndexResult { pub ok: bool, pub degraded: bool, pub reason: Option<String> }
pub struct QueryResult { pub provider: &'static str, pub degraded: bool, pub confidence: f64, pub reason: Option<String>, pub payload: serde_json::Value }

pub trait KnowledgeProvider {
    fn name(&self) -> &'static str;
    fn probe(&self) -> ProbeResult;
    fn index(&self, root: &str) -> IndexResult;
    fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult;
}

pub fn find_on_path(cmd: &str) -> Option<std::path::PathBuf>;   // PATH 探测（macOS/Windows 兼容）
```

**翻译自：** `packages/core/src/codebase/mcp-query.ts`（intent 枚举与 result 结构语义）、`packages/codebase-memory/src/lifecycle.ts`（探测/安装/降级语义——Rust 版不做"安装"，只探测与诊断）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/knowledge.rs`：
```rust
use sdd_core::knowledge::*;

#[test]
fn find_on_path_locates_git() {
    // git 一定在 PATH 中
    let found = find_on_path("git").expect("git 应可探测到");
    assert!(found.exists());
}

#[test]
fn gitnexus_probe_reports_unavailable_without_failure() {
    // 不依赖真实安装：probe 永远返回结构而非 panic
    let p = GitNexusProvider::default();
    let r = p.probe();
    assert!(r.available || r.message.is_some());
}

#[test]
fn codegraph_probe_same_shape() {
    let p = CodeGraphProvider::default();
    let r = p.probe();
    assert!(r.available || r.message.is_some());
}

#[test]
fn query_when_unavailable_is_degraded() {
    let p = GitNexusProvider::default();
    let r = p.query(".", KnowledgeIntent::Impact, "foo");
    if !p.probe().available {
        assert!(r.degraded);
        assert!(r.reason.is_some());
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test knowledge
```
Expected: FAIL（模块不存在）。

- [ ] **Step 3: 实现 knowledge 模块**

`knowledge/provider.rs`：
```rust
use std::path::PathBuf;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnowledgeIntent {
    Impact, Context, Explore, Callers, Callees, RelatedFiles, Tests, Routes, Architecture,
}

impl KnowledgeIntent {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Impact => "impact", Self::Context => "context", Self::Explore => "explore",
            Self::Callers => "callers", Self::Callees => "callees", Self::RelatedFiles => "related-files",
            Self::Tests => "tests", Self::Routes => "routes", Self::Architecture => "architecture" }
    }
    pub fn from_str(s: &str) -> Option<Self> { /* 反查 */ }
}

pub struct ProbeResult { pub available: bool, pub version: Option<String>, pub message: Option<String> }
pub struct IndexResult { pub ok: bool, pub degraded: bool, pub reason: Option<String> }
pub struct QueryResult { pub provider: &'static str, pub degraded: bool, pub confidence: f64, pub reason: Option<String>, pub payload: Value }

pub trait KnowledgeProvider {
    fn name(&self) -> &'static str;
    fn probe(&self) -> ProbeResult;
    fn index(&self, root: &str) -> IndexResult;
    fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult;
}

pub fn find_on_path(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in [cmd, &format!("{cmd}.exe")] {
            let candidate = dir.join(name);
            if candidate.is_file() { return Some(candidate); }
        }
    }
    None
}

pub fn run_command(bin: &std::path::Path, args: &[&str], cwd: &str, timeout_ms: u64)
    -> Result<std::process::Output, std::io::Error> {
    use std::process::Command;
    let mut child = Command::new(bin).args(args).current_dir(cwd)
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
    // 等待 + 超时（Child::wait_timeout 需要 nightly？不——用 spawn 后手动轮询 wait_timeout）
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Ok(Some(status)) = child.try_wait() { /* 读 stdout/stderr 并返回 */ }
        if std::time::Instant::now() > deadline { let _ = child.kill(); return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "命令超时")); }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
```
> `run_command` 是核心工具函数：后续所有 gitnexus/codegraph/git 调用复用它；try_wait + 轮询实现超时（std::process::Child 的 wait_timeout 不稳定，禁止使用）。

`knowledge/gitnexus.rs`（命令映射：index=`gitnexus analyze`，impact=`gitnexus impact --summary-only`，context=`gitnexus context`，其余 intent 转 `gitnexus query`；输出原样进 payload，超时 120s）：
```rust
pub struct GitNexusProvider { pub bin: Option<std::path::PathBuf> }

impl Default for GitNexusProvider {
    fn default() -> Self { Self { bin: find_on_path("gitnexus") } }
}

impl KnowledgeProvider for GitNexusProvider {
    fn name(&self) -> &'static str { "gitnexus" }
    fn probe(&self) -> ProbeResult {
        match &self.bin {
            Some(bin) => match run_command(bin, &["--version"], ".", 15_000) {
                Ok(out) if out.status.success() => ProbeResult {
                    available: true,
                    version: Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
                    message: None,
                },
                _ => ProbeResult { available: false, version: None, message: Some("gitnexus 命令不可用".into()) },
            },
            None => ProbeResult { available: false, version: None, message: Some("gitnexus 未在 PATH 中找到".into()) },
        }
    }
    fn index(&self, root: &str) -> IndexResult {
        let Some(bin) = &self.bin else { return IndexResult { ok: false, degraded: true, reason: Some("gitnexus 不可用".into()) } };
        match run_command(bin, &["analyze"], root, 600_000) {
            Ok(out) if out.status.success() => IndexResult { ok: true, degraded: false, reason: None },
            Ok(out) => IndexResult { ok: false, degraded: true,
                reason: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()) },
            Err(e) => IndexResult { ok: false, degraded: true, reason: Some(e.to_string()) },
        }
    }
    fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        // impact → ["impact", "--summary-only", query]；context → ["context", query]；
        // 其余 → ["query", query]；输出 stdout 进 payload{output}；失败/不可用 → degraded
        todo_query(root, intent, query, &self.bin)
    }
}
```
> `todo_query` 按上述命令映射真实实现（禁止保留 todo）：执行失败返回 `QueryResult { provider:"gitnexus", degraded:true, confidence:0.3, reason:Some(err), payload: json!({"intent": intent.as_str()}) }`，成功返回 `degraded:false, confidence:0.8, payload: json!({"output": stdout})`。

`knowledge/codegraph.rs`（命令映射：index=`codegraph init`，explore/callers/callees/impact 用对应子命令，其余转 `codegraph query`；`--path` 传 root；超时 600s 索引/60s 查询）。

`knowledge/mod.rs`：`pub mod provider; pub mod gitnexus; pub mod codegraph;` + 导出。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test knowledge
```
Expected: 4 个测试 PASS（探测真实执行，不可用时断言 degraded 分支）。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/knowledge/ crates/sdd-core/tests/knowledge.rs
git commit -m "feat: 实现知识图谱 Provider（GitNexus/CodeGraph 探测与子进程调用）"
```

---

### Task 6: knowledge 路由与降级链 + init 集成

**Files:**
- Create: `crates/sdd-core/src/knowledge/router.rs`、`crates/sdd-core/src/knowledge/fallback_scan.rs`
- Modify: `crates/sdd-core/src/knowledge/mod.rs`、`crates/sdd-core/src/commands/init.rs`（接入索引与诊断写入）
- Test: `crates/sdd-core/tests/knowledge_router.rs`

**Interfaces:**
- Consumes: T5 `KnowledgeProvider`/`KnowledgeIntent`/`QueryResult`
- Produces:
```rust
pub struct KnowledgeRouter { pub gitnexus: GitNexusProvider, pub codegraph: CodeGraphProvider }
impl KnowledgeRouter {
    pub fn new() -> Self;
    pub fn initialize(&self, root: &str) -> Vec<serde_json::Value>;   // 对可用引擎索引，返回诊断列表
    pub fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult;
    pub fn status(&self) -> Vec<serde_json::Value>;                   // 两引擎 installed/indexed 诊断
    pub fn fallback_scan(root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult; // 受限文件扫描
}
```

**翻译自：** `packages/core/src/codebase/codebase-adapter.ts`（initialize/fallback 语义、EXCLUDED_DIRECTORIES、安全扩展名、密钥文件跳过）、`mcp-query.ts`（降级 confidence ≤0.45 / 精确 ≥0.6）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/knowledge_router.rs`：
```rust
use sdd_core::knowledge::router::KnowledgeRouter;
use sdd_core::knowledge::provider::KnowledgeIntent;

#[test]
fn initialize_writes_diagnostics_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    let router = KnowledgeRouter::new();
    let diags = router.initialize(dir.path().to_string_lossy().to_string());
    // 无论引擎是否可用，都必须返回诊断且不 panic
    assert_eq!(diags.len(), 2); // gitnexus + codegraph 各一条
}

#[test]
fn query_returns_known_shape() {
    let dir = tempfile::tempdir().unwrap();
    let router = KnowledgeRouter::new();
    let r = router.query(dir.path().to_string_lossy().to_string(), KnowledgeIntent::Impact, "main");
    assert_eq!(r.provider, "gitnexus"); // impact 主路由 GitNexus
    assert!(r.confidence <= 0.99);
}

#[test]
fn fallback_scan_never_panics_and_is_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let r = KnowledgeRouter::fallback_scan(dir.path().to_string_lossy().to_string(), KnowledgeIntent::Architecture, "");
    assert!(r.degraded);
    assert!(r.payload.get("codebaseSummary").is_some());
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test knowledge_router
```
Expected: FAIL。

- [ ] **Step 3: 实现 router 与 fallback_scan**

`knowledge/router.rs`：
```rust
use super::codegraph::CodeGraphProvider;
use super::gitnexus::GitNexusProvider;
use super::provider::{KnowledgeIntent, KnowledgeProvider, QueryResult};

pub struct KnowledgeRouter {
    pub gitnexus: GitNexusProvider,
    pub codegraph: CodeGraphProvider,
}

impl KnowledgeRouter {
    pub fn new() -> Self {
        Self { gitnexus: GitNexusProvider::default(), codegraph: CodeGraphProvider::default() }
    }

    /// 初始化：对 PATH 中存在的引擎执行索引；返回两引擎诊断（写 .sdd/index/knowledge.json）
    pub fn initialize(&self, root: &str) -> Vec<serde_json::Value> {
        let mut diags = Vec::new();
        for p in [&self.gitnexus as &dyn KnowledgeProvider, &self.codegraph as &dyn KnowledgeProvider] {
            let probe = p.probe();
            let index = if probe.available { p.index(root) } else { /* 未安装：不执行 */ };
            diags.push(serde_json::json!({
                "provider": p.name(),
                "installed": probe.available,
                "version": probe.version,
                "indexed": index.ok,          // 未安装时 indexed=false
                "degraded": probe.available && index.degraded,
                "message": probe.message.or(index.reason),
            }));
        }
        // 写 .sdd/index/knowledge.json（失败不阻断）
        let _ = std::fs::create_dir_all(format!("{root}/.sdd/index"));
        let _ = std::fs::write(format!("{root}/.sdd/index/knowledge.json"), serde_json::to_string_pretty(&diags).unwrap());
        diags
    }

    /// intent 路由：impact→gitnexus 优先；explore/callers/callees→codegraph 优先；
    /// context→gitnexus 优先；其余→gitnexus 优先；主路由不可用→次路由→fallback_scan
    pub fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        let primary: &dyn KnowledgeProvider = match intent {
            KnowledgeIntent::Explore | KnowledgeIntent::Callers | KnowledgeIntent::Callees => &self.codegraph,
            _ => &self.gitnexus,
        };
        let secondary: &dyn KnowledgeProvider = if std::ptr::eq(primary, &self.gitnexus) { &self.codegraph } else { &self.gitnexus };
        let p1 = primary.probe();
        if p1.available {
            let r = primary.query(root, intent, query);
            if !r.degraded { return r; }
        }
        let p2 = secondary.probe();
        if p2.available {
            let r = secondary.query(root, intent, query);
            if !r.degraded { return r; }
        }
        Self::fallback_scan(root, intent, query)
    }

    pub fn status(&self) -> Vec<serde_json::Value> { /* 同 initialize 的诊断结构，不含索引动作 */ }
}
```

`knowledge/fallback_scan.rs`（翻译自 codebase-adapter.ts 的 fallback()：scanFiles 遍历 + EXCLUDED_DIRECTORIES + 密钥文件跳过 + 关键字扫描 + 候选文件摘要；输出三字段 codebaseSummary/packageStructure/architecture）：
```rust
use super::provider::{KnowledgeIntent, QueryResult};

const EXCLUDED_DIRECTORIES: [&str; 7] = [".git", ".sdd", "node_modules", "target", "build", "dist", "coverage", "logs"];

pub fn fallback_scan(root: &str, intent: KnowledgeIntent, _query: &str) -> QueryResult {
    let files = scan_files(root, 2000);
    // 按 intent 生成对应 payload 结构：
    // Architecture/RelatedFiles → { codebaseSummary, packageStructure, architecture }
    // Impact → { files, symbols, tests, risks }（空数组）
    // 其余 → { output: 文件列表 + 摘要 }
    let payload = serde_json::json!({
        "codebaseSummary": format!("# 代码库摘要\n\n当前使用 fallback-file-scan 降级模式……\n\n{}", files.join("\n- ")),
        "packageStructure": "…",
        "architecture": "…",
        "intent": intent.as_str(),
    });
    QueryResult { provider: "fallback-file-scan", degraded: true, confidence: 0.3,
        reason: Some("GitNexus 与 CodeGraph 均不可用".into()), payload }
}

fn scan_files(root: &str, limit: usize) -> Vec<String> { /* 深度优先遍历，跳过隐藏目录与排除目录，返回相对路径排序列表；密钥文件名直接跳过（id_rsa/id_ed25519/kubeconfig/application-prod.*/.pem/.key/.p12/.jks） */ }
```

- [ ] **Step 4: init 集成（Modify commands/init.rs）**

在 run_init 中，写 state 后调用 `KnowledgeRouter::new().initialize(cwd)`，诊断列表放入 CommandResult.warnings（仅当有 degraded 时）；state 仍为 INDEX_READY（失败不阻断初始化）。

- [ ] **Step 5: 跑测试确认通过**

```bash
cargo test -p sdd-core --test knowledge_router && cargo test --workspace
```
Expected: 全 PASS；`initialize_writes_diagnostics_without_failing` 在无引擎环境验证降级路径。

- [ ] **Step 6: Commit**

```bash
git add crates/sdd-core/src/knowledge/ crates/sdd-core/src/commands/init.rs crates/sdd-core/tests/knowledge_router.rs
git commit -m "feat: 实现知识图谱意图路由与降级链并接入 init"
```

---

### Task 7: codebase 命令（status/doctor/index/query/rebuild）

**Files:**
- Create: `crates/sdd-core/src/commands/codebase.rs`
- Modify: `crates/sdd-core/src/lib.rs`（分发 "codebase"）
- Test: `crates/sdd-cli/tests/codebase_cli.rs`

**Interfaces:**
- Consumes: T6 `KnowledgeRouter`
- Produces: `commands::codebase::run_codebase(cwd, args) -> Result<CommandResult, SddError>`（args 为 `{ sub: "status|doctor|index|query|rebuild", query?, intent? }`）

**翻译自：** `packages/core/src/commands/codebase.ts`（分发）+ `packages/cli/src/commands/codebase.ts`（参数校验）；5 个子命令行为见 docs/CLI.md 139-153 行。

- [ ] **Step 1: 写失败测试**

`crates/sdd-cli/tests/codebase_cli.rs`：
```rust
use std::process::Command;

fn sdd_in(dir: &std::path::Path) -> Command { let mut c = Command::new(env!("CARGO_BIN_EXE_sdd")); c.current_dir(dir); c }

#[test]
fn codebase_status_returns_json() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd_in(dir.path()).args(["codebase", "status", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("gitnexus") && text.contains("codegraph"));
}

#[test]
fn codebase_invalid_subcommand_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd_in(dir.path()).args(["codebase", "frobnicate"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn codebase_query_returns_payload() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd_in(dir.path()).args(["codebase", "query", "hello", "--intent", "impact", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("provider"));
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-cli --test codebase_cli
```
Expected: FAIL（codebase 子命令未实现/未分发）。

- [ ] **Step 3: 实现 codebase 命令**

`commands/codebase.rs`：
```rust
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::knowledge::router::KnowledgeRouter;
use crate::knowledge::provider::KnowledgeIntent;

pub fn run_codebase(cwd: &str, args: &serde_json::Value) -> Result<CommandResult, SddError> {
    let sub = args.get("sub").and_then(|v| v.as_str()).ok_or_else(|| {
        SddError::new("E_INVALID_PHASE_COMMAND", "codebase 需要子命令（status/doctor/index/query/rebuild）")
    })?;
    let router = KnowledgeRouter::new();
    let result: serde_json::Value = match sub {
        "status" => serde_json::json!({ "providers": router.status() }),
        "doctor" => serde_json::json!({ "providers": router.status(), "note": "探测 PATH 中的 gitnexus 与 codegraph 命令" }),
        "index" => serde_json::json!({ "providers": router.initialize(cwd) }),
        "rebuild" => serde_json::json!({ "providers": router.initialize(cwd) }), // 语义同 index，重建索引
        "query" => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let intent = args.get("intent").and_then(|v| v.as_str())
                .and_then(KnowledgeIntent::from_str).unwrap_or(KnowledgeIntent::Impact);
            router.query(cwd, intent, q)
        }
        _ => return Err(SddError::new("E_INVALID_PHASE_COMMAND",
            &format!("未知 codebase 子命令：{sub}"))),
    };
    Ok(CommandResult { ok: true, state: "INDEX_READY".to_string(), exit_code: 0, change_id: None,
        next: None, data: Some(result), warnings: None, error: None })
}
```

`lib.rs` 分发：`"codebase"` → `commands::codebase::run_codebase(&request.cwd, request.args.as_ref().unwrap_or(&serde_json::Value::Null))`。

CLI 侧 `crates/sdd-cli/src/main.rs`：`Codebase` 变体扩展为 clap 子命令结构：
```rust
Codebase {
    #[command(subcommand)]
    sub: CodebaseSub,
}
#[derive(Subcommand)]
enum CodebaseSub {
    Status { #[arg(long)] json: bool },
    Doctor { #[arg(long)] json: bool },
    Index, Rebuild,
    Query { query: String, #[arg(long)] intent: Option<String>, #[arg(long)] json: bool },
}
```
（args 序列化为 `{"sub": "...", "query": "...", "intent": "..."}` 传给 core；`--json` 仅控制渲染，core 输出仍为结构化 data。）

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-cli --test codebase_cli && cargo test --workspace
```
Expected: 3 个测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/commands/codebase.rs crates/sdd-core/src/lib.rs crates/sdd-cli/src/main.rs crates/sdd-cli/tests/codebase_cli.rs
git commit -m "feat: 实现 codebase 子命令（status/doctor/index/query/rebuild）"
```

---

### Task 8: 分发前置检查 + new 命令与 spec engine

**Files:**
- Create: `crates/sdd-core/src/engines/mod.rs`、`crates/sdd-core/src/engines/spec/mod.rs`、`crates/sdd-core/src/engines/spec/spec_engine.rs`、`crates/sdd-core/src/engines/spec/model.rs`、`crates/sdd-core/src/commands/new.rs`
- Modify: `crates/sdd-core/src/lib.rs`
- Test: `crates/sdd-core/tests/new_spec.rs`

**Interfaces:**
- Consumes: T2/T4 状态层与 init/status
- Produces:
```rust
pub struct Requirement { pub id: String, pub text: String }
pub struct Scenario { pub id: String, pub title: String, pub when: String, pub then: String, pub given: Option<String> }
pub struct Spec { pub id: String, pub summary: String, pub requirements: Vec<Requirement>, pub scenarios: Vec<Scenario> }
pub struct SpecEngine { pub requirement_id_gen: ... }
impl SpecEngine {
    pub fn generate_spec(&self, requirement: &str, codebase_summary: Option<&str>) -> Spec;
    pub fn parse_spec_md(&self, content: &str) -> Result<Spec, SddError>;   // 解析 spec.md
    pub fn render_spec_md(&self, spec: &Spec) -> String;                     // 渲染 spec.md
}
pub fn run_new(cwd: &str, args: &serde_json::Value, spec_engine: &SpecEngine) -> Result<CommandResult, SddError>;
```

**翻译自：** `packages/core/src/commands/new.ts`（澄清链：无 requirement → 进入 CLARIFYING 状态返回问题）、`packages/core/src/engines/spec/spec-engine.ts`、`semantic-lexicon.ts`（语义词典）、`packages/core/src/engines/openspec/model.ts`（Requirement/Scenario 结构）、`renderer.ts`。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/new_spec.rs`：
```rust
use sdd_core::commands::new::run_new;
use sdd_core::engines::spec::{SpecEngine, Spec};
use sdd_core::contracts::CommandRequest;
use serde_json::json;

#[test]
fn new_without_requirement_enters_clarifying() {
    let dir = tempfile::tempdir().unwrap();
    sdd_core::run(&CommandRequest { command: "init".into(), cwd: dir.path().to_string_lossy().to_string(), args: None }).unwrap();
    let result = run_new(&dir.path().to_string_lossy().to_string(), &json!({ "requirement": null }), &SpecEngine::new()).unwrap();
    assert_eq!(result.state, "CLARIFYING");
    assert!(!result.ok); // 等待用户澄清
}

#[test]
fn new_with_requirement_writes_spec() {
    let dir = tempfile::tempdir().unwrap();
    sdd_core::run(&CommandRequest { command: "init".into(), cwd: dir.path().to_string_lossy().to_string(), args: None }).unwrap();
    let result = run_new(&dir.path().to_string_lossy().to_string(), &json!({ "requirement": "实现订单取消功能" }), &SpecEngine::new()).unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "SPEC_READY");
    assert!(dir.path().join(".sdd/changes").exists());
    // spec.json 已写入且可解析
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes")).unwrap().next().unwrap().unwrap().path();
    let spec_path = change_dir.join("spec.json");
    let raw = std::fs::read_to_string(&spec_path).unwrap();
    let spec: Spec = serde_json::from_str(&raw).unwrap();
    assert!(!spec.requirements.is_empty());
}

#[test]
fn spec_parse_render_roundtrip() {
    let engine = SpecEngine::new();
    let spec = engine.generate_spec("实现订单取消功能", None);
    let md = engine.render_spec_md(&spec);
    let parsed = engine.parse_spec_md(&md).unwrap();
    assert_eq!(parsed.requirements.len(), spec.requirements.len());
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test new_spec
```
Expected: FAIL。

- [ ] **Step 3: 实现 spec engine 与 new 命令**

`engines/spec/model.rs`：Requirement/Scenario/Spec 结构（字段对应 openspec model.ts，id 生成沿用 requirement-ids.ts 的规则：`R-001`、`S-001` 递增）。

`engines/spec/spec_engine.rs`：
- `generate_spec`：把 requirement 文本拆分为需求条目（按行/语义词典识别"当…时/并且/那么"等场景标记，翻译自 spec-engine.ts 的解析与 semantic-lexicon.ts）；生成 R-*/S-* 编号；spec 写入 `changes/<change-id>/spec.json`（camelCase 序列化）+ `spec.md`（renderer.ts 格式）
- `parse_spec_md` / `render_spec_md`：翻译自 openspec/parser.ts 与 renderer.ts 的标记格式（`# 需求` / `## R-001` / `- 当…` 结构）

`commands/new.rs`：
```rust
pub fn run_new(cwd: &str, args: &serde_json::Value, engine: &SpecEngine) -> Result<CommandResult, SddError> {
    let requirement = args.get("requirement").and_then(|v| v.as_str())
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let Some(req) = requirement else {
        // 进入澄清状态：写 CLARIFYING + 返回待回答问题
        let clarifying = state.clone_with_phase("CLARIFYING");
        store.write(&clarifying)?;
        return Ok(CommandResult { ok: false, state: "CLARIFYING".into(), exit_code: 0, change_id: None,
            next: Some("sdd new --answers '<JSON answers>'".into()),
            data: Some(json!({"question": "请补充需求细节（涉及的功能、边界与验收标准）"})),
            warnings: None, error: None });
    };
    // 创建 change：.sdd/changes/<change-id>/ 目录（change-id 格式沿用 change-id.ts：日期-序号）
    let change_id = change_id_for(cwd);
    std::fs::create_dir_all(format!("{cwd}/.sdd/changes/{change_id}"))?;
    let spec = engine.generate_spec(&req, None);
    engine.write_spec(cwd, &change_id, &spec)?;
    store.write(&state.clone_with_phase("SPEC_READY").with_change(Some(change_id.clone())))?;
    Ok(CommandResult { ok: true, state: "SPEC_READY".into(), exit_code: 0, change_id: Some(change_id),
        next: Some("sdd design".into()), data: Some(json!({"spec": spec})), warnings: None, error: None })
}
```
> `change_id_for` 沿用 `packages/core/src/commands/change-id.ts` 的格式（如 `20260802-01`）；`WorkflowState::clone_with_phase/with_change` 添加为 T2 结构的辅助方法。

`lib.rs` 前置检查（对齐 core.ts 逻辑）：
```rust
// run() 内，init/status/codebase 之外的分支先读状态：
let store = StateStore::new(request.cwd.clone());
let state = store.read()?;
if state.phase == "ARCHIVED" && command != "archive" && command != "new" {
    return Err(SddError::new("E_ARCHIVED_READONLY", "已归档的变更为只读状态"));
}
if state.phase == "NOT_INITIALIZED" {
    return Err(SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令").with_next("sdd init"));
}
```
且"new"命令仅在 NOT_INITIALIZED/INDEX_READY/ARCHIVED 下允许（其他阶段 E_INVALID_PHASE_COMMAND，next 指向 status.next）。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test new_spec && cargo test --workspace
```
Expected: 3 个测试 PASS；`new_without_requirement_enters_clarifying` 验证澄清链。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/engines/ crates/sdd-core/src/commands/new.rs crates/sdd-core/src/lib.rs crates/sdd-core/src/state/ crates/sdd-core/tests/new_spec.rs
git commit -m "feat: 实现 new 命令与 spec engine（澄清链与规格生成）"
```

---

### Task 9: design 与 plan 命令（tdd-engine + context-pack）

**Files:**
- Create: `crates/sdd-core/src/engines/tdd/mod.rs`、`crates/sdd-core/src/engines/tdd/tdd_engine.rs`、`crates/sdd-core/src/commands/design.rs`、`crates/sdd-core/src/commands/plan.rs`、`crates/sdd-core/src/build/context_pack.rs`
- Test: `crates/sdd-core/tests/design_plan.rs`

**Interfaces:**
- Consumes: T8 `Spec`、change 目录
- Produces:
```rust
pub struct TaskDef { pub id: String, pub title: String, pub phase: String /* RED|GREEN|REFACTOR|VERIFY */,
    pub summary: String, pub acceptance: Vec<String> }
pub struct Plan { pub change_id: String, pub tasks: Vec<TaskDef>, pub readable_plan: String,
    pub test_plan: Vec<String>, pub context_summary: String }
pub struct TddEngine { /* 从 spec 生成任务链 */ }
impl TddEngine {
    pub fn generate_design(&self, spec: &Spec) -> String;      // design.md（人工确认用）
    pub fn generate_plan(&self, spec: &Spec, change_id: &str) -> Plan; // RED/GREEN/REFACTOR/VERIFY 链
    pub fn write_plan(&self, cwd: &str, change_id: &str, plan: &Plan) -> Result<(), SddError>;
}
pub fn run_design(cwd, args) -> Result<CommandResult, SddError>;  // DESIGN_READY
pub fn run_plan(cwd, args) -> Result<CommandResult, SddError>;    // PLAN_READY，写 plan.json + plan.md
```

**翻译自：** `packages/core/src/commands/design.ts`、`plan.ts`、`engines/tdd/tdd-engine.ts`（四阶段任务链生成）、`build/context-pack.ts`（上下文包按任务生成——本任务先支持 plan.json 中的 context_summary，单任务 Context Pack 在 T13 随 build next 生成）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/design_plan.rs`：
```rust
use sdd_core::engines::spec::SpecEngine;
use sdd_core::engines::tdd::TddEngine;

fn fixture_spec() -> sdd_core::engines::spec::Spec {
    SpecEngine::new().generate_spec("实现订单取消功能：用户可取消未发货订单", None)
}

#[test]
fn tdd_plan_has_red_green_refactor_verify() {
    let plan = TddEngine::new().generate_plan(&fixture_spec(), "20260802-01");
    let phases: Vec<&str> = plan.tasks.iter().map(|t| t.phase.as_str()).collect();
    assert!(phases.iter().any(|p| *p == "RED"));
    assert!(phases.iter().any(|p| *p == "GREEN"));
    assert!(phases.iter().any(|p| *p == "REFACTOR"));
    assert!(phases.iter().any(|p| *p == "VERIFY"));
}

#[test]
fn design_then_plan_updates_phases() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    sdd_core::run(&sdd_core::contracts::CommandRequest { command: "init".into(), cwd: cwd.clone(), args: None }).unwrap();
    // 走 new → design → plan 全链
    let req = |c: &str| sdd_core::contracts::CommandRequest { command: c.into(), cwd: cwd.clone(), args: None };
    sdd_core::run(&req("new")).unwrap(); // 无 requirement 进澄清
    let new_req = serde_json::json!({ "requirement": "实现订单取消功能" });
    // new 需要 args；本测试用 run_new 直接调用并写 SPEC_READY
    sdd_core::commands::new::run_new(&cwd, &new_req, &SpecEngine::new()).unwrap();
    let d = sdd_core::run(&req("design")).unwrap();
    assert!(d.ok && d.state == "DESIGN_READY");
    let p = sdd_core::run(&req("plan")).unwrap();
    assert!(p.ok && p.state == "PLAN_READY");
    assert!(dir.path().join(".sdd/changes").join("20260802-01").join("plan.json").exists());
}
```
> change-id 在测试中使用固定值会造成目录冲突（两次 run 若 change 已存在）：`run_new` 在 change 已存在时复用当前 change（对齐 Node 版 E_ACTIVE_CHANGE_EXISTS 语义——若已存在活动 change 且命令是 new，Node 版抛 E_ACTIVE_CHANGE_EXISTS；实现时对齐：第二次 new 抛该错误码。测试改为单次 new 后直接 design/plan）。

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test design_plan
```
Expected: FAIL。

- [ ] **Step 3: 实现 tdd-engine、design、plan**

`engines/tdd/tdd_engine.rs`（翻译自 tdd-engine.ts 任务链生成）：
```rust
pub fn generate_plan(&self, spec: &Spec, change_id: &str) -> Plan {
    // 每个 requirement 生成 RED/GREEN/REFACTOR 任务 + 全局 VERIFY 任务：
    // TASK-{n:03}-{PHASE}，例如 TASK-001-RED；acceptance 由 scenario 的 then 条款构成
    // 任务定义见 packages/core/src/engines/tdd/tdd-engine.ts 的 buildTasks 语义
    ...
}
```

`commands/design.rs`（读 change 下 spec.json → TddEngine.generate_design → 写 design.md → 状态 DESIGN_READY）；`commands/plan.rs`（读 spec → generate_plan → 写 plan.json（Plan serde）+ plan.md → 状态 PLAN_READY，next="sdd build next"）。

`build/context_pack.rs`：`pub fn render_context_summary(spec, plan, rules: &[String]) -> String`（翻译自 context-pack.ts 的摘要渲染，包含规格/计划/项目规则/Policy 摘要；T13 扩展为单任务包）。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test design_plan && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/engines/tdd/ crates/sdd-core/src/commands/design.rs crates/sdd-core/src/commands/plan.rs crates/sdd-core/src/build/ crates/sdd-core/tests/design_plan.rs
git commit -m "feat: 实现 design 与 plan 命令（TDD 四阶段任务链）"
```

---

### Task 10: git 层（inspector + 快照/delta + 文件范围校验）

**Files:**
- Create: `crates/sdd-core/src/git/mod.rs`、`crates/sdd-core/src/git/inspector.rs`
- Test: `crates/sdd-core/tests/git_inspector.rs`

**Interfaces:**
- Consumes: T5 `run_command`
- Produces:
```rust
pub struct GitSnapshot { pub head: String, pub files: Vec<String>, pub diff: Vec<GitDelta> }
pub struct GitDelta { pub path: String, pub status: String /* A|M|D */ }
pub struct GitInspector;
impl GitInspector {
    pub fn snapshot(cwd: &str) -> Result<GitSnapshot, SddError>;       // git rev-parse HEAD + ls-files + status --porcelain
    pub fn compute_delta(cwd: &str, base_head: &str) -> Result<Vec<GitDelta>, SddError>; // git diff --name-status base..HEAD
    pub fn changed_files(cwd: &str) -> Result<Vec<String>, SddError>;  // git status --porcelain
    pub fn is_git_repo(cwd: &str) -> bool;
}
```

**翻译自：** `packages/core/src/git/git-inspector.ts`。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/git_inspector.rs`：
```rust
use sdd_core::git::GitInspector;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?} 失败");
}

#[test]
fn delta_detects_created_modified_deleted() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "t@t.t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    std::fs::write(dir.path().join("a.txt"), "1").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let base = GitInspector::snapshot(dir.path().to_string_lossy().to_string()).unwrap();
    std::fs::write(dir.path().join("b.txt"), "2").unwrap();
    std::fs::write(dir.path().join("a.txt"), "2").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "change"]);
    let delta = GitInspector::compute_delta(&dir.path().to_string_lossy().to_string(), &base.head).unwrap();
    assert!(delta.iter().any(|d| d.path == "b.txt" && d.status == "A"));
    assert!(delta.iter().any(|d| d.path == "a.txt" && d.status == "M"));
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test git_inspector
```
Expected: FAIL。

- [ ] **Step 3: 实现 GitInspector**

`git/inspector.rs`：用 `run_command`（T5）执行 git 子命令；`snapshot` 捕获 `rev-parse HEAD` 与 `status --porcelain`；`compute_delta` 用 `git diff --name-status <base>..HEAD` 解析 `A/M/D` 前缀；非 git 仓库返回 `E_PATH_OUTSIDE_REPO`；翻译自 git-inspector.ts 的解析逻辑（porcelain 格式 `XY path`）。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test git_inspector
```
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/git/ crates/sdd-core/tests/git_inspector.rs
git commit -m "feat: 实现 git 检查层（快照、delta 与文件范围）"
```

---

### Task 11: build 命令（task-executor、AgentActionRequired、契约变更点）

**Files:**
- Create: `crates/sdd-core/src/build/mod.rs`、`crates/sdd-core/src/build/task_executor.rs`、`crates/sdd-core/src/build/task_result.rs`、`crates/sdd-core/src/commands/build.rs`
- Modify: `crates/sdd-core/src/contracts.rs`（AgentActionRequired 完整化）
- Modify: `crates/sdd-core/src/security/`（新建 mod，先含 task_scope.rs——允许/禁止文件校验）
- Test: `crates/sdd-core/tests/build_flow.rs`

**Interfaces:**
- Consumes: T9 `Plan/TaskDef`、T10 `GitInspector`
- Produces:
```rust
pub struct TaskExecutionResult { pub task_id: String, pub status: String /* completed|failed */,
    pub evidence: Vec<String>, pub verification: Vec<serde_json::Value>, pub files_changed: Vec<String> }
pub enum BuildCommand { Next, Complete }
pub fn run_build(cwd, args) -> Result<CommandResult, SddError>;
// build next → BUILD_WAITING_AGENT，返回 actionRequired（含 context pack 路径、allowed/expected/forbidden files、verification 命令、codebase.provider）
// build complete → 校验结果（结构/任务身份/git delta/TDD evidence/verification），写 runs/<run-id>/tasks/<task>.result.json
```

**翻译自：** `packages/core/src/commands/build.ts`、`build/task-executor.ts`、`build/task-result-normalizer.ts`、`security/task-scope.ts`、`quality/tdd-evidence.ts`（evidence 校验部分）。**契约变更点**：`AgentActionRequired.codebase.provider` 改为 `"gitnexus" | "codegraph" | "fallback-file-scan"`（对齐 T6 路由结果；若 query 未走 knowledge 时取 router 的可用引擎名）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/build_flow.rs`：
```rust
#[test]
fn build_next_returns_action_required_with_known_provider() {
    // 完整准备：init → new → design → plan
    // 断言：ok=true, state=BUILD_WAITING_AGENT, actionRequired 存在,
    // actionRequired.codebase.provider ∈ {"gitnexus","codegraph","fallback-file-scan"}
}

#[test]
fn build_complete_with_invalid_result_rejected() {
    // 构造伪造 task result（缺少 evidence）→ run_build complete
    // 断言：E_TDD_EVIDENCE_REQUIRED / E_AGENT_TASK_FAILED 之一，退出码 7
}

#[test]
fn build_complete_undeclared_file_change_rejected() {
    // git 变更包含不在 allowedFiles 中的文件 → E_UNDECLARED_FILE_CHANGE，退出码 10
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test build_flow
```
Expected: FAIL。

- [ ] **Step 3: 实现 build 命令与执行器**

`contracts.rs` 完整化（替换 T1 占位）：
```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentActionRequired {
    pub r#type: String,            // "AGENT_TASK_EXECUTION"
    pub task_id: String,
    pub change_id: String,
    pub context_pack: String,
    pub allowed_files: Vec<String>,
    pub expected_new_files: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub verification: Vec<VerificationCommand>,
    pub result_file: String,
    pub codebase: CodebaseProviderInfo,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodebaseProviderInfo { pub provider: String, pub degraded: bool }  // provider: gitnexus|codegraph|fallback-file-scan
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VerificationCommand { pub command: String, pub args: Vec<String> }
```

`commands/build.rs`（build next/complete 分发；next 复用 T9 的 context_pack，生成单任务 Context Pack 目录 `.sdd/context-packs/<task-id>/`，允许/禁止文件来自 task 定义与安全层；complete 走 TDD evidence 校验 + Git delta 校验 + 写运行级结果 `.sdd/runs/<run-id>/tasks/<task>.result.json`，对齐 build.ts 的裁决顺序）。

`security/task_scope.rs`（翻译自 task-scope.ts）：`pub fn validate_file_change(delta_paths: &[String], allowed: &[String], expected_new: &[String], forbidden: &[String]) -> Result<(), SddError>`——路径规范化（resolve + symlink 检查）、允许集/期望新增集/禁止集裁决。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test build_flow && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/build/ crates/sdd-core/src/commands/build.rs crates/sdd-core/src/security/ crates/sdd-core/src/contracts.rs crates/sdd-core/tests/build_flow.rs
git commit -m "feat: 实现 build 命令与任务执行裁决（含 codebase provider 契约变更）"
```

---

### Task 12: verify / review / archive 质量链

**Files:**
- Create: `crates/sdd-core/src/quality/mod.rs`、`crates/sdd-core/src/quality/traceability.rs`、`crates/sdd-core/src/quality/tdd_evidence.rs`、`crates/sdd-core/src/quality/report.rs`、`crates/sdd-core/src/quality/minimality.rs`、`crates/sdd-core/src/quality/deterministic_review.rs`、`crates/sdd-core/src/security/secrets_scanner.rs`、`crates/sdd-core/src/commands/verify.rs`、`crates/sdd-core/src/commands/review.rs`、`crates/sdd-core/src/commands/archive.rs`
- Test: `crates/sdd-core/tests/quality_chain.rs`

**Interfaces:**
- Consumes: T11 任务结果、T10 Git delta
- Produces:
```rust
pub struct Report { pub kind: String /* verify|review */, pub summary: String, pub passed: bool,
    pub issues: Vec<Issue> }
pub struct Issue { pub code: String, pub severity: String, pub message: String }
pub fn run_verify(cwd, args) -> Result<CommandResult, SddError>;   // VERIFY_READY，写 report(kind=verify)
pub fn run_review(cwd, args) -> Result<CommandResult, SddError>;   // REVIEW_READY，写 report(kind=review)
pub fn run_archive(cwd, args) -> Result<CommandResult, SddError>;  // ARCHIVED，收敛为 archive.json/md/.archived
```

**翻译自：** `packages/core/src/commands/verify.ts`、`review.ts`、`archive.ts`、`quality/verify-report.ts`、`review-report.ts`、`traceability.ts`、`tdd-evidence.ts`、`minimality-review.ts`、`deterministic-review.ts`、`security/secrets-scanner.ts`。归档收敛语义（三文件 + `.archived` 组合哈希）见 docs/architecture.md 归档节。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/quality_chain.rs`：
```rust
#[test]
fn verify_fails_when_tasks_incomplete() { /* E_VERIFY_REQUIRED 或 E_TDD_EVIDENCE_REQUIRED，退出码 3 或 7 */ }

#[test]
fn review_detects_unplanned_dependency_change() {
    // 构造 change：plan 未声明依赖，但 git delta 显示依赖文件变更 → E_UNPLANNED_DEPENDENCY，退出码 8
}

#[test]
fn archive_converges_to_three_files() {
    // 全链完成（init→new→design→plan→build→verify→review）后 archive：
    // .sdd/changes/<id>/ 只剩 archive.json/archive.md/.archived；状态 ARCHIVED
}

#[test]
fn secrets_scanner_flags_keys() {
    // 变更文件含 "BEGIN PRIVATE KEY" / "aws_access_key_id" → E_SECURITY_BLOCKED，退出码 10
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test quality_chain
```
Expected: FAIL。

- [ ] **Step 3: 实现质量链**

- `quality/tdd_evidence.rs`：RED 任务要求先有失败测试证据、GREEN 要求测试通过证据、VERIFY 要求全量验证（翻译自 tdd-evidence.ts 的裁决矩阵）
- `quality/traceability.rs`：Requirement/Scenario → 任务 → 证据覆盖矩阵（翻译自 traceability.ts）
- `quality/deterministic_review.rs` + `minimality.rs`：变更文件行数/文件数指标、`sdd-debt` 标记扫描、依赖 delta（翻译自 deterministic-review.ts、minimality-review.ts、dependency-delta.ts——Rust 版依赖声明从 `Cargo.toml` 解析 `[dependencies]` 变更）
- `security/secrets_scanner.rs`：密钥模式扫描（翻译自 secrets-scanner.ts 的规则集）
- `commands/verify.rs`/`review.rs`/`archive.rs`：阶段推进 + report schema 校验 + 归档三文件收敛

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test quality_chain && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/quality/ crates/sdd-core/src/security/secrets_scanner.rs crates/sdd-core/src/commands/verify.rs crates/sdd-core/src/commands/review.rs crates/sdd-core/src/commands/archive.rs crates/sdd-core/tests/quality_chain.rs
git commit -m "feat: 实现 verify/review/archive 质量链"
```

---

### Task 13: protocol 与 policies 模块

**Files:**
- Create: `crates/sdd-core/src/protocol/mod.rs`、`crates/sdd-core/src/protocol/validate.rs`、`crates/sdd-core/src/policies/mod.rs`、`crates/sdd-core/src/policies/resolver.rs`、`crates/sdd-core/src/policies/compiler.rs`、`crates/sdd-core/src/policies/digest.rs`
- Test: `crates/sdd-core/tests/protocol_policies.rs`

**Interfaces:**
- Consumes: T1 `CommandResult`、assets 路径（T14 提供，先用相对路径常量 `assets/policies/`）
- Produces:
```rust
pub fn validate_task_result(raw: &serde_json::Value) -> Result<TaskExecutionResult, SddError>; // protocol/validate.rs
pub struct PolicyBundle { pub name: String, pub source: String, pub digest: String }
pub fn resolve_policies(cwd: &str) -> Result<Vec<PolicyBundle>, SddError>; // policies/resolver.rs
pub fn compile_policy(policy_md: &str) -> Result<Vec<PolicyRule>, SddError>; // policies/compiler.rs
```

**翻译自：** `packages/agent-protocol/src/validate.ts`、`types/*`（action-required/task-result/agent-capability/policy）、`packages/agent-policies/src/*`（resolver/compiler/digest/registry）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/protocol_policies.rs`：
```rust
#[test]
fn invalid_task_result_rejected() { /* 缺 taskId/status 枚举非法 → Err */ }

#[test]
fn policy_compile_extracts_rules() {
    let md = "# Policy\n\n## 规则\n- 先写测试（RED）\n- 禁止修改未声明文件";
    let rules = sdd_core::policies::compile_policy(md).unwrap();
    assert!(!rules.is_empty());
}

#[test]
fn policy_digest_is_stable() { /* 同一输入两次 digest 相同 */ }
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test protocol_policies
```
Expected: FAIL。

- [ ] **Step 3: 实现 protocol 与 policies**

- `protocol/validate.rs`：TaskExecutionResult 结构校验（taskId/status/evidence/verification/filesChanged 的类型与枚举），错误映射 E_AGENT_TASK_FAILED
- `policies/resolver.rs`：从 `assets/policies/` 读取策略文件（Ponytail 受控 Policy，见 vendor 快照），计算 digest（SHA-256，翻译自 digest.ts）
- `policies/compiler.rs`：把策略 markdown 编译为规则列表（翻译自 compiler.ts 的规则提取）

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test protocol_policies && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/protocol/ crates/sdd-core/src/policies/ crates/sdd-core/tests/protocol_policies.rs
git commit -m "feat: 平移 Agent 协议校验与策略解析模块"
```

---

### Task 14: assets 迁移（adapter 模板与 vendor 快照）

**Files:**
- Move: `packages/claude-code-adapter/*` → `assets/adapters/claude-code/`；`packages/codex-adapter/*` → `assets/adapters/codex/`；`packages/opencode-adapter/*` → `assets/adapters/opencode/`；`packages/generic-agent-adapter/*` → `assets/adapters/generic-agent/`
- Move: `vendor/*` → `assets/vendor/`（openspec/superpowers/mattpocock-skills 原样）
- Modify: `crates/sdd-core/src/commands/init.rs`（init 时按 `--agent` 参数复制对应 adapter 模板到项目）
- Test: `crates/sdd-core/tests/assets.rs`

**Interfaces:**
- Consumes: T4 init
- Produces: `fn write_adapter_files(assets_root: &str, agent: &str, project_root: &str) -> Result<(), SddError>`（把 assets/adapters/<agent>/ 下文件复制到项目 `.claude/commands` 等目标目录，路径映射见各 manifest.json 的 commandsDir/skillsDir）

**翻译自：** `packages/core/src/install/project-installer.ts`（init 时写入 Agent 接入文件）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/assets.rs`：
```rust
#[test]
fn init_writes_adapter_files_for_claude() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    sdd_core::commands::init::run_init(&cwd).unwrap(); // 默认 claude 或传入 agent
    assert!(dir.path().join(".claude/commands/sdd.auto.md").exists());
}

#[test]
fn init_with_codex_writes_rules() {
    // run_init(&cwd) with agent=codex → .codex/rules/sdd-harness.md 存在
}
```
> run_init 需支持 `--agent` 参数（默认 claude，与 Node 版 `sdd init --agent codex` 对齐）：args 中取 `agent`。

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test assets
```
Expected: FAIL。

- [ ] **Step 3: 迁移资产并实现写入**

```bash
mkdir -p assets/adapters assets/vendor
git mv packages/claude-code-adapter assets/adapters/claude-code
git mv packages/codex-adapter assets/adapters/codex
git mv packages/opencode-adapter assets/adapters/opencode
git mv packages/generic-agent-adapter assets/adapters/generic-agent
git mv vendor assets/vendor
```
各 adapter 的 `manifest.json` 中 `commandsDir`/`skillsDir` 决定复制目标（如 claude-code：`.claude/commands`、`.claude/skills/sdd-harness`；codex：`.codex/commands`、`.codex/skills/sdd-harness`）；`write_adapter_files` 实现递归复制 + 幂等（已存在同名文件不覆盖，除非内容不同）。模板中引用 `codebase-memory-mcp` 的文案（如有）改为 GitNexus/CodeGraph 表述。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test assets && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add -A assets/ crates/sdd-core/src/commands/init.rs crates/sdd-core/tests/assets.rs
git commit -m "feat: 迁移 adapter 模板与 vendor 快照为资产并接入 init 写入"
```

---

### Task 15: auto 命令与 loop 引擎

**Files:**
- Create: `crates/sdd-core/src/loop/mod.rs`、`crates/sdd-core/src/loop/engine.rs`、`crates/sdd-core/src/loop/spec.rs`、`crates/sdd-core/src/loop/store.rs`、`crates/sdd-core/src/loop/decision.rs`、`crates/sdd-core/src/commands/auto.rs`
- Test: `crates/sdd-core/tests/auto_loop.rs`

**Interfaces:**
- Consumes: 全部命令的 `run_*`
- Produces:
```rust
pub fn run_auto(cwd: &str, args: &serde_json::Value) -> Result<CommandResult, SddError>;
// 循环：new→design→plan→build→verify→review→archive；确定性步骤自动推进，
// 遇到澄清/Agent 编码/失败预算耗尽/人工决策时暂停（返回当前状态与原因）
```

**翻译自：** `packages/core/src/commands/auto.ts`、`loop/loop-engine.ts`、`loop-decision.ts`、`loop-spec.ts`、`loop-store.ts`、`loop-events.ts`。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/auto_loop.rs`：
```rust
#[test]
fn auto_pauses_on_clarification() {
    // sdd auto 无需求 → 暂停在 CLARIFYING 且 ok=false
}

#[test]
fn auto_runs_deterministic_steps() {
    // 有需求、无真实构建执行器 → 推进到 build 等待 Agent 后暂停（BUILD_WAITING_AGENT）
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test auto_loop
```
Expected: FAIL。

- [ ] **Step 3: 实现 loop 引擎**

- `loop/decision.rs`：确定性步骤判定（翻译自 loop-decision.ts：哪些命令自动执行、哪些暂停）
- `loop/spec.rs`：loop 规格序列化（loop.json 结构）
- `loop/store.rs`：运行记录（.sdd/loop/ 目录）
- `loop/engine.rs`：主循环（翻译自 loop-engine.ts，失败预算/事件记录）
- `commands/auto.rs`：入口

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test auto_loop && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/loop/ crates/sdd-core/src/commands/auto.rs crates/sdd-core/tests/auto_loop.rs
git commit -m "feat: 实现 auto 命令与 loop 自动流程引擎"
```

---

### Task 16: openspec 与 superpowers 引擎平移

**Files:**
- Create: `crates/sdd-core/src/engines/openspec/mod.rs`、`crates/sdd-core/src/engines/openspec/parser.rs`、`crates/sdd-core/src/engines/openspec/renderer.rs`、`crates/sdd-core/src/engines/openspec/validator.rs`、`crates/sdd-core/src/engines/superpowers/mod.rs`、`crates/sdd-core/src/engines/superpowers/planner.rs`
- Modify: `crates/sdd-core/src/engines/mod.rs`
- Test: `crates/sdd-core/tests/engines.rs`

**Interfaces:**
- Consumes: T8 Spec 模型
- Produces:
```rust
pub fn parse_openspec_doc(content: &str) -> Result<OpenspecDoc, SddError>;   // openspec/parser.rs
pub fn render_openspec_doc(doc: &OpenspecDoc) -> String;                      // openspec/renderer.rs
pub fn validate_openspec_doc(doc: &OpenspecDoc) -> Vec<String>;               // openspec/validator.rs（返回问题列表）
pub struct SuperpowersPlanner;
impl SuperpowersPlanner { pub fn plan_from_spec(&self, spec: &Spec) -> Plan; } // superpowers/planner.rs
```

**翻译自：** `packages/core/src/engines/openspec/*`（model/parser/renderer/validator/requirement-ids）、`engines/superpowers/planner.ts`、`project-commands.ts`、`protocol.ts`（Ponytail 受控 Policy 的 planner 语义）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/engines.rs`：
```rust
#[test]
fn openspec_parse_render_roundtrip() { /* 与 T8 的 spec 链一致：生成→渲染→解析→结构相等 */ }

#[test]
fn openspec_validator_reports_missing_requirement_ids() { /* 无 id 的需求 → 问题列表非空 */ }

#[test]
fn superpowers_planner_keeps_four_phases() { /* 与 TddEngine 输出兼容（RED/GREEN/REFACTOR/VERIFY） */ }
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test engines
```
Expected: FAIL。

- [ ] **Step 3: 实现引擎**

- `openspec/`：模型结构平移（Requirement/Scenario/Impact 等，见 openspec/model.ts）；parser/renderer 逐行对齐标记格式；validator 检查 id 唯一性/引用完整性
- `superpowers/planner.rs`：从 spec 生成带 Policy 来源摘要的计划（翻译自 planner.ts 的语义，不安装上游 npm 包）

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test engines && cargo test --workspace
```
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/engines/openspec/ crates/sdd-core/src/engines/superpowers/ crates/sdd-core/tests/engines.rs
git commit -m "feat: 平移 openspec 与 superpowers 引擎"
```

---

### Task 17: git-isolation（worktree 隔离）

**Files:**
- Create: `crates/sdd-core/src/git/isolation.rs`
- Modify: `crates/sdd-core/src/git/mod.rs`、`crates/sdd-core/src/commands/init.rs`（配置读取 workflow.gitIsolation）
- Test: `crates/sdd-core/tests/git_isolation.rs`

**Interfaces:**
- Produces:
```rust
pub struct GitIsolationManager;
impl GitIsolationManager {
    pub fn ensure_worktree(cwd: &str, change_id: &str) -> Result<WorktreeHandle, SddError>; // 创建/复用分支+worktree
    pub fn release(&self, handle: WorktreeHandle) -> Result<(), SddError>;                  // 只回收句柄，不 merge/push
}
pub struct WorktreeHandle { pub worktree_path: String, pub branch: String }
```

**翻译自：** `packages/core/src/git-isolation/manager.ts`、`model.ts`、`git-runner.ts`（worktree/branch 管理；系统不自动 merge/push/reset/clean）。

- [ ] **Step 1: 写失败测试**

`crates/sdd-core/tests/git_isolation.rs`：
```rust
#[test]
fn ensure_worktree_creates_branch_and_dir() {
    let dir = tempfile::tempdir().unwrap();
    // git init + commit base + .sdd/config.json 配 workflow.gitIsolation=true
    let handle = GitIsolationManager::ensure_worktree(&cwd, "20260802-01").unwrap();
    assert!(std::path::Path::new(&handle.worktree_path).exists());
    // .sdd/ 在控制根目录，业务代码在 worktree
}
```

- [ ] **Step 2: 跑测试确认失败**

```bash
cargo test -p sdd-core --test git_isolation
```
Expected: FAIL。

- [ ] **Step 3: 实现 isolation**

`git/isolation.rs`：`git worktree add <path> -b sdd/<change-id>`（翻译自 manager.ts 的分支命名与复用逻辑）；`release` 只删除句柄引用（drop 语义），不执行 merge/push/reset/clean/delete-worktree（对齐 Node 版约束）。

- [ ] **Step 4: 跑测试确认通过**

```bash
cargo test -p sdd-core --test git_isolation
```
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/sdd-core/src/git/isolation.rs crates/sdd-core/src/git/mod.rs crates/sdd-core/tests/git_isolation.rs
git commit -m "feat: 实现 git worktree 隔离"
```

---

### Task 18: 移除 Node 残留并更新项目文档

**Files:**
- Delete: `package.json`、`package-lock.json`、`tsconfig.json`、`tsconfig.base.json`、`eslint.config.js`、`vitest.config.ts`、`node_modules/`、`scripts/*.mjs`、`scripts/lib/`（install.sh/uninstall.sh 保留改造）、`packages/`（已迁至 crates/assets）、`THIRD_PARTY_NOTICES.md`（重写）
- Modify: `scripts/install.sh`、`scripts/uninstall.sh`（改为构建 Rust 二进制并注册全局命令）
- Modify: `README.md`、`docs/architecture.md`、`docs/CLI.md`、`docs/security.md`、`docs/command-contract.md`、`docs/state-machine.md`、`docs/adapters.md`、`docs/schemas.md`、`docs/agent-native-ux-spec.md`、`AGENTS.md`、`CLAUDE.md`
- Test: `crates/sdd-core/tests/end_to_end.rs`（完整工作流 e2e）

**Global Constraints 补充：** 文档中所有 `codebase-memory-mcp`、`packages/*` 引用替换为 Rust 表述（knowledge 模块 / crates 路径）；README 安装节改为 `cargo build --release` + install.sh；THIRD_PARTY_NOTICES.md 移除 codebase-memory-mcp 条目（其许可证不再适用），新增 GitNexus/CodeGraph 为外部 CLI 依赖说明。

- [ ] **Step 1: 写 e2e 集成测试（在删除前验证全链）**

`crates/sdd-core/tests/end_to_end.rs`：
```rust
#[test]
fn full_workflow_init_to_archive() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let run = |cmd: &str, args: Option<serde_json::Value>| {
        sdd_core::run(&sdd_core::contracts::CommandRequest {
            command: cmd.into(), cwd: cwd.clone(), args,
        }).unwrap_or_else(|e| panic!("命令 {cmd} 失败: {e}"))
    };
    let init = run("init", None);
    assert_eq!(init.state, "INDEX_READY");
    let new = run("new", Some(serde_json::json!({ "requirement": "实现订单取消功能" })));
    assert!(new.ok, "new 应成功");
    let design = run("design", None);
    assert!(design.ok);
    let plan = run("plan", None);
    assert!(plan.ok);
    let build = run("build", Some(serde_json::json!({ "sub": "next" })));
    assert_eq!(build.state, "BUILD_WAITING_AGENT");
    // 模拟 Agent 完成 RED 任务后提交结果（写 runs/<run-id>/tasks/<task>.result.json 后 complete）
    // …（构造 evidence/verification 合法的结果文件，走 build complete）
    let verify = run("verify", None);
    let review = run("review", None);
    let archive = run("archive", None);
    assert!(archive.ok && archive.state == "ARCHIVED");
}
```
> 该测试需要 Task 11 的 build complete 裁决可构造合法结果；若 verify 因任务未完成而失败，测试内先完成全部任务链（4 个任务逐个 complete），再 verify/review/archive。

- [ ] **Step 2: 实现 install.sh / uninstall.sh 改造**

`scripts/install.sh` 保留整体框架（检测/回滚/注册），核心动作改为：
```bash
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"
install -m 0755 "$REPO_ROOT/target/release/sdd" "$PREFIX/bin/sdd"
```
（Windows 下复制 `sdd.exe` 到用户 bin；`lib/installation.sh` 中原 npm/npx 清理逻辑删除，改为清理旧的 `sdd` 二进制。）

- [ ] **Step 3: 删除 Node 残留**

```bash
git rm -r package.json package-lock.json tsconfig.json tsconfig.base.json eslint.config.js vitest.config.ts scripts/validate-release.mjs scripts/validate-lockfile.mjs scripts/validate-schemas.mjs scripts/vendor-manifest.mjs packages node_modules 2>/dev/null
```
> `node_modules` 若被 .gitignore 忽略则直接删除目录；删除前确认 `git status` 中 `packages/` 已无未迁移文件（各包 src 均已平移至 crates/）。

- [ ] **Step 4: 更新文档**

README（能力清单去掉"自动托管 codebase-memory-mcp"，改为"自动探测并索引 GitNexus / CodeGraph 知识图谱，不可用时降级受限文件扫描"；环境要求改为 Rust 工具链）、docs/* 同步更新（architecture.md 的分层图中 `codebase-memory adapter` 改为 `knowledge 适配器（GitNexus/CodeGraph）`；CLI.md 的 codebase 子命令描述更新 provider 枚举）、AGENTS.md/CLAUDE.md 的包结构段更新为 crates 结构。

- [ ] **Step 5: 全量验证**

```bash
cargo build --release && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check
```
Expected: 全绿；e2e 测试 PASS。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: 移除 Node 残留并迁移为纯 Rust 项目（含文档与安装脚本更新）"
```

---

### Task 19: 全量审查与修复（自检 + clippy 零告警）

**Files:**
- Modify: 视自检结果
- Test: `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`

- [ ] **Step 1: 契约逐项核对**

对照 `docs/command-contract.md` 与 `docs/CLI.md`，逐命令核对：命令名/参数/`--json` 输出键名（camelCase）/退出码/错误码。列出差异并修复。
重点核对点：CommandResult 的 JSON 序列化键名（changeId/exitCode/actionRequired 等必须与原 Node 版输出一致，serde rename 使用 `#[serde(rename_all = "camelCase")]`）。

- [ ] **Step 2: 全量验证**

```bash
cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check
```
Expected: 全绿。

- [ ] **Step 3: Commit（如无差异可跳过）**

```bash
git add -A && git commit -m "fix: 修复契约核对与 clippy 告警"
```

---

### Task 20: open-code-review-delegate 审核与最终提交

**Files:**
- Modify: 视审核发现
- 前置：安装 `npm install -g @alibaba-group/open-code-review`（提供 `ocr` CLI；委派模式不需要 LLM 配置，由宿主代理执行审查）

- [ ] **Step 1: 安装 ocr CLI 并确认可用**

```bash
npm install -g @alibaba-group/open-code-review && ocr --version
```
Expected: 版本号输出。若安装失败，改用仓库内 `ocr delegate preview --help` 验证。

- [ ] **Step 2: 获取审查文件清单与规则**

```bash
ocr delegate preview                       # 工作区模式：列出可审查文件
ocr delegate rule <crates/sdd-core/src/...>  # 按文件获取审查规则组
```
Expected: 文件清单与规则输出。

- [ ] **Step 3: 逐文件审查（宿主代理执行）**

按 delegate 模式工作流：对每个预览文件，取 `git diff HEAD -- <path>`，结合规则组做深入审查；每条发现记录 `path/content/start_line/end_line/category/severity`；按严重级别分类：Critical/High 必须修复；Medium 附上下文处理；Low 仅在明确有价值时记录；误报静默丢弃。

- [ ] **Step 4: 修复 Critical/High 问题并回归**

```bash
cargo test --workspace && cargo clippy --workspace -- -D warnings
```
Expected: 修复后全绿。

- [ ] **Step 5: 最终提交（不推送）**

```bash
git add -A
git commit -m "fix: 按 open-code-review-delegate 审核结果修复问题"
git log --oneline -5   # 确认提交
```
**明确不执行 `git push`。**

---

## Self-Review 记录（写作时执行）

**Spec 覆盖检查：**
- 双 crate 形态 → T1/T18（骨架与清理）
- 移除 codebase-memory-mcp → T3（schema）/T14（模板文案）/T18（文档与依赖）/T6（替代实现）
- GitNexus/CodeGraph 双引擎按 intent 路由 → T5/T6/T7
- 存储重构 schema 5 个 → T3
- 契约稳定（命令/退出码/E_ 错误码）→ T1/T19（契约核对）
- 9 模块划分 → T1（骨架）+ 各任务模块创建
- fixtures 集成测试 → T18 e2e
- open-code-review-delegate → T20
- 不推送 → T20 Step 5

**占位符扫描：** 所有 `todo_*` 命名均为"按指明的源文件翻译实现"的执行指引，源文件路径与语义已在步骤内指明；无 TBD/待办模糊项。

**类型一致性：** `WorkflowState`（T2）→ T4/T8 复用并扩展辅助方法（T8 明示）；`CommandResult` 字段（T1）全链一致；`KnowledgeIntent/QueryResult`（T5）→ T6/T7 一致；`AgentActionRequired`（T1 占位 → T11 完整化）已显式标注替换点；`Plan/TaskDef`（T9）→ T11/T16 一致。
