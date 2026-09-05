//! 项目原生规格校验器。

use super::model::{SpecDocument, SpecValidationFailure, TechnicalDesign};

/// 校验规格模型，返回问题列表（空列表 = 通过）
pub fn validate_spec(document: &SpecDocument) -> Vec<SpecValidationFailure> {
    let mut failures: Vec<SpecValidationFailure> = Vec::new();
    let mut seen_ids: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    if document.requirements.is_empty() {
        failures.push(SpecValidationFailure {
            code: "SPEC_REQUIREMENT_REQUIRED".to_string(),
            path: "requirements".to_string(),
            message: "规格必须至少包含一个需求".to_string(),
        });
    }

    for (req_index, requirement) in document.requirements.iter().enumerate() {
        let req_path = format!("requirements[{req_index}]");
        if !valid_requirement_id(&requirement.id) {
            failures.push(SpecValidationFailure {
                code: "SPEC_REQUIREMENT_ID_INVALID".to_string(),
                path: format!("{req_path}.id"),
                message: format!("需求 ID {} 格式无效", requirement.id),
            });
        }
        if invalid_text(&requirement.title) || invalid_text(&requirement.statement) {
            failures.push(SpecValidationFailure {
                code: "SPEC_REQUIREMENT_INCOMPLETE".to_string(),
                path: req_path.clone(),
                message: format!("{} 的标题和需求描述必须是非空单行文本", requirement.id),
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
            if !valid_scenario_id(&requirement.id, &scenario.id) {
                failures.push(SpecValidationFailure {
                    code: "SPEC_SCENARIO_ID_INVALID".to_string(),
                    path: format!("{sc_path}.id"),
                    message: format!("场景 ID {} 与所属需求不一致", scenario.id),
                });
            }
            if invalid_text(&scenario.title)
                || invalid_steps(&scenario.given)
                || invalid_steps(&scenario.when)
                || invalid_steps(&scenario.then)
            {
                failures.push(SpecValidationFailure {
                    code: "SPEC_SCENARIO_REQUIRED".to_string(),
                    path: sc_path.clone(),
                    message: format!("场景 {} 必须有标题及非空的前提、操作和结果", scenario.id),
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

    validate_technical_design(&document.technical_design, &mut failures);

    failures
}

fn validate_technical_design(design: &TechnicalDesign, failures: &mut Vec<SpecValidationFailure>) {
    if invalid_text(&design.summary)
        || invalid_steps(&design.current_state)
        || design.decisions.is_empty()
        || invalid_steps(&design.affected_files)
        || invalid_steps(&design.interfaces)
        || invalid_steps(&design.error_handling)
        || invalid_steps(&design.test_strategy)
        || invalid_steps(&design.risks)
        || invalid_steps(&design.rollback)
        || design.decisions.iter().any(|decision| {
            invalid_text(&decision.title)
                || invalid_text(&decision.decision)
                || invalid_text(&decision.rationale)
        })
        || design
            .data_changes
            .iter()
            .any(|change| invalid_text(change))
    {
        failures.push(SpecValidationFailure {
            code: "SPEC_TECHNICAL_DESIGN_INCOMPLETE".to_string(),
            path: "technicalDesign".to_string(),
            message: "技术设计必须包含完整的事实、决策、接口、错误处理、测试、风险和回滚信息"
                .to_string(),
        });
    }
}

fn valid_requirement_id(id: &str) -> bool {
    id.strip_prefix("REQ-").is_some_and(|sequence| {
        sequence.len() == 3 && sequence.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn valid_scenario_id(requirement_id: &str, id: &str) -> bool {
    id.strip_prefix(&format!("{requirement_id}-SC-"))
        .is_some_and(|sequence| {
            sequence.len() == 3 && sequence.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn invalid_text(value: &str) -> bool {
    value.trim().is_empty() || value.contains(['\r', '\n', '\0'])
}

fn invalid_steps(steps: &[String]) -> bool {
    steps.is_empty() || steps.iter().any(|step| invalid_text(step))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::spec::{
        SpecRequirement, SpecScenario, TechnicalDesign, TechnicalDesignDecision,
    };

    #[test]
    fn rejects_scenario_id_from_another_requirement() {
        let document = SpecDocument {
            requirements: vec![SpecRequirement {
                id: "REQ-001".to_string(),
                title: "成功行为".to_string(),
                statement: "用户完成目标行为".to_string(),
                scenarios: vec![SpecScenario {
                    id: "REQ-002-SC-001".to_string(),
                    title: "行为成功".to_string(),
                    given: vec!["前置条件成立".to_string()],
                    when: vec!["用户执行操作".to_string()],
                    then: vec!["系统返回成功结果".to_string()],
                }],
            }],
            technical_design: TechnicalDesign {
                summary: "最小设计".to_string(),
                current_state: vec!["现有行为".to_string()],
                decisions: vec![TechnicalDesignDecision {
                    title: "决策".to_string(),
                    decision: "实施".to_string(),
                    rationale: "满足需求".to_string(),
                }],
                affected_files: vec!["README.md".to_string()],
                interfaces: vec!["接口".to_string()],
                data_changes: vec![],
                error_handling: vec!["失败上抛".to_string()],
                test_strategy: vec!["运行测试".to_string()],
                risks: vec!["风险".to_string()],
                rollback: vec!["回退".to_string()],
            },
        };

        assert!(validate_spec(&document)
            .iter()
            .any(|failure| failure.code == "SPEC_SCENARIO_ID_INVALID"));
    }
}
