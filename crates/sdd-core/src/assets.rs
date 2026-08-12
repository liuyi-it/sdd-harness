//! OMP 资产层：把项目级 Skill、命令和 subagent 模板写入业务项目。
//!
//! 模板通过 include_str! 嵌入二进制，`sdd init` 默认写入 `.omp/`。

use std::fs;
use std::path::PathBuf;

use crate::error::SddError;

/// 资产文件描述：源路径（assets/adapters/omp 下）→ 项目内目标相对路径。
pub struct AssetFile {
    pub key: &'static str,
    pub target: &'static str,
    pub content: &'static str,
}

/// OMP 原生资产（include_str! 编译期嵌入）。
pub const ADAPTER_ASSETS: [AssetFile; 11] = [
    AssetFile {
        key: "omp/skills/sdd-harness/SKILL.md",
        target: ".omp/skills/sdd-harness/SKILL.md",
        content: include_str!("../../../assets/adapters/omp/skills/sdd-harness/SKILL.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.md",
        target: ".omp/commands/sdd.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.init.md",
        target: ".omp/commands/sdd.init.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.init.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.new.md",
        target: ".omp/commands/sdd.new.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.new.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.change.md",
        target: ".omp/commands/sdd.change.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.change.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.status.md",
        target: ".omp/commands/sdd.status.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.status.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.plan.md",
        target: ".omp/commands/sdd.plan.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.plan.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.verify.md",
        target: ".omp/commands/sdd.verify.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.verify.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.review.md",
        target: ".omp/commands/sdd.review.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.review.md"),
    },
    AssetFile {
        key: "omp/commands/sdd.archive.md",
        target: ".omp/commands/sdd.archive.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.archive.md"),
    },
    AssetFile {
        key: "omp/agents/sdd-worker.md",
        target: ".omp/agents/sdd-worker.md",
        content: include_str!("../../../assets/adapters/omp/agents/sdd-worker.md"),
    },
];

/// 写入 OMP 模板（幂等：目标已存在且内容相同则跳过，过期模板直接更新）。
pub fn write_adapter_files(project_root: &str) -> Result<Vec<String>, SddError> {
    let mut written = Vec::new();
    for asset in ADAPTER_ASSETS {
        let target = PathBuf::from(project_root).join(asset.target);
        let existing = match fs::read_to_string(&target) {
            Ok(content) => Some(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("读取已有 OMP 文件 {} 失败：{error}", target.display()),
                ));
            }
        };
        if existing.as_deref() == Some(asset.content) {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建目录失败：{e}")))?;
        }
        fs::write(&target, asset.content).map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("写入 {} 失败：{e}", asset.target),
            )
        })?;
        written.push(format!("写入：{}", asset.target));
    }
    Ok(written)
}
