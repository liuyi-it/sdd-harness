//! `.sdd/runtime.json` 统一保存 SDD 的机器可读数据。
//!
//! changes 下只保留可读 Markdown；状态、配置、制品索引、规格模型、计划任务、
//! 报告、任务结果、Context Pack、知识索引和 auto loop 全部通过本模块原子更新。

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::SddError;
use crate::state::state_store::{validate_run_id, WorkflowState, SDD_DIR};

pub const RUNTIME_FILE: &str = "runtime.json";
pub const RUNTIME_CHECKSUM_FILE: &str = "runtime.json.sha256";
pub const RUNTIME_SCHEMA_VERSION: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDocument {
    pub schema_version: u32,
    pub state: WorkflowState,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub artifacts: Value,
    #[serde(default)]
    pub changes: BTreeMap<String, Value>,
    #[serde(default)]
    pub runs: BTreeMap<String, Value>,
    #[serde(rename = "loop", default)]
    pub loop_state: Value,
    #[serde(default)]
    pub index: Value,
}

impl Default for RuntimeDocument {
    fn default() -> Self {
        Self {
            schema_version: RUNTIME_SCHEMA_VERSION,
            state: WorkflowState::not_initialized(),
            config: json!({}),
            artifacts: json!({
                "schemaVersion": "2.0.0",
                "artifacts": {},
            }),
            changes: BTreeMap::new(),
            runs: BTreeMap::new(),
            loop_state: json!({
                "runs": {},
                "events": {},
            }),
            index: json!({}),
        }
    }
}

pub struct RuntimeStore {
    root: PathBuf,
}

impl RuntimeStore {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self {
            root: PathBuf::from(cwd.into()),
        }
    }

    pub fn runtime_path(&self) -> PathBuf {
        self.root.join(SDD_DIR).join(RUNTIME_FILE)
    }

    pub fn sdd_dir(&self) -> PathBuf {
        self.root.join(SDD_DIR)
    }

    fn backup_path(&self) -> PathBuf {
        self.sdd_dir().join("runtime.json.bak")
    }

    pub fn read(&self) -> Result<RuntimeDocument, SddError> {
        let path = self.runtime_path();
        if !path.exists() {
            return Ok(RuntimeDocument::default());
        }
        // 主文件或其校验和损坏时仅接受同样通过校验的备份。
        match self.read_path(&path) {
            Ok(document) => Ok(document),
            Err(primary_error) => {
                let backup = self.backup_path();
                if backup.exists() {
                    self.read_path(&backup).or(Err(primary_error))
                } else {
                    Err(primary_error)
                }
            }
        }
    }

    fn read_path(&self, path: &Path) -> Result<RuntimeDocument, SddError> {
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
        crate::schema::validate_json("runtime", &value)?;
        let document: RuntimeDocument = serde_json::from_value(value).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("runtime.json 结构解析失败：{error}"),
            )
        })?;
        if document.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!(
                    "runtime schemaVersion {} 与当前版本 {} 不匹配",
                    document.schema_version, RUNTIME_SCHEMA_VERSION
                ),
            ));
        }
        validate_identifiers(&document.state)?;
        Ok(document)
    }

    fn verify_checksum(&self, path: &Path, raw: &str) -> Result<(), SddError> {
        let sidecar = checksum_path(path);
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

    pub fn write(&self, document: &RuntimeDocument) -> Result<(), SddError> {
        validate_identifiers(&document.state)?;
        let dir = self.sdd_dir();
        fs::create_dir_all(&dir).map_err(|error| {
            SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{error}"))
        })?;
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
        // 原子写：临时文件写入 + fsync 落盘，rename 提交，最后对目录做 best-effort fsync
        let temp = dir.join("runtime.json.tmp");
        let mut temp_file = File::create(&temp).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("创建 runtime 临时文件失败：{error}"),
            )
        })?;
        temp_file.write_all(content.as_bytes()).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("写入 runtime 临时文件失败：{error}"),
            )
        })?;
        temp_file.sync_all().map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("同步 runtime 临时文件失败：{error}"),
            )
        })?;
        let path = self.runtime_path();
        if path.exists() && self.read_path(&path).is_ok() {
            self.backup_current(&path)?;
        }
        fs::rename(&temp, &path).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("提交 runtime.json 失败：{error}"),
            )
        })?;
        sync_dir(&dir);
        write_checksum_sidecar(&path, &content)?;
        Ok(())
    }

    fn backup_current(&self, path: &Path) -> Result<(), SddError> {
        let backup = self.backup_path();
        fs::copy(path, &backup).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("备份 runtime.json 失败：{error}"),
            )
        })?;
        fs::copy(checksum_path(path), checksum_path(&backup)).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("备份 runtime 校验和失败：{error}"),
            )
        })?;
        Ok(())
    }

    pub fn update<F>(&self, update: F) -> Result<RuntimeDocument, SddError>
    where
        F: FnOnce(&mut RuntimeDocument),
    {
        let mut document = self.read()?;
        update(&mut document);
        self.write(&document)?;
        Ok(document)
    }
}

