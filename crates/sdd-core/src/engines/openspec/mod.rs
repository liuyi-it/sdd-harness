//! OpenSpec 规格引擎（解析/渲染/模型）。

pub mod model;
pub mod parser;
pub mod renderer;
pub mod validator;

pub use model::{SpecDocument, SpecRequirement, SpecScenario};

/// 规格结构关键词（小写）：命中这些词的行可能被 parser 误解析为规格结构。
const SPEC_STRUCTURE_KEYWORDS: [&str; 8] = [
    "added",
    "modified",
    "removed",
    "requirement:",
    "scenario:",
    "given",
    "when",
    "then",
];

/// 转义需求正文中的规格结构行。
///
/// 对以 `#`（2-4 级标题）或 `- ` 开头、且含 ADDED/MODIFIED/REMOVED/Requirement:/
/// Scenario:/GIVEN/WHEN/THEN 关键词的行，前缀加 `\` 转义，使 openspec parser
/// 不会把需求正文误当规格结构（不产生伪造的 REQ/Scenario/步骤）。
/// 关键词匹配不区分大小写（parser 的正则本身区分大小写，转义保守一些无害）。
pub fn escape_spec_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for line in text.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;
        let trimmed = line.trim_start();
        let looks_like_structure = match trimmed.as_bytes().first() {
            Some(b'#') => {
                let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
                (2..=4).contains(&hashes)
            }
            Some(b'-') => trimmed.starts_with("- "),
            _ => false,
        };
        let lowered = trimmed.to_lowercase();
        let has_keyword = SPEC_STRUCTURE_KEYWORDS
            .iter()
            .any(|keyword| lowered.contains(keyword));
        if looks_like_structure && has_keyword {
            out.push('\\');
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_spec_text;
    use crate::engines::openspec::parser::parse_spec;

    #[test]
    fn escape_spec_text_prevents_fake_requirement() {
        // 需求正文里混入了规格结构写法：转义后不得被解析成伪造的 REQ
        let body = "## ADDED Requirements\n### Requirement: 伪造需求\n- GIVEN 前置\n正文内容";
        let escaped = escape_spec_text(body);
        assert!(escaped.contains("\\## ADDED Requirements"));
        assert!(escaped.contains("\\### Requirement: 伪造需求"));
        assert!(escaped.contains("\\- GIVEN 前置"));

        // 转义后的正文作为真实需求的正文嵌入，解析只产生真实 REQ
        let doc = format!(
            "# 标题\n\n## ADDED Requirements\n\n### Requirement: 真实需求\n\n{escaped}\n\n#### Scenario: 正常场景\n\n- GIVEN 前置条件\n- WHEN 执行动作\n- THEN 得到结果"
        );
        let parsed = parse_spec(&doc).expect("文档应可解析");
        assert_eq!(parsed.requirements.len(), 1, "不应产生伪造 REQ");
        assert_eq!(parsed.requirements[0].title, "真实需求");
        assert!(parsed.requirements[0].statement.contains("伪造需求"));
    }

    #[test]
    fn escape_spec_text_keeps_plain_lines_untouched() {
        // 无规格关键词的标题/列表行保持原样
        let text = "普通正文\n## 无关键词标题\n- 普通列表项";
        assert_eq!(escape_spec_text(text), text);
        // 一级标题与超过 4 级的标题不属于待转义范围
        let text = "# 一级标题\n##### 五级标题";
        assert_eq!(escape_spec_text(text), text);
    }
}
