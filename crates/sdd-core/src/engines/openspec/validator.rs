//! OpenSpec 规格校验器（翻译自 早期 Node 实现）。

use super::model::{SpecDocument, SpecValidationFailure};

/// 校验规格模型，返回问题列表（空列表 = 通过）
pub fn validate_spec(document: &SpecDocument) -> Vec<SpecValidationFailure> {
    let mut failures: Vec<SpecValidationFailure> = Vec::new();
    let mut seen_ids: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for (req_index, requirement) in document.requirements.iter().enumerate() {
        let req_path = format!("requirements[{req_index}]");
        // 规范性关键词：statement 必须包含 SHALL
        if !requirement.statement.to_uppercase().contains("SHALL") {
            failures.push(SpecValidationFailure {
                code: "SPEC_NORMATIVE_KEYWORD_REQUIRED".to_string(),
                path: format!("{req_path}.statement"),
                message: format!("{} 的 statement 必须包含规范性关键词 SHALL", requirement.id),
            });
        }
        // 重复 id
        if let Some(prev) = seen_ids.insert(requirement.id.clone(), req_path.clone()) {
            failures.push(SpecValidationFailure {
                code: "SPEC_DUPLICATE_ID".to_string(),
                path: req_path.clone(),
                message: format!("需求 id {} 重复（首次出现于 {prev}）", requirement.id),
            });
        }
        // 每个需求必须有 Scenario
        if requirement.scenarios.is_empty() {
            failures.push(SpecValidationFailure {
                code: "SPEC_SCENARIO_REQUIRED".to_string(),
                path: format!("{req_path}.scenarios"),
                message: format!("需求 {} 必须至少包含一个 Scenario", requirement.id),
            });
        }
        for (sc_index, scenario) in requirement.scenarios.iter().enumerate() {
            let sc_path = format!("{req_path}.scenarios[{sc_index}]");
            if scenario.given.is_empty() || scenario.when.is_empty() || scenario.then.is_empty() {
                failures.push(SpecValidationFailure {
                    code: "SPEC_SCENARIO_REQUIRED".to_string(),
                    path: sc_path.clone(),
                    message: format!(
                        "Scenario {} 必须包含 GIVEN/WHEN/THEN 各至少一步",
                        scenario.id
                    ),
                });
            }
            // 场景 id 唯一性
            if let Some(prev) = seen_ids.insert(scenario.id.clone(), sc_path.clone()) {
                failures.push(SpecValidationFailure {
                    code: "SPEC_DUPLICATE_ID".to_string(),
                    path: sc_path.clone(),
                    message: format!("场景 id {} 重复（首次出现于 {prev}）", scenario.id),
                });
            }
        }
    }

    // delta 冲突：同一需求 id 出现多个 operation（ADDED 后又 MODIFIED）视为冲突
    let mut ops_by_id: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for requirement in &document.requirements {
        if let Some(prev_op) = ops_by_id.insert(&requirement.id, &requirement.operation) {
            if prev_op != requirement.operation {
                failures.push(SpecValidationFailure {
                    code: "SPEC_DELTA_CONFLICT".to_string(),
                    path: format!("requirements[{}].operation", requirement.id),
                    message: format!(
                        "需求 {} 的 delta 操作冲突：{} 与 {}",
                        requirement.id, prev_op, requirement.operation
                    ),
                });
            }
        }
    }

    failures
}