pub fn read_change(cwd: &str, change_id: &str) -> Result<Value, SddError> {
    crate::git::isolation::validate_change_id(change_id)?;
    let document = RuntimeStore::new(cwd.to_string()).read()?;
    document
        .changes
        .get(change_id)
        .cloned()
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", &format!("变更 {change_id} 不存在")))
}

pub fn read_change_field(
    cwd: &str,
    change_id: &str,
    field: &str,
) -> Result<Option<Value>, SddError> {
    Ok(read_change(cwd, change_id)?.get(field).cloned())
}

pub fn write_change_field(
    cwd: &str,
    change_id: &str,
    field: &str,
    value: Value,
) -> Result<(), SddError> {
    write_change_fields(cwd, change_id, [(field, value)])
}

pub fn write_change_fields<'a>(
    cwd: &str,
    change_id: &str,
    fields: impl IntoIterator<Item = (&'a str, Value)>,
) -> Result<(), SddError> {
    crate::git::isolation::validate_change_id(change_id)?;
    let fields: Vec<_> = fields.into_iter().collect();
    RuntimeStore::new(cwd.to_string()).update(move |document| {
        let change = document
            .changes
            .entry(change_id.to_string())
            .or_insert_with(|| json!({}));
        if !change.is_object() {
            *change = json!({});
        }
        for (field, value) in fields {
            change[field] = value;
        }
    })?;
    Ok(())
}

pub fn write_run_field(cwd: &str, run_id: &str, field: &str, value: Value) -> Result<(), SddError> {
    write_run_fields(cwd, run_id, [(field, value)])
}

pub fn write_run_fields<'a>(
    cwd: &str,
    run_id: &str,
    fields: impl IntoIterator<Item = (&'a str, Value)>,
) -> Result<(), SddError> {
    validate_run_id(run_id)?;
    let fields: Vec<_> = fields.into_iter().collect();
    RuntimeStore::new(cwd.to_string()).update(move |document| {
        let run = document
            .runs
            .entry(run_id.to_string())
            .or_insert_with(|| json!({}));
        if !run.is_object() {
            *run = json!({});
        }
        for (field, value) in fields {
            run[field] = value;
        }
    })?;
    Ok(())
}

pub fn read_run_field(cwd: &str, run_id: &str, field: &str) -> Result<Option<Value>, SddError> {
    validate_run_id(run_id)?;
    let document = RuntimeStore::new(cwd.to_string()).read()?;
    Ok(document
        .runs
        .get(run_id)
        .and_then(|run| run.get(field))
        .cloned())
}

pub fn read_index_field(cwd: &str, field: &str) -> Result<Option<Value>, SddError> {
    let document = RuntimeStore::new(cwd.to_string()).read()?;
    Ok(document.index.get(field).cloned())
}

pub fn write_index(cwd: &str, diagnostics: Value, summary: String) -> Result<(), SddError> {
    RuntimeStore::new(cwd.to_string()).update(|document| {
        document.index = json!({
            "diagnostics": diagnostics,
            "summary": summary,
            "updatedAt": crate::state::state_store::now_iso(),
        });
    })?;
    Ok(())
}

pub fn write_config(cwd: &str, config: Value) -> Result<(), SddError> {
    RuntimeStore::new(cwd.to_string()).update(|document| document.config = config)?;
    Ok(())
}

pub fn read_config(cwd: &str) -> Result<Value, SddError> {
    Ok(RuntimeStore::new(cwd.to_string()).read()?.config)
}

pub fn write_loop_run(cwd: &str, run_id: &str, run: Value) -> Result<(), SddError> {
    validate_run_id(run_id)?;
    RuntimeStore::new(cwd.to_string()).update(|document| {
        if !document.loop_state.is_object() {
            document.loop_state = json!({ "runs": {}, "events": {} });
        }
        if document
            .loop_state
            .get("runs")
            .and_then(Value::as_object)
            .is_none()
        {
            document.loop_state["runs"] = json!({});
        }
        document.loop_state["runs"][run_id] = run;
    })?;
    Ok(())
}

