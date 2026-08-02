//! 策略解析：从策略目录读取并编译受控 Policy。
//!
//! 翻译自 `packages/agent-policies/src/resolver.ts`：
//! 按阶段返回适用的策略包（含摘要）。

use std::path::PathBuf;

use super::compiler::{compile_policy, PolicyRule};
use super::digest::digest;
use crate::error::SddError;

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyBundle {
    pub name: String,
    pub source: String,
    pub digest: String,
    pub rules: Vec<PolicyRule>,
}

/// 从 assets/policies 目录解析策略（目录不存在时返回空列表）
pub fn resolve_policies(policies_root: &str) -> Result<Vec<PolicyBundle>, SddError> {
    let dir = PathBuf::from(policies_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut bundles = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取策略目录失败：{e}")))?
    {
        let entry = entry
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取策略条目失败：{e}")))?;
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "md").unwrap_or(false) {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取策略失败：{e}")))?;
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "policy".to_string());
            bundles.push(PolicyBundle {
                name,
                source: content.clone(),
                digest: digest(&content),
                rules: compile_policy(&content),
            });
        }
    }
    Ok(bundles)
}
