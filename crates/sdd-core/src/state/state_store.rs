//! StateStore 负责工作流状态的读取、校验与原子写入。
//!
//! 维护 `.sdd/state.json` 这一工作流事实源。翻译自 Node 版
//! `packages/core/src/state/state-store.ts`，字段语义保持一致，
//! 磁盘格式按"允许重构"决策精简（camelCase 键名保持可读性）。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::SddError;

pub const SDD_DIR: &str = ".sdd";
pub const STATE_FILE: &str = "state.json";

/// 当前状态文件 schema 版本（Rust 版新格式）
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

/// 任务状态枚举（与 Node 版 taskStatusSchema 一致）
pub const TASK_STATUS_PENDING: &str = "PENDING";
pub const TASK_STATUS_BUILDING: &str = "BUILDING";
pub const TASK_STATUS_DONE: &str = "DONE";
pub const TASK_STATUS_FAILED: &str = "FAILED";
pub const TASK_STATUS_SKIPPED: &str = "SKIPPED";

/// 索引状态枚举（与 Node 版 indexStatus 一致）
pub const INDEX_STATUS_MISSING: &str = "MISSING";
pub const INDEX_STATUS_INDEXING: &str = "INDEXING";
pub const INDEX_STATUS_INDEX_READY: &str = "INDEX_READY";
pub const INDEX_STATUS_STALE: &str = "STALE";
pub const INDEX_STATUS_UNAVAILABLE: &str = "UNAVAILABLE";

/// worktree 隔离信息
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WorkspaceInfo {
    #[serde(default)]
    pub branch_name: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    pub baseline_commit: String,
}

/// 工作流状态（对应 Node 版 WorkflowState 的核心字段）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowState {
    pub schema_version: u32,
    pub version: u32,
    pub updated_at: String,
    pub initialized: bool,
    pub current_change_id: Option<String>,
    pub current_run_id: Option<String>,
    /// 活动 loop 摘要（深层结构在 loop 引擎任务中细化）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_loop: Option<serde_json::Value>,
    /// 等待中的 Agent 任务
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_agent_task: Option<serde_json::Value>,
    pub current_phase: String,
    pub index_status: String,
    /// 知识图谱提供方：gitnexus | codegraph | fallback-file-scan
    pub codebase_provider: String,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub last_command: Option<String>,
    pub last_error: Option<String>,
    pub previous_phase: Option<String>,
    pub in_progress_phase: Option<String>,
    pub failed_command: Option<String>,
    pub failed_reason: Option<String>,
    pub interrupted_command: Option<String>,
    pub recoverable: bool,
    pub suggested_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    #[serde(default)]
    pub tasks: HashMap<String, String>,
    #[serde(default)]
    pub artifacts: HashMap<String, String>,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::not_initialized()
    }
}

impl WorkflowState {
    /// 初始状态（对应 createInitialState，codebaseProvider 改为默认 gitnexus）
    pub fn not_initialized() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            version: 1,
            updated_at: now_iso(),
            initialized: false,
            current_change_id: None,
            current_run_id: None,
            active_loop: None,
            pending_agent_task: None,
            current_phase: "NOT_INITIALIZED".to_string(),
            index_status: INDEX_STATUS_MISSING.to_string(),
            codebase_provider: "gitnexus".to_string(),
            degraded: false,
            degraded_reason: None,
            last_command: None,
            last_error: None,
            previous_phase: None,
            in_progress_phase: None,
            failed_command: None,
            failed_reason: None,
            interrupted_command: None,
            recoverable: true,
            suggested_command: Some("sdd init".to_string()),
            workspace: None,
            tasks: HashMap::new(),
            artifacts: HashMap::new(),
        }
    }

    /// 生成带阶段变更的新状态（保留其他字段）
    pub fn with_phase(&self, phase: &str) -> Self {
        let mut next = self.clone();
        next.current_phase = phase.to_string();
        next.updated_at = now_iso();
        next
    }

    pub fn with_change(&self, change_id: Option<String>) -> Self {
        let mut next = self.clone();
        next.current_change_id = change_id;
        next.updated_at = now_iso();
        next
    }

    pub fn with_run(&self, run_id: Option<String>) -> Self {
        let mut next = self.clone();
        next.current_run_id = run_id;
        next.updated_at = now_iso();
        next
    }
}

pub struct StateStore {
    root: PathBuf,
}

impl StateStore {
    pub fn new(cwd: String) -> Self {
        Self {
            root: PathBuf::from(cwd),
        }
    }

    pub fn sdd_dir(&self) -> PathBuf {
        self.root.join(SDD_DIR)
    }

    pub fn state_path(&self) -> PathBuf {
        self.sdd_dir().join(STATE_FILE)
    }

    /// 读取状态；文件不存在时返回初始状态（未初始化）
    pub fn read(&self) -> Result<WorkflowState, SddError> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(WorkflowState::not_initialized());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取状态文件失败：{}", e)))?;
        serde_json::from_str::<WorkflowState>(&raw).map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("状态文件 JSON 解析失败：{}", e),
            )
        })
    }

    /// 原子写入：临时文件 + rename，避免半写入状态
    pub fn write(&self, state: &WorkflowState) -> Result<(), SddError> {
        let dir = self.sdd_dir();
        fs::create_dir_all(&dir).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{}", e))
        })?;
        let path = self.state_path();
        let tmp = dir.join("state.json.tmp");
        let content = serde_json::to_string_pretty(state)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化状态失败：{}", e)))?;
        fs::write(&tmp, content)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入临时状态失败：{}", e)))?;
        fs::rename(&tmp, &path)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("提交状态文件失败：{}", e)))
    }

    /// 读-改-写原子更新
    pub fn update<F>(&self, f: F) -> Result<WorkflowState, SddError>
    where
        F: FnOnce(&mut WorkflowState),
    {
        let mut state = self.read()?;
        f(&mut state);
        state.updated_at = now_iso();
        state.version = state.version.saturating_add(1);
        self.write(&state)?;
        Ok(state)
    }
}

/// 当前 ISO 时间字符串
pub fn now_iso() -> String {
    // 无外部依赖：使用系统时间格式化（精度到秒即可，语义与 Node 的 ISO 字符串一致）
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso_epoch(secs)
}

pub fn format_iso_epoch(secs: u64) -> String {
    // 简单实现：只保证形如 "YYYY-MM-DDTHH:MM:SSZ"（UTC）
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// 天数转公历日期（Howard Hinnant 算法）
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// 路径是否为 .sdd 目录
pub fn is_sdd_path(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(SDD_DIR))
}
