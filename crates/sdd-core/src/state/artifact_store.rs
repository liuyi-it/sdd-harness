//! `.sdd/artifacts.json` 集中记录制品路径、输入与内容哈希。

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde_json::json;

use crate::error::SddError;

pub fn record_artifact(
    cwd: &str,
    key: &str,
    artifact_type: &str,
    content_path: &str,
    content: &str,
    inputs: serde_json::Value,
) -> Result<(), SddError> {
    let path = PathBuf::from(cwd).join(".sdd/artifacts.json");
    fs::create_dir_all(path.parent().expect("artifacts.json 必须有父目录"))
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{e}")))?;
    validate_content_path(content_path)?;
    let mut registry = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path).map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取 artifacts.json 失败：{e}"),
            )
        })?)
        .map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("artifacts.json 解析失败：{e}"),
            )
        })?
    } else {
        json!({ "schemaVersion": "2.0.0", "artifacts": {} })
    };
    let item = json!({
        "type": artifact_type,
        "hash": crate::policies::digest::digest(content),
        "contentPath": content_path,
        "status": "READY",
        "inputs": inputs,
    });
    crate::schema::validate_json("artifact", &item)?;
    registry
        .get_mut("artifacts")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "artifacts.json 缺少 artifacts 对象"))?
        .insert(key.to_string(), item);

    let content = serde_json::to_string_pretty(&registry).map_err(|e| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("序列化 artifacts.json 失败：{e}"),
        )
    })?;
    let tmp = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut file = fs::File::create(&tmp).map_err(|e| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("创建 artifacts 临时文件失败：{e}"),
        )
    })?;
    file.write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("写入 artifacts 临时文件失败：{e}"),
            )
        })?;
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("清理 artifacts 备份失败：{e}"),
                )
            })?;
        }
        fs::rename(&path, &backup).map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("备份 artifacts.json 失败：{e}"),
            )
        })?;
    }
    fs::rename(&tmp, &path).map_err(|e| {
        if backup.exists() && !path.exists() {
            if let Err(restore_error) = fs::copy(&backup, &path) {
                return SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("提交 artifacts.json 失败：{e}；恢复备份也失败：{restore_error}"),
                );
            }
        }
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("提交 artifacts.json 失败：{e}"),
        )
    })
}

pub fn verify_artifact(cwd: &str, key: &str) -> Result<(), SddError> {
    let registry_path = PathBuf::from(cwd).join(".sdd/artifacts.json");
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&registry_path).map_err(|e| {
            SddError::new(
                "E_MISSING_ARTIFACT",
                &format!("读取 artifacts.json 失败：{e}"),
            )
        })?)
        .map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("artifacts.json 解析失败：{e}"),
            )
        })?;
    let pointer = format!("/artifacts/{}", key.replace('~', "~0").replace('/', "~1"));
    let item = registry
        .pointer(&pointer)
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
    let content = fs::read(resolve_content_path(cwd, content_path)?).map_err(|e| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("读取制品 {content_path} 失败：{e}"),
        )
    })?;
    if crate::policies::digest::digest_bytes(&content) != expected {
        return Err(SddError::new(
            "E_COMPONENT_INTEGRITY_FAILED",
            &format!("制品哈希不匹配：{content_path}"),
        ));
    }
    Ok(())
}

fn validate_content_path(relative: &str) -> Result<(), SddError> {
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
    let root = PathBuf::from(cwd)
        .canonicalize()
        .map_err(|e| SddError::new("E_PATH_OUTSIDE_REPO", &format!("解析项目路径失败：{e}")))?;
    let path = root.join(relative);
    let resolved = path.canonicalize().map_err(|e| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("解析制品路径 {relative} 失败：{e}"),
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
