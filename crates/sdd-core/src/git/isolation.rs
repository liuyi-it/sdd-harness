//! 可选 Git worktree 隔离：只创建或验证工作区，不执行 merge/push/reset/clean/删除。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::SddError;
use crate::knowledge::provider::run_command;

#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeHandle {
    pub worktree_path: String,
    pub branch: String,
    pub baseline_commit: String,
}

pub struct GitIsolationManager;

impl GitIsolationManager {
    pub fn enabled(cwd: &str) -> Result<bool, SddError> {
        let config = crate::state::runtime_store::read_config(cwd)?;
        if config == serde_json::Value::Null || config == serde_json::json!({}) {
            return Ok(false);
        }
        if !config
            .get("workflow")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "runtime.json 的 config 必须包含 workflow 对象",
            ));
        }
        Ok(config
            .pointer("/workflow/gitIsolation")
            .and_then(|value| value.as_bool())
            .or_else(|| {
                config
                    .pointer("/git/createWorktree")
                    .and_then(|value| value.as_bool())
            })
            .unwrap_or(false))
    }

    pub fn ensure_worktree(cwd: &str, change_id: &str) -> Result<WorktreeHandle, SddError> {
        validate_change_id(change_id)?;
        let root = PathBuf::from(cwd)
            .canonicalize()
            .map_err(|e| SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法解析仓库路径：{e}")))?;
        let baseline_commit = git_stdout(&root, &["rev-parse", "HEAD"])?;
        let branch = format!("sdd/{change_id}");
        let path = root.join(".sdd/worktrees").join(change_id);
        let registered = worktrees(&root)?
            .into_iter()
            .find(|entry| same_worktree(&entry.path, &path));

        match (path.exists(), registered) {
            (false, None) => {
                fs::create_dir_all(path.parent().expect("worktree 必须有父目录")).map_err(|e| {
                    SddError::new(
                        "E_STATE_CORRUPTED",
                        &format!("创建 worktree 父目录失败：{e}"),
                    )
                })?;
                let path_text = display_path(&path);
                let output = git_output(
                    &root,
                    &[
                        "worktree",
                        "add",
                        "-b",
                        &branch,
                        &path_text,
                        &baseline_commit,
                    ],
                )?;
                if !output.status.success() {
                    return Err(git_error("创建 worktree 失败", &output));
                }
            }
            (true, Some(entry)) => {
                if entry.branch.as_deref() != Some(branch.as_str()) {
                    return Err(SddError::new(
                        "E_STATE_CORRUPTED",
                        &format!(
                            "worktree 分支不匹配：期望 {branch}，实际 {}",
                            entry.branch.as_deref().unwrap_or("detached")
                        ),
                    ));
                }
                if entry.head != baseline_commit {
                    return Err(SddError::new(
                        "E_STATE_CORRUPTED",
                        "worktree HEAD 已偏离控制仓库基线，拒绝复用",
                    ));
                }
                if !git_stdout(&path, &["status", "--porcelain"])?
                    .trim()
                    .is_empty()
                {
                    return Err(SddError::new(
                        "E_CONCURRENT_RUN",
                        "worktree 存在未提交改动，拒绝复用",
                    ));
                }
            }
            _ => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("worktree 路径与 Git 注册状态不一致：{}", path.display()),
                ));
            }
        }

        Ok(WorktreeHandle {
            worktree_path: display_path(&path),
            branch,
            baseline_commit,
        })
    }

    pub fn release(_handle: WorktreeHandle) -> Result<(), SddError> {
        Ok(())
    }
}

/// Windows 下 `canonicalize()` 会返回 `\\?\` 前缀路径，git 不认且与
/// `git worktree list` 输出的普通路径不相等；统一去掉该前缀再比较/使用。
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.strip_prefix(r"\\?\UNC\")
            .map(|rest| format!(r"\\{rest}"))
            .unwrap_or_else(|| {
                text.strip_prefix(r"\\?\")
                    .map(str::to_string)
                    .unwrap_or_else(|| text.to_string())
            })
    } else {
        text.to_string()
    }
}

fn same_worktree(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => Path::new(&display_path(a)) == Path::new(&display_path(b)),
    }
}

struct WorktreeEntry {
    path: PathBuf,
    head: String,
    branch: Option<String>,
}

fn worktrees(root: &Path) -> Result<Vec<WorktreeEntry>, SddError> {
    let output = git_stdout(root, &["worktree", "list", "--porcelain"])?;
    let mut entries = Vec::new();
    let mut path = None;
    let mut head = None;
    let mut branch = None;
    for line in output.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let (Some(path), Some(head)) = (path.take(), head.take()) {
                entries.push(WorktreeEntry {
                    path,
                    head,
                    branch: branch.take(),
                });
            }
        } else if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("HEAD ") {
            head = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        }
    }
    Ok(entries)
}

pub fn validate_change_id(change_id: &str) -> Result<(), SddError> {
    if change_id.is_empty()
        || !change_id.chars().all(|character| {
            (character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
                || (!character.is_ascii() && character.is_alphanumeric())
        })
    {
        return Err(SddError::new(
            "E_SECURITY_BLOCKED",
            &format!("非法 changeId：{change_id}"),
        ));
    }
    Ok(())
}

fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String, SddError> {
    let output = git_output(cwd, args)?;
    if !output.status.success() {
        return Err(git_error("Git 命令失败", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<std::process::Output, SddError> {
    run_command(
        Path::new("git"),
        args,
        cwd.to_string_lossy().as_ref(),
        30_000,
    )
    .map_err(|e| SddError::new("E_COMPONENT_UNAVAILABLE", &format!("执行 git 失败：{e}")))
}

fn git_error(message: &str, output: &std::process::Output) -> SddError {
    SddError::new(
        "E_STATE_CORRUPTED",
        &format!(
            "{message}：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    )
}
