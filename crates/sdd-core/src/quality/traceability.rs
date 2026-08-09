//! 可追溯性矩阵：Requirement/Scenario → 任务 → 证据覆盖。
//!
//! 翻译自 早期 Node 实现：
//! verify 检查规格中的每个 Requirement/Scenario 都有对应任务与完成证据。

use crate::engines::superpowers::protocol::TaskDefinition;
use crate::error::SddError;

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

/// 从 spec.json 提取需求与场景 id
pub fn extract_spec_ids(spec_json: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let mut requirements = Vec::new();
    let mut scenarios = Vec::new();
    if let Some(model) = spec_json.get("model").and_then(|m| m.as_object()) {
        if let Some(reqs) = model.get("requirements").and_then(|r| r.as_array()) {
            for req in reqs {
                if let Some(id) = req.get("id").and_then(|v| v.as_str()) {
                    requirements.push(id.to_string());
                }
                if let Some(scs) = req.get("scenarios").and_then(|s| s.as_array()) {
                    for sc in scs {
                        if let Some(sc_id) = sc.get("id").and_then(|v| v.as_str()) {
                            scenarios.push(sc_id.to_string());
                        }
                    }
                }
            }
        }
    }
    (requirements, scenarios)
}

/// 转换为标准错误（供 verify 命令使用）
pub fn to_verify_error(gaps: &[String]) -> SddError {
    SddError::new(
        "E_VERIFY_REQUIRED",
        &format!("规格覆盖不完整：{}", gaps.join("；")),
    )
    .with_next("sdd build next")
}
