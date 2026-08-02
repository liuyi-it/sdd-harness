//! 受限文件扫描降级：GitNexus 与 CodeGraph 均不可用时使用。
//!
//! 翻译自 Node 版 `packages/core/src/codebase/codebase-adapter.ts` 的
//! fallback()：目录遍历 + 排除目录 + 密钥文件跳过 + 关键字扫描 + 候选文件摘要。
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

const SAFE_TEXT_EXTENSIONS: [&str; 10] = [
    ".ts", ".tsx", ".js", ".jsx", ".json", ".md", ".yml", ".yaml", ".java", ".rs",
];

const KEYWORD_RULES: [(&str, [&str; 3]); 4] = [
    ("service", ["service", "Service", "services"]),
    ("controller", ["controller", "Controller", "controllers"]),
    ("route", ["route", "router", "endpoint"]),
    ("test", ["describe(", "it(", "test("]),
];

/// 密钥文件名直接跳过（与 Node 版 isSecretFile 一致）
pub fn is_secret_file(name: &str) -> bool {
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

/// 降级文件扫描（返回 degraded=true 的同结构结果）
pub fn fallback_scan(root: &str, intent: KnowledgeIntent, _query: &str) -> QueryResult {
    let files = scan_files(root, 2_000);
    let payload = match intent {
        KnowledgeIntent::Impact => json!({
            "intent": "impact",
            "files": [],
            "symbols": [],
            "tests": [],
            "risks": [],
            "codebaseSummary": summary_text(&files),
        }),
        _ => json!({
            "intent": intent.as_str(),
            "codebaseSummary": summary_text(&files),
            "packageStructure": package_structure(&files),
            "architecture": architecture_text(&files),
        }),
    };
    QueryResult {
        provider: "fallback-file-scan",
        degraded: true,
        confidence: 0.3,
        reason: Some("GitNexus 与 CodeGraph 均不可用".to_string()),
        payload,
    }
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
    let mut dirs: Vec<String> = files
        .iter()
        .filter_map(|f| f.rsplit_once('/').map(|(d, _)| d.to_string()))
        .filter(|d| !d.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    dirs.sort();
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
fn scan_files(root: &str, limit: usize) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut queue: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = queue.pop() {
        if result.len() >= limit {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name != ".github" {
                continue;
            }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if !EXCLUDED_DIRECTORIES.contains(&name.as_str()) {
                    queue.push(path);
                }
            } else if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                if is_secret_file(&name) {
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                result.push(relative);
                if result.len() >= limit {
                    break;
                }
            }
        }
    }
    result.sort();
    result
}

/// 安全文本文件判断
pub fn is_safe_text_file(file: &str) -> bool {
    SAFE_TEXT_EXTENSIONS.iter().any(|ext| file.ends_with(ext))
}

/// 关键字扫描（供 codebase query 降级结果使用）
pub fn keyword_matches(files: &[String], root: &str) -> Vec<String> {
    let mut matches = Vec::new();
    for (label, patterns) in KEYWORD_RULES {
        let mut hit_files: Vec<String> = Vec::new();
        for file in files.iter().take(80) {
            if !is_safe_text_file(file) {
                continue;
            }
            let full = std::path::PathBuf::from(root).join(file);
            if let Ok(content) = std::fs::read_to_string(&full) {
                if patterns.iter().any(|p| content.contains(p)) {
                    hit_files.push(file.clone());
                }
            }
            if hit_files.len() >= 3 {
                break;
            }
        }
        if !hit_files.is_empty() {
            matches.push(format!("- {label}：{}", hit_files.join("、")));
        }
    }
    matches
}
