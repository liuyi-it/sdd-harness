//! 资产层：Adapter 模板嵌入与 init 写入。
//!
//! 模板文件通过 include_str! 嵌入二进制，`sdd init` 时按 Agent 写出到项目
//! 对应目录（.claude/commands、.codex/rules 等），单二进制分发。
//! 翻译自 Node 版 `packages/core/src/install/project-installer.ts` 的写入语义。

use std::fs;
use std::path::PathBuf;

use crate::error::SddError;

/// 资产文件描述：源路径（assets/adapters 下）→ 项目内目标相对路径
pub struct AssetFile {
    /// 资产唯一键（如 "claude-code/commands/sdd.auto.md"）
    pub key: &'static str,
    /// 项目内目标路径（相对项目根）
    pub target: &'static str,
    pub content: &'static str,
}

/// 全部 adapter 资产（include_str! 编译期嵌入）
pub const ADAPTER_ASSETS: [AssetFile; 9] = [
    // claude-code
    AssetFile {
        key: "claude-code/commands/sdd.auto.md",
        target: ".claude/commands/sdd.auto.md",
        content: include_str!("../../../assets/adapters/claude-code/commands/sdd.auto.md"),
    },
    AssetFile {
        key: "claude-code/commands/sdd.status.md",
        target: ".claude/commands/sdd.status.md",
        content: include_str!("../../../assets/adapters/claude-code/commands/sdd.status.md"),
    },
    AssetFile {
        key: "claude-code/AGENTS.md",
        target: "AGENTS.md",
        content: include_str!("../../../assets/adapters/claude-code/AGENTS.md"),
    },
    // codex
    AssetFile {
        key: "codex/rules/sdd-harness.md",
        target: ".codex/rules/sdd-harness.md",
        content: include_str!("../../../assets/adapters/codex/rules/sdd-harness.md"),
    },
    AssetFile {
        key: "codex/skills/sdd.md",
        target: ".codex/skills/sdd-harness/sdd.md",
        content: include_str!("../../../assets/adapters/codex/skills/sdd.md"),
    },
    // opencode
    AssetFile {
        key: "opencode/rules/sdd-harness.md",
        target: ".opencode/rules/sdd-harness.md",
        content: include_str!("../../../assets/adapters/opencode/rules/sdd-harness.md"),
    },
    AssetFile {
        key: "opencode/docs/opencode-setup.md",
        target: ".opencode/docs/opencode-setup.md",
        content: include_str!("../../../assets/adapters/opencode/docs/opencode-setup.md"),
    },
    // generic-agent
    AssetFile {
        key: "generic-agent/docs/AGENT_PROTOCOL.md",
        target: "AGENT_PROTOCOL.md",
        content: include_str!("../../../assets/adapters/generic-agent/docs/AGENT_PROTOCOL.md"),
    },
    AssetFile {
        key: "generic-agent/examples/minimal-agent.mjs",
        target: "examples/minimal-agent.mjs",
        content: include_str!("../../../assets/adapters/generic-agent/examples/minimal-agent.mjs"),
    },
];

/// 每个 Agent 对应的资产键前缀
pub fn assets_for_agent(agent: &str) -> Vec<&'static str> {
    let prefix = match agent {
        "claude" => "claude-code/",
        "codex" => "codex/",
        "opencode" => "opencode/",
        // generic 与未知 agent 一律写 generic-agent
        _ => "generic-agent/",
    };
    ADAPTER_ASSETS
        .iter()
        .filter(|asset| asset.key.starts_with(prefix))
        .map(|asset| asset.key)
        .collect()
}

/// 把指定 Agent 的模板写入项目（幂等：目标已存在且内容相同则跳过）
pub fn write_adapter_files(
    project_root: &str,
    agent: &str,
    force: bool,
) -> Result<Vec<String>, SddError> {
    let mut written = Vec::new();
    let prefix = match agent {
        "claude" => "claude-code/",
        "codex" => "codex/",
        "opencode" => "opencode/",
        _ => "generic-agent/",
    };
    for asset in ADAPTER_ASSETS.iter().filter(|a| a.key.starts_with(prefix)) {
        let target = PathBuf::from(project_root).join(asset.target);
        let existing = fs::read_to_string(&target).ok();
        if existing.as_deref() == Some(asset.content) {
            continue;
        }
        if existing.is_some() && !force {
            // 用户已有同名文件且内容不同：不覆盖，记录跳过
            written.push(format!("跳过（已存在且内容不同）：{}", asset.target));
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

/// 已注册的 agent 列表
pub fn known_agents() -> [&'static str; 4] {
    ["claude", "codex", "opencode", "generic"]
}
