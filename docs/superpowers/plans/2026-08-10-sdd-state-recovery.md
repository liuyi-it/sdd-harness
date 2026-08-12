# SDD 澄清状态恢复与自动续跑修复实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with review checkpoints.

**Goal:** 让 `sdd new`、`sdd auto` 在澄清阶段或进程中断后可幂等恢复，避免 `NEW_STARTED` 把工作流锁死在不可执行状态。

**Architecture:** 保留 `NEW_STARTED` 作为短暂的可恢复阶段，不新增手工修改 `.sdd` 文件的旁路。`new` 根据当前变更、运行输入和规格制品识别中断续跑；`auto --resume` 显式把答案传递给 `new`；状态查询和阶段错误给出可执行的恢复命令。通过运行时 JSON 的原子写入和现有状态锁保证并发安全。

**Tech Stack:** Rust 2021、Cargo workspace、`serde_json`、现有 `StateStore`/`RuntimeStore`、内置单元测试与集成测试、Clap CLI。

## Global Constraints

- 保持现有 `CommandRequest`/`CommandResult` camelCase JSON 契约。
- 不直接修改 `.sdd/state.json`、`.sdd/runtime.json` 或其它运行时状态；所有恢复必须经 Core API 完成。
- 不回滚或重构当前已有的 runtime 迁移改动；仅复用 `RuntimeStore`/`StateStore` 读写。
- 不改变已有 `INDEX_READY -> new -> CLARIFYING/SPEC_READY` 的正常结果。
- `NEW_STARTED` 只允许在存在当前 change/run 时续跑；续跑时由 `run_new` 从 runtime 取得需求，若需求输入缺失返回 `E_MISSING_ARTIFACT`，不得猜测回滚。
- 不在本计划中重写 `SpecEngine` 的领域问题词典；先修复状态可恢复性，澄清模板按独立后续变更处理。
- 既有工作区存在未提交用户改动；只修改本计划列出的源文件、测试、契约和文档。

---

### Task 1: 为 `new` 建立幂等中断续跑边界

**Files:**
- Modify: `crates/sdd-core/src/commands/new.rs`
- Test: `crates/sdd-core/tests/new_spec.rs`
- Test: `crates/sdd-core/tests/end_to_end.rs`

**Interfaces:**
- `run_new` 继续保持现有签名。
- 新增私有判断 `can_resume_new(state: &WorkflowState) -> bool`：仅当 `currentPhase == NEW_STARTED` 且同时存在合法 `currentChangeId`、`currentRunId` 时返回 true。
- `continuing` 包含 `CLARIFYING`、`SPEC_READY`、`FAILED` 和可恢复的 `NEW_STARTED`。

- [ ] **Step 1: 写中断续跑失败测试**

在临时项目中先正常执行 `new` 使其进入 `CLARIFYING`，随后只通过 `StateStore::update` 将 phase 模拟为 `NEW_STARTED`，保留 current change/run 和 runtime 需求输入；再次调用 `run_new` 并传入完整答案，断言当前变更被继续使用、最终状态为 `SPEC_READY`，而不是 `E_ACTIVE_CHANGE_EXISTS`。

```rust
#[test]
fn new_answers_resume_interrupted_new_started_change() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let first = new_request(dir.path(), "实现 review 命令");
    assert_eq!(first.state, "CLARIFYING");

    let cwd = dir.path().to_string_lossy().to_string();
    sdd_core::state::StateStore::new(cwd.clone())
        .update(|state| {
            state.current_phase = "NEW_STARTED".into();
            state.in_progress_phase = Some("NEW_STARTED".into());
        })
        .unwrap();

    let answers = json!({
        "Q-ACTOR": "项目开发者",
        "Q-AUTHORIZATION": "仅当前变更可执行",
        "Q-ACTION": "调用 review 后端",
        "Q-INTERFACE": "sdd review --json",
        "Q-PRECONDITION": "verify 已完成",
        "Q-RESULT": "生成结构化报告",
        "Q-FAILURE": "失败返回稳定错误码",
        "Q-TEST": "覆盖成功、缺失和失败路径"
    });
    let result = run_new(
        &cwd,
        Some(&json!({ "answers": answers })),
        &SpecEngine::new(),
    )
    .unwrap();
    assert_eq!(result.state, "SPEC_READY");
}
```

