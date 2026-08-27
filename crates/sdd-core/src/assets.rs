//! Agent 资产层：把宿主原生 Skill 与 subagent 配置写入业务项目。

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::contracts::HostAdapter;
use crate::error::SddError;

struct AssetFile {
    adapter: HostAdapter,
    target: &'static str,
    content: &'static str,
}

pub(crate) struct AdapterWriteResult {
    pub(crate) written: Vec<&'static str>,
    pub(crate) overwritten: Vec<&'static str>,
}

const ADAPTER_ASSETS: [AssetFile; 20] = [
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
        target: ".codex/agents/sdd-worker-complex.toml",
        content: include_str!("../../../assets/adapters/codex/agents/sdd-worker-complex.toml"),
    },
    AssetFile {
        adapter: HostAdapter::Codex,
        target: ".codex/agents/sdd-reviewer.toml",
        content: include_str!("../../../assets/adapters/codex/agents/sdd-reviewer.toml"),
    },
    AssetFile {
        adapter: HostAdapter::Codex,
        target: ".codex/agents/sdd-architect.toml",
        content: include_str!("../../../assets/adapters/codex/agents/sdd-architect.toml"),
    },
];

pub(crate) fn write_adapter_files(
    project_root: &str,
    adapter: HostAdapter,
) -> Result<AdapterWriteResult, SddError> {
    let root = fs::canonicalize(project_root).map_err(|error| {
        SddError::new("E_STATE_CORRUPTED", &format!("解析项目根目录失败：{error}"))
    })?;
    let mut written = Vec::new();
    let mut overwritten = Vec::new();
    let mut prepared_directories = std::collections::HashSet::new();
    for asset in adapter_assets(adapter) {
        let target = asset_target(&root, asset.target)?;
        let existing_matches = match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(SddError::new(
                    "E_SECURITY_BLOCKED",
                    &format!("Agent 资产路径包含符号链接：{}", target.display()),
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("Agent 资产目标不是普通文件：{}", target.display()),
                ));
            }
            Ok(metadata) => {
                let expected_len = u64::try_from(asset.content.len())
                    .expect("内嵌 Agent 资产长度必须可表示为 u64");
                if metadata.len() != expected_len {
                    Some(false)
                } else {
                    let mut existing = Vec::with_capacity(asset.content.len() + 1);
                    fs::File::open(&target)
                        .and_then(|file| file.take(expected_len + 1).read_to_end(&mut existing))
                        .map_err(|error| {
                            SddError::new(
                                "E_STATE_CORRUPTED",
                                &format!("读取已有 Agent 文件 {} 失败：{error}", target.display()),
                            )
                        })?;
                    Some(existing == asset.content.as_bytes())
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("检查已有 Agent 文件 {} 失败：{error}", target.display()),
                ));
            }
        };
        if existing_matches == Some(true) {
            continue;
        }
        if existing_matches.is_some() {
            overwritten.push(asset.target);
        }
        if let Some(parent) = target.parent() {
            if prepared_directories.insert(parent.to_path_buf()) {
                fs::create_dir_all(parent).map_err(|error| {
                    SddError::new("E_STATE_CORRUPTED", &format!("创建目录失败：{error}"))
                })?;
            }
        }
        crate::safe_fs::atomic_write(&target, asset.content.as_bytes(), asset.target)?;
        written.push(asset.target);
    }
    Ok(AdapterWriteResult {
        written,
        overwritten,
    })
}

fn asset_target(root: &Path, relative: &str) -> Result<PathBuf, SddError> {
    let mut target = root.to_path_buf();
    for component in Path::new(relative).components() {
        let Component::Normal(component) = component else {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "内嵌 Agent 资产路径不是安全相对路径",
            ));
        };
        target.push(component);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(SddError::new(
                    "E_SECURITY_BLOCKED",
                    &format!("Agent 资产路径包含符号链接：{}", target.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("检查 Agent 资产路径 {} 失败：{error}", target.display()),
                ));
            }
        }
    }
    Ok(target)
}

fn adapter_assets(adapter: HostAdapter) -> impl Iterator<Item = &'static AssetFile> {
    ADAPTER_ASSETS
        .iter()
        .filter(move |asset| asset.adapter == adapter)
}
