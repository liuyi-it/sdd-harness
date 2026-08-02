//! GitInspector：Git 快照、变更 delta 与文件范围校验。
//!
//! 翻译自 Node 版 `packages/core/src/git/git-inspector.ts`：
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
        let out = git(cwd, &["status", "--porcelain"])?;
        if !out.status.success() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // 格式：XY path 或 XY -> path
            let line = line.trim_start_matches([' ', '?', '!']);
            let line = line.trim_start();
            let Some((_, path)) = line.split_once(' ') else {
                if !line.is_empty() {
                    files.push(line.to_string());
                }
                continue;
            };
            let path = path.trim();
            if !path.is_empty() {
                files.push(path.to_string());
            }
        }
        Ok(files)
    }

    /// 文件是否在仓库内（路径安全校验前置；归一化 .. 组件）
    pub fn path_within_repo(cwd: &str, relative: &str) -> bool {
        use std::path::Component;
        let root = PathBuf::from(cwd)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(cwd));
        let joined = root.join(relative);
        // 归一化 `.`/`..` 组件后再比较，防止 `../outside` 前缀绕过
        let mut normalized = PathBuf::new();
        for component in joined.components() {
            match component {
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::CurDir => {}
                other => normalized.push(other.as_os_str()),
            }
        }
        normalized.starts_with(&root)
    }
}

fn git(cwd: &str, args: &[&str]) -> Result<std::process::Output, SddError> {
    run_command(&PathBuf::from("git"), args, cwd, 30_000)
        .map_err(|e| SddError::new("E_PATH_OUTSIDE_REPO", &format!("git 命令执行失败：{e}")))
}
