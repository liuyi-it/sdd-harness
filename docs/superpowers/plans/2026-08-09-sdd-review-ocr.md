# `sdd review` OCR 集成实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `sdd review` 先执行确定性审查，再自动调用可选的 Alibaba Open Code Review；缺少 `ocr` 时只警告并返回原版结果，OCR 已启动但失败时硬失败。

**Architecture:** 在 `quality::ocr` 中隔离配置解析、JSON 模型、finding 校验和外部进程执行；`commands::review` 只负责确定性门禁、串行编排、结果合并和 runtime 报告写入。默认 `quality.ocr.mode=auto`，由 `ocr` 的可发现性决定是否补充审查。

**Tech Stack:** Rust 2021、标准库 `std::process`/`std::thread`、现有 `serde`/`serde_json`、Cargo workspace 集成测试。

## Global Constraints

- 必须保持 Rust edition 2021、现有 `CommandRequest`/`CommandResult` camelCase 契约。
- 不新增 Cargo 依赖；OCR 通过 `Command::new` 直接执行，不得经过 shell。
- `sdd review` 必须先完成原版确定性审查；敏感信息、范围和其他硬阻断不得启动 OCR。
- `quality.ocr.mode=auto` 找不到 OCR 时返回原版结果并携带 `W_OCR_NOT_FOUND`；`required` 找不到 OCR 返回 `E_REVIEW_BACKEND_UNAVAILABLE`；`off` 不启动 OCR。
- OCR 已启动后发生启动失败（非 NotFound）、超时、非零退出、失败状态或非法 JSON/finding 必须硬失败，不得静默回退。
- 外部 finding 的路径、行号、严重级别和类别必须校验；不得持久化 API key、prompt、thinking 或完整 stderr。
- 每个实现任务只运行针对自身变更的测试；跳过 formatter、clippy 和 workspace 全量测试，统一在最终验收阶段运行。
- 不修改、回滚或重构当前已有的 runtime 迁移；只通过现有 runtime store 读写配置和报告。

---

### Task 1: 扩展报告、错误码和 OCR 配置契约

**Files:**
- Modify: `crates/sdd-core/src/contracts.rs:40-72`
- Modify: `crates/sdd-core/src/quality/report.rs:5-80`
- Modify: `crates/sdd-core/src/commands/init.rs:155-226`
- Modify: `schemas/report.schema.json:15-32`
- Test: `crates/sdd-core/src/quality/report.rs`（新增序列化单元测试）
- Test: `crates/sdd-core/tests/contracts.rs`
- Test: `crates/sdd-core/tests/init_status.rs`

**Interfaces:**
- Produces `E_REVIEW_BACKEND_UNAVAILABLE`、`E_REVIEW_BACKEND_TIMEOUT`、`E_REVIEW_BACKEND_FAILED`、`E_REVIEW_BACKEND_INVALID_OUTPUT` 的稳定退出码映射。
- `Issue` 新增全是可选的 `category: Option<String>`、`start_line: Option<u32>`、`end_line: Option<u32>`、`existing_code: Option<String>`、`suggestion_code: Option<String>`、`origin: Option<String>` 字段，保持 `#[serde(rename_all = "camelCase")]`。
- init 的默认 runtime config 写入 `quality.ocr.mode="auto"` 与 `quality.ocr.command="ocr"`；旧配置缺少该节点时由后续 Task 2 按默认值处理。

- [x] **Step 1: 写报告字段的失败测试**

在 `quality/report.rs` 的测试模块中加入一个 `Issue` 序列化测试，先断言以下 JSON 字段存在且为 camelCase：

```rust
#[test]
fn issue_serializes_ocr_location_and_suggestion_fields() {
    let issue = Issue {
        code: "OCR_FINDING".into(),
        severity: "high".into(),
        message: "输入未校验".into(),
        file: Some("src/handler.rs".into()),
        category: Some("security".into()),
        start_line: Some(42),
        end_line: Some(43),
        existing_code: Some("old".into()),
        suggestion_code: Some("new".into()),
        origin: Some("ocr".into()),
    };
    let value = serde_json::to_value(issue).unwrap();
    assert_eq!(value["startLine"], 42);
    assert_eq!(value["endLine"], 43);
    assert_eq!(value["suggestionCode"], "new");
    assert!(value.get("start_line").is_none());
}
```

