//! Agent 资产层：把宿主原生 Skill 与 subagent 配置写入业务项目。

use std::fs;
use std::path::PathBuf;

use crate::contracts::HostAdapter;
use crate::error::SddError;

struct AssetFile {
    adapter: HostAdapter,
    target: &'static str,
    content: &'static str,
}

const ADAPTER_ASSETS: [AssetFile; 18] = [
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/skills/sdd-harness/SKILL.md",
        content: include_str!("../../../assets/adapters/omp/skills/sdd-harness/SKILL.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/config.yml",
        content: include_str!("../../../assets/adapters/omp/config.yml"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.init.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.init.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.new.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.new.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.change.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.change.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.status.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.status.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.plan.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.plan.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.verify.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.verify.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.review.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.review.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/commands/sdd.archive.md",
        content: include_str!("../../../assets/adapters/omp/commands/sdd.archive.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/agents/sdd-worker.md",
        content: include_str!("../../../assets/adapters/omp/agents/sdd-worker.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/agents/sdd-worker-simple.md",
        content: include_str!("../../../assets/adapters/omp/agents/sdd-worker-simple.md"),
    },
    AssetFile {
        adapter: HostAdapter::Omp,
        target: ".omp/agents/sdd-worker-complex.md",
        content: include_str!("../../../assets/adapters/omp/agents/sdd-worker-complex.md"),
    },
    AssetFile {
        adapter: HostAdapter::Codex,
        target: ".agents/skills/sdd-harness/SKILL.md",
        content: include_str!("../../../assets/adapters/codex/skills/sdd-harness/SKILL.md"),
    },
    AssetFile {
        adapter: HostAdapter::Codex,
        target: ".codex/agents/sdd-explorer.toml",
        content: include_str!("../../../assets/adapters/codex/agents/sdd-explorer.toml"),
    },
    AssetFile {
        adapter: HostAdapter::Codex,
        target: ".codex/agents/sdd-worker.toml",
        content: include_str!("../../../assets/adapters/codex/agents/sdd-worker.toml"),
    },
    AssetFile {
        adapter: HostAdapter::Codex,
        target: ".codex/agents/sdd-reviewer.toml",
        content: include_str!("../../../assets/adapters/codex/agents/sdd-reviewer.toml"),
    },
];

pub(crate) fn write_adapter_files(
    project_root: &str,
    adapter: HostAdapter,
) -> Result<Vec<&'static str>, SddError> {
    let root = PathBuf::from(project_root);
    let mut written = Vec::new();
    for asset in adapter_assets(adapter) {
        let target = root.join(asset.target);
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
            fs::create_dir_all(parent).map_err(|error| {
                SddError::new("E_STATE_CORRUPTED", &format!("创建目录失败：{error}"))
            })?;
        }
        fs::write(&target, asset.content).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("写入 {} 失败：{error}", asset.target),
            )
        })?;
        written.push(asset.target);
    }
    Ok(written)
}

/// 返回已存在且内容与内置模板不同的项目内目标路径。
pub(crate) fn detect_overwrites(root: &str, adapter: HostAdapter) -> Vec<&'static str> {
    let root = PathBuf::from(root);
    adapter_assets(adapter)
        .filter(|asset| match fs::read_to_string(root.join(asset.target)) {
            Ok(content) => content != asset.content,
            Err(_) => false,
        })
        .map(|asset| asset.target)
        .collect()
}

fn adapter_assets(adapter: HostAdapter) -> impl Iterator<Item = &'static AssetFile> {
    ADAPTER_ASSETS
        .iter()
        .filter(move |asset| asset.adapter == adapter)
}
