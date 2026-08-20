//! 敏感信息扫描（翻译自 早期 Node 实现）。
//!
//! 检测密钥材料与凭据模式；命中时以 E_SECURITY_BLOCKED 阻断审查/提交。
//! 正则经 OnceLock 缓存，避免每次扫描重复编译。

use regex::Regex;
use std::sync::OnceLock;

/// generic-secret 的引号包裹形态（值 ≥ 8 字符）
const GENERIC_SECRET_QUOTED: &str =
    r#"(?i)(?:password|passwd|secret|token)\s*[:=]\s*['"][^'"]{8,}['"]"#;
/// generic-secret 的无引号形态（值 8-64 字符）；按行扫描并排除占位词所在行
const GENERIC_SECRET_UNQUOTED: &str =
    r#"(?i)(?:password|passwd|secret|token)\s*[:=]\s*[A-Za-z0-9_\-./+=]{8,64}\b"#;

/// 敏感模式集合（覆盖常见密钥与凭据格式）；正则只编译一次并缓存。
fn secrets_patterns() -> &'static [(&'static str, Regex)] {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            vec![
                (
                    "private-key",
                    Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH |)PRIVATE KEY-----").unwrap(),
                ),
                (
                    "aws-access-key",
                    Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),
                ),
                // 键+值匹配：单独出现变量名（如注释/文档提到 aws_secret_access_key）不再误报
                (
                    "aws-secret",
                    Regex::new(r#"(?i)aws_secret_access_key\s*[:=]\s*['"]?[A-Za-z0-9/+=]{16,}"#)
                        .unwrap(),
                ),
                (
                    "github-token",
                    Regex::new(r"(?i)gh[pousr]_[A-Za-z0-9]{20,}").unwrap(),
                ),
                (
                    "github-pat",
                    Regex::new(r"github_pat_[A-Za-z0-9_]{20,}").unwrap(),
                ),
                (
                    "slack-token",
                    Regex::new(r"xox[baprs]-[A-Za-z0-9-]{10,}").unwrap(),
                ),
                (
                    "google-api-key",
                    Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap(),
                ),
                // JWT：eyJ 开头的三段 base64url
                (
                    "jwt",
                    Regex::new(
                        r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
                    )
                    .unwrap(),
                ),
                // Authorization 头：Bearer/Basic 凭证
                (
                    "authorization-header",
                    Regex::new(r"(?i)authorization\s*[:=]\s*(bearer|basic)\s+\S{8,}").unwrap(),
                ),
                (
                    "connection-string",
                    Regex::new(r"(?i)(?:mongodb|postgres(?:ql)?|mysql|redis)://[^\s@]+:[^\s@]+@")
                        .unwrap(),
                ),
            ]
        })
        .as_slice()
}

/// 扫描内容中的敏感模式，返回命中列表
pub fn scan_secrets(content: &str) -> Vec<(String, String)> {
    let mut hits = Vec::new();
    for (name, re) in secrets_patterns() {
        if re.is_match(content) {
            hits.push((name.to_string(), re.to_string()));
        }
    }
    // generic-secret 引号包裹形态（全内容匹配）
    static QUOTED: OnceLock<Regex> = OnceLock::new();
    let quoted = QUOTED.get_or_init(|| Regex::new(GENERIC_SECRET_QUOTED).unwrap());
    if quoted.is_match(content) {
        hits.push(("generic-secret".to_string(), quoted.to_string()));
    }
    // generic-secret 无引号值分支：逐行检查并跳过占位词（示例/文档不误报）
    static UNQUOTED: OnceLock<Regex> = OnceLock::new();
    let unquoted = UNQUOTED.get_or_init(|| Regex::new(GENERIC_SECRET_UNQUOTED).unwrap());
    for line in content.lines() {
        if line_contains_placeholder(line) {
            continue;
        }
        if unquoted.is_match(line) {
            hits.push(("generic-secret".to_string(), unquoted.to_string()));
            break;
        }
    }
    hits
}

/// 占位词所在行视为示例而非真实密钥
fn line_contains_placeholder(line: &str) -> bool {
    const PLACEHOLDERS: &[&str] = &[
        "example",
        "xxx",
        "your",
        "placeholder",
        "sample",
        "dummy",
        "changeme",
    ];
    PLACEHOLDERS
        .iter()
        .any(|word| contains_ignore_ascii_case(line, word))
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(needle.as_bytes()))
}

/// 校验变更文件是否含敏感信息；命中返回 E_SECURITY_BLOCKED
pub fn validate_no_secrets<'a>(
    files_with_content: impl IntoIterator<Item = (&'a str, &'a str)>,
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

#[cfg(test)]
mod tests {
    use super::{line_contains_placeholder, scan_secrets};

    #[test]
    fn jwt_and_authorization_are_detected() {
        let jwt = "token=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let hits = scan_secrets(jwt);
        assert!(hits.iter().any(|(name, _)| name == "jwt"));
        let auth = "Authorization: Bearer abcdefgh12345678";
        let hits = scan_secrets(auth);
        assert!(hits.iter().any(|(name, _)| name == "authorization-header"));
    }

    #[test]
    fn github_pat_is_detected() {
        let hits = scan_secrets("github_pat_11ABCDEFGHIJKLMNOPQRST_abcdefghijklmnopqrstuvwxyz");
        assert!(hits.iter().any(|(name, _)| name == "github-pat"));
    }

    #[test]
    fn aws_secret_requires_key_value_pair() {
        // 变量名本身（无值）不再误报
        let hits = scan_secrets("fn check(aws_secret_access_key: &str) {}");
        assert!(!hits.iter().any(|(name, _)| name == "aws-secret"));
        // 键+值命中
        let hits =
            scan_secrets("aws_secret_access_key = \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\"");
        assert!(hits.iter().any(|(name, _)| name == "aws-secret"));
    }

    #[test]
    fn generic_secret_unquoted_skips_placeholder_lines() {
        // 占位词行不命中
        let hits = scan_secrets("password=your_password_here");
        assert!(!hits.iter().any(|(name, _)| name == "generic-secret"));
        // 真实无引号值命中
        let hits = scan_secrets("password=hunter2secretvalue");
        assert!(hits.iter().any(|(name, _)| name == "generic-secret"));
        // 引号包裹值仍命中
        let hits = scan_secrets("token = \"s3cr3t-t0ken-value\"");
        assert!(hits.iter().any(|(name, _)| name == "generic-secret"));
    }

    #[test]
    fn placeholder_marker_lines_are_excluded() {
        assert!(line_contains_placeholder("password = example12345"));
        assert!(line_contains_placeholder("TOKEN=YOUR_TOKEN_HERE"));
        assert!(!line_contains_placeholder("password=realvalue9x"));
    }

    #[test]
    fn clean_content_has_no_hits() {
        let clean = scan_secrets("fn main() { println!(\"hi\"); }");
        assert!(clean.is_empty());
    }
}
