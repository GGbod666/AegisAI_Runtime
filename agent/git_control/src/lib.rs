#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRepository {
    pub root: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitStatusSnapshot {
    pub repo_root: PathBuf,
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged_files: usize,
    pub unstaged_files: usize,
    pub untracked_files: usize,
    pub ignored_files: usize,
}

impl GitStatusSnapshot {
    pub fn is_dirty(&self) -> bool {
        self.staged_files > 0 || self.unstaged_files > 0 || self.untracked_files > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCheckpointPlan {
    pub label: String,
    pub branch_name: String,
    pub tag_name: Option<String>,
    pub commit_message: String,
}

impl GitCheckpointPlan {
    pub fn for_label(snapshot: &GitStatusSnapshot, label: &str) -> Self {
        let label = sanitize_label(label);
        let head_prefix = snapshot
            .head_oid
            .as_deref()
            .map(|value| value.chars().take(7).collect::<String>())
            .unwrap_or_else(|| "unknown".to_string());

        Self {
            branch_name: format!("checkpoints/{label}"),
            tag_name: Some(format!("checkpoint/{label}")),
            commit_message: format!("checkpoint: {label} @ {head_prefix}"),
            label,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitError {
    NotRepository(PathBuf),
    CommandFailed(String),
    ParseError(String),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRepository(path) => {
                write!(f, "path is not inside a git repository: {}", path.display())
            }
            Self::CommandFailed(message) => write!(f, "{message}"),
            Self::ParseError(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for GitError {}

pub trait GitCommandRunner {
    fn runner_name(&self) -> &str;

    fn run(&mut self, cwd: &Path, args: &[String]) -> Result<String, GitError>;
}

#[derive(Default)]
pub struct SystemGitCommandRunner;

impl GitCommandRunner for SystemGitCommandRunner {
    fn runner_name(&self) -> &str {
        "system-git"
    }

    fn run(&mut self, cwd: &Path, args: &[String]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .map_err(|error| GitError::CommandFailed(format!("failed to run git: {error}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.contains("not a git repository") {
                Err(GitError::NotRepository(cwd.to_path_buf()))
            } else if stderr.is_empty() {
                Err(GitError::CommandFailed(format!(
                    "git command failed with status {}",
                    output.status
                )))
            } else {
                Err(GitError::CommandFailed(stderr))
            }
        }
    }
}

pub struct GitControl<R> {
    runner: R,
}

impl<R> GitControl<R>
where
    R: GitCommandRunner,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn discover_repository(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<GitRepository, GitError> {
        let path = path.as_ref();
        let root = self.runner.run(
            path,
            &["rev-parse".to_string(), "--show-toplevel".to_string()],
        )?;
        Ok(GitRepository {
            root: PathBuf::from(root),
        })
    }

    pub fn snapshot(&mut self, path: impl AsRef<Path>) -> Result<GitStatusSnapshot, GitError> {
        let repository = self.discover_repository(path)?;
        let status = self.runner.run(
            &repository.root,
            &[
                "status".to_string(),
                "--porcelain=v2".to_string(),
                "--branch".to_string(),
            ],
        )?;
        parse_porcelain_v2_snapshot(repository.root, &status)
    }
}

fn parse_porcelain_v2_snapshot(
    repo_root: PathBuf,
    raw: &str,
) -> Result<GitStatusSnapshot, GitError> {
    let mut snapshot = GitStatusSnapshot {
        repo_root,
        ..GitStatusSnapshot::default()
    };

    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            if value != "(initial)" {
                snapshot.head_oid = Some(value.to_string());
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("# branch.head ") {
            if value != "(detached)" {
                snapshot.branch = Some(value.to_string());
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            snapshot.upstream = Some(value.to_string());
            continue;
        }

        if let Some(value) = line.strip_prefix("# branch.ab ") {
            let mut parts = value.split_whitespace();
            let ahead = parts
                .next()
                .and_then(|item| item.strip_prefix('+'))
                .and_then(|item| item.parse::<u32>().ok())
                .ok_or_else(|| {
                    GitError::ParseError(format!("invalid branch.ab ahead field: {line}"))
                })?;
            let behind = parts
                .next()
                .and_then(|item| item.strip_prefix('-'))
                .and_then(|item| item.parse::<u32>().ok())
                .ok_or_else(|| {
                    GitError::ParseError(format!("invalid branch.ab behind field: {line}"))
                })?;
            snapshot.ahead = ahead;
            snapshot.behind = behind;
            continue;
        }

        if let Some(path_kind) = line.chars().next() {
            match path_kind {
                '1' | '2' | 'u' => classify_tracked_change(&mut snapshot, line)?,
                '?' => snapshot.untracked_files += 1,
                '!' => snapshot.ignored_files += 1,
                _ => {}
            }
        }
    }

    Ok(snapshot)
}

fn classify_tracked_change(snapshot: &mut GitStatusSnapshot, line: &str) -> Result<(), GitError> {
    let xy = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| GitError::ParseError(format!("missing xy status field: {line}")))?;
    let mut chars = xy.chars();
    let index_status = chars
        .next()
        .ok_or_else(|| GitError::ParseError(format!("missing index status: {line}")))?;
    let worktree_status = chars
        .next()
        .ok_or_else(|| GitError::ParseError(format!("missing worktree status: {line}")))?;

    if index_status != '.' {
        snapshot.staged_files += 1;
    }
    if worktree_status != '.' {
        snapshot.unstaged_files += 1;
    }

    Ok(())
}

fn sanitize_label(raw: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_sep = false;

    for ch in raw.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            previous_was_sep = false;
            ch.to_ascii_lowercase()
        } else {
            if previous_was_sep {
                continue;
            }
            previous_was_sep = true;
            '-'
        };
        normalized.push(mapped);
    }

    normalized.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::{GitCheckpointPlan, GitCommandRunner, GitControl, GitError, GitStatusSnapshot};
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};

    struct FakeGitRunner {
        responses: VecDeque<Result<String, GitError>>,
    }

    impl FakeGitRunner {
        fn new(responses: Vec<Result<String, GitError>>) -> Self {
            Self {
                responses: responses.into(),
            }
        }
    }

    impl GitCommandRunner for FakeGitRunner {
        fn runner_name(&self) -> &str {
            "fake-git"
        }

        fn run(&mut self, _cwd: &Path, _args: &[String]) -> Result<String, GitError> {
            self.responses.pop_front().unwrap_or_else(|| {
                Err(GitError::CommandFailed("missing fake response".to_string()))
            })
        }
    }

    #[test]
    fn parses_porcelain_v2_snapshot_and_counts_file_buckets() {
        let repo_root = PathBuf::from("/workspace/aegisai");
        let mut control = GitControl::new(FakeGitRunner::new(vec![
            Ok(repo_root.display().to_string()),
            Ok([
                "# branch.oid 0123456789abcdef0123456789abcdef01234567",
                "# branch.head main",
                "# branch.upstream origin/main",
                "# branch.ab +2 -1",
                "1 M. N... 100644 100644 100644 abcdef abcdef src/main.rs",
                "1 .M N... 100644 100644 100644 abcdef abcdef README.md",
                "? docs/linux_vm_checklist.md",
                "! target",
            ]
            .join("\n")),
        ]));

        let snapshot = control.snapshot(&repo_root).expect("snapshot should parse");

        assert_eq!(snapshot.repo_root, repo_root);
        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        assert_eq!(
            snapshot.head_oid.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(snapshot.upstream.as_deref(), Some("origin/main"));
        assert_eq!(snapshot.ahead, 2);
        assert_eq!(snapshot.behind, 1);
        assert_eq!(snapshot.staged_files, 1);
        assert_eq!(snapshot.unstaged_files, 1);
        assert_eq!(snapshot.untracked_files, 1);
        assert_eq!(snapshot.ignored_files, 1);
        assert!(snapshot.is_dirty());
    }

    #[test]
    fn discover_repository_reports_non_repo_path() {
        let path = PathBuf::from("/workspace/not-a-repo");
        let mut control = GitControl::new(FakeGitRunner::new(vec![Err(GitError::NotRepository(
            path.clone(),
        ))]));

        let error = control
            .discover_repository(&path)
            .expect_err("missing git repo should surface");

        assert_eq!(error, GitError::NotRepository(path));
    }

    #[test]
    fn checkpoint_plan_sanitizes_label_and_embeds_head_prefix() {
        let snapshot = GitStatusSnapshot {
            repo_root: PathBuf::from("/workspace/aegisai"),
            branch: Some("main".to_string()),
            head_oid: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            staged_files: 1,
            ..GitStatusSnapshot::default()
        };

        let plan = GitCheckpointPlan::for_label(&snapshot, "Linux VM Baseline #1");

        assert_eq!(plan.label, "linux-vm-baseline-1");
        assert_eq!(plan.branch_name, "checkpoints/linux-vm-baseline-1");
        assert_eq!(
            plan.tag_name.as_deref(),
            Some("checkpoint/linux-vm-baseline-1")
        );
        assert_eq!(
            plan.commit_message,
            "checkpoint: linux-vm-baseline-1 @ 0123456"
        );
    }
}
