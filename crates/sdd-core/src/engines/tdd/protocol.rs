//! TDD 任务协议类型。

use serde::{Deserialize, Serialize};

use crate::engines::spec::SpecDocument;

pub const PHASES: [&str; 4] = ["RED", "GREEN", "REFACTOR", "VERIFY"];

pub(crate) fn valid_task_id(task_id: &str) -> bool {
    let Some(rest) = task_id.strip_prefix("TASK-") else {
        return false;
    };
    let Some((sequence, phase)) = rest.split_once('-') else {
        return false;
    };
    sequence.len() == 3
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
        && PHASES.contains(&phase)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDefinition {
    pub id: String,
    pub title: String,
    pub phase: String,
    pub requirements: Vec<String>,
    pub scenarios: Vec<String>,
    pub depends_on: Vec<String>,
    pub allowed_files: Vec<String>,
    pub expected_new_files: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub verification: Vec<String>,
    pub done_criteria: Vec<String>,
    pub slice_type: String,
    pub user_visible_outcome: String,
    pub acceptance_criteria: Vec<String>,
    pub test_seam: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequirementPlan {
    pub id: String,
    pub title: String,
    pub scenarios: Vec<ScenarioRef>,
    pub source_files: Vec<String>,
    pub test_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct PlanArtifacts {
    pub tasks: Vec<TaskDefinition>,
    pub tasks_markdown: String,
    pub test_plan: String,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanningInput<'a> {
    pub specification: &'a SpecDocument,
    pub design: &'a str,
    pub impact: &'a str,
    pub codebase_summary: &'a str,
}

/// 当前任务阶段标题。
pub(crate) fn phase_title(phase: &str) -> &'static str {
    match phase {
        "RED" => "先写失败测试",
        "GREEN" => "最小实现",
        "REFACTOR" => "保持测试绿色并重构",
        "VERIFY" => "完整验证",
        _ => unreachable!("任务阶段已由当前 schema 校验"),
    }
}

/// 当前任务阶段执行指令。
pub(crate) fn phase_instruction(phase: &str) -> &'static str {
    match phase {
        "RED" => "先写测试并观察其因目标行为缺失而预期失败。",
        "GREEN" => "编写最小实现使关联测试通过。",
        "REFACTOR" => "在重构过程中保持测试绿色。",
        "VERIFY" => "运行完整验证命令并确认全部通过。",
        _ => unreachable!("任务阶段已由当前 schema 校验"),
    }
}

/// 当前任务阶段完成标准。
pub(crate) fn done_criteria(phase: &str, scenarios: &[String]) -> Vec<String> {
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
        _ => unreachable!("任务阶段已由当前 schema 校验"),
    }
}
