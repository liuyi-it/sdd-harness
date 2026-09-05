//! `.sdd/runtime.json` 统一保存 SDD 的机器可读数据。
//!
//! changes 下只保留可读 Markdown；状态、配置、制品索引、规格模型、计划任务、
//! 报告、任务结果、独立 change 工作流和知识索引全部通过本模块原子更新。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::SddError;
use crate::state::state_store::{validate_run_id, ChangeWorkflow, WorkflowState};

pub const RUNTIME_FILE: &str = "runtime.json";
pub const RUNTIME_CHECKSUM_FILE: &str = "runtime.json.sha256";
pub const RUNTIME_SCHEMA_VERSION: u32 = 7;
const CONFIG_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeDocument {
    pub schema_version: u32,
    pub state: WorkflowState,
    pub config: Value,
    pub artifacts: Value,
    pub changes: BTreeMap<String, Value>,
    pub runs: BTreeMap<String, Value>,
    pub workflows: BTreeMap<String, ChangeWorkflow>,
    pub index: Value,
}

impl Default for RuntimeDocument {
    fn default() -> Self {
        Self {
            schema_version: RUNTIME_SCHEMA_VERSION,
            state: WorkflowState::not_initialized(),
            config: json!({
                "schemaVersion": CONFIG_SCHEMA_VERSION,
                "hostAdapter": "codex",
                "workflow": {
                    "gitIsolation": false
                },
                "contextPack": { "maxSizeKb": 30 },
                "audit": { "maxSizeMb": 5, "maxFiles": 200 }
            }),
            artifacts: json!({
                "schemaVersion": "2.0.0",
                "artifacts": {},
            }),
            changes: BTreeMap::new(),
            runs: BTreeMap::new(),
            workflows: BTreeMap::new(),
            index: json!({}),
        }
    }
}

pub struct RuntimeStore {
    root: PathBuf,
}

