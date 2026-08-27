//! GitInspector：Git 基线、工作区变更与文件范围校验。
//!
//! 非 git 仓库返回 E_PATH_OUTSIDE_REPO。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::SddError;
use crate::subprocess::run_command;

pub struct GitInspector;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RepoEntryContent {
    Missing,
    TooLarge,
    Content(Vec<u8>),
}

enum ResolvedRepoEntry {
    Missing,
    Symlink(Vec<u8>),
    File(PathBuf),
}

impl GitInspector {
    /// 当前仓库是否 git 仓库
    pub fn is_git_repo(cwd: &str) -> Result<bool, SddError> {
        let output = git(cwd, &["rev-parse", "--is-inside-work-tree"])?;
        if !output.status.success() {
            return Ok(false);
        }
        match output.stdout.as_slice() {
            b"true\n" | b"true\r\n" | b"true" => Ok(true),
            b"false\n" | b"false\r\n" | b"false" => Ok(false),
            _ => Err(SddError::new(
                "E_COMPONENT_UNAVAILABLE",
                "git rev-parse 返回了非预期的工作区状态",
            )),
        }
    }

    /// 当前 HEAD 提交。
    pub fn head(cwd: &str) -> Result<String, SddError> {
        let out = git(cwd, &["rev-parse", "HEAD"])?;
        if !out.status.success() {
            return Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                &format!(
                    "无法获取 HEAD 提交：{}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        let head = std::str::from_utf8(&out.stdout)
            .map_err(|_| SddError::new("E_COMPONENT_UNAVAILABLE", "Git HEAD 不是有效 UTF-8"))?
            .trim()
            .to_string();
        if !matches!(head.len(), 40 | 64) || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Err(SddError::new("E_PATH_OUTSIDE_REPO", "无法获取 HEAD 提交"))
        } else {
            Ok(head)
        }
    }

    /// 当前工作区未提交变更（git status --porcelain）
    pub fn changed_files(cwd: &str) -> Result<Vec<String>, SddError> {
        let out = git(
            cwd,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        if !out.status.success() {
            // fail-closed：git 状态读取失败不能当作"无变更"，否则会漏掉事实核对
            return Err(SddError::new(
                "E_COMPONENT_UNAVAILABLE",
                &format!(
                    "git status 执行失败：{}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        parse_changed_files(&out.stdout)
    }

    /// 业务变更：过滤 SDD 自身运行产物（.sdd/）。
    /// 状态文件（runtime.json 等）的损坏检测由 runtime 校验和边车负责，
    /// 此处只做路径过滤，不校验内容。
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
        if files.is_empty() {
            return Ok(hashes);
        }
        let root = canonical_repo_root(cwd)?;
        for path in files {
            let value = Self::entry_digest(&root, path)?;
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
        let head = Self::head(cwd)?;
        let mut files = Self::business_changes(cwd)?;
        files.sort();
        files.dedup();

        let mut input = head.into_bytes();
        if files.is_empty() {
            return Ok(crate::policies::digest::digest_bytes(&input));
        }
        let root = canonical_repo_root(cwd)?;
        for file in files {
            input.push(0);
            input.extend_from_slice(file.as_bytes());
            input.push(0);
            match Self::entry_digest(&root, &file)? {
                Some(digest) => input.extend_from_slice(digest.as_bytes()),
                None => input.extend_from_slice(b"<deleted>"),
            }
        }
        Ok(crate::policies::digest::digest_bytes(&input))
    }

    /// 读取 Git 条目的真实工作区语义；符号链接按链接文本读取，不跟随目标内容。
    pub(crate) fn read_entry_with_limit(
        cwd: &str,
        relative: &str,
        maximum: usize,
    ) -> Result<RepoEntryContent, SddError> {
        let root = canonical_repo_root(cwd)?;
        match Self::resolve_repo_entry(&root, relative)? {
            ResolvedRepoEntry::Missing => Ok(RepoEntryContent::Missing),
            ResolvedRepoEntry::Symlink(content) => {
                if content.len() > maximum {
                    Ok(RepoEntryContent::TooLarge)
                } else {
                    Ok(RepoEntryContent::Content(content))
                }
            }
            ResolvedRepoEntry::File(path) => {
                let maximum_u64 = u64::try_from(maximum)
                    .map_err(|_| SddError::new("E_REVIEW_FAILED", "审计读取上限超出 u64 范围"))?;
                if std::fs::metadata(&path)
                    .map_err(|error| read_entry_error(relative, error))?
                    .len()
                    > maximum_u64
                {
                    return Ok(RepoEntryContent::TooLarge);
                }
                let read_limit = maximum_u64
                    .checked_add(1)
                    .ok_or_else(|| SddError::new("E_REVIEW_FAILED", "审计读取上限溢出"))?;
                let mut content = Vec::new();
                let bytes = std::fs::File::open(&path)
                    .map_err(|error| read_entry_error(relative, error))?
                    .take(read_limit)
                    .read_to_end(&mut content)
                    .map_err(|error| read_entry_error(relative, error))?;
                if bytes > maximum {
                    Ok(RepoEntryContent::TooLarge)
                } else {
                    Ok(RepoEntryContent::Content(content))
                }
            }
        }
    }

    fn entry_digest(root: &Path, relative: &str) -> Result<Option<String>, SddError> {
        match Self::resolve_repo_entry(root, relative)? {
            ResolvedRepoEntry::Missing => Ok(None),
            ResolvedRepoEntry::Symlink(content) => {
                Ok(Some(crate::policies::digest::digest_bytes(&content)))
            }
            ResolvedRepoEntry::File(path) => {
                let file = std::fs::File::open(&path)
                    .map_err(|error| read_entry_error(relative, error))?;
                crate::policies::digest::digest_reader(file)
                    .map(Some)
                    .map_err(|error| read_entry_error(relative, error))
            }
        }
    }

    fn resolve_repo_entry(root: &Path, relative: &str) -> Result<ResolvedRepoEntry, SddError> {
        let normalized = validated_relative_path(relative)?;
        let raw = raw_repo_path(root, &normalized);
        // 先解析整条路径，确保符号链接没有逃逸仓库且不是悬空链接。
        let resolved = resolve_repo_path(root, &normalized, relative)?;
        match std::fs::symlink_metadata(&raw) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target =
                    std::fs::read_link(&raw).map_err(|error| read_entry_error(relative, error))?;
                Ok(ResolvedRepoEntry::Symlink(path_bytes(&target)))
            }
            Ok(metadata) if metadata.is_file() => Ok(ResolvedRepoEntry::File(resolved)),
            Ok(_) => Err(SddError::new(
                "E_PATH_OUTSIDE_REPO",
                &format!("Git 变更路径不是文件：{relative}"),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ResolvedRepoEntry::Missing)
            }
            Err(error) => Err(read_entry_error(relative, error)),
        }
    }

    /// 将不可信的仓库相对路径解析为安全路径，并拒绝绝对路径、跨目录与 symlink 逃逸。
    pub fn resolve_repo_path(cwd: &str, relative: &str) -> Result<PathBuf, SddError> {
        let root = canonical_repo_root(cwd)?;
        let normalized = validated_relative_path(relative)?;
        resolve_repo_path(&root, &normalized, relative)
    }
}

fn canonical_repo_root(cwd: &str) -> Result<PathBuf, SddError> {
    PathBuf::from(cwd).canonicalize().map_err(|error| {
        SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法解析仓库路径：{error}"))
    })
}

pub(crate) fn validated_relative_path(relative: &str) -> Result<String, SddError> {
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
    Ok(normalized)
}

fn raw_repo_path(root: &Path, normalized: &str) -> PathBuf {
    normalized
        .split('/')
        .filter(|part| *part != ".")
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn resolve_repo_path(root: &Path, normalized: &str, original: &str) -> Result<PathBuf, SddError> {
    let mut candidate = root.to_path_buf();
    for part in normalized.split('/').filter(|part| *part != ".") {
        candidate.push(part);
        let metadata = match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(SddError::new(
                    "E_PATH_OUTSIDE_REPO",
                    &format!("检查路径失败：{error}"),
                ));
            }
        };
        let resolved = candidate.canonicalize().map_err(|error| {
            let code = if metadata.file_type().is_symlink() {
                "E_SYMLINK_BLOCKED"
            } else {
                "E_PATH_OUTSIDE_REPO"
            };
            SddError::new(code, &format!("无法解析路径：{error}"))
        })?;
        if !resolved.starts_with(root) {
            return Err(SddError::new(
                "E_SYMLINK_BLOCKED",
                &format!("路径通过符号链接逃逸仓库：{original}"),
            ));
        }
        candidate = resolved;
    }
    Ok(candidate)
}

fn read_entry_error(relative: &str, error: std::io::Error) -> SddError {
    SddError::new(
        "E_REVIEW_FAILED",
        &format!("读取 Git 变更文件 {relative} 失败：{error}"),
    )
}

#[cfg(unix)]
fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
}

fn parse_changed_files(output: &[u8]) -> Result<Vec<String>, SddError> {
    let mut records = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' {
            return Err(SddError::new(
                "E_COMPONENT_UNAVAILABLE",
                "git status 返回了畸形 porcelain v1 记录",
            ));
        }
        let status = &record[..2];
        files.push(status_path(&record[3..])?);
        if status.contains(&b'R') || status.contains(&b'C') {
            let source = records.next().ok_or_else(|| {
                SddError::new(
                    "E_COMPONENT_UNAVAILABLE",
                    "git status 的重命名或复制记录缺少来源路径",
                )
            })?;
            files.push(status_path(source)?);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn status_path(raw: &[u8]) -> Result<String, SddError> {
    let path = std::str::from_utf8(raw).map_err(|_| {
        SddError::new(
            "E_PATH_OUTSIDE_REPO",
            "git status 包含非 UTF-8 路径，当前 JSON 契约无法安全表示",
        )
    })?;
    if path.is_empty() {
        return Err(SddError::new(
            "E_COMPONENT_UNAVAILABLE",
            "git status 返回了空路径",
        ));
    }
    Ok(path.to_string())
}

/// 统一 Git 执行入口：关闭 fsmonitor、pager 与交互式凭据提示。
fn git(cwd: &str, args: &[&str]) -> Result<std::process::Output, SddError> {
    let mut full_args = vec!["-c", "core.fsmonitor=false", "--no-pager"];
    full_args.extend_from_slice(args);
    run_command(
        std::path::Path::new("git"),
        &full_args,
        std::path::Path::new(cwd),
        Duration::from_secs(30),
        &[("GIT_TERMINAL_PROMPT", "0")],
    )
    .map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::TimedOut {
            "E_TIMEOUT"
        } else {
            "E_COMPONENT_UNAVAILABLE"
        };
        SddError::new(code, &format!("git 命令执行失败：{error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::parse_changed_files;

    #[test]
    fn porcelain_parser_handles_rename_and_deduplicates() {
        let files = parse_changed_files(b"R  new.rs\0old.rs\0 M same.rs\0 M same.rs\0").unwrap();
        assert_eq!(files, ["new.rs", "old.rs", "same.rs"]);
    }

    #[test]
    fn porcelain_parser_rejects_malformed_and_non_utf8_records() {
        assert_eq!(
            parse_changed_files(b"M\0").unwrap_err().code,
            "E_COMPONENT_UNAVAILABLE"
        );
        assert_eq!(
            parse_changed_files(b"R  new.rs\0").unwrap_err().code,
            "E_COMPONENT_UNAVAILABLE"
        );
        assert_eq!(
            parse_changed_files(b"?? \xff\0").unwrap_err().code,
            "E_PATH_OUTSIDE_REPO"
        );
    }
}