运行：`cargo test -p sdd-core quality::report::tests::issue_serializes_ocr_location_and_suggestion_fields`

预期：失败，原因是 `Issue` 尚无这些字段。

- [x] **Step 2: 写错误码和默认配置失败测试**

在 `crates/sdd-core/tests/contracts.rs` 增加四个错误码退出码断言；在 `init_status.rs` 的 init 测试中增加：

```rust
assert_eq!(runtime["config"]["quality"]["ocr"]["mode"], "auto");
assert_eq!(runtime["config"]["quality"]["ocr"]["command"], "ocr");
```

运行：`cargo test -p sdd-core --test contracts --test init_status`

预期：失败，原因是错误码未映射且默认 config 未写入 OCR 节点。

- [x] **Step 3: 实现最小契约变更**

在 `contracts.rs` 的 `error_exit_codes` 中加入：

```rust
"E_REVIEW_BACKEND_UNAVAILABLE" => 5,
"E_REVIEW_BACKEND_TIMEOUT" => 124,
"E_REVIEW_BACKEND_FAILED" => 8,
"E_REVIEW_BACKEND_INVALID_OUTPUT" => 8,
```

扩展 `Issue` 的可选字段并为每个字段添加 `skip_serializing_if = "Option::is_none"`。在 init 默认 `quality` 对象中加入：

```rust
"ocr": { "mode": "auto", "command": "ocr" }
```

在 `report.schema.json` 的 issue properties 中增加同名 camelCase 字段，类型分别为 `string|null`、`integer|null`、`string|null`；保留 `additionalProperties: true`。

- [x] **Step 4: 运行针对性测试并提交**

运行：`cargo test -p sdd-core --lib quality::report::tests && cargo test -p sdd-core --test contracts --test init_status`

预期：相关测试通过，输出无失败。提交：`feat: 扩展 review OCR 报告契约`。

---

### Task 2: 实现 OCR 配置、JSON 适配器和安全进程执行器

**Files:**
- Create: `crates/sdd-core/src/quality/ocr.rs`
- Modify: `crates/sdd-core/src/quality/mod.rs:3-6`
- Test: `crates/sdd-core/src/quality/ocr.rs`（模块单元测试）

**Interfaces:**
- `pub enum OcrMode { Auto, Off, Required }`
- `pub struct OcrConfig { pub mode: OcrMode, pub command: String }`
- `impl OcrConfig { pub fn from_config(config: &serde_json::Value) -> Result<Self, SddError>; }`
- `pub struct OcrComment`：反序列化官方 snake_case 字段 `path/content/suggestion_code/existing_code/start_line/end_line/category/severity`。
- `pub struct OcrOutput`：反序列化 `status/comments/session_id/summary`，缺失 comments 时按空数组处理。
- `pub enum OcrExecution { NotFound, Completed(OcrOutput) }`
- `pub trait OcrExecutor { fn execute(&self, cwd: &Path, command: &str, timeout: Duration) -> Result<OcrExecution, SddError>; }`
- `pub struct SystemOcrExecutor;` 实现无 shell 的子进程执行。
- `pub fn parse_output(bytes: &[u8]) -> Result<OcrOutput, SddError>;`
- `pub fn validate_output(output: OcrOutput, cwd: &Path, changed_files: &BTreeSet<String>) -> Result<OcrOutput, SddError>;`

- [x] **Step 1: 写配置和 JSON 解析失败测试**

先在新模块中写测试，覆盖默认 config、`off`、`required`、非法 mode、成功 JSON、缺失 comments、非法 status：

