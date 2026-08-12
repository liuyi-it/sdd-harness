//! runtime.json 的 artifacts 节点集中记录制品路径、输入与内容哈希。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::error::SddError;
use crate::state::runtime_store::{read_virtual_content, RuntimeStore};

pub fn record_artifact(
    cwd: &str,
    key: &str,
    artifact_type: &str,
    content_path: &str,
    content: &str,
    inputs: serde_json::Value,
) -> Result<(), SddError> {
    validate_content_path(content_path)?;
    let item = json!({
        "type": artifact_type,
        "hash": crate::policies::digest::digest(content),
        "contentPath": content_path,
        "status": "READY",
        "inputs": inputs,
    });
    crate::schema::validate_json("artifact", &item)?;
    RuntimeStore::new(cwd.to_string()).update(|document| {
        if !document.artifacts.is_object() {
            document.artifacts = json!({
                "schemaVersion": "2.0.0",
                "artifacts": {},
            });
        }
        if document
            .artifacts
            .get("artifacts")
            .and_then(serde_json::Value::as_object)
            .is_none()
        {
            document.artifacts["artifacts"] = json!({});
        }
        document.artifacts["artifacts"][key] = item;
    })?;
    Ok(())
}

pub fn verify_artifact(cwd: &str, key: &str) -> Result<(), SddError> {
    let document = RuntimeStore::new(cwd.to_string()).read()?;
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
        .and_then(|value| value.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "制品条目缺少 contentPath"))?;
    let expected = item
        .get("hash")
        .and_then(|value| value.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "制品条目缺少 hash"))?;
    let actual = if content_path.starts_with("runtime://") {
        let content = read_virtual_content(cwd, content_path)?;
        crate::policies::digest::digest(&content)
    } else {
        let content = fs::read(resolve_content_path(cwd, content_path)?).map_err(|error| {
            SddError::new(
                "E_MISSING_ARTIFACT",
                &format!("读取制品 {content_path} 失败：{error}"),
            )
        })?;
        crate::policies::digest::digest_bytes(&content)
    };
    if actual != expected {
        return Err(SddError::new(
            "E_COMPONENT_INTEGRITY_FAILED",
            &format!("制品哈希不匹配：{content_path}"),
        ));
    }
    Ok(())
}

fn validate_content_path(relative: &str) -> Result<(), SddError> {
    if relative.starts_with("runtime://") {
        let suffix = relative.trim_start_matches("runtime://");
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
            .any(|part| part == ".." || part.is_empty())
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
