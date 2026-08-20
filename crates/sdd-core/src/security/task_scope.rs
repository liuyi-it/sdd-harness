//! 任务文件范围校验（翻译自 早期 Node 实现）。
//!
//! 裁决顺序：路径规范化 → 禁止文件（E_SECURITY_BLOCKED）→ 允许文件集
//! （E_UNDECLARED_FILE_CHANGE）→ 期望新增文件核对。
//!
//! 禁止文件的简单文件名匹配大小写不敏感（.env 能命中 .Env/.ENV 变体）；
//! glob 编译失败直接返回错误——内部规则损坏属 bug，不再静默回退空匹配。

use std::borrow::Cow;

use regex::Regex;

use crate::error::SddError;

/// 校验实际变更是否落在任务允许范围内
pub fn validate_file_change(
    delta_paths: &[String],
    allowed_files: &[String],
    expected_new_files: &[String],
    forbidden_files: &[String],
) -> Result<(), SddError> {
    let forbidden_patterns = compile_patterns(forbidden_files, true)?;
    let allowed_patterns = compile_patterns(allowed_files, false)?;
    let expected_patterns = compile_patterns(expected_new_files, false)?;

    for path in delta_paths {
        // 禁止文件命中 → 安全阻断（大小写不敏感）
        if forbidden_patterns
            .iter()
            .any(|pattern| pattern.matches(path))
        {
            return Err(SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("变更文件 {path} 命中禁止范围"),
            ));
        }
        // 允许文件集为空 = 不允许任何变更
        if allowed_files.is_empty() {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!("任务不允许任何文件变更，发现 {path}"),
            ));
        }
        // 不在允许集 → 未声明变更
        if !allowed_patterns.iter().any(|pattern| pattern.matches(path)) {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!("变更文件 {path} 不在允许范围内"),
            ));
        }
    }
    for (expected, pattern) in expected_new_files.iter().zip(expected_patterns.iter()) {
        if !delta_paths.iter().any(|path| pattern.matches(path)) {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!("任务声明的预期新增文件未出现：{expected}"),
            ));
        }
    }
    Ok(())
}

fn compile_patterns(
    patterns: &[String],
    case_insensitive: bool,
) -> Result<Vec<PathPattern>, SddError> {
    patterns
        .iter()
        .map(|pattern| PathPattern::compile(pattern, case_insensitive))
        .collect()
}

enum PathPattern {
    Exact {
        path: String,
        case_insensitive: bool,
    },
    Directory {
        path: String,
        case_insensitive: bool,
    },
    Glob(Regex),
}

impl PathPattern {
    fn compile(pattern: &str, case_insensitive: bool) -> Result<Self, SddError> {
        let normalized = normalize_path(pattern);
        if let Some(directory) = normalized.strip_suffix("/**") {
            return Ok(Self::Directory {
                path: directory.trim_end_matches('/').to_string(),
                case_insensitive,
            });
        }
        if normalized.contains('*') {
            // 简单 glob：* 匹配单段；编译一次后服务本次任务内所有文件。
            return Ok(Self::Glob(glob_to_regex(&normalized, case_insensitive)?));
        }
        Ok(Self::Exact {
            path: normalized.into_owned(),
            case_insensitive,
        })
    }

    fn matches(&self, path: &str) -> bool {
        let path = normalize_path(path);
        match self {
            Self::Exact {
                path: expected,
                case_insensitive,
            } => {
                if *case_insensitive {
                    path.eq_ignore_ascii_case(expected.as_str())
                } else {
                    path == expected.as_str()
                }
            }
            Self::Directory {
                path: directory,
                case_insensitive,
            } => directory_matches(&path, directory, *case_insensitive),
            Self::Glob(regex) => regex.is_match(&path),
        }
    }
}

fn normalize_path(path: &str) -> Cow<'_, str> {
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
}

fn directory_matches(path: &str, directory: &str, case_insensitive: bool) -> bool {
    if !case_insensitive {
        return path == directory
            || path
                .strip_prefix(directory)
                .is_some_and(|remaining| remaining.starts_with('/'));
    }

    let path_bytes = path.as_bytes();
    let directory_bytes = directory.as_bytes();
    path_bytes.eq_ignore_ascii_case(directory_bytes)
        || (path_bytes.len() > directory_bytes.len()
            && path_bytes[directory_bytes.len()] == b'/'
            && path_bytes[..directory_bytes.len()].eq_ignore_ascii_case(directory_bytes))
}

fn glob_to_regex(glob: &str, case_insensitive: bool) -> Result<Regex, SddError> {
    let mut pattern = String::from("^");
    if case_insensitive {
        pattern.push_str("(?i)");
    }
    let mut chars = glob.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    pattern.push_str("(?:.*/)?");
                } else {
                    pattern.push_str(".*");
                }
            }
            '*' => pattern.push_str("[^/]*"),
            '.' | '+' | '(' | ')' | '^' | '$' | '|' | '[' | ']' | '{' | '}' | '?' | '\\' => {
                pattern.push('\\');
                pattern.push(ch);
            }
            '/' => pattern.push('/'),
            other => pattern.push(other),
        }
    }
    pattern.push('$');
    Regex::new(&pattern).map_err(|error| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("任务范围 glob 编译失败（内部规则损坏）：{glob}：{error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, path: &str) -> bool {
        PathPattern::compile(pattern, false).unwrap().matches(path)
    }

    fn matches_ci(pattern: &str, path: &str) -> bool {
        PathPattern::compile(pattern, true).unwrap().matches(path)
    }

    #[test]
    fn directory_pattern_matches_children() {
        assert!(matches("src/**", "src/lib.rs"));
        assert!(matches("src/**", "src"));
        assert!(!matches("src/**", "tests/lib.rs"));
    }

    #[test]
    fn exact_path_matches() {
        assert!(matches("src/lib.rs", "src/lib.rs"));
        assert!(!matches("src/lib.rs", "src/other.rs"));
    }

    #[test]
    fn double_star_matches_nested_paths() {
        assert!(matches("**/credentials*", "credentials.json"));
        assert!(matches("**/credentials*", "config/prod/credentials.yml"));
        assert!(!matches("**/credentials*", "config/prod/settings.yml"));
    }

    #[test]
    fn forbidden_simple_name_matches_case_insensitively() {
        for variant in [".env", ".Env", ".ENV"] {
            assert!(matches_ci(".env", variant), "{variant} 应命中禁止文件 .env");
        }
        // 允许集保持大小写敏感
        assert!(!matches("src/lib.rs", "SRC/LIB.RS"));
    }

    #[test]
    fn complex_glob_compiles_and_matches() {
        // glob 编译器转义所有正则元字符，非法规则极难构造；此处验证复杂规则
        // 能正常编译匹配（不再有静默回退 ^$ 的路径）
        assert!(matches("**/test-*.rs", "src/test-util.rs"));
        assert!(!matches("**/test-*.rs", "src/lib.rs"));
        // 含正则元字符的字面 glob 按字面匹配
        assert!(matches("a(b).rs", "a(b).rs"));
        assert!(!matches("a(b).rs", "ab.rs"));
    }
}
