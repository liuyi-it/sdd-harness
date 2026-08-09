//! GitInspector：Git 快照、变更 delta 与文件范围校验。
//!
//! 翻译自 早期 Node 实现：
//! - snapshot：捕获 HEAD 与工作区状态
//! - compute_delta：两个提交间的 A/M/D 变更
//! - changed_files：当前工作区未提交变更
//!
//! 非 git 仓库返回 E_PATH_OUTSIDE_REPO。

use std::path::PathBuf;

use crate::error::SddError;
use crate::knowledge::provider::run_command;

#[derive(Debug, Clone, PartialEq)]
pub struct GitDelta {
    pub path: String,
    pub status: String, // A | M | D
}

#[derive(Debug, Clone)]
pub struct GitSnapshot {
    pub head: String,
    pub files: Vec<String>,
}

pub struct GitInspector;

impl GitInspector {
    /// 当前仓库是否 git 仓库
    pub fn is_git_repo(cwd: &str) -> bool {
        match git(cwd, &["rev-parse", "--is-inside-work-tree"]) {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    /// 快照：HEAD 提交 + 已跟踪文件列表
    pub fn snapshot(cwd: &str) -> Result<GitSnapshot, SddError> {
        if !Self::is_git_repo(cwd) {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                "当前目录不是 git 仓库",
            ));
        }
        let head = match git(cwd, &["rev-parse", "HEAD"]) {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => return Err(SddError::new("E_PATH_OUTSIDE_REPO", "无法获取 HEAD 提交")),
        };
        let files = match git(cwd, &["ls-files"]) {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            _ => Vec::new(),
        };
        Ok(GitSnapshot { head, files })
    }

