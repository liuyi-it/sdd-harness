//! StateStore 负责工作流状态的读取、校验与更新。
//!
//! 状态是 `.sdd/runtime.json` 的 `state` 节点；runtime 文件由 `RuntimeStore`
//! 统一原子写入，避免状态、配置和制品索引之间出现半更新。

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SddError;
use crate::state::runtime_store::{RuntimeStore, RUNTIME_FILE};

pub const SDD_DIR: &str = ".sdd";

/// 当前状态节点 schema 版本。
pub const CURRENT_SCHEMA_VERSION: u32 = 4;

pub const TASK_STATUS_PENDING: &str = "PENDING";
pub const TASK_STATUS_BUILDING: &str = "BUILDING";
pub const TASK_STATUS_DONE: &str = "DONE";
pub const TASK_STATUS_FAILED: &str = "FAILED";

pub const INDEX_STATUS_MISSING: &str = "MISSING";
pub const INDEX_STATUS_INDEX_READY: &str = "INDEX_READY";
pub const INDEX_STATUS_UNAVAILABLE: &str = "UNAVAILABLE";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceInfo {
    pub branch_name: Option<String>,
    pub worktree_path: Option<String>,
    pub baseline_commit: String,
    pub baseline_changed_files: Vec<String>,
    pub baseline_file_hashes: std::collections::BTreeMap<String, Option<String>>,
    pub baseline_cargo_manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowState {
    pub schema_version: u32,
    pub version: u32,
    pub updated_at: String,
    pub initialized: bool,
    pub current_phase: String,
    pub index_status: String,
    pub codebase_provider: String,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub last_command: Option<String>,
}

/// 单个 change 的独立工作流。顶层 state 只保存项目初始化与索引状态，允许多个
/// change 同时处于未完成阶段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeWorkflow {
    pub run_id: String,
    pub phase: String,
    pub updated_at: String,
    pub last_command: Option<String>,
    pub previous_phase: Option<String>,
    pub in_progress_phase: Option<String>,
    pub failed_command: Option<String>,
    pub failed_reason: Option<String>,
    pub suggested_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_agent_action: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    pub tasks: HashMap<String, String>,
    pub quality_fix_rounds: u8,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::not_initialized()
    }
}

impl WorkflowState {
    pub fn not_initialized() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            version: 1,
            updated_at: now_iso(),
            initialized: false,
            current_phase: "NOT_INITIALIZED".to_string(),
            index_status: INDEX_STATUS_MISSING.to_string(),
            codebase_provider: "codegraph".to_string(),
            degraded: false,
            degraded_reason: None,
            last_command: None,
        }
    }
}

impl ChangeWorkflow {
    pub fn new(run_id: String, workspace: Option<WorkspaceInfo>) -> Self {
        Self {
            run_id,
            phase: "SPEC_WAITING_AGENT".to_string(),
            updated_at: now_iso(),
            last_command: Some("sdd spec".to_string()),
            previous_phase: None,
            in_progress_phase: Some("SPECIFICATION".to_string()),
            failed_command: None,
            failed_reason: None,
            suggested_command: Some("sdd spec --result-json '<JSON>'".to_string()),
            pending_agent_action: None,
            workspace,
            tasks: HashMap::new(),
            quality_fix_rounds: 0,
        }
    }

    pub(crate) fn clear_failure(&mut self) {
        self.failed_command = None;
        self.failed_reason = None;
    }

    pub(crate) fn record_failure(&mut self, command: impl Into<String>, reason: impl Into<String>) {
        self.failed_command = Some(command.into());
        self.failed_reason = Some(reason.into());
    }
}

pub struct StateStore {
    root: PathBuf,
}

impl StateStore {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { root: cwd.into() }
    }

    fn sdd_dir(&self) -> PathBuf {
        self.root.join(SDD_DIR)
    }

    pub fn state_path(&self) -> PathBuf {
        self.sdd_dir().join(RUNTIME_FILE)
    }

    /// runtime 文件不存在时返回初始状态（未初始化）。
    pub fn read(&self) -> Result<WorkflowState, SddError> {
        Ok(RuntimeStore::new(self.root.clone()).read()?.state)
    }

    pub fn update<F>(&self, f: F) -> Result<WorkflowState, SddError>
    where
        F: FnOnce(&mut WorkflowState),
    {
        let store = RuntimeStore::new(self.root.clone());
        let (_, document) =
            store.try_update(|document| apply_state_update(&mut document.state, f))?;
        Ok(document.state)
    }
}