```rust
#[test]
fn missing_ocr_config_defaults_to_auto_command() {
    let config = OcrConfig::from_config(&serde_json::json!({})).unwrap();
    assert_eq!(config.mode, OcrMode::Auto);
    assert_eq!(config.command, "ocr");
}

#[test]
fn parses_off_mode_and_structured_comment() {
    let config = OcrConfig::from_config(&serde_json::json!({
        "quality": { "ocr": { "mode": "off", "command": "/opt/ocr" } }
    })).unwrap();
    assert_eq!(config.mode, OcrMode::Off);
    let output = parse_output(br#"{
        "status":"success",
        "session_id":"s-1",
        "comments":[{"path":"src/a.rs","content":"修复","start_line":2,"end_line":3,"category":"bug","severity":"high"}]
    }"#).unwrap();
    assert_eq!(output.comments[0].start_line, 2);
    assert_eq!(output.session_id.as_deref(), Some("s-1"));
}
```

运行：`cargo test -p sdd-core quality::ocr::tests`

预期：失败，原因是模块和类型尚未实现。

- [x] **Step 2: 写 finding、状态和路径校验失败测试**

```rust
#[test]
fn rejects_failed_status() {
    let dir = tempfile::tempdir().unwrap();
    let output = parse_output(br#"{"status":"failed","comments":[]}"#).unwrap();
    let error = validate_output(output, dir.path(), &std::collections::BTreeSet::new())
        .unwrap_err();
    assert_eq!(error.code, "E_REVIEW_BACKEND_FAILED");
}

#[test]
fn rejects_path_escape() {
    let dir = tempfile::tempdir().unwrap();
    let output = parse_output(br#"{
        "status":"success",
        "comments":[{"path":"../secret","content":"x","start_line":1,"end_line":1,"category":"bug","severity":"low"}]
    }"#).unwrap();
    let error = validate_output(
        output,
        dir.path(),
        &std::iter::once("../secret".to_string()).collect(),
    ).unwrap_err();
    assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
}

#[test]
fn rejects_zero_or_reversed_line_range_and_unknown_metadata() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.rs"), "fn main() {}\n").unwrap();
    let changed = std::iter::once("src/a.rs".to_string()).collect();
    for raw in [
        br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":0,"end_line":1,"category":"bug","severity":"low"}]}"#.as_slice(),
        br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":2,"end_line":1,"category":"bug","severity":"low"}]}"#.as_slice(),
        br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"unknown","severity":"low"}]}"#.as_slice(),
        br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"bug","severity":"urgent"}]}"#.as_slice(),
    ] {
        let output = parse_output(raw).unwrap();
        let error = validate_output(output, dir.path(), &changed).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }
}
```

运行：`cargo test -p sdd-core quality::ocr::tests`

预期：失败，原因是校验器尚不存在。


- [x] **Step 3: 实现配置和数据模型**

实现 `OcrConfig::from_config`：缺失 `quality.ocr` 时返回 `auto/ocr`；`mode` 只接受 `auto/off/required`；command 必须是非空字符串。非法配置返回 `E_STATE_CORRUPTED`。

实现 `OcrOutput`/`OcrComment` 的 `Deserialize`，使用 `#[serde(default)]` 处理可选数组和元数据；状态只允许 `success`、`skipped`，其他状态在 `validate_output` 转成 `E_REVIEW_BACKEND_FAILED`。

- [x] **Step 4: 实现输出和安全校验**

`validate_output` 必须：

- 确认 comment path 是相对路径、没有 `..`、存在于 `changed_files`；通过 `GitInspector::resolve_repo_path` 检查仓库边界；
- 确认文件可读且行数覆盖 `start_line..=end_line`；
- 确认 category 属于 `bug/security/performance/maintainability/test/style/documentation/other`；
- 确认 severity 属于 `critical/high/medium/low`；
- 对空 path、零行号、逆序行号、非法建议内容返回 `E_REVIEW_BACKEND_INVALID_OUTPUT`；
- 不读取或保存 `thinking` 字段。

