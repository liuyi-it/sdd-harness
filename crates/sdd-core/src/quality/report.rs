//! 报告模型：verify/review 报告（对应 report.schema.json）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub kind: String, // verify | review
    pub summary: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimality: Option<serde_json::Value>,
}

impl Report {
    pub fn new(kind: &str, change_id: Option<String>) -> Self {
        Self {
            kind: kind.to_string(),
            summary: String::new(),
            passed: false,
            change_id,
            issues: Vec::new(),
            minimality: None,
        }
    }
}

/// 生成供人阅读的报告 Markdown；机器字段仍以 Value 存在 runtime.json。
pub fn render_report_markdown(report: &Report) -> String {
    let mut lines = vec![
        format!("# {} 报告", report.kind),
        String::new(),
        format!("- 结果：{}", if report.passed { "通过" } else { "未通过" }),
        format!("- 摘要：{}", report.summary),
        String::new(),
        "## 问题".to_string(),
        String::new(),
    ];
    if report.issues.is_empty() {
        lines.push("- 无问题。".to_string());
    } else {
        for issue in &report.issues {
            let file = issue
                .file
                .as_deref()
                .map(|path| match (issue.start_line, issue.end_line) {
                    (Some(start), Some(end)) => format!("（{path}:{start}-{end}）"),
                    (Some(start), None) => format!("（{path}:{start}）"),
                    _ => format!("（{path}）"),
                });
            let metadata = [
                issue
                    .origin
                    .as_deref()
                    .map(|origin| format!("origin={origin}")),
                issue
                    .category
                    .as_deref()
                    .map(|category| format!("category={category}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            let metadata = if metadata.is_empty() {
                String::new()
            } else {
                format!(" [{}]", metadata.join("，"))
            };
            lines.push(format!(
                "- [{}] {}：{}{}{}",
                issue.severity,
                issue.code,
                issue.message,
                file.unwrap_or_default(),
                metadata
            ));
        }
    }
    if let Some(minimality) = &report.minimality {
        lines.extend([
            String::new(),
            "## 最小性信息".to_string(),
            String::new(),
            format!(
                "```json\n{}\n```",
                serde_json::to_string_pretty(minimality).unwrap_or_default()
            ),
        ]);
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::Issue;

    #[test]
    fn issue_serializes_ocr_location_and_suggestion_fields() {
        let issue = Issue {
            code: "OCR_FINDING".into(),
            severity: "high".into(),
            message: "输入未校验".into(),
            file: Some("src/handler.rs".into()),
            category: Some("security".into()),
            start_line: Some(42),
            end_line: Some(43),
            existing_code: Some("old".into()),
            suggestion_code: Some("new".into()),
            origin: Some("ocr".into()),
        };
        let value = serde_json::to_value(&issue).unwrap();
        assert_eq!(value["startLine"], 42);
        assert_eq!(value["endLine"], 43);
        assert_eq!(value["suggestionCode"], "new");
        assert_eq!(value["existingCode"], "old");
        assert_eq!(value["category"], "security");
        assert_eq!(value["origin"], "ocr");
        assert!(value.get("start_line").is_none());
        assert!(value.get("end_line").is_none());
        assert!(value.get("suggestion_code").is_none());
    }

    #[test]
    fn report_with_ocr_fields_passes_schema_validation() {
        let mut report = super::Report::new("review", Some("change-1".to_string()));
        report.summary = "发现 1 个审查发现".to_string();
        report.passed = false;
        report.issues = vec![Issue {
            code: "OCR_FINDING".into(),
            severity: "high".into(),
            message: "输入未校验".into(),
            file: Some("src/handler.rs".into()),
            category: Some("security".into()),
            start_line: Some(42),
            end_line: Some(43),
            existing_code: Some("old".into()),
            suggestion_code: Some("new".into()),
            origin: Some("ocr".into()),
        }];
        let value = serde_json::to_value(&report).unwrap();
        assert!(crate::schema::validate_json("report", &value).is_ok());
    }

    #[test]
    fn issue_omits_optional_fields_when_none() {
        let issue = Issue {
            code: "W_CHANGE_SIZE".into(),
            severity: "low".into(),
            message: "检测到 1 个变更文件".into(),
            file: None,
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        };
        let value = serde_json::to_value(&issue).unwrap();
        assert_eq!(value["code"], "W_CHANGE_SIZE");
        assert!(value.get("category").is_none());
        assert!(value.get("startLine").is_none());
        assert!(value.get("endLine").is_none());
        assert!(value.get("existingCode").is_none());
        assert!(value.get("suggestionCode").is_none());
        assert!(value.get("origin").is_none());
    }
    #[test]
    fn renders_ocr_location_and_metadata_in_same_finding() {
        let mut report = super::Report::new("review", Some("change-1".to_string()));
        report.passed = false;
        report.issues.push(Issue {
            code: "OCR_FINDING".into(),
            severity: "high".into(),
            message: "输入未校验".into(),
            file: Some("src/handler.rs".into()),
            category: Some("security".into()),
            start_line: Some(42),
            end_line: Some(43),
            existing_code: None,
            suggestion_code: Some("return Err(err);".into()),
            origin: Some("ocr".into()),
        });
        let markdown = super::render_report_markdown(&report);
        assert!(markdown.contains("src/handler.rs:42-43"));
        assert!(markdown.contains("origin=ocr"));
        assert!(markdown.contains("category=security"));
    }
}