pub(crate) fn apply_state_update<F>(state: &mut WorkflowState, update: F) -> Result<(), SddError>
where
    F: FnOnce(&mut WorkflowState),
{
    update(state);
    state.updated_at = now_iso();
    state.version = state.version.checked_add(1).ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "状态版本号已达到 u32 上限，无法继续更新",
        )
    })?;
    Ok(())
}

pub(crate) fn apply_workflow_update<F>(
    workflow: &mut ChangeWorkflow,
    update: F,
) -> Result<(), SddError>
where
    F: FnOnce(&mut ChangeWorkflow),
{
    let old_phase = workflow.phase.clone();
    update(workflow);
    if workflow.phase != old_phase {
        workflow.previous_phase = Some(old_phase);
    }
    workflow.updated_at = now_iso();
    Ok(())
}

pub(crate) fn validate_state(state: &WorkflowState) -> Result<(), SddError> {
    // 状态只接受当前 schema，不执行隐式迁移。
    if state.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!(
                "状态 schemaVersion {} 与当前版本 {} 不兼容",
                state.schema_version, CURRENT_SCHEMA_VERSION
            ),
        ));
    }
    if (!state.initialized
        && !matches!(
            state.current_phase.as_str(),
            "NOT_INITIALIZED" | "INITIALIZING"
        ))
        || (state.initialized
            && matches!(
                state.current_phase.as_str(),
                "NOT_INITIALIZED" | "INITIALIZING"
            ))
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "initialized 与工作流阶段不一致",
        ));
    }
    match state.index_status.as_str() {
        INDEX_STATUS_MISSING if state.initialized => {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "已初始化状态不得缺少代码库索引结果",
            ));
        }
        INDEX_STATUS_INDEX_READY if !state.initialized || state.degraded => {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "INDEX_READY 必须对应已初始化且非降级状态",
            ));
        }
        INDEX_STATUS_UNAVAILABLE if !state.initialized || !state.degraded => {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "UNAVAILABLE 必须对应已初始化且降级状态",
            ));
        }
        INDEX_STATUS_MISSING | INDEX_STATUS_INDEX_READY | INDEX_STATUS_UNAVAILABLE => {}
        _ => {
            return Err(SddError::new("E_STATE_CORRUPTED", "未知代码库索引状态"));
        }
    }
    if state.degraded {
        if state.codebase_provider != "fallback-file-scan"
            || !state
                .degraded_reason
                .as_deref()
                .is_some_and(|reason| !reason.trim().is_empty())
        {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "降级状态必须使用 fallback-file-scan 并记录原因",
            ));
        }
    } else if state.codebase_provider != "codegraph" || state.degraded_reason.is_some() {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "非降级状态必须使用 codegraph 且不得保留降级原因",
        ));
    }
    let value = serde_json::to_value(state)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &format!("序列化状态失败：{error}")))?;
    crate::schema::validate_json("state", &value)
}

