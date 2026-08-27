//! 可追溯性矩阵：Requirement/Scenario → 任务 → 证据覆盖。
//!
//! 规格、设计、任务和验证证据的可追踪性检查：
//! verify 检查规格中的每个 Requirement/Scenario 都有对应任务与完成证据。

use crate::engines::spec::SpecDocument;
use crate::engines::tdd::TaskDefinition;

/// 检查任务覆盖是否完整；返回缺失列表
pub fn coverage_gaps(
    requirement_ids: &[String],
    scenario_ids: &[String],
    tasks: &[TaskDefinition],
    done_task_ids: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut gaps = Vec::new();
    // 每个 Requirement 至少一个任务
    for req_id in requirement_ids {
        if !tasks.iter().any(|t| t.requirements.contains(req_id)) {
            gaps.push(format!("需求 {req_id} 没有对应任务"));
        }
    }
    // 每个 Scenario 至少在一个 DONE 任务中覆盖
    for scenario_id in scenario_ids {
        let covered = tasks
            .iter()
            .any(|t| t.scenarios.contains(scenario_id) && done_task_ids.contains(&t.id));
        if !covered {
            gaps.push(format!("Scenario {scenario_id} 没有被已完成任务覆盖"));
        }
    }
    gaps
}

/// 从权威规格模型提取需求与场景 ID。
pub fn extract_spec_ids(specification: &SpecDocument) -> (Vec<String>, Vec<String>) {
    let requirements = specification
        .requirements
        .iter()
        .map(|requirement| requirement.id.clone())
        .collect();
    let scenarios = specification
        .requirements
        .iter()
        .flat_map(|requirement| requirement.scenarios.iter())
        .map(|scenario| scenario.id.clone())
        .collect();
    (requirements, scenarios)
}
