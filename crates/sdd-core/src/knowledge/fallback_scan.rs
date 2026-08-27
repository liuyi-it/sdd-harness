//! 受限文件扫描降级：CodeGraph 不可用时使用。
//!
//! 执行目录遍历、排除目录、密钥文件跳过、关键字扫描与候选文件摘要。
//! 结果显式标记 degraded=true，不被静默隐藏。

use serde_json::json;

use super::provider::{KnowledgeIntent, QueryResult};

const EXCLUDED_DIRECTORIES: [&str; 8] = [
    ".git",
    ".sdd",
    "node_modules",
    "target",
    "build",
    "dist",
    "coverage",
    "logs",
];

const FILE_LIMIT: usize = 2_000;
const ENTRY_LIMIT: usize = 10_000;
const ISSUE_LIMIT: usize = 10;

/// 密钥文件名直接跳过。
fn is_secret_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "id_rsa"
        || lower == "id_ed25519"
        || lower == "kubeconfig"
        || lower == "application-prod.yml"
        || lower == "application-prod.yaml"
        || lower == "application-prod.properties"
        || [".pem", ".key", ".p12", ".jks"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

/// 降级文件扫描。调用方必须传入触发降级的真实原因；遍历不完整时会把范围限制和
/// 文件系统错误一并暴露在 reason 与 payload.scan 中。
pub fn fallback_scan(
    root: &str,
    intent: KnowledgeIntent,
    query: &str,
    degradation_reason: &str,
) -> QueryResult {
    let scan = scan_files(root, FILE_LIMIT, ENTRY_LIMIT);
    let matches = matching_files(&scan.files, query);
    let visible_files = if query.trim().is_empty() || matches.is_empty() {
        scan.files.as_slice()
    } else {
        matches.as_slice()
    };
    let scan_complete = !scan.truncated && scan.issue_count == 0;
    let scan_metadata = json!({
        "complete": scan_complete,
        "fileLimit": FILE_LIMIT,
        "entryLimit": ENTRY_LIMIT,
        "visitedEntries": scan.visited_entries,
        "truncated": scan.truncated,
        "issueCount": scan.issue_count,
        "issues": scan.issues,
    });
    let payload = match intent {
        KnowledgeIntent::Impact => json!({
            "intent": "impact",
            "files": visible_files,
            "symbols": [],
            "tests": [],
            "risks": [],
            "codebaseSummary": summary_text(visible_files),
            "scan": scan_metadata,
        }),
        _ => json!({
            "intent": intent.as_str(),
            "codebaseSummary": summary_text(visible_files),
            "packageStructure": package_structure(visible_files),
            "architecture": architecture_text(visible_files),
            "scan": scan_metadata,
        }),
    };
    let mut reasons = vec![degradation_reason.trim().to_string()];
    if scan.file_limit_reached {
        reasons.push(format!("文件扫描达到 {FILE_LIMIT} 个文件上限"));
    }
    if scan.entry_limit_reached {
        reasons.push(format!("文件扫描达到 {ENTRY_LIMIT} 个目录条目上限"));
    }
    if scan.issue_count > 0 {
        reasons.push(format!("文件扫描遇到 {} 个读取错误", scan.issue_count));
    }
    QueryResult {
        provider: "fallback-file-scan",
        degraded: true,
        confidence: 0.3,
        reason: Some(reasons.join("；")),
        payload,
    }
}

#[derive(Debug)]
struct ScanOutcome {
    files: Vec<String>,
    issues: Vec<String>,
    issue_count: usize,
    truncated: bool,
    file_limit_reached: bool,
    entry_limit_reached: bool,
    visited_entries: usize,
}

impl ScanOutcome {
    fn record_issue(&mut self, issue: String) {
        self.issue_count += 1;
        if self.issues.len() < ISSUE_LIMIT {
            self.issues.push(issue);
        }
    }
}

fn matching_files(files: &[String], query: &str) -> Vec<String> {
    let terms: std::collections::HashSet<String> = query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| !term.is_empty())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut best_score = 0;
    let mut matches = Vec::new();
    for file in files {
        let lower = file.to_lowercase();
        let score = terms.iter().filter(|term| lower.contains(*term)).count();
        if score > best_score {
            best_score = score;
            matches.clear();
            matches.push(file.clone());
        } else if score > 0 && score == best_score {
            matches.push(file.clone());
        }
    }
    matches
}

