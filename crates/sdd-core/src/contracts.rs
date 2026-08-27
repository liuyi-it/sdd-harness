//! 对外稳定契约：命令集合、阶段枚举、错误码到退出码映射、请求/响应结构。
//!
//! `codebase.provider` 使用 `codegraph | fallback-file-scan`。

use serde::{Deserialize, Serialize};

/// 当前 SDD 可原生接入的宿主 Agent。
///
/// 该类型是 CLI 注入的 `hostAdapter` 与资产层之间唯一的边界，避免在各层散落
/// 字符串字面量和不一致的可选值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAdapter {
    Codex,
    Omp,
}

impl HostAdapter {
    pub const DEFAULT: Self = Self::Codex;

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Omp => "omp",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "codex" => Some(Self::Codex),
            "omp" => Some(Self::Omp),
            _ => None,
        }
    }
}

/// 当前工作流会真实持久化的阶段枚举。
pub const PHASES: [&str; 14] = [
    "NOT_INITIALIZED",
    "INITIALIZING",
    "INDEX_READY",
    "NEW_STARTED",
    "CLARIFYING",
    "SPEC_READY",
    "DESIGN_READY",
    "PLAN_READY",
    "BUILD_WAITING_AGENT",
    "BUILD_READY",
    "VERIFY_READY",
    "REVIEW_READY",
    "ARCHIVED",
    "PAUSED",
];

/// 错误码 → 退出码的唯一映射表，未登记错误统一按内部错误退出。
const ERROR_EXIT_CODES: [(&str, i32); 30] = [
    ("E_NOT_INITIALIZED", 3),
    ("E_INVALID_PHASE_COMMAND", 3),
    ("E_ACTIVE_CHANGE_EXISTS", 3),
    ("E_MISSING_CHANGE", 4),
    ("E_MISSING_ARTIFACT", 4),
    ("E_INVALID_REQUIREMENT", 6),
    ("E_COMPONENT_UNAVAILABLE", 5),
    ("E_COMPONENT_INTEGRITY_FAILED", 10),
    ("E_UNRESOLVED_BLOCKER", 6),
    ("E_VERIFY_REQUIRED", 3),
    ("E_REVIEW_REQUIRED", 3),
    ("E_VERIFY_FAILED", 7),
    ("E_TDD_EVIDENCE_REQUIRED", 7),
    ("E_AGENT_TASK_FAILED", 7),
    ("E_UNDECLARED_FILE_CHANGE", 10),
    ("E_REVIEW_FAILED", 8),
    ("E_REVIEW_BACKEND_UNAVAILABLE", 5),
    ("E_REVIEW_BACKEND_TIMEOUT", 124),
    ("E_REVIEW_BACKEND_FAILED", 8),
    ("E_REVIEW_BACKEND_INVALID_OUTPUT", 8),
    ("E_UNPLANNED_DEPENDENCY", 8),
    ("E_ARCHIVED_READONLY", 3),
    ("E_CONCURRENT_RUN", 9),
    ("E_LOCK_TIMEOUT", 9),
    ("E_TIMEOUT", 124),
    ("E_STATE_CORRUPTED", 1),
    ("E_SECURITY_BLOCKED", 10),
    ("E_PATH_OUTSIDE_REPO", 10),
    ("E_SYMLINK_BLOCKED", 10),
    ("E_GENERATION_FAILED", 5),
];

/// 错误码到退出码的映射。
pub fn error_exit_codes(code: &str) -> i32 {
    ERROR_EXIT_CODES
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, exit_code)| *exit_code)
        .unwrap_or(1)
}

/// 结构化警告。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CliWarning {
    /// 警告码，如 "W_KNOWLEDGE_UNAVAILABLE"
    pub code: String,
    /// 人类可读警告信息
    pub message: String,
    /// 建议的下一步命令
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

impl CliWarning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            next: None,
        }
    }

    pub fn with_next(mut self, next: impl Into<String>) -> Self {
        self.next = Some(next.into());
        self
    }
}

/// 验证命令（build next 的 verification 项）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCommand {
    pub command: String,
    pub args: Vec<String>,
}

/// 知识图谱提供方信息（provider 为 codegraph/fallback-file-scan）
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodebaseProviderInfo {
    pub provider: String,
    pub degraded: bool,
}

/// Agent 行动要求（build next 返回此结构，结果通过 inline JSON 提交）。
#[derive(Debug, Clone, Serialize, PartialEq)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_bundle: Option<serde_json::Value>,
}

/// Core 请求结构
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub command: String,
    pub cwd: String,
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

/// 错误结构
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// Core 响应结构（JSON 输出契约使用 camelCase 键名）。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub ok: bool,
    pub state: String,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<RenderedOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<CliWarning>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_required: Option<AgentActionRequired>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