impl RuntimeStore {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { root: cwd.into() }
    }

    pub fn read(&self) -> Result<RuntimeDocument, SddError> {
        self.read_with_source().map(|(document, _)| document)
    }

    fn read_with_source(&self) -> Result<(RuntimeDocument, bool), SddError> {
        let Some(dir) = crate::state::paths::existing_sdd_dir(&self.root)? else {
            return Ok((RuntimeDocument::default(), false));
        };
        let path = dir.join(RUNTIME_FILE);
        crate::safe_fs::reject_symlink(&path, "runtime.json")?;
        if !path.exists() {
            return Ok((RuntimeDocument::default(), false));
        }
        // 主文件或其校验和损坏时仅接受同样通过校验的备份。
        match self.read_path(&path) {
            Ok(document) => Ok((document, true)),
            Err(error) if error.code == "E_STATE_VERSION_UNSUPPORTED" => Err(error),
            Err(primary_error) => {
                let backup = dir.join("runtime.json.bak");
                crate::safe_fs::reject_symlink(&backup, "runtime 备份")?;
                if backup.exists() {
                    match self.read_path(&backup) {
                        Ok(document) => Ok((document, false)),
                        Err(backup_error) => Err(SddError::new(
                            "E_STATE_CORRUPTED",
                            &format!(
                                "主 runtime 损坏：{}；备份也不可用：{}",
                                primary_error.message, backup_error.message
                            ),
                        )),
                    }
                } else {
                    Err(primary_error)
                }
            }
        }
    }

    fn read_path(&self, path: &Path) -> Result<RuntimeDocument, SddError> {
        crate::safe_fs::reject_symlink(path, "runtime 数据文件")?;
        let raw = fs::read_to_string(path).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取 runtime.json 失败：{error}"),
            )
        })?;
        self.verify_checksum(path, &raw)?;
        let value: Value = serde_json::from_str(&raw).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("runtime.json 解析失败：{error}"),
            )
        })?;
        let found_version = value
            .get("schemaVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.json 缺少 schemaVersion"))?;
        if found_version != u64::from(RUNTIME_SCHEMA_VERSION) {
            return Err(SddError::new(
                "E_STATE_VERSION_UNSUPPORTED",
                &format!(
                    "当前 .sdd 使用 runtime schemaVersion {found_version}，sdd 仅支持版本 {RUNTIME_SCHEMA_VERSION}；请先备份并删除旧 .sdd，再重新执行 sdd init"
                ),
            ));
        }
        crate::schema::validate_json("runtime", &value)?;
        let document: RuntimeDocument = serde_json::from_value(value).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("runtime.json 结构解析失败：{error}"),
            )
        })?;
        validate_document(&document, &self.root)?;
        Ok(document)
    }

    fn verify_checksum(&self, path: &Path, raw: &str) -> Result<(), SddError> {
        let sidecar = checksum_path(path);
        crate::safe_fs::reject_symlink(&sidecar, "runtime 校验和")?;
        let expected = fs::read_to_string(&sidecar).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取 runtime 校验和失败：{error}"),
            )
        })?;
        if !crate::state::checksum::verify(raw.as_bytes(), &expected) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "runtime.json 与校验和不一致（文件可能已损坏）",
            ));
        }
        Ok(())
    }

    fn write_document(
        &self,
        document: &RuntimeDocument,
        primary_verified: bool,
    ) -> Result<(), SddError> {
        validate_document(document, &self.root)?;
        let dir = crate::state::paths::ensure_sdd_dir(&self.root)?;
        let value = serde_json::to_value(document).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("序列化 runtime.json 失败：{error}"),
            )
        })?;
        crate::schema::validate_json("runtime", &value)?;
        let content = serde_json::to_string_pretty(&value).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("格式化 runtime.json 失败：{error}"),
            )
        })?;
        let path = dir.join(RUNTIME_FILE);
        crate::safe_fs::reject_symlink(&path, "runtime.json")?;
        if path.exists() && (primary_verified || self.read_path(&path).is_ok()) {
            self.backup_current(&path, &dir)?;
        }
        crate::safe_fs::atomic_write(&path, content.as_bytes(), "runtime.json")?;
        write_checksum_sidecar(&path, &content)?;
        Ok(())
    }

    fn backup_current(&self, path: &Path, dir: &Path) -> Result<(), SddError> {
        crate::safe_fs::reject_symlink(path, "待备份 runtime.json")?;
        let content = fs::read(path).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取待备份 runtime.json 失败：{error}"),
            )
        })?;
        let source_checksum = checksum_path(path);
        crate::safe_fs::reject_symlink(&source_checksum, "待备份 runtime 校验和")?;
        let checksum = fs::read(source_checksum).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取待备份 runtime 校验和失败：{error}"),
            )
        })?;
        let backup = dir.join("runtime.json.bak");
        crate::safe_fs::atomic_write(&backup, &content, "runtime 备份")?;
        crate::safe_fs::atomic_write(&checksum_path(&backup), &checksum, "runtime 备份校验和")?;
        Ok(())
    }

    pub fn update<F>(&self, update: F) -> Result<RuntimeDocument, SddError>
    where
        F: FnOnce(&mut RuntimeDocument),
    {
        self.try_update(|document| {
            update(document);
            Ok(())
        })
        .map(|(_, document)| document)
    }

    pub fn try_update<T, F>(&self, update: F) -> Result<(T, RuntimeDocument), SddError>
    where
        F: FnOnce(&mut RuntimeDocument) -> Result<T, SddError>,
    {
        let root = self.root.to_str().ok_or_else(|| {
            SddError::new(
                "E_PATH_OUTSIDE_REPO",
                "项目根目录不是有效 UTF-8，无法写入 JSON 契约",
            )
        })?;
        let _guard =
            crate::state::file_lock::lock_sdd(root, "runtime transaction", None, Some(5_000))?;
        let (mut document, primary_verified) = self.read_with_source()?;
        let result = update(&mut document)?;
        self.write_document(&document, primary_verified)?;
        Ok((result, document))
    }
}