    /// 两个基线间的变更 delta（git diff --name-status <base>..HEAD）
    pub fn compute_delta(cwd: &str, base_head: &str) -> Result<Vec<GitDelta>, SddError> {
        if !Self::is_git_repo(cwd) {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                "当前目录不是 git 仓库",
            ));
        }
        let out = git(cwd, &["diff", "--name-status", base_head, "HEAD"])?;
        if !out.status.success() {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                &format!(
                    "git diff 失败：{}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        let mut deltas = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let status = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            if path.is_empty() {
                continue;
            }
            let status_code = match status.chars().next() {
                Some('A') => "A",
                Some('M') => "M",
                Some('D') => "D",
                _ => "M",
            };
            deltas.push(GitDelta {
                path,
                status: status_code.to_string(),
            });
        }
        Ok(deltas)
    }

    /// 当前工作区未提交变更（git status --porcelain）
    pub fn changed_files(cwd: &str) -> Result<Vec<String>, SddError> {
        if !Self::is_git_repo(cwd) {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                "当前目录不是 git 仓库",
            ));
        }
        let out = git(
            cwd,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        if !out.status.success() {
            return Ok(Vec::new());
        }
        let records: Vec<&[u8]> = out.stdout.split(|byte| *byte == 0).collect();
        let mut files = Vec::new();
        let mut index = 0;
        while index < records.len() {
            let record = records[index];
            if record.len() < 4 {
                index += 1;
                continue;
            }
            let status = &record[..2];
            let path = String::from_utf8_lossy(&record[3..]).to_string();
            if status.contains(&b'R') || status.contains(&b'C') {
                index += 1;
                if let Some(source) = records.get(index) {
                    let source = String::from_utf8_lossy(source).to_string();
                    if !source.is_empty() {
                        files.push(source);
                    }
                }
            }
            if !path.is_empty() {
                files.push(path);
            }
            index += 1;
        }
        Ok(files)
    }

    pub fn business_changes(cwd: &str) -> Result<Vec<String>, SddError> {
        Ok(Self::changed_files(cwd)?
            .into_iter()
            .filter(|path| path != ".sdd" && !path.starts_with(".sdd/"))
            .collect())
    }

    pub fn file_hashes(
        cwd: &str,
        files: &[String],
    ) -> Result<std::collections::BTreeMap<String, Option<String>>, SddError> {
        let mut hashes = std::collections::BTreeMap::new();
        for path in files {
            let resolved = Self::resolve_repo_path(cwd, path)?;
            let value = match std::fs::read(&resolved) {
                Ok(content) => Some(crate::policies::digest::digest_bytes(&content)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(SddError::new(
                        "E_STATE_CORRUPTED",
                        &format!("读取 Git 变更文件 {path} 失败：{error}"),
                    ));
                }
            };
            hashes.insert(path.clone(), value);
        }
        Ok(hashes)
    }

    pub fn changes_since(
        cwd: &str,
        baseline_files: &[String],
        baseline_hashes: &std::collections::BTreeMap<String, Option<String>>,
    ) -> Result<Vec<String>, SddError> {
        let baseline: std::collections::BTreeSet<String> = baseline_files.iter().cloned().collect();
        let current: std::collections::BTreeSet<String> =
            Self::business_changes(cwd)?.into_iter().collect();
        let all: Vec<String> = baseline.union(&current).cloned().collect();
        let current_hashes = Self::file_hashes(cwd, &all)?;
        Ok(all
            .into_iter()
            .filter(|path| {
                !baseline.contains(path) || baseline_hashes.get(path) != current_hashes.get(path)
            })
            .collect())
    }

    /// 当前 HEAD 与业务工作区内容的稳定指纹；忽略 SDD 自身运行产物。
    pub fn workspace_fingerprint(cwd: &str) -> Result<String, SddError> {
        let snapshot = Self::snapshot(cwd)?;
        let mut files = Self::business_changes(cwd)?;
        files.sort();
        files.dedup();

        let mut input = snapshot.head.into_bytes();
        for file in files {
            input.push(0);
            input.extend_from_slice(file.as_bytes());
            input.push(0);
            let path = Self::resolve_repo_path(cwd, &file)?;
            match std::fs::read(path) {
                Ok(content) => input
                    .extend_from_slice(crate::policies::digest::digest_bytes(&content).as_bytes()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    input.extend_from_slice(b"<deleted>")
                }
                Err(error) => {
                    return Err(SddError::new(
                        "E_PATH_OUTSIDE_REPO",
                        &format!("读取变更文件 {file} 失败：{error}"),
                    ));
                }
            }
        }
        Ok(crate::policies::digest::digest_bytes(&input))
    }

    /// 读取 HEAD 中的文件；未跟踪或基线中不存在时返回 None。
    pub fn file_at_head(cwd: &str, relative: &str) -> Result<Option<String>, SddError> {
        Self::resolve_repo_path(cwd, relative)?;
        let spec = format!("HEAD:{}", relative.replace('\\', "/"));
        let output = git(cwd, &["show", &spec])?;
        if !output.status.success() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    }

    /// 将不可信的仓库相对路径解析为安全路径，并拒绝绝对路径、跨目录与 symlink 逃逸。
    pub fn resolve_repo_path(cwd: &str, relative: &str) -> Result<PathBuf, SddError> {
        let normalized = relative.replace('\\', "/");
        let first = normalized.split('/').next().unwrap_or("");
        if normalized.is_empty()
            || normalized.starts_with('/')
            || first.ends_with(':')
            || normalized
                .split('/')
                .any(|part| part == ".." || part.is_empty())
        {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                &format!("路径不在仓库内：{relative}"),
            ));
        }

        let root = PathBuf::from(cwd)
            .canonicalize()
            .map_err(|e| SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法解析仓库路径：{e}")))?;
        let mut candidate = root.clone();
        for part in normalized.split('/').filter(|part| *part != ".") {
            candidate.push(part);
            if candidate.exists() {
                let resolved = candidate.canonicalize().map_err(|e| {
                    SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法解析路径：{e}"))
                })?;
                if !resolved.starts_with(&root) {
                    return Err(SddError::new(
                        "E_SYMLINK_BLOCKED",
                        &format!("路径通过符号链接逃逸仓库：{relative}"),
                    ));
                }
                candidate = resolved;
            }
        }
        Ok(candidate)
    }

    pub fn path_within_repo(cwd: &str, relative: &str) -> bool {
        Self::resolve_repo_path(cwd, relative).is_ok()
    }
}

fn git(cwd: &str, args: &[&str]) -> Result<std::process::Output, SddError> {
    run_command(&PathBuf::from("git"), args, cwd, 30_000)
        .map_err(|e| SddError::new("E_PATH_OUTSIDE_REPO", &format!("git 命令执行失败：{e}")))
}
