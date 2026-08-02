//! OpenSpec markdown 渲染器（翻译自 `packages/core/src/engines/openspec/renderer.ts`）。

use super::model::{SpecDocument, SpecScenario};

/// 渲染 OpenSpec 文档；字段含 CR/LF/NUL 或 statement 以 #/- 开头时返回错误
pub fn render_spec(document: &SpecDocument) -> Result<String, String> {
    assert_document_render_safe(document)?;
    let mut lines: Vec<String> = vec![format!("# {}", document.title)];
    let mut prior_operation: Option<String> = None;

    for requirement in &document.requirements {
        if prior_operation.as_deref() != Some(requirement.operation.as_str()) {
            lines.push(String::new());
            lines.push(format!("## {} Requirements", requirement.operation));
            prior_operation = Some(requirement.operation.clone());
        }
        lines.push(String::new());
        lines.push(format!("### Requirement: {}", requirement.title));
        lines.push(requirement.statement.clone());
        for scenario in &requirement.scenarios {
            render_scenario(&mut lines, scenario);
        }
    }

    Ok(format!("{}\n", lines.join("\n")))
}

fn assert_document_render_safe(document: &SpecDocument) -> Result<(), String> {
    assert_render_safe(&document.title, "title")?;
    for (req_index, requirement) in document.requirements.iter().enumerate() {
        let path = format!("requirements[{req_index}]");
        assert_render_safe(&requirement.title, &format!("{path}.title"))?;
        assert_render_safe(&requirement.statement, &format!("{path}.statement"))?;
        assert_statement_render_safe(&requirement.statement, &format!("{path}.statement"))?;
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

fn assert_statement_render_safe(statement: &str, path: &str) -> Result<(), String> {
    let trimmed = statement.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('-') {
        return Err(format!(
            "OpenSpec 字段 {path} statement 不可注入 Markdown 结构"
        ));
    }
    Ok(())
}

fn assert_render_safe(value: &str, path: &str) -> Result<(), String> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(format!("OpenSpec 字段 {path} 不可包含 CR、LF 或 NUL"));
    }
    Ok(())
}

fn render_scenario(lines: &mut Vec<String>, scenario: &SpecScenario) {
    lines.push(String::new());
    lines.push(format!("#### Scenario: {}", scenario.title));
    for step in &scenario.given {
        lines.push(format!("- GIVEN {step}"));
    }
    for step in &scenario.when {
        lines.push(format!("- WHEN {step}"));
    }
    for step in &scenario.then {
        lines.push(format!("- THEN {step}"));
    }
}
