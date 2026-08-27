//! 项目原生规格 Markdown 渲染器。

use super::model::{SpecDocument, SpecScenario};

/// 渲染供人工审核的验收标准。
pub fn render_spec(document: &SpecDocument) -> Result<String, String> {
    assert_document_render_safe(document)?;
    let mut lines = Vec::new();

    for requirement in &document.requirements {
        lines.push(format!("### {}：{}", requirement.id, requirement.title));
        lines.push(String::new());
        lines.push(format!("- 需求：{}", requirement.statement));
        for scenario in &requirement.scenarios {
            render_scenario(&mut lines, scenario);
        }
        lines.push(String::new());
    }

    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    Ok(lines.join("\n") + "\n")
}

fn assert_document_render_safe(document: &SpecDocument) -> Result<(), String> {
    for (req_index, requirement) in document.requirements.iter().enumerate() {
        let path = format!("requirements[{req_index}]");
        assert_render_safe(&requirement.title, &format!("{path}.title"))?;
        assert_render_safe(&requirement.statement, &format!("{path}.statement"))?;
        for (sc_index, scenario) in requirement.scenarios.iter().enumerate() {
            let sc_path = format!("{path}.scenarios[{sc_index}]");
            assert_render_safe(&scenario.title, &format!("{sc_path}.title"))?;
            for (step_key, steps) in [
                ("given", &scenario.given),
                ("when", &scenario.when),
                ("then", &scenario.then),
            ] {
                for (step_index, step) in steps.iter().enumerate() {
                    assert_render_safe(step, &format!("{sc_path}.{step_key}[{step_index}]"))?;
                }
            }
        }
    }
    Ok(())
}

fn assert_render_safe(value: &str, path: &str) -> Result<(), String> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(format!("规格字段 {path} 不可包含 CR、LF 或 NUL"));
    }
    Ok(())
}

fn render_scenario(lines: &mut Vec<String>, scenario: &SpecScenario) {
    lines.push(String::new());
    lines.push(format!("#### {}：{}", scenario.id, scenario.title));
    for step in &scenario.given {
        lines.push(format!("- 前提：{step}"));
    }
    for step in &scenario.when {
        lines.push(format!("- 操作：{step}"));
    }
    for step in &scenario.then {
        lines.push(format!("- 结果：{step}"));
    }
}