`parse_output` 使用 `serde_json::from_slice`，解析失败统一返回 `E_REVIEW_BACKEND_INVALID_OUTPUT`，消息只包含解析失败的安全摘要。

- [x] **Step 5: 实现 `SystemOcrExecutor`**

用 `Command::new(command)`、固定 argv `review`, `--format`, `json`、`current_dir(cwd)`、`Stdio::piped()` 启动；不得调用 `sh -c`。Unix 下为后端建立独立 process group，stdout/stderr 各用 reader thread 持续 drain，最多保存 4 MiB，超过部分只标记截断；主线程用 `try_wait` + 20ms sleep 轮询 `timeout`，超时、状态读取失败或父进程退出后均清理该进程组再回收 reader，避免后代继承管道造成永久等待。

仅当 spawn 返回 `ErrorKind::NotFound` 时返回 `OcrExecution::NotFound`；其他 spawn 错误返回 `E_REVIEW_BACKEND_UNAVAILABLE`。非零退出返回 `E_REVIEW_BACKEND_FAILED`，成功退出后调用 `parse_output`。

- [x] **Step 6: 运行适配器测试并提交**

运行：`cargo test -p sdd-core quality::ocr::tests`

预期：全部适配器测试通过，输出无失败。提交：`feat: 增加 OCR JSON 适配器`。

---

### Task 3: 将 OCR 串入 review 命令并保留原版降级行为

**Files:**
- Modify: `crates/sdd-core/src/commands/review.rs:26-282`
- Modify: `crates/sdd-core/src/quality/report.rs:42-80`
- Test: `crates/sdd-core/src/commands/review.rs`（新增编排单元测试）
- Test: `crates/sdd-core/tests/quality_chain.rs`

**Interfaces:**
- 保持现有 `pub fn run_review(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError>` 不变。
- 新增私有 `run_review_with_executor<E: OcrExecutor>`、`merge_ocr_comment`、`set_ocr_status` 和 `ocr_metadata`，公开入口使用 `SystemOcrExecutor`；warning 由编排函数构造为结构化 JSON。

- [x] **Step 1: 写原版先行和 OCR 缺失测试**

在 review 命令测试模块定义 `FakeExecutor`（记录 `calls: usize`，返回构造时保存的 `Result<OcrExecution, SddError>`），并定义 `review_fixture()`：创建临时 git 项目，执行现有 init→new→design→plan→build complete→verify 流程，返回临时目录、cwd 和 change ID。先写：

```rust
#[test]
fn original_blocker_prevents_ocr_execution() {
    let fixture = review_fixture();
    let changed = fixture.root.join("src/secret.rs");
    std::fs::create_dir_all(changed.parent().unwrap()).unwrap();
    std::fs::write(&changed, "aws_access_key_id=AKIAIOSFODNN7EXAMPLE").unwrap();
    let executor = FakeExecutor::completed_empty();
    let error = run_review_with_executor(&fixture.cwd, None, &executor).unwrap_err();
    assert_eq!(error.code, "E_SECURITY_BLOCKED");
    assert_eq!(executor.calls(), 0);
}

#[test]
fn missing_ocr_returns_original_result_with_warning() {
    let fixture = review_fixture();
    let executor = FakeExecutor::not_found();
    let result = run_review_with_executor(&fixture.cwd, None, &executor).unwrap();
    assert_eq!(result.state, "REVIEW_READY");
    assert_eq!(result.exit_code, 0);
    assert!(result.data.as_ref().unwrap()["report"]["passed"].as_bool().unwrap());
    assert_eq!(result.warnings.as_ref().unwrap()[0]["code"], "W_OCR_NOT_FOUND");
    let report = read_review_report(&fixture.cwd, &fixture.change_id);
    assert_eq!(report["minimality"]["ocr"]["status"], "not-found");
}
```

测试必须读取 `CommandResult.warnings` 和 runtime `reports.review`，证明缺失 OCR 不改变原版 `passed/state/exitCode`；不能只断言日志字符串。