fn validate_document(document: &RuntimeDocument, root: &Path) -> Result<(), SddError> {
    crate::state::state_store::validate_state(&document.state)?;
    for (change_id, change) in &document.changes {
        crate::git::isolation::validate_change_id(change_id)?;
        validate_change(change_id, change)?;
    }
    for (run_id, run) in &document.runs {
        validate_run_id(run_id)?;
        validate_run(run_id, run)?;
        let change_id = run["changeId"]
            .as_str()
            .expect("validate_run 已确认 changeId");
        if !document.changes.contains_key(change_id) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("run {run_id} 绑定的 change {change_id} 不存在"),
            ));
        }
    }
    if document.workflows.len() != document.changes.len()
        || document
            .changes
            .keys()
            .any(|change_id| !document.workflows.contains_key(change_id))
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "changes 与 workflows 必须一一对应",
        ));
    }
    for (change_id, workflow) in &document.workflows {
        crate::state::state_store::validate_change_workflow(workflow)?;
        let run = document.runs.get(&workflow.run_id).ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("change {change_id} 的 workflow 缺少对应 run"),
            )
        })?;
        if run.get("changeId").and_then(Value::as_str) != Some(change_id.as_str()) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("change {change_id} 的 workflow 与 run 不一致"),
            ));
        }
    }
    validate_artifacts(&document.artifacts)?;
    validate_index(document)?;
    crate::schema::validate_json("config", &document.config)?;
    validate_workspace_binding(document, root)?;
    Ok(())
}

fn validate_workspace_binding(document: &RuntimeDocument, root: &Path) -> Result<(), SddError> {
    let isolated = document
        .config
        .pointer("/workflow/gitIsolation")
        .and_then(Value::as_bool)
        .expect("config schema 已确认 workflow.gitIsolation");
    for (change_id, workflow) in &document.workflows {
        let workspace = match workflow.workspace.as_ref() {
            Some(workspace) => workspace,
            None if !isolated => continue,
            None => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("启用 Git 隔离的变更 {change_id} 必须绑定受管 workspace"),
                ));
            }
        };
        if !isolated {
            if workspace.branch_name.is_some() || workspace.worktree_path.is_some() {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    "未启用 Git 隔离时 workspace 不得绑定分支或 worktree",
                ));
            }
            continue;
        }
        let expected_branch = format!("sdd/{change_id}");
        if workspace.branch_name.as_deref() != Some(expected_branch.as_str()) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "隔离 workspace 分支与 changeId 不一致",
            ));
        }
        let stored_path = workspace
            .worktree_path
            .as_deref()
            .expect("workflow 校验已确认 branch/worktree 成对存在");
        let stored = Path::new(stored_path);
        crate::safe_fs::reject_symlink(stored, "隔离 worktree")?;
        let root_text = root.to_str().ok_or_else(|| {
            SddError::new(
                "E_PATH_OUTSIDE_REPO",
                "项目根目录不是有效 UTF-8，无法验证隔离 worktree",
            )
        })?;
        let expected = crate::state::paths::worktrees_dir(root_text, false)?.join(change_id);
        crate::safe_fs::reject_symlink(&expected, "隔离 worktree")?;
        let expected = expected.canonicalize().map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("解析预期隔离 worktree 失败：{error}"),
            )
        })?;
        let stored = stored.canonicalize().map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("解析 workspace.worktreePath 失败：{error}"),
            )
        })?;
        let control_root = root.canonicalize().map_err(|error| {
            SddError::new(
                "E_PATH_OUTSIDE_REPO",
                &format!("解析项目根目录失败：{error}"),
            )
        })?;
        if stored != expected || !stored.starts_with(control_root) {
            return Err(SddError::new(
                "E_SYMLINK_BLOCKED",
                "workspace.worktreePath 与当前变更的受管 worktree 不一致",
            ));
        }
    }
    Ok(())
}

fn validate_artifacts(registry: &Value) -> Result<(), SddError> {
    let root = registry
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.artifacts 必须是对象"))?;
    if root.len() != 2
        || root.get("schemaVersion").and_then(Value::as_str) != Some("2.0.0")
        || !root.contains_key("artifacts")
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "runtime.artifacts 必须使用当前 2.0.0 结构",
        ));
    }
    let entries = root
        .get("artifacts")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "runtime.artifacts.artifacts 必须是对象",
            )
        })?;
    for (key, item) in entries {
        if key.is_empty() || key.len() > 512 || key.chars().any(char::is_control) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "runtime artifact key 非法",
            ));
        }
        crate::schema::validate_json("artifact", item)?;
        let content_path = item
            .get("contentPath")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SddError::new("E_STATE_CORRUPTED", "runtime artifact 缺少 contentPath")
            })?;
        crate::state::artifact_store::validate_content_path(content_path)?;
    }
    Ok(())
}

