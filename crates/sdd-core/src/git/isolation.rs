//! 可选 Git worktree 隔离：只创建或验证工作区，不执行 merge/push/reset/clean/删除。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::SddError;
use crate::subprocess::run_command;

#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeHandle {
    pub worktree_path: String,
    pub branch: String,
    pub baseline_commit: String,
}

pub struct GitIsolationManager;

impl GitIsolationManager {
    pub fn enabled(config: &serde_json::Value) -> Result<bool, SddError> {
        crate::schema::validate_json("config", config)?;
        config
            .pointer("/workflow/gitIsolation")
            .and_then(|value| value.as_bool())
            .ok_or_else(|| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    "runtime.json 缺少 workflow.gitIsolation",
                )
            })
    }

    pub fn ensure_worktree(cwd: &str, change_id: &str) -> Result<WorktreeHandle, SddError> {
        validate_change_id(change_id)?;
        let root = PathBuf::from(cwd)
            .canonicalize()
            .map_err(|e| SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法解析仓库路径：{e}")))?;
        let baseline_commit = git_stdout(&root, &["rev-parse", "HEAD"])?;
        let branch = format!("sdd/{change_id}");
        let worktrees_root = crate::state::paths::worktrees_dir(cwd, true)?;
        let path = worktrees_root.join(change_id);
        let registered = worktrees(&root)?
            .into_iter()
            .find(|entry| same_worktree(&entry.path, &path));

        match (path.exists(), registered) {
            (false, None) => {
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

#[derive(Debug)]
struct WorktreeEntry {
    path: PathBuf,
    head: String,
    branch: Option<String>,
}

fn worktrees(root: &Path) -> Result<Vec<WorktreeEntry>, SddError> {
    let output = git_output(root, &["worktree", "list", "--porcelain", "-z"])?;
    if !output.status.success() {
        return Err(git_error("读取 Git worktree 列表失败", &output));
    }
    parse_worktrees(&output.stdout)
}

fn parse_worktrees(output: &[u8]) -> Result<Vec<WorktreeEntry>, SddError> {
    let mut entries = Vec::new();
    let mut path = None;
    let mut head = None;
    let mut branch = None;
    for field in output.split(|byte| *byte == 0) {
        if field.is_empty() {
            if path.is_some() || head.is_some() || branch.is_some() {
                let entry_path = path.take().ok_or_else(malformed_worktree_output)?;
                let entry_head = head.take().ok_or_else(malformed_worktree_output)?;
                entries.push(WorktreeEntry {
                    path: entry_path,
                    head: entry_head,
                    branch: branch.take(),
                });
            }
            continue;
        }
        let field = std::str::from_utf8(field).map_err(|_| {
            SddError::new(
                "E_PATH_OUTSIDE_REPO",
                "Git worktree 列表包含非 UTF-8 字段，当前 JSON 契约无法安全表示",
            )
        })?;
        if let Some(value) = field.strip_prefix("worktree ") {
            if path.replace(PathBuf::from(value)).is_some() {
                return Err(malformed_worktree_output());
            }
        } else if let Some(value) = field.strip_prefix("HEAD ") {
            if head.replace(value.to_string()).is_some() {
                return Err(malformed_worktree_output());
            }
        } else if let Some(value) = field.strip_prefix("branch refs/heads/") {
            if branch.replace(value.to_string()).is_some() {
                return Err(malformed_worktree_output());
            }
        }
    }
    if path.is_some() || head.is_some() || branch.is_some() {
        return Err(malformed_worktree_output());
    }
    Ok(entries)
}

fn malformed_worktree_output() -> SddError {
    SddError::new(
        "E_COMPONENT_UNAVAILABLE",
        "Git worktree porcelain 输出结构不完整",
    )
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
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| SddError::new("E_COMPONENT_UNAVAILABLE", "Git 命令返回了非 UTF-8 文本"))
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<std::process::Output, SddError> {
    // 与 inspector.rs 一致：关闭 fsmonitor、pager 与交互式凭据提示。
    let mut full_args = vec!["-c", "core.fsmonitor=false", "--no-pager"];
    full_args.extend_from_slice(args);
    run_command(
        Path::new("git"),
        &full_args,
        cwd,
        Duration::from_secs(30),
        &[("GIT_TERMINAL_PROMPT", "0")],
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

#[cfg(test)]
mod tests {
    use super::parse_worktrees;

    #[test]
    fn worktree_parser_uses_nul_delimited_porcelain() {
        let entries = parse_worktrees(
            b"worktree /repo/main\0HEAD abc\0branch refs/heads/main\0\0worktree /repo/feature\nname\0HEAD def\0detached\0\0",
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert_eq!(entries[1].path.to_string_lossy(), "/repo/feature\nname");
        assert_eq!(entries[1].branch, None);
    }

    #[test]
    fn worktree_parser_rejects_incomplete_and_non_utf8_output() {
        assert_eq!(
            parse_worktrees(b"worktree /repo\0\0").unwrap_err().code,
            "E_COMPONENT_UNAVAILABLE"
        );
        assert_eq!(
            parse_worktrees(b"worktree \xff\0HEAD abc\0\0")
                .unwrap_err()
                .code,
            "E_PATH_OUTSIDE_REPO"
        );
    }
}