- [ ] **Step 2: 运行失败测试**

运行：

```bash
cargo test -p sdd-core --test new_spec new_answers_resume_interrupted_new_started_change
```

预期：当前实现返回 `E_ACTIVE_CHANGE_EXISTS`，证明阶段门禁无法识别可恢复的 `NEW_STARTED`。

- [ ] **Step 3: 实现最小续跑判断**

在阶段前置检查前读取当前 state，并将 `continuing` 扩展为：

```rust
let interrupted_new = can_resume_new(&state);
let continuing = matches!(
    state.current_phase.as_str(),
    "CLARIFYING" | "SPEC_READY" | "FAILED"
) || interrupted_new;
```

续跑时复用现有 `currentChangeId`、`currentRunId`、workspace、runtime `runs.<runId>.input` 和 change 目录，不重新创建目录、不创建新 worktree。

将需求语义分析移动到第一次写入 `NEW_STARTED` 之前。这样纯分析失败不会留下新的中间状态；文件和 runtime 写入仍受现有 `sdd new` 锁保护。

- [ ] **Step 4: 运行通过测试并覆盖无 spec 的恢复**

新增一个断点测试：`NEW_STARTED` 具备 current change/run 但尚无 spec 制品时，`sdd new --answers` 仍可重新生成 spec；若缺少 runtime 需求输入，则必须返回 `E_MISSING_ARTIFACT`，不得创建新变更。

运行：

```bash
cargo test -p sdd-core --test new_spec
cargo test -p sdd-core --test end_to_end
```

预期：新增续跑测试和既有规格流程全部通过。

---

### Task 2: 让 `auto --resume` 传递澄清答案并处理 `NEW_STARTED`

**Files:**
- Modify: `crates/sdd-core/src/commands/auto.rs`
- Modify: `crates/sdd-cli/src/main.rs`
- Test: `crates/sdd-core/tests/auto_loop.rs`
- Test: `crates/sdd-cli/tests/cli_smoke.rs`

**Interfaces:**
- `auto` 新增可选 `answers` JSON 对象，与 `new --answers` 使用同一 camelCase payload。
- `run_auto` 的第一阶段将 `NEW_STARTED` 纳入 `new` 续跑集合，并把 `answers` 透传给 `run_new`。
- `sdd auto --resume --answers '{"Q-ACTION":"调用 review 后端"}'` 是澄清恢复的自动入口，不新增第二套答案格式。

- [ ] **Step 1: 写 auto 续跑失败测试**

在 `auto_loop.rs` 中使用现有 `setup` fixture，先进入 `CLARIFYING`，再模拟 `NEW_STARTED`，验证 resume 和 answers 两条路径。
```rust
#[test]
fn auto_resume_retries_new_started_and_accepts_answers() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let first = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": "实现 review 命令" })),
    })
    .unwrap();
    assert_eq!(first.state, "CLARIFYING");

    sdd_core::state::StateStore::new(cwd.clone())
        .update(|state| {
            state.current_phase = "NEW_STARTED".into();
            state.in_progress_phase = Some("NEW_STARTED".into());
        })
        .unwrap();

    let paused = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "resume": true })),
    })
    .unwrap();
    assert_eq!(paused.state, "CLARIFYING");

    let resumed = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({
            "resume": true,
            "answers": {
                "Q-ACTOR": "项目开发者",
                "Q-AUTHORIZATION": "仅当前变更可执行",
                "Q-ACTION": "调用 review 后端",
                "Q-INTERFACE": "sdd review --json",
                "Q-PRECONDITION": "verify 已完成",
                "Q-RESULT": "生成结构化报告",
                "Q-FAILURE": "失败返回稳定错误码",
                "Q-TEST": "覆盖成功、缺失和失败路径"
            }
        })),
    })
    .unwrap();
    assert_eq!(resumed.state, "BUILD_WAITING_AGENT");
}
```