fn validate_index(document: &RuntimeDocument) -> Result<(), SddError> {
    let index = document
        .index
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.index 必须是对象"))?;
    if document.state.index_status == crate::state::state_store::INDEX_STATUS_MISSING {
        if !index.is_empty() {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "索引状态为 MISSING 时 runtime.index 必须为空",
            ));
        }
        return Ok(());
    }
    if index.len() != 3 {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "runtime.index 必须包含 diagnostics、summary 和 updatedAt",
        ));
    }
    let diagnostics = index
        .get("diagnostics")
        .and_then(Value::as_array)
        .filter(|diagnostics| diagnostics.len() == 1)
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "runtime.index.diagnostics 必须只包含一条 CodeGraph 诊断",
            )
        })?;
    let diagnostic = diagnostics[0]
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "CodeGraph 索引诊断必须是对象"))?;
    let expected_fields = [
        "provider",
        "installed",
        "version",
        "indexed",
        "degraded",
        "reason",
    ];
    if diagnostic.len() != expected_fields.len()
        || diagnostic
            .keys()
            .any(|field| !expected_fields.contains(&field.as_str()))
        || diagnostic.get("provider").and_then(Value::as_str) != Some("codegraph")
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "CodeGraph 索引诊断字段非法",
        ));
    }
    let installed = diagnostic
        .get("installed")
        .and_then(Value::as_bool)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "索引诊断缺少 installed"))?;
    let version = diagnostic
        .get("version")
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "索引诊断缺少 version"))?;
    if installed
        != version
            .as_str()
            .is_some_and(|version| !version.trim().is_empty())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "CodeGraph 索引诊断的 installed 与 version 不一致",
        ));
    }
    let indexed = diagnostic
        .get("indexed")
        .and_then(Value::as_bool)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "索引诊断缺少 indexed"))?;
    let degraded = diagnostic
        .get("degraded")
        .and_then(Value::as_bool)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "索引诊断缺少 degraded"))?;
    let reason = diagnostic
        .get("reason")
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "索引诊断缺少 reason"))?;
    if indexed == degraded
        || (indexed && !installed)
        || (degraded
            && !reason
                .as_str()
                .is_some_and(|reason| !reason.trim().is_empty()))
        || (!degraded && !reason.is_null())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "CodeGraph 索引诊断的 indexed、degraded 与 reason 不一致",
        ));
    }
    let expected_status = if degraded {
        crate::state::state_store::INDEX_STATUS_UNAVAILABLE
    } else {
        crate::state::state_store::INDEX_STATUS_INDEX_READY
    };
    let expected_provider = if degraded {
        "fallback-file-scan"
    } else {
        "codegraph"
    };
    let expected_summary_prefix = if degraded {
        crate::knowledge::FALLBACK_SUMMARY_PREFIX
    } else {
        crate::knowledge::CODEGRAPH_SUMMARY_PREFIX
    };
    if document.state.index_status != expected_status
        || document.state.degraded != degraded
        || document.state.codebase_provider != expected_provider
        || document.state.degraded_reason.as_deref() != reason.as_str()
        || !index
            .get("summary")
            .and_then(Value::as_str)
            .is_some_and(|summary| summary.starts_with(expected_summary_prefix))
        || !index
            .get("updatedAt")
            .and_then(Value::as_str)
            .is_some_and(|updated_at| !updated_at.trim().is_empty())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "runtime.index 与工作流索引状态不一致",
        ));
    }
    Ok(())
}

