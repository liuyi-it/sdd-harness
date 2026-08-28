//! runtime.json 的 artifacts 节点集中记录制品路径、输入与内容哈希。

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::error::SddError;
use crate::state::runtime_store::{RuntimeDocument, RuntimeStore};

pub struct ArtifactRecord<'a> {
    pub key: &'a str,
    pub artifact_type: &'a str,
    pub content_path: &'a str,
    pub inputs: Value,
}

pub fn record_artifacts<'a>(
    cwd: &str,
    records: impl IntoIterator<Item = ArtifactRecord<'a>>,
) -> Result<(), SddError> {
    let records: Vec<_> = records.into_iter().collect();
    for record in &records {
        validate_content_path(record.content_path)?;
    }
    RuntimeStore::new(cwd.to_string())
        .try_update(move |document| record_artifacts_in(cwd, document, records))?;
    Ok(())
}

pub(crate) fn record_artifacts_in<'a>(
    cwd: &str,
    document: &mut RuntimeDocument,
    records: Vec<ArtifactRecord<'a>>,
) -> Result<(), SddError> {
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        validate_content_path(record.content_path)?;
        let item = json!({
            "type": record.artifact_type,
            "hash": content_digest(cwd, document, record.content_path)?,
            "contentPath": record.content_path,
            "status": "READY",
            "inputs": record.inputs,
        });
        crate::schema::validate_json("artifact", &item)?;
        items.push((record.key, item));
    }

    let entries = document
        .artifacts
        .get_mut("artifacts")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "runtime artifacts.artifacts 必须是对象",
            )
        })?;
    for (key, item) in items {
        entries.insert(key.to_string(), item);
    }
    Ok(())
}

pub fn verify_artifacts<I, S>(cwd: &str, keys: I) -> Result<(), SddError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut keys = keys.into_iter();
    let Some(first) = keys.next() else {
        return Ok(());
    };
    let document = RuntimeStore::new(cwd.to_string()).read()?;
    verify_artifacts_in(cwd, &document, std::iter::once(first).chain(keys))
}

pub(crate) fn verify_artifacts_in<I, S>(
    cwd: &str,
    document: &RuntimeDocument,
    keys: I,
) -> Result<(), SddError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for key in keys {
        verify_artifact(cwd, document, key.as_ref())?;
    }
    Ok(())
}

fn verify_artifact(cwd: &str, document: &RuntimeDocument, key: &str) -> Result<(), SddError> {
    let item = document
        .artifacts
        .pointer(&format!(
            "/artifacts/{}",
            key.replace('~', "~0").replace('/', "~1")
        ))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", &format!("制品清单缺少条目：{key}")))?;
    crate::schema::validate_json("artifact", item)?;
    let content_path = item
        .get("contentPath")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "制品条目缺少 contentPath"))?;
    let expected = item
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "制品条目缺少 hash"))?;
    let actual = content_digest(cwd, document, content_path)?;
    if actual != expected {
        return Err(SddError::new(
            "E_COMPONENT_INTEGRITY_FAILED",
            &format!("制品哈希不匹配：{content_path}"),
        ));
    }
    Ok(())
}

fn content_digest(
    cwd: &str,
    document: &RuntimeDocument,
    content_path: &str,
) -> Result<String, SddError> {
    validate_content_path(content_path)?;
    if content_path.starts_with("runtime://") {
        return Ok(crate::policies::digest::digest(&virtual_content(
            document,
            content_path,
        )?));
    }
    let file = fs::File::open(resolve_content_path(cwd, content_path)?).map_err(|error| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("读取制品 {content_path} 失败：{error}"),
        )
    })?;
    crate::policies::digest::digest_reader(file).map_err(|error| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("读取制品 {content_path} 失败：{error}"),
        )
    })
}

fn virtual_content(document: &RuntimeDocument, path: &str) -> Result<String, SddError> {
    let suffix = path.strip_prefix("runtime://").ok_or_else(|| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("不是 runtime 虚拟路径：{path}"),
        )
    })?;
    let parts: Vec<&str> = suffix.split('/').collect();
    let value = match parts.as_slice() {
        ["config"] => Some(&document.config),
        ["index", field] => document.index.get(*field),
        ["changes", change_id, field] => document
            .changes
            .get(*change_id)
            .and_then(|change| change.get(*field)),
        ["changes", change_id, "reports", report] => document
            .changes
            .get(*change_id)
            .and_then(|change| change.get("reports"))
            .and_then(|reports| reports.get(*report)),
        ["runs", run_id, field] => document.runs.get(*run_id).and_then(|run| run.get(*field)),
        ["runs", run_id, "tasks", task_id] => document
            .runs
            .get(*run_id)
            .and_then(|run| run.get("tasks"))
            .and_then(|tasks| tasks.get(*task_id)),
        _ => None,
    }
    .ok_or_else(|| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("runtime 制品内容不存在：{path}"),
        )
    })?;
    serde_json::to_string_pretty(value).map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("格式化 runtime 制品失败：{error}"),
        )
    })
}

pub(crate) fn validate_content_path(relative: &str) -> Result<(), SddError> {
    if relative.starts_with("runtime://") {
        let suffix = relative
            .strip_prefix("runtime://")
            .expect("starts_with 已确认 runtime 虚拟路径前缀");
        if suffix.is_empty()
            || suffix.starts_with('/')
            || suffix
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                &format!("runtime 制品路径非法：{relative}"),
            ));
        }
        return Ok(());
    }
    let normalized = relative.replace('\\', "/");
    let first = normalized.split('/').next().unwrap_or("");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || first.ends_with(':')
        || normalized
            .split('/')
            .any(|part| part == "." || part == ".." || part.is_empty())
    {
        return Err(SddError::new(
            "E_PATH_OUTSIDE_REPO",
            &format!("制品路径不在项目内：{relative}"),
        ));
    }
    Ok(())
}

fn resolve_content_path(cwd: &str, relative: &str) -> Result<PathBuf, SddError> {
    validate_content_path(relative)?;
    let root = PathBuf::from(cwd).canonicalize().map_err(|error| {
        SddError::new("E_PATH_OUTSIDE_REPO", &format!("解析项目路径失败：{error}"))
    })?;
    let path = root.join(relative);
    let resolved = path.canonicalize().map_err(|error| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("解析制品路径 {relative} 失败：{error}"),
        )
    })?;
    if !resolved.starts_with(&root) {
        return Err(SddError::new(
            "E_SYMLINK_BLOCKED",
            &format!("制品路径通过符号链接逃逸项目：{relative}"),
        ));
    }
    Ok(resolved)
}