- [ ] **Step 2: 运行失败测试**

运行：

```bash
cargo test -p sdd-core --test auto_loop auto_resume_retries_new_started_and_accepts_answers
```

预期：当前实现不会处理 `NEW_STARTED`，且 `auto` 不会传递答案。

- [ ] **Step 3: 实现 auto 参数透传**

在 `run_auto` 构造 `new_args` 时复制 `answers`：

```rust
for key in ["changeId", "nonInteractive", "timeout", "answers"] {
    if let Some(value) = args.get(key) {
        new_args.insert(key.to_string(), value.clone());
    }
}
```

将第一阶段条件扩展为：

- 无 current change/run 的 `NEW_STARTED` 不自动猜测恢复对象；`new` 续跑在缺少输入时返回 `E_MISSING_ARTIFACT` 并建议 `sdd status`，有 current change/run 时阶段建议为 `sdd auto --resume`。

```rust
if matches!(
    phase.as_str(),
    "INDEX_READY" | "CLARIFYING" | "FAILED" | "PAUSED" | "NEW_STARTED"
) {
    // INDEX_READY 仍要求 requirement；其它续跑阶段允许从 runtime runs.input 读取。
}
```

在 Clap `Command::Auto` 增加 `answers: Option<serde_json::Value>`，复用 `parse_answers`，并在 `build_request` 中写入 `answers`。

- [ ] **Step 4: 运行 auto 与 CLI 契约测试**

运行：

```bash
cargo test -p sdd-core --test auto_loop
cargo test -p sdd-cli --test cli_smoke
```

预期：`--answers` 仅接受 JSON 对象，旧版 auto 参数和所有既有 loop 测试保持通过。

---

### Task 3: 修正状态建议和阶段错误的恢复指引

**Files:**
- Modify: `crates/sdd-core/src/commands/status.rs`
- Modify: `crates/sdd-core/src/lib.rs`
- Modify: `crates/sdd-core/src/commands/new.rs`
- Test: `crates/sdd-core/tests/contracts.rs`
- Test: `crates/sdd-core/tests/end_to_end.rs`

**Interfaces:**
- `next_command("NEW_STARTED")` 返回 `sdd auto --resume`。
- `ensure_phase` 在 `NEW_STARTED` 上拒绝 design/verify/review 时，`CommandError.next` 必须是 `sdd auto --resume`，不再返回泛化的 `sdd status`。
- 无 current change/run 的 `NEW_STARTED` 仍返回 `E_STATE_CORRUPTED`/`E_ACTIVE_CHANGE_EXISTS`，且 next 为 `sdd status`，避免猜测恢复对象。

- [ ] **Step 1: 写恢复提示测试**

```rust
#[test]
fn new_started_points_to_auto_resume() {
    assert_eq!(
        sdd_core::commands::status::next_command("NEW_STARTED").as_deref(),
        Some("sdd auto --resume")
    );
}
```

再用临时项目验证阶段错误的 next 字段：

```rust
let dir = tempfile::tempdir().unwrap();
std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
let cwd = dir.path().to_string_lossy().to_string();
sdd_core::run(&sdd_core::contracts::CommandRequest {
    command: "init".into(),
    cwd: cwd.clone(),
    args: None,
})
.unwrap();
sdd_core::state::StateStore::new(cwd.clone())
    .update(|state| {
        state.current_phase = "NEW_STARTED".into();
        state.current_change_id = Some("change-test".into());
        state.current_run_id = Some("run-test".into());
    })
    .unwrap();
let error = sdd_core::run(&sdd_core::contracts::CommandRequest {
    command: "verify".into(),
    cwd,
    args: None,
})
.unwrap_err();
assert_eq!(error.next.as_deref(), Some("sdd auto --resume"));
```

