//! Agent 资产层：把项目级 Skill、命令和 subagent 模板写入业务项目。
//!
//! 模板通过 include_str! 嵌入二进制，`sdd init` 默认写入 `.omp/`，也可选择
//! OpenCode 原生目录。

use std::fs;
use std::path::PathBuf;

use crate::error::SddError;

/// 资产文件描述：源路径（assets/adapters/<adapter> 下）→ 项目内目标相对路径。
pub struct AssetFile {
    pub key: &'static str,
    pub target: &'static str,
    pub content: &'static str,
}

/// OMP 与 OpenCode 原生资产（include_str! 编译期嵌入）。
pub const ADAPTER_ASSETS: [AssetFile; 27] = [
    AssetFile {
        key: "omp/skills/sdd-harness/SKILL.md",
        target: ".omp/skills/sdd-harness/SKILL.md",
        content: include_str!("../../../assets/adapters/omp/skills/sdd-harness/SKILL.md"),
    },
    AssetFile {
        key: "omp/config.yml",
        target: ".omp/config.yml",
        content: include_str!("../../../assets/adapters/omp/config.yml"),
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
    AssetFile {
        key: "omp/agents/sdd-worker-simple.md",
        target: ".omp/agents/sdd-worker-simple.md",
        content: include_str!("../../../assets/adapters/omp/agents/sdd-worker-simple.md"),
    },
    AssetFile {
        key: "omp/agents/sdd-worker-complex.md",
        target: ".omp/agents/sdd-worker-complex.md",
        content: include_str!("../../../assets/adapters/omp/agents/sdd-worker-complex.md"),
    },
    AssetFile {
        key: "opencode/skills/sdd-harness/SKILL.md",
        target: ".opencode/skills/sdd-harness/SKILL.md",
        content: include_str!("../../../assets/adapters/opencode/skills/sdd-harness/SKILL.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd.md",
        target: ".opencode/commands/sdd.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-init.md",
        target: ".opencode/commands/sdd-init.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-init.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-new.md",
        target: ".opencode/commands/sdd-new.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-new.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-change.md",
        target: ".opencode/commands/sdd-change.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-change.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-status.md",
        target: ".opencode/commands/sdd-status.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-status.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-plan.md",
        target: ".opencode/commands/sdd-plan.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-plan.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-verify.md",
        target: ".opencode/commands/sdd-verify.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-verify.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-review.md",
        target: ".opencode/commands/sdd-review.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-review.md"),
    },
    AssetFile {
        key: "opencode/commands/sdd-archive.md",
        target: ".opencode/commands/sdd-archive.md",
        content: include_str!("../../../assets/adapters/opencode/commands/sdd-archive.md"),
    },
    AssetFile {
        key: "opencode/agents/sdd-worker-simple.md",
        target: ".opencode/agents/sdd-worker-simple.md",
        content: include_str!("../../../assets/adapters/opencode/agents/sdd-worker-simple.md"),
    },
    AssetFile {
        key: "opencode/agents/sdd-worker.md",
        target: ".opencode/agents/sdd-worker.md",
        content: include_str!("../../../assets/adapters/opencode/agents/sdd-worker.md"),
    },
    AssetFile {
        key: "opencode/agents/sdd-worker-complex.md",
        target: ".opencode/agents/sdd-worker-complex.md",
        content: include_str!("../../../assets/adapters/opencode/agents/sdd-worker-complex.md"),
    },
];

/// 写入默认 OMP 模板。
pub fn write_adapter_files(project_root: &str) -> Result<Vec<String>, SddError> {
    write_adapter_files_for(project_root, "omp")
}

/// 写入指定适配器模板；目前只支持 OMP 与 OpenCode。
pub fn write_adapter_files_for(project_root: &str, adapter: &str) -> Result<Vec<String>, SddError> {
    let prefix = match adapter {
        "omp" | "opencode" => format!("{adapter}/"),
        _ => {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "Agent 仅支持 omp 或 opencode",
            ));
        }
    };
    write_assets(project_root, &prefix)
}

/// 写入一个适配器的资产（幂等：目标已存在且内容相同则跳过，过期模板直接更新）。
fn write_assets(project_root: &str, prefix: &str) -> Result<Vec<String>, SddError> {
    let mut written = Vec::new();
    for asset in ADAPTER_ASSETS {
        if !asset.key.starts_with(prefix) {
            continue;
        }
        let target = PathBuf::from(project_root).join(asset.target);
        let existing = match fs::read_to_string(&target) {
            Ok(content) => Some(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("读取已有 Agent 文件 {} 失败：{error}", target.display()),
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
