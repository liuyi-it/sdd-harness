//! 任务文件范围校验（翻译自 早期 Node 实现）。
//!
//! 裁决顺序：路径规范化 → 禁止文件（E_SECURITY_BLOCKED）→ 允许文件集
//! （E_UNDECLARED_FILE_CHANGE）→ 期望新增文件核对。

use crate::error::SddError;

/// 校验实际变更是否落在任务允许范围内
pub fn validate_file_change(
    delta_paths: &[String],
    allowed_files: &[String],
    expected_new_files: &[String],
    forbidden_files: &[String],
) -> Result<(), SddError> {
    for path in delta_paths {
        // 禁止文件命中 → 安全阻断
        if forbidden_files
            .iter()
            .any(|pattern| pattern_matches(pattern, path))
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
        if !allowed_files
            .iter()
            .any(|pattern| pattern_matches(pattern, path))
        {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!("变更文件 {path} 不在允许范围内"),
            ));
        }
    }
    for expected in expected_new_files {
        if !delta_paths
            .iter()
            .any(|path| pattern_matches(expected, path))
        {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!("任务声明的预期新增文件未出现：{expected}"),
            ));
        }
    }
    Ok(())
}

/// 模式匹配：支持 `/**` 结尾的目录模式与精确路径
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let normalized = pattern.replace('\\', "/");
    let path = path.replace('\\', "/");
    if let Some(dir) = normalized.strip_suffix("/**") {
        let dir = dir.trim_end_matches('/');
        return path == dir || path.starts_with(&format!("{dir}/"));
    }
    if normalized.contains('*') {
        // 简单 glob：* 匹配单段
        let re = glob_to_regex(&normalized);
        return re.is_match(&path);
    }
    path == normalized
}

fn glob_to_regex(glob: &str) -> regex::Regex {
    let mut pattern = String::from("^");
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
    regex::Regex::new(&pattern).unwrap_or_else(|_| regex::Regex::new("^$").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_pattern_matches_children() {
        assert!(pattern_matches("src/**", "src/lib.rs"));
        assert!(pattern_matches("src/**", "src"));
        assert!(!pattern_matches("src/**", "tests/lib.rs"));
    }

    #[test]
    fn exact_path_matches() {
        assert!(pattern_matches("src/lib.rs", "src/lib.rs"));
        assert!(!pattern_matches("src/lib.rs", "src/other.rs"));
    }

    #[test]
    fn double_star_matches_nested_paths() {
        assert!(pattern_matches("**/credentials*", "credentials.json"));
        assert!(pattern_matches(
            "**/credentials*",
            "config/prod/credentials.yml"
        ));
        assert!(!pattern_matches(
            "**/credentials*",
            "config/prod/settings.yml"
        ));
    }
}