- [ ] **Step 2: 实现下一步映射和安全错误**

只增加 `NEW_STARTED` 的确定性映射，不放宽 verify/review 的阶段门禁；恢复仍必须先经过 `new`/`auto`。

- [ ] **Step 3: 运行契约测试**

运行：

```bash
cargo test -p sdd-core --test contracts --test end_to_end
```

预期：错误码、退出码和 next 字段稳定。

---

### Task 4: 更新用户可见契约和 OMP 操作提示

**Files:**
- Modify: `docs/CLI.md`
- Modify: `docs/state-machine.md`
- Modify: `docs/command-contract.md`
- Modify: `assets/adapters/omp/commands/sdd.new.md`
- Modify: `assets/adapters/omp/commands/sdd.md`
- Test: `crates/sdd-cli/tests/cli_smoke.rs`

**Interfaces:**
- 文档公开 `NEW_STARTED` 的恢复流程：优先 `sdd auto --resume`，手动回答使用 `sdd new --answers`，自动回答使用 `sdd auto --resume --answers`。
- OMP 模板遇到 `CLARIFYING` 不重试空命令，不要求用户直接编辑 `.sdd` 文件。
- 文档明确：当前 CLI 必须与工作区 Core/runtime schema 版本匹配；升级 sdd 后再执行工作流，禁止用旧二进制读取新 runtime。

- [ ] **Step 1: 更新 CLI 和状态机说明**

加入完整示例：

```bash
sdd auto "在 review 命令中接入外部代码审查后端"
# 收到 CLARIFYING 后
sdd auto --resume --answers '{"Q-ACTION":"调用 review 后端"}'
# 若进程中断并停在 NEW_STARTED
sdd auto --resume
```

- [ ] **Step 2: 更新 OMP 命令模板**

明确 OMP Agent 先读取 `CommandResult.next`；遇到 `sdd auto --resume` 时继续当前变更，不调用新的 `sdd new`，不直接修改 `.sdd`。

- [ ] **Step 3: 运行 CLI 帮助和 schema 验证**

运行：

```bash
cargo run -q -p sdd-cli -- auto --help
cargo test -p sdd-cli --test cli_smoke
cargo test -p sdd-core --test schema_validator
```

预期：帮助显示 `--answers`，CLI JSON 契约和所有 schema 测试通过。

---

## 最终验收

1. 运行 `cargo fmt --check`。
2. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
3. 运行 `cargo test --workspace`。
4. 通过 `crates/sdd-core/tests/auto_loop.rs` 的临时 fixture 执行澄清→中断模拟→`auto --resume`→answers→`BUILD_WAITING_AGENT` smoke；需要单独验证 `new --answers` 直达 `SPEC_READY`。
5. 再次运行 `sdd verify --json`、`sdd review --json`；若当前 `.sdd` 仍是旧版 metadata，必须先报告版本/格式不匹配，不直接迁移或删除用户状态。
6. 不直接编辑 `.sdd` 状态文件；所有修复都通过 Core 命令完成。

## 方案自审

- **需求覆盖：** Task 1 覆盖 `NEW_STARTED` 中断和 `new --answers`；Task 2 覆盖 auto resume/answers；Task 3 覆盖错误恢复提示；Task 4 覆盖用户和 OMP 操作契约。
- **风险控制：** 不放宽 verify/review 的质量门禁，不自动猜测缺失 change/run，不删除旧状态文件，不改 SpecEngine 的现有问题词典。
- **类型一致性：** CLI `answers` 使用 `serde_json::Value`，Core 透传到既有 `NewArgs.answers: HashMap<String, String>`；`NEW_STARTED` 续跑复用既有 `currentChangeId/currentRunId`。
- **已知边界：** 旧版 PATH 二进制与当前 runtime migration 的兼容迁移不在本次代码修复中；验收使用仓库当前 Core，并在文档中明确版本匹配要求。
- **没有未决设计选择：** 采用最小安全修复，不新增 `recover` 命令，不引入新的状态文件或依赖。