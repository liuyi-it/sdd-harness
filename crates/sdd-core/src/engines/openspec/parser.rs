//! OpenSpec markdown 解析器（翻译自 `packages/core/src/engines/openspec/parser.ts`）。

use regex::Regex;

use super::model::{SpecDocument, SpecRequirement, SpecScenario};

const DELTA_HEADING: &str = r"^## (ADDED|MODIFIED|REMOVED) Requirements$";
const REQUIREMENT_HEADING: &str = r"^### Requirement:(.*)$";
const SCENARIO_HEADING: &str = r"^#### Scenario:(.*)$";
const STEP: &str = r"^-\s+(GIVEN|WHEN|THEN)\s+(.+)$";

/// 解析 OpenSpec markdown；格式非法时返回错误（语义对齐 parser.ts 的 throw）
pub fn parse_spec(markdown: &str) -> Result<SpecDocument, String> {
    if markdown.contains('\0') {
        return Err("OpenSpec 文档不可包含 NUL".to_string());
    }
    let normalized = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let first_content = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .ok_or_else(|| "OpenSpec 文档缺少一级标题".to_string())?;

    let title_line = lines[first_content].trim();
    let title_match = Regex::new(r"^# ([^#].*)$")
        .map_err(|e| e.to_string())?
        .captures(title_line);
    let title = match title_match {
        Some(caps) => caps.get(1).unwrap().as_str().trim().to_string(),
        None => return Err(format!("第 {} 行必须是一级文档标题", first_content + 1)),
    };

    let delta_re = Regex::new(DELTA_HEADING).unwrap();
    let requirement_re = Regex::new(REQUIREMENT_HEADING).unwrap();
    let scenario_re = Regex::new(SCENARIO_HEADING).unwrap();
    let step_re = Regex::new(STEP).unwrap();

    let mut requirements: Vec<SpecRequirement> = Vec::new();
    let mut operation: Option<String> = None;
    let mut requirement: Option<SpecRequirement> = None;
    let mut scenario: Option<SpecScenario> = None;
    let mut statement_lines: Vec<String> = Vec::new();

    for (offset, raw_line) in lines.iter().enumerate().skip(first_content + 1) {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let line_no = offset + 1;

        if let Some(caps) = delta_re.captures(line) {
            if let Some(req) = requirement.take() {
                requirements.push(finish_requirement(req, &mut statement_lines));
            }
            scenario = None;
            operation = Some(caps.get(1).unwrap().as_str().to_string());
            continue;
        }

        if let Some(caps) = requirement_re.captures(line) {
            if operation.is_none() {
                return Err(format!("第 {line_no} 行的 Requirement 缺少 delta 标题"));
            }
            let req_title = caps.get(1).unwrap().as_str().trim().to_string();
            if req_title.is_empty() {
                return Err(format!("第 {line_no} 行的 Requirement 标题不能为空"));
            }
            if let Some(req) = requirement.take() {
                requirements.push(finish_requirement(req, &mut statement_lines));
            }
            scenario = None;
            let requirement_index = requirements.len() + 1;
            requirement = Some(SpecRequirement {
                id: format!("REQ-{}", pad(requirement_index)),
                title: req_title,
                statement: String::new(),
                operation: operation.clone().unwrap(),
                scenarios: Vec::new(),
            });
            continue;
        }

        if let Some(caps) = scenario_re.captures(line) {
            let req = requirement
                .as_mut()
                .ok_or_else(|| format!("第 {line_no} 行存在孤立 Scenario"))?;
            let scenario_title = caps.get(1).unwrap().as_str().trim().to_string();
            if scenario_title.is_empty() {
                return Err(format!("第 {line_no} 行的 Scenario 标题不能为空"));
            }
            scenario = Some(SpecScenario {
                id: format!("{}-SC-{}", req.id, pad(req.scenarios.len() + 1)),
                title: scenario_title,
                given: Vec::new(),
                when: Vec::new(),
                then: Vec::new(),
            });
            req.scenarios.push(scenario.clone().unwrap());
            continue;
        }

        if let Some(caps) = step_re.captures(line) {
            let scenario = scenario
                .as_mut()
                .ok_or_else(|| format!("第 {line_no} 行存在孤立场景步骤"))?;
            let keyword = caps.get(1).unwrap().as_str().to_lowercase();
            let step_text = caps.get(2).unwrap().as_str().trim().to_string();
            match keyword.as_str() {
                "given" => scenario.given.push(step_text),
                "when" => scenario.when.push(step_text),
                "then" => scenario.then.push(step_text),
                _ => unreachable!("STEP 正则只匹配 GIVEN/WHEN/THEN"),
            }
            continue;
        }

        if line.starts_with('#') {
            return Err(format!("第 {line_no} 行的标题层级或格式非法"));
        }
        if line.starts_with('-') {
            return Err(format!("第 {line_no} 行的场景步骤格式非法"));
        }
        if requirement.is_none() {
            return Err(format!("第 {line_no} 行的内容不属于任何 Requirement"));
        }
        if scenario.is_some() {
            return Err(format!("第 {line_no} 行的 Scenario 只允许包含场景步骤"));
        }
        statement_lines.push(line.to_string());
    }

    if let Some(req) = requirement.take() {
        requirements.push(finish_requirement(req, &mut statement_lines));
    }

    Ok(SpecDocument {
        title,
        requirements,
    })
}

fn finish_requirement(
    mut req: SpecRequirement,
    statement_lines: &mut Vec<String>,
) -> SpecRequirement {
    req.statement = statement_lines.join(" ").trim().to_string();
    statement_lines.clear();
    req
}

fn pad(value: usize) -> String {
    format!("{value:03}")
}