pub(crate) fn validate_change_workflow(workflow: &ChangeWorkflow) -> Result<(), SddError> {
    validate_run_id(&workflow.run_id)?;
    if !crate::contracts::PHASES.contains(&workflow.phase.as_str())
        || matches!(
            workflow.phase.as_str(),
            "NOT_INITIALIZED" | "INITIALIZING" | "INDEX_READY"
        )
    {
        return Err(SddError::new("E_STATE_CORRUPTED", "change 工作流阶段无效"));
    }
    if let Some(workspace) = workflow.workspace.as_ref() {
        validate_workspace(workspace)?;
    }
    for task_id in workflow.tasks.keys() {
        if !crate::engines::tdd::valid_task_id(task_id) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("工作流包含无效任务 ID：{task_id}"),
            ));
        }
    }
    let building = workflow
        .tasks
        .iter()
        .filter(|(_, status)| status.as_str() == TASK_STATUS_BUILDING)
        .count();
    match workflow.phase.as_str() {
        "SPEC_WAITING_AGENT" | "PLAN_WAITING_AGENT" => {
            validate_pending_phase_action(
                workflow
                    .pending_agent_action
                    .as_ref()
                    .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "生成阶段缺少待处理行动"))?,
                match workflow.phase.as_str() {
                    "SPEC_WAITING_AGENT" => "SPECIFICATION",
                    "PLAN_WAITING_AGENT" => "PLAN",
                    _ => unreachable!("match 分支已限定生成阶段"),
                },
            )?;
            if building != 0 {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    "生成阶段不得存在 BUILDING 任务",
                ));
            }
        }
        "BUILD_WAITING_AGENT" => {
            let task_id =
                validate_pending_agent_task(workflow.pending_agent_action.as_ref().ok_or_else(
                    || SddError::new("E_STATE_CORRUPTED", "构建阶段缺少待处理行动"),
                )?)?;
            if building != 1
                || workflow.tasks.get(task_id).map(String::as_str) != Some(TASK_STATUS_BUILDING)
            {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    "BUILD_WAITING_AGENT 必须对应唯一 BUILDING 任务",
                ));
            }
        }
        "QUALITY_WAITING_FIX" => {
            validate_pending_fix_action(
                workflow
                    .pending_agent_action
                    .as_ref()
                    .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "质量阶段缺少待处理修复"))?,
            )?;
            if building != 0 {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    "质量阶段不得存在 BUILDING 任务",
                ));
            }
        }
        _ if workflow.pending_agent_action.is_some() || building != 0 => {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "稳定阶段不得保留待处理行动或 BUILDING 任务",
            ));
        }
        _ => {}
    }
    if workflow.failed_command.is_some() != workflow.failed_reason.is_some() {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "failedCommand 与 failedReason 必须同时存在",
        ));
    }
    for (label, value) in [
        ("lastCommand", workflow.last_command.as_deref()),
        ("previousPhase", workflow.previous_phase.as_deref()),
        ("inProgressPhase", workflow.in_progress_phase.as_deref()),
        ("suggestedCommand", workflow.suggested_command.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("{label} 不得为空"),
            ));
        }
    }
    Ok(())
}

fn validate_pending_phase_action(pending: &serde_json::Value, phase: &str) -> Result<(), SddError> {
    let fields = pending
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "阶段行动必须是对象"))?;
    if fields.get("type").and_then(serde_json::Value::as_str) != Some("AGENT_PHASE_EXECUTION")
        || fields.get("phase").and_then(serde_json::Value::as_str) != Some(phase)
        || !fields
            .get("since")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(SddError::new("E_STATE_CORRUPTED", "阶段行动内容无效"));
    }
    Ok(())
}

fn validate_pending_fix_action(pending: &serde_json::Value) -> Result<(), SddError> {
    let fields = pending
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "修复行动必须是对象"))?;
    let fix_id = fields
        .get("fixId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if fields.get("type").and_then(serde_json::Value::as_str) != Some("AGENT_FIX_EXECUTION")
        || !fix_id.strip_prefix("FIX-").is_some_and(|sequence| {
            sequence.len() == 3 && sequence.bytes().all(|byte| byte.is_ascii_digit())
        })
        || !fields
            .get("since")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        || !fields
            .get("userAuthorized")
            .is_some_and(serde_json::Value::is_boolean)
    {
        return Err(SddError::new("E_STATE_CORRUPTED", "修复行动内容无效"));
    }
    Ok(())
}

fn validate_pending_agent_task(pending: &serde_json::Value) -> Result<&str, SddError> {
    let fields = pending
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "pendingAgentAction 必须是对象"))?;
    let expected = ["taskId", "since", "gitBaseline"];
    if fields.len() != expected.len()
        || fields
            .keys()
            .any(|field| !expected.contains(&field.as_str()))
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "pendingAgentAction 字段无效",
        ));
    }
    let task_id = fields
        .get("taskId")
        .and_then(serde_json::Value::as_str)
        .filter(|task_id| crate::engines::tdd::valid_task_id(task_id))
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "pendingAgentAction.taskId 无效"))?;
    if !fields
        .get("since")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|since| !since.trim().is_empty())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "pendingAgentAction.since 无效",
        ));
    }
    validate_git_baseline(
        fields
            .get("gitBaseline")
            .expect("已确认 gitBaseline 字段存在"),
    )?;
    Ok(task_id)
}