fn summary_text(files: &[String]) -> String {
    let listed: Vec<String> = files.iter().take(200).map(|f| format!("- {f}")).collect();
    let omitted = if files.len() > 200 {
        format!("\n- ……其余 {} 个文件已省略", files.len() - 200)
    } else {
        String::new()
    };
    format!(
        "# 代码库摘要\n\n当前使用 fallback-file-scan 降级模式，以下结果仅来自受限文件扫描。\n\n## 文件名搜索\n\n{}",
        listed.join("\n") + &omitted
    )
}

fn package_structure(files: &[String]) -> String {
    let dirs: std::collections::BTreeSet<String> = files
        .iter()
        .filter_map(|f| f.rsplit_once('/').map(|(d, _)| d.to_string()))
        .filter(|d| !d.is_empty())
        .collect();
    format!(
        "# 包结构\n\n{}",
        dirs.iter()
            .map(|d| format!("- {d}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn architecture_text(files: &[String]) -> String {
    format!(
        "# 架构\n\n由受限的降级文件扫描生成；符号与调用关系不可用。\n\n发现文件数：{}",
        files.len()
    )
}

/// 深度优先遍历文件（跳过隐藏目录与排除目录，密钥文件跳过）
fn scan_files(root: &str, file_limit: usize, entry_limit: usize) -> ScanOutcome {
    let root = std::path::PathBuf::from(root);
    let mut outcome = ScanOutcome {
        files: Vec::new(),
        issues: Vec::new(),
        issue_count: 0,
        truncated: false,
        file_limit_reached: false,
        entry_limit_reached: false,
        visited_entries: 0,
    };
    let mut queue = vec![root.clone()];
    while let Some(dir) = queue.pop() {
        if outcome.files.len() >= file_limit {
            outcome.truncated = true;
            outcome.file_limit_reached = true;
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                outcome.record_issue(format!("读取目录 {} 失败：{error}", dir.display()));
                continue;
            }
        };
        let mut directories = Vec::new();
        let mut files = Vec::new();
        let mut entry_limit_reached = false;
        for entry in entries {
            if outcome.visited_entries >= entry_limit {
                outcome.truncated = true;
                outcome.entry_limit_reached = true;
                entry_limit_reached = true;
                break;
            }
            outcome.visited_entries += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    outcome.record_issue(format!("读取目录 {} 的条目失败：{error}", dir.display()));
                    continue;
                }
            };
            let path = entry.path();
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => {
                    outcome
                        .record_issue(format!("跳过无法用 UTF-8 表示的路径：{}", path.display()));
                    continue;
                }
            };
            if name.starts_with('.') && name != ".github" {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    outcome.record_issue(format!("读取文件类型 {} 失败：{error}", path.display()));
                    continue;
                }
            };
            if file_type.is_dir() {
                if !EXCLUDED_DIRECTORIES.contains(&name.as_str()) {
                    directories.push(path);
                }
            } else if file_type.is_file() {
                if is_secret_file(&name) {
                    continue;
                }
                files.push(path);
            }
        }
        if !entry_limit_reached {
            directories.sort();
            queue.extend(directories.into_iter().rev());
        }
        files.sort();
        let remaining = file_limit - outcome.files.len();
        if files.len() > remaining {
            outcome.truncated = true;
            outcome.file_limit_reached = true;
        }
        for path in files.into_iter().take(remaining) {
            let Some(relative) = path
                .strip_prefix(&root)
                .ok()
                .and_then(std::path::Path::to_str)
            else {
                outcome.record_issue(format!(
                    "跳过无法表示为仓库 UTF-8 相对路径的条目：{}",
                    path.display()
                ));
                continue;
            };
            outcome.files.push(relative.replace('\\', "/"));
        }
        if entry_limit_reached {
            break;
        }
    }
    outcome.files.sort();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_stops_at_the_total_directory_entry_limit() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..5 {
            std::fs::create_dir(dir.path().join(format!("dir-{index}"))).unwrap();
        }

        let scan = scan_files(dir.path().to_str().unwrap(), 100, 3);

        assert!(scan.truncated);
        assert!(scan.entry_limit_reached);
        assert!(!scan.file_limit_reached);
        assert_eq!(scan.visited_entries, 3);
    }
}
