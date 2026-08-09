//! 敏感信息扫描（翻译自 早期 Node 实现）。
//!
//! 检测密钥材料与凭据模式；命中时以 E_SECURITY_BLOCKED 阻断审查/提交。

use regex::Regex;

/// 敏感模式集合（覆盖常见密钥与凭据格式）
pub fn secrets_patterns() -> Vec<(&'static str, Regex)> {
    vec![
        (
            "private-key",
            Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH |)PRIVATE KEY-----").unwrap(),
        ),
        (
            "aws-access-key",
            Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),
        ),
        (
            "aws-secret",
            Regex::new(r"(?i)aws_secret_access_key\s*[:=]").unwrap(),
        ),
        (
            "github-token",
            Regex::new(r"(?i)gh[pousr]_[A-Za-z0-9]{20,}").unwrap(),
        ),
        (
            "slack-token",
            Regex::new(r"xox[baprs]-[A-Za-z0-9-]{10,}").unwrap(),
        ),
        (
            "google-api-key",
            Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap(),
        ),
        (
            "generic-secret",
            Regex::new(r#"(?i)(?:password|passwd|secret|token)\s*[:=]\s*['"][^'"]{8,}['"]"#)
                .unwrap(),
        ),
        (
            "connection-string",
            Regex::new(r"(?i)(?:mongodb|postgres(?:ql)?|mysql|redis)://[^\s@]+:[^\s@]+@").unwrap(),
        ),
    ]
}

/// 扫描内容中的敏感模式，返回命中列表
pub fn scan_secrets(content: &str) -> Vec<(String, String)> {
    let mut hits = Vec::new();
    for (name, re) in secrets_patterns() {
        if re.is_match(content) {
            hits.push((name.to_string(), re.to_string()));
        }
    }
    hits
}

/// 校验变更文件是否含敏感信息；命中返回 E_SECURITY_BLOCKED
pub fn validate_no_secrets(
    files_with_content: &[(String, String)],
) -> Result<(), crate::error::SddError> {
    for (file, content) in files_with_content {
        let hits = scan_secrets(content);
        if !hits.is_empty() {
            let names: Vec<String> = hits.iter().map(|(n, _)| n.clone()).collect();
            return Err(crate::error::SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("文件 {file} 包含敏感信息（{}）", names.join("、")),
            ));
        }
    }
    Ok(())
}
