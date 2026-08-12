//! 对外稳定契约：命令集合、阶段枚举、错误码到退出码映射、请求/响应结构。
//!
//! 翻译自 早期 Node 实现，字段名与枚举值保持一致。
//! `codebase.provider` 使用 `gitnexus | codegraph | fallback-file-scan`。

use serde::{Deserialize, Serialize};

/// 命令集合（12 个，与 Node 版一致）
pub const COMMANDS: [&str; 12] = [
    "init", "auto", "new", "change", "design", "plan", "build", "verify", "review", "archive",
    "status", "codebase",
];

/// 阶段枚举（22 个，与 Node 版一致）
pub const PHASES: [&str; 22] = [
    "NOT_INITIALIZED",
    "INITIALIZING",
    "INDEXING",
    "INDEX_READY",
    "NEW_STARTED",
    "CLARIFYING",
    "SPEC_READY",
    "DESIGNING",
    "DESIGN_READY",
    "PLANNING",
    "PLAN_READY",
    "BUILDING",
    "BUILD_WAITING_AGENT",
    "BUILD_READY",
    "VERIFYING",
    "VERIFY_READY",
    "REVIEWING",
    "REVIEW_READY",
    "ARCHIVING",
    "ARCHIVED",
    "FAILED",
    "PAUSED",
];

/// 错误码到退出码的映射（与 Node 版 ERROR_EXIT_CODES 逐字一致）
pub fn error_exit_codes(code: &str) -> i32 {
    match code {
        "E_NOT_INITIALIZED" => 3,
        "E_INVALID_PHASE_COMMAND" => 3,
        "E_ACTIVE_CHANGE_EXISTS" => 3,
        "E_MISSING_CHANGE" => 4,
        "E_MISSING_ARTIFACT" => 4,
        "E_INVALID_REQUIREMENT" => 6,
        "E_COMPONENT_UNAVAILABLE" => 5,
        "E_COMPONENT_INTEGRITY_FAILED" => 10,
        "E_DEGRADED_MODE" => 0,
        "E_UNRESOLVED_BLOCKER" => 6,
        "E_VERIFY_REQUIRED" => 3,
        "E_REVIEW_REQUIRED" => 3,
        "E_VERIFY_FAILED" => 7,
        "E_TDD_EVIDENCE_REQUIRED" => 7,
        "E_AGENT_TASK_FAILED" => 7,
        "E_UNDECLARED_FILE_CHANGE" => 10,
        "E_REVIEW_FAILED" => 8,
        "E_REVIEW_BACKEND_UNAVAILABLE" => 5,
        "E_REVIEW_BACKEND_TIMEOUT" => 124,
        "E_REVIEW_BACKEND_FAILED" => 8,
        "E_REVIEW_BACKEND_INVALID_OUTPUT" => 8,
        "E_UNPLANNED_DEPENDENCY" => 8,
        "E_ARCHIVED_READONLY" => 3,
        "E_CONCURRENT_RUN" => 9,
        "E_LOCK_TIMEOUT" => 9,
        "E_TIMEOUT" => 124,
        "E_INTERRUPTED" => 130,
        "E_STATE_CORRUPTED" => 1,
        "E_SECURITY_BLOCKED" => 10,
        "E_PATH_OUTSIDE_REPO" => 10,
        "E_SYMLINK_BLOCKED" => 10,
        "E_PARALLEL_FILE_CONFLICT" => 3,
        _ => 1,
    }
}

/// 结构化警告（对应 Node 版 CliWarning）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CliWarning {
    /// 警告码，如 "W_KNOWLEDGE_UNAVAILABLE"
    pub code: String,
    /// 人类可读警告信息
    pub message: String,
    /// 建议的下一步命令
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// 额外诊断详情
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// 验证命令（build next 的 verification 项）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCommand {
    pub command: String,
    pub args: Vec<String>,
}

/// 知识图谱提供方信息（契约变更：provider 为 gitnexus/codegraph/fallback-file-scan）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodebaseProviderInfo {
    pub provider: String,
    pub degraded: bool,
}

/// Agent 行动要求（build next 返回此结构，结果通过 inline JSON 提交）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentActionRequired {
    #[serde(rename = "type")]
    pub action_type: String,
    pub task_id: String,
    pub change_id: String,
    /// 完整 Context Pack 内容；不再返回 `.sdd/context-packs` 文件路径。
    pub context_pack: String,
    pub allowed_files: Vec<String>,
    pub expected_new_files: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub verification: Vec<VerificationCommand>,
    /// 固定为 `inline-json`，提示 Adapter 使用 `build complete --result-json`。
    pub result_transport: String,
    pub codebase: CodebaseProviderInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_bundle: Option<serde_json::Value>,
}

/// Core 请求结构
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub command: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// 错误结构
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// Core 响应结构（JSON 输出契约，camelCase 键名与 Node 版一致）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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
    pub rendered: Option<RenderedOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_required: Option<AgentActionRequired>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}

/// 渲染输出
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderedOutput {
    pub format: String,
    pub content: String,
}

impl CommandResult {
    /// 构造一个成功的简单结果
    pub fn ok(state: &str) -> Self {
        Self {
            ok: true,
            state: state.to_string(),
            exit_code: 0,
            change_id: None,
            next: None,
            data: None,
            rendered: None,
            warnings: None,
            action_required: None,
            error: None,
        }
    }

    /// 从错误构造失败结果
    pub fn from_error(state: &str, error: &crate::error::SddError) -> Self {
        Self {
            ok: false,
            state: state.to_string(),
            exit_code: error.exit_code,
            change_id: None,
            next: None,
            data: None,
            rendered: None,
            warnings: None,
            action_required: None,
            error: Some(error.to_command_error()),
        }
    }
}

/// 检查错误码是否为已知错误码
pub fn is_known_error_code(code: &str) -> bool {
    matches!(
        code,
        "E_NOT_INITIALIZED"
            | "E_INVALID_PHASE_COMMAND"
            | "E_ACTIVE_CHANGE_EXISTS"
            | "E_MISSING_CHANGE"
            | "E_MISSING_ARTIFACT"
            | "E_INVALID_REQUIREMENT"
            | "E_COMPONENT_UNAVAILABLE"
            | "E_COMPONENT_INTEGRITY_FAILED"
            | "E_DEGRADED_MODE"
            | "E_UNRESOLVED_BLOCKER"
            | "E_VERIFY_REQUIRED"
            | "E_REVIEW_REQUIRED"
            | "E_VERIFY_FAILED"
            | "E_TDD_EVIDENCE_REQUIRED"
            | "E_AGENT_TASK_FAILED"
            | "E_UNDECLARED_FILE_CHANGE"
            | "E_REVIEW_FAILED"
            | "E_REVIEW_BACKEND_UNAVAILABLE"
            | "E_REVIEW_BACKEND_TIMEOUT"
            | "E_REVIEW_BACKEND_FAILED"
            | "E_REVIEW_BACKEND_INVALID_OUTPUT"
            | "E_UNPLANNED_DEPENDENCY"
            | "E_ARCHIVED_READONLY"
            | "E_CONCURRENT_RUN"
            | "E_LOCK_TIMEOUT"
            | "E_TIMEOUT"
            | "E_INTERRUPTED"
            | "E_STATE_CORRUPTED"
            | "E_SECURITY_BLOCKED"
            | "E_PATH_OUTSIDE_REPO"
            | "E_SYMLINK_BLOCKED"
            | "E_PARALLEL_FILE_CONFLICT"
    )
}