fn validate_change(change_id: &str, change: &Value) -> Result<(), SddError> {
    let fields = change.as_object().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("change {change_id} 必须是对象"),
        )
    })?;
    if fields
        .keys()
        .any(|field| !matches!(field.as_str(), "spec" | "plan" | "reports" | "archive"))
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("change {change_id} 包含未知字段"),
        ));
    }
    if fields.get("spec").is_some_and(|value| !value.is_object())
        || fields.get("plan").is_some_and(|value| !value.is_object())
        || fields
            .get("reports")
            .is_some_and(|value| !value.is_object())
        || fields
            .get("archive")
            .is_some_and(|value| !value.is_object())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("change {change_id} 字段类型无效"),
        ));
    }
    if let Some(spec) = fields.get("spec") {
        crate::schema::validate_json("spec", spec)?;
        if spec.get("status").and_then(Value::as_str) == Some("READY") {
            crate::engines::spec::model_from_record(spec)?;
        }
    }
    if let Some(reports) = fields.get("reports").and_then(Value::as_object) {
        for (kind, report) in reports {
            if kind != "quality" {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("change {change_id} 包含未知报告 {kind}"),
                ));
            }
            crate::schema::validate_json("report", report)?;
        }
    }
    Ok(())
}

fn validate_run(run_id: &str, run: &Value) -> Result<(), SddError> {
    let fields = run
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("run {run_id} 必须是对象")))?;
    if fields
        .keys()
        .any(|field| !matches!(field.as_str(), "changeId" | "input" | "tasks"))
        || !fields
            .get("changeId")
            .and_then(Value::as_str)
            .is_some_and(|change_id| crate::git::isolation::validate_change_id(change_id).is_ok())
        || !fields.get("input").is_some_and(Value::is_string)
        || !fields.get("tasks").is_some_and(Value::is_object)
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("run {run_id} 结构无效"),
        ));
    }
    if let Some(tasks) = fields.get("tasks") {
        let tasks = tasks.as_object().ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("run {run_id}.tasks 必须是对象"),
            )
        })?;
        for (task_id, result) in tasks {
            crate::schema::validate_json("task-result", result)?;
            if result.get("taskId").and_then(Value::as_str) != Some(task_id.as_str()) {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("run {run_id} 的任务结果 key 与 taskId 不一致"),
                ));
            }
        }
    }
    if let Some(events) = fields.get("events") {
        let events = events.as_array().ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("run {run_id}.events 必须是数组"),
            )
        })?;
        let change_id = fields["changeId"]
            .as_str()
            .expect("前置校验已确认 changeId");
        for event in events {
            validate_business_run_event(run_id, change_id, event)?;
        }
    }
    Ok(())
}

fn validate_business_run_event(
    run_id: &str,
    change_id: &str,
    event: &Value,
) -> Result<(), SddError> {
    let fields = event.as_object().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("run {run_id}.events 包含非对象事件"),
        )
    })?;
    let expected = [
        "schemaVersion",
        "eventId",
        "runId",
        "type",
        "changeId",
        "createdAt",
    ];
    if fields.len() != expected.len()
        || fields
            .keys()
            .any(|field| !expected.contains(&field.as_str()))
        || fields.get("schemaVersion").and_then(Value::as_str) != Some("1.0.0")
        || fields.get("runId").and_then(Value::as_str) != Some(run_id)
        || fields.get("changeId").and_then(Value::as_str) != Some(change_id)
        || fields.get("type").and_then(Value::as_str) != Some("REQUIREMENT_REVISED")
        || !fields
            .get("eventId")
            .and_then(Value::as_str)
            .is_some_and(|event_id| validate_run_id(event_id).is_ok())
        || !fields
            .get("createdAt")
            .and_then(Value::as_str)
            .is_some_and(|created_at| !created_at.trim().is_empty())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("run {run_id}.events 包含无效事件"),
        ));
    }
    Ok(())
}

/// 将数据文件的 SHA-256 校验和以 tmp+rename 原子写入边车。
fn write_checksum_sidecar(path: &Path, content: &str) -> Result<(), SddError> {
    let checksum = crate::state::checksum::compute(content.as_bytes());
    let sidecar = checksum_path(path);
    crate::safe_fs::atomic_write(
        &sidecar,
        format!("{checksum}\n").as_bytes(),
        "runtime 校验和",
    )
}

fn checksum_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .expect("runtime 内部路径必须包含文件名")
        .to_string_lossy();
    path.with_file_name(format!("{name}.sha256"))
}