pub fn append_loop_event(cwd: &str, run_id: &str, event: Value) -> Result<(), SddError> {
    validate_run_id(run_id)?;
    RuntimeStore::new(cwd.to_string()).update(|document| {
        if !document.loop_state.is_object() {
            document.loop_state = json!({ "runs": {}, "events": {} });
        }
        if document
            .loop_state
            .get("events")
            .and_then(Value::as_object)
            .is_none()
        {
            document.loop_state["events"] = json!({});
        }
        if document.loop_state["events"]
            .get(run_id)
            .and_then(Value::as_array)
            .is_none()
        {
            document.loop_state["events"][run_id] = json!([]);
        }
        document.loop_state["events"][run_id]
            .as_array_mut()
            .expect("events 数组已初始化")
            .push(event);
    })?;
    Ok(())
}

pub fn read_loop_events(cwd: &str, run_id: &str) -> Result<Vec<Value>, SddError> {
    validate_run_id(run_id)?;
    let document = RuntimeStore::new(cwd.to_string()).read()?;
    Ok(document
        .loop_state
        .get("events")
        .and_then(|events| events.get(run_id))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// 解析 artifact_store 使用的 runtime:// 虚拟路径。
pub fn read_virtual_content(cwd: &str, path: &str) -> Result<String, SddError> {
    let suffix = path.strip_prefix("runtime://").ok_or_else(|| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("不是 runtime 虚拟路径：{path}"),
        )
    })?;
    let parts: Vec<&str> = suffix.split('/').filter(|part| !part.is_empty()).collect();
    let document = RuntimeStore::new(cwd.to_string()).read()?;
    let value = match parts.as_slice() {
        ["config"] => document.config,
        ["index", field] => document.index.get(*field).cloned().unwrap_or(Value::Null),
        ["changes", change_id, field] => document
            .changes
            .get(*change_id)
            .and_then(|change| change.get(*field))
            .cloned()
            .unwrap_or(Value::Null),
        ["changes", change_id, "reports", report] => document
            .changes
            .get(*change_id)
            .and_then(|change| change.get("reports"))
            .and_then(|reports| reports.get(*report))
            .cloned()
            .unwrap_or(Value::Null),
        ["runs", run_id, field] => document
            .runs
            .get(*run_id)
            .and_then(|run| run.get(*field))
            .cloned()
            .unwrap_or(Value::Null),
        ["runs", run_id, "tasks", task_id] => document
            .runs
            .get(*run_id)
            .and_then(|run| run.get("tasks"))
            .and_then(|tasks| tasks.get(*task_id))
            .cloned()
            .unwrap_or(Value::Null),
        ["loop", "runs", run_id] => document
            .loop_state
            .get("runs")
            .and_then(|runs| runs.get(*run_id))
            .cloned()
            .unwrap_or(Value::Null),
        ["loop", "events", run_id] => document
            .loop_state
            .get("events")
            .and_then(|events| events.get(*run_id))
            .cloned()
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    serde_json::to_string_pretty(&value).map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("格式化 runtime 制品失败：{error}"),
        )
    })
}

fn validate_identifiers(state: &WorkflowState) -> Result<(), SddError> {
    if let Some(change_id) = state.current_change_id.as_deref() {
        crate::git::isolation::validate_change_id(change_id)?;
    }
    if let Some(run_id) = state.current_run_id.as_deref() {
        validate_run_id(run_id)?;
    }
    Ok(())
}

/// 将数据文件的 SHA-256 校验和以 tmp+rename 原子写入边车。
fn write_checksum_sidecar(path: &Path, content: &str) -> Result<(), SddError> {
    let checksum = crate::state::checksum::compute(content.as_bytes());
    let sidecar = checksum_path(path);
    let temp = sidecar.with_extension("sha256.tmp");
    let mut file = File::create(&temp).map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("创建 runtime 校验和临时文件失败：{error}"),
        )
    })?;
    file.write_all(format!("{checksum}\n").as_bytes())
        .map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("写入 runtime 校验和临时文件失败：{error}"),
            )
        })?;
    file.sync_all().map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("同步 runtime 校验和临时文件失败：{error}"),
        )
    })?;
    fs::rename(&temp, &sidecar).map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("提交 runtime 校验和失败：{error}"),
        )
    })
}

fn checksum_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .expect("runtime 内部路径必须包含文件名")
        .to_string_lossy();
    path.with_file_name(format!("{name}.sha256"))
}

/// 目录 fsync：保证 rename 后的目录项落盘（防断电丢文件）。
/// Unix 下打开目录文件 sync_all；Windows 不支持目录句柄，跳过（best-effort）。
#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Ok(dir_file) = File::open(dir) {
        let _ = dir_file.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) {}
