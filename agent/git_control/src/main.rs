use std::env;
use std::path::PathBuf;

use aegisai_git_control::{GitCheckpointPlan, GitControl, SystemGitCommandRunner};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse(env::args().skip(1))?;
    let mut control = GitControl::new(SystemGitCommandRunner);

    match cli.command {
        CommandKind::Status => {
            let snapshot = control.snapshot(&cli.path)?;
            println!("{}", render_status_snapshot(&snapshot));
        }
        CommandKind::Checkpoint { ref label } => {
            let snapshot = control.snapshot(&cli.path)?;
            let plan = GitCheckpointPlan::for_label(&snapshot, label);
            println!("{}", render_checkpoint_plan(&plan));
        }
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommandKind {
    Status,
    Checkpoint { label: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cli {
    path: PathBuf,
    command: CommandKind,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            path: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            command: CommandKind::Status,
        }
    }
}

impl Cli {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut cli = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--path" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--path expects a directory".to_string())?;
                    cli.path = PathBuf::from(value);
                }
                "status" => cli.command = CommandKind::Status,
                "checkpoint" => {
                    let mut label = None;
                    while let Some(next) = args.next() {
                        match next.as_str() {
                            "--label" => {
                                label = Some(args.next().ok_or_else(|| {
                                    "checkpoint --label expects a value".to_string()
                                })?);
                            }
                            other => {
                                return Err(format!(
                                    "unknown checkpoint argument `{other}`\n\n{}",
                                    Self::usage()
                                ));
                            }
                        }
                    }

                    cli.command = CommandKind::Checkpoint {
                        label: label.ok_or_else(|| {
                            "checkpoint requires --label <value>\n\n".to_string() + &Self::usage()
                        })?,
                    };
                    break;
                }
                "--help" | "-h" => return Err(Self::usage()),
                other => {
                    return Err(format!("unknown argument `{other}`\n\n{}", Self::usage()));
                }
            }
        }

        Ok(cli)
    }

    fn usage() -> String {
        [
            "Usage: aegisai-git-control [--path <dir>] <command>",
            "",
            "Commands:",
            "  status",
            "  checkpoint --label <value>",
        ]
        .join("\n")
    }
}

fn render_checkpoint_plan(plan: &GitCheckpointPlan) -> String {
    let mut lines = vec![
        format!("label: {}", plan.label),
        format!("branch: {}", plan.branch_name),
        format!("commit_message: {}", plan.commit_message),
    ];

    if let Some(tag_name) = &plan.tag_name {
        lines.push(format!("tag: {tag_name}"));
    }

    lines.join("\n")
}

fn render_status_snapshot(snapshot: &aegisai_git_control::GitStatusSnapshot) -> String {
    [
        format!("repo_root: {}", snapshot.repo_root.display()),
        format!(
            "branch: {}",
            snapshot.branch.as_deref().unwrap_or("(detached)")
        ),
        format!(
            "head: {}",
            snapshot.head_oid.as_deref().unwrap_or("(unknown)")
        ),
        format!(
            "upstream: {}",
            snapshot.upstream.as_deref().unwrap_or("(none)")
        ),
        format!("ahead: {}", snapshot.ahead),
        format!("behind: {}", snapshot.behind),
        format!("dirty: {}", snapshot.is_dirty()),
        format!("staged_files: {}", snapshot.staged_files),
        format!("unstaged_files: {}", snapshot.unstaged_files),
        format!("untracked_files: {}", snapshot.untracked_files),
        format!("ignored_files: {}", snapshot.ignored_files),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aegisai_git_control::GitStatusSnapshot;

    use super::{
        render_checkpoint_plan, render_status_snapshot, Cli, CommandKind, GitCheckpointPlan,
    };

    #[test]
    fn cli_parses_status_command_with_custom_path() {
        let cli = Cli::parse(
            ["--path", "/workspace/aegisai", "status"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(cli.path, PathBuf::from("/workspace/aegisai"));
        assert_eq!(cli.command, CommandKind::Status);
    }

    #[test]
    fn cli_parses_checkpoint_command() {
        let cli = Cli::parse(
            ["checkpoint", "--label", "Linux VM Baseline"]
                .into_iter()
                .map(str::to_string),
        )
        .expect("cli should parse");

        assert_eq!(
            cli.command,
            CommandKind::Checkpoint {
                label: "Linux VM Baseline".to_string()
            }
        );
    }

    #[test]
    fn checkpoint_rendering_includes_branch_and_commit_message() {
        let snapshot = GitStatusSnapshot {
            repo_root: PathBuf::from("/workspace/aegisai"),
            branch: Some("main".to_string()),
            head_oid: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            ..GitStatusSnapshot::default()
        };

        let plan = GitCheckpointPlan::for_label(&snapshot, "Linux VM Baseline");
        let rendered = render_checkpoint_plan(&plan);

        assert!(rendered.contains("label: linux-vm-baseline"));
        assert!(rendered.contains("branch: checkpoints/linux-vm-baseline"));
        assert!(rendered.contains("commit_message: checkpoint: linux-vm-baseline @ 0123456"));
    }

    #[test]
    fn status_rendering_includes_dirty_counts() {
        let snapshot = GitStatusSnapshot {
            repo_root: PathBuf::from("/workspace/aegisai"),
            branch: Some("main".to_string()),
            head_oid: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 1,
            staged_files: 1,
            unstaged_files: 2,
            untracked_files: 3,
            ignored_files: 4,
        };

        let rendered = render_status_snapshot(&snapshot);

        assert!(rendered.contains("repo_root: /workspace/aegisai"));
        assert!(rendered.contains("branch: main"));
        assert!(rendered.contains("ahead: 2"));
        assert!(rendered.contains("behind: 1"));
        assert!(rendered.contains("dirty: true"));
        assert!(rendered.contains("staged_files: 1"));
        assert!(rendered.contains("unstaged_files: 2"));
        assert!(rendered.contains("untracked_files: 3"));
    }
}