运行：`cargo test -p sdd-core commands::review::tests`

预期：失败，原因是 review 尚未调用可注入 OCR executor。

- [x] **Step 2: 写成功合并和 OCR 硬失败测试**

继续使用 `review_fixture()` 和 `FakeExecutor`，写：

```rust
#[test]
fn successful_ocr_comments_are_merged_into_runtime_report() {
    let fixture = review_fixture_with_changed_file("src/a.rs", "fn main() {}\n");
    let output = OcrOutput::success(vec![OcrComment {
        path: "src/a.rs".into(),
        content: "应处理错误".into(),
        existing_code: None,
        suggestion_code: None,
        start_line: 1,
        end_line: 1,
        category: "bug".into(),
        severity: "medium".into(),
    }]);
    let executor = FakeExecutor::completed(output);
    let result = run_review_with_executor(&fixture.cwd, None, &executor).unwrap();
    let report = read_review_report(&fixture.cwd, &fixture.change_id);
    assert!(result.ok);
    assert!(report["issues"].as_array().unwrap().iter().any(|issue| {
        issue["origin"] == "ocr"
            && issue["category"] == "bug"
            && issue["startLine"] == 1
            && issue["file"] == "src/a.rs"
    }));
    assert!(std::fs::read_to_string(fixture.change_dir.join("review-report.md"))
        .unwrap()
        .contains("src/a.rs:1-1"));
}

#[test]
fn existing_ocr_failure_is_not_downgraded() {
    let fixture = review_fixture_with_changed_file("src/a.rs", "fn main() {}\n");
    let executor = FakeExecutor::failed("E_REVIEW_BACKEND_FAILED");
    let error = run_review_with_executor(&fixture.cwd, None, &executor).unwrap_err();
    assert_eq!(error.code, "E_REVIEW_BACKEND_FAILED");
    let report = read_review_report(&fixture.cwd, &fixture.change_id);
    assert_eq!(report["passed"], false);
    assert_eq!(
        crate::state::StateStore::new(fixture.cwd.clone())
            .read()
            .unwrap()
            .current_phase,
        "VERIFY_READY"
    );
}
```

运行：`cargo test -p sdd-core commands::review::tests`

预期：失败，原因是编排和报告合并尚未实现。


- [x] **Step 3: 重构 review 为确定性阶段和 OCR 阶段**

保持当前确定性扫描逻辑和错误优先级。将 OCR 调用放在确定性 `report.passed` 判断之后、最终报告序列化之前：

1. 读取 runtime config 并构造 `OcrConfig`；
2. `mode=off` 或 changed_files 为空时跳过；
3. 确定性 report 未通过时先落盘原版失败报告并返回，不调用 executor；
4. `mode=auto/required` 调用 executor；
5. `NotFound + auto` 写 `W_OCR_NOT_FOUND` warning、把报告元数据记为 `backend=deterministic, ocr=not-found`，不改变报告结论；
6. `NotFound + required` 返回 `E_REVIEW_BACKEND_UNAVAILABLE`；
7. `Completed` 经过 `validate_output`，将每条 comment 转为 `Issue { code: "OCR_FINDING", origin: Some("ocr"), ... }` 并追加；
8. 重新计算 `report.passed`；
9. 把 OCR session ID、filesReviewed 和 comments 计数加入既有 `minimality` 对象；
10. 以最终 report 写入 runtime 和 `review-report.md`，成功结果把 warnings 返回给 `CommandResult`。

OCR 错误必须先构造并保存 `passed=false` 的报告，再更新状态为 `VERIFY_READY`/next `sdd review`，然后返回对应错误。为了避免部分结果误导，不把 OCR 的 `partial` 当作成功。

- [x] **Step 4: 更新 Markdown 渲染**

在 `render_report_markdown` 中把可选定位渲染为 `file:start-end`，把 `origin` 和 `category` 作为同一 finding 的附加信息；缺失新字段的原版 finding 保持现有格式。