fn validate_git_baseline(baseline: &serde_json::Value) -> Result<(), SddError> {
    let fields = baseline.as_object().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "pendingAgentAction.gitBaseline 必须是对象",
        )
    })?;
    let available = fields
        .get("available")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "pendingAgentAction.gitBaseline.available 必须是布尔值",
            )
        })?;
    if !available {
        if fields.len() != 1 {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "不可用的 gitBaseline 只能包含 available=false",
            ));
        }
        return Ok(());
    }
    let expected = ["available", "head", "changedFiles", "changedFileHashes"];
    if fields.len() != expected.len()
        || fields
            .keys()
            .any(|field| !expected.contains(&field.as_str()))
        || !fields
            .get("head")
            .and_then(serde_json::Value::as_str)
            .is_some_and(valid_git_oid)
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "可用的 gitBaseline 字段无效",
        ));
    }
    validate_file_hash_snapshot(
        fields
            .get("changedFiles")
            .expect("已确认 changedFiles 字段存在"),
        fields
            .get("changedFileHashes")
            .expect("已确认 changedFileHashes 字段存在"),
        "gitBaseline",
    )
}

fn validate_workspace(workspace: &WorkspaceInfo) -> Result<(), SddError> {
    if !valid_git_oid(&workspace.baseline_commit)
        || workspace.branch_name.is_some() != workspace.worktree_path.is_some()
        || workspace
            .branch_name
            .as_deref()
            .is_some_and(|branch| branch.trim().is_empty())
        || workspace
            .worktree_path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty() || !std::path::Path::new(path).is_absolute())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "workspace 的 Git 基线或隔离路径无效",
        ));
    }
    let files = serde_json::json!(workspace.baseline_changed_files);
    let hashes = serde_json::json!(workspace.baseline_file_hashes);
    validate_file_hash_snapshot(&files, &hashes, "workspace")
}

fn valid_git_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_file_hash_snapshot(
    files: &serde_json::Value,
    hashes: &serde_json::Value,
    label: &str,
) -> Result<(), SddError> {
    let files = files
        .as_array()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("{label} 文件必须是数组")))?;
    let mut unique = std::collections::BTreeSet::new();
    for file in files {
        let file = file.as_str().ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("{label} 文件路径必须是字符串"),
            )
        })?;
        validate_fact_path(file)?;
        if !unique.insert(file) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("{label} 包含重复文件路径"),
            ));
        }
    }
    let hashes = hashes.as_object().ok_or_else(|| {
        SddError::new("E_STATE_CORRUPTED", &format!("{label} 文件哈希必须是对象"))
    })?;
    if hashes.len() != unique.len() || hashes.keys().any(|path| !unique.contains(path.as_str())) {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{label} 文件与哈希键不一致"),
        ));
    }
    if hashes.values().any(|hash| {
        !hash.is_null()
            && !hash.as_str().is_some_and(|hash| {
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    }) {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{label} 包含无效文件哈希"),
        ));
    }
    Ok(())
}

fn validate_fact_path(path: &str) -> Result<(), SddError> {
    crate::state::artifact_store::validate_content_path(path)?;
    let normalized = path.replace('\\', "/");
    if normalized == ".git"
        || normalized.starts_with(".git/")
        || normalized == ".sdd"
        || normalized.starts_with(".sdd/")
        || normalized.split('/').any(|part| part == ".")
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{path} 不是安全业务文件路径"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_run_id(run_id: &str) -> Result<(), SddError> {
    if run_id.is_empty()
        || !run_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(SddError::new(
            "E_SECURITY_BLOCKED",
            &format!("runId 包含非法字符：{run_id}"),
        ));
    }
    Ok(())
}

pub(crate) fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .expect("系统时间必须晚于 UNIX_EPOCH");
    format_iso_epoch(secs)
}

pub(crate) fn unique_id(prefix: &str) -> Result<String, SddError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("系统时间早于 UNIX_EPOCH：{error}"),
            )
        })?
        .as_nanos();
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(format!(
        "{prefix}-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

fn format_iso_epoch(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
