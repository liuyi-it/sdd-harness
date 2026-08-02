//! 任务协议类型（翻译自 `packages/core/src/engines/superpowers/protocol.ts`）。

use serde::{Deserialize, Serialize};

pub const PHASES: [&str; 4] = ["RED", "GREEN", "REFACTOR", "VERIFY"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskDefinition {
    pub id: String,
    pub title: String,
    pub phase: String,
    pub status: String,
    pub requirements: Vec<String>,
    pub scenarios: Vec<String>,
    pub depends_on: Vec<String>,
    pub allowed_files: Vec<String>,
    pub expected_new_files: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub verification: Vec<String>,
    pub done_criteria: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_visible_outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_seam: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_refs: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequirementPlan {
    pub id: String,
    pub title: String,
    pub scenarios: Vec<ScenarioRef>,
    pub source_files: Vec<String>,
    pub test_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct PlanArtifacts {
    pub tasks: Vec<TaskDefinition>,
    pub tasks_markdown: String,
    pub test_plan: String,
    pub context: String,
    pub context_packs: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct PlanningInput {
    pub spec: String,
    pub design: String,
    pub impact: String,
    pub codebase_summary: String,
}

/// 阶段标题（与 phaseTitle 一致）
pub fn phase_title(phase: &str) -> &'static str {
    match phase {
        "RED" => "先写失败测试",
        "GREEN" => "最小实现",
        "REFACTOR" => "保持测试绿色并重构",
        "VERIFY" => "完整验证",
        _ => "未知阶段",
    }
}

/// 阶段指令（与 phaseInstruction 一致）
pub fn phase_instruction(phase: &str) -> &'static str {
    match phase {
        "RED" => "先写测试并观察其因目标行为缺失而预期失败。",
        "GREEN" => "编写最小实现使关联测试通过。",
        "REFACTOR" => "在重构过程中保持测试绿色。",
        "VERIFY" => "运行完整验证命令并确认全部通过。",
        _ => "",
    }
}

/// 完成标准（与 doneCriteria 一致）
pub fn done_criteria(phase: &str, scenarios: &[String]) -> Vec<String> {
    let scenario_text = if scenarios.is_empty() {
        "关联需求".to_string()
    } else {
        scenarios.join("、")
    };
    match phase {
        "RED" => vec![format!("{scenario_text} 的测试已编写并以预期原因失败")],
        "GREEN" => vec![format!("{scenario_text} 以最小实现通过测试")],
        "REFACTOR" => vec![format!("{scenario_text} 在重构后保持测试通过")],
        "VERIFY" => vec![format!("{scenario_text} 的完整验证命令全部通过")],
        _ => vec![],
    }
}