- [x] **Step 5: 运行质量链测试并提交**

运行：`cargo test -p sdd-core --test quality_chain`

预期：现有质量链和新增 OCR 编排测试通过，输出无失败。提交：`feat: 串联确定性 review 与 OCR`。

---

### Task 4: 完成文档、OMP 资产和契约验收测试

**Files:**
- Modify: `docs/CLI.md`（review 命令章节）
- Modify: `docs/command-contract.md`（验证/审查章节）
- Modify: `docs/schemas.md`（报告字段章节）
- Modify: `assets/adapters/omp/commands/sdd.review.md`
- Test: `crates/sdd-cli/tests/cli_smoke.rs`
- Test: `crates/sdd-core/tests/schema_validator.rs`
- Test: `crates/sdd-core/tests/quality_chain.rs`

**Interfaces:**
- OMP 模板继续只调用 `sdd review --json`；不展示内部 JSON、runtime 路径或 OCR prompt。
- 文档公开 `quality.ocr.mode` 的 `auto/off/required` 语义、`W_OCR_NOT_FOUND` 和四个 OCR 错误码。

- [x] **Step 1: 写 CLI/config/schema 回归测试**

在 `cli_smoke.rs` 增加 `sdd review --help` 仍成功显示 review 命令的测试；在 `schema_validator.rs` 增加包含 OCR 可选字段的报告通过 `validate_json("report", ...)` 的测试；在 `quality_chain.rs` 增加 `mode=off` 时 review 不产生 OCR warning 的测试。

```rust
#[test]
fn report_schema_accepts_ocr_optional_fields() {
    let report = serde_json::json!({
        "kind": "review",
        "summary": "ok",
        "passed": true,
        "changeId": "demo",
        "issues": [{
            "code": "OCR_FINDING",
            "severity": "medium",
            "message": "建议处理错误",
            "file": "src/a.rs",
            "category": "bug",
            "startLine": 1,
            "endLine": 1,
            "suggestionCode": "return Err(err);",
            "origin": "ocr"
        }]
    });
    assert!(sdd_core::schema::validate_json("report", &report).is_ok());
}
```

运行：`cargo test -p sdd-cli --test cli_smoke && cargo test -p sdd-core --test schema_validator --test quality_chain`

预期：相关测试通过；若失败，错误必须定位到 CLI 兼容性、schema 字段或 `mode=off` 分支，而不是依赖文档文本。


- [x] **Step 2: 更新 CLI、契约和 OMP 文档**

明确写出：原版确定性审查先行；`auto` 找不到 `ocr` 只警告回退；OCR 已启动后失败硬失败；`off` 禁用；`required` 缺少即错误。OMP 命令模板保留中文输出要求，并补充“若提示 `W_OCR_NOT_FOUND`，按原版 review 结论处理”。

- [x] **Step 3: 运行契约测试并提交**

运行：`cargo test -p sdd-cli --test cli_smoke && cargo test -p sdd-core --test schema_validator --test quality_chain`

预期：测试通过。提交：`docs: 补充 review OCR 契约`。

---

## 最终验收（主 Agent）

1. 读取全部任务 diff，确认只包含计划文件和既有 runtime 迁移的必要兼容修改。
2. 运行 `cargo fmt --check`。
3. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
4. 运行 `cargo test --workspace`。
5. 用质量链 fixture 做实际 smoke：覆盖 `mode=off`、auto 缺少后回退、started OCR 失败硬失败、成功 finding 合并和阻断 finding 的稳定错误码；适配器测试覆盖 timeout 及后代管道回收。
6. 运行 `sdd verify --json` 与 `sdd review --json`，按当前 SDD 变更的允许文件和报告验证结果处理。
7. 执行 `sdd review` 前后检查 runtime report、warning、状态和退出码；确认 OCR 缺少时不阻断、OCR 运行失败时阻断。
8. 主 Agent 进行最终代码审查，确认没有凭据、完整 stderr、prompt、thinking 或生成产物进入变更。
