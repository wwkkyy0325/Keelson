//! A read-only git tool for reviewing the working tree.
//!
//! The shell tool can run git, but execution is confirm-only. Status and an
//! unstaged diff are read operations and are useful before a turn reports
//! done, so the host exposes them without a permission card.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use keel_core::{
    AccessKind, ArgValue, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool, ToolArgs,
    ToolCtx, ToolError, ToolResult,
};

const MAX_GIT_OUTPUT_BYTES: usize = 32 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(15);

/// Shows the workspace status or its unstaged diff.
pub struct GitDiffTool;

impl GitDiffTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitDiffTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn schema(&self) -> keel_core::ToolSchema {
        keel_core::ToolSchema {
            name: "git_diff".to_owned(),
            description: "Show the workspace git status (`mode = \"status\"`) or its unstaged \
                          diff (`mode = \"diff\"`, the default). `path` limits the output. \
                          Read-only; no confirmation."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![
                ParamSpec {
                    name: "mode".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
                ParamSpec {
                    name: "path".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        let path = match args.get("path") {
            Some(ArgValue::String(value)) => value.as_str(),
            Some(_) => {
                return Err(ToolError::invalid_arguments(
                    "argument `path` must be string",
                ))
            }
            None => ".",
        };
        Ok(vec![PermissionRequest {
            tool: "git_diff".to_owned(),
            access: AccessKind::Read,
            path: Some(std::path::PathBuf::from(path)),
            detail: None,
        }])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        if ctx.cancel.is_cancelled() {
            return ToolResult::failure(ctx.call_id, "git_diff", ToolError::cancelled());
        }
        let mode = match args.get("mode") {
            None => "diff",
            Some(ArgValue::String(value)) if value == "status" || value == "diff" => value.as_str(),
            Some(ArgValue::String(_)) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "git_diff",
                    ToolError::invalid_arguments("argument `mode` must be `status` or `diff`"),
                )
            }
            Some(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "git_diff",
                    ToolError::invalid_arguments("argument `mode` must be string"),
                )
            }
        };
        let path = match args.get("path") {
            None => None,
            Some(ArgValue::String(value)) => Some(value.as_str()),
            Some(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "git_diff",
                    ToolError::invalid_arguments("argument `path` must be string"),
                )
            }
        };
        let root = ctx.workspace_root.as_path();
        if !is_repository(root).await {
            return ToolResult::success(ctx.call_id, "git_diff", "not a git repository".to_owned());
        }
        let mut command = quiet_git();
        command
            .arg("-c")
            .arg("color.ui=false")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(root)
            .arg(mode);
        match mode {
            "status" => {
                command.arg("--short").arg("--untracked-files=all");
            }
            _ => {
                command.arg("--no-ext-diff");
            }
        }
        if let Some(path) = path {
            command.arg("--").arg(path);
        }
        let output = match tokio::time::timeout(GIT_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "git_diff",
                    ToolError::io(format!("cannot run git: {error}")),
                )
            }
            Err(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "git_diff",
                    ToolError::timeout(GIT_TIMEOUT),
                )
            }
        };
        if !output.status.success() {
            return ToolResult::failure(
                ctx.call_id,
                "git_diff",
                ToolError::io(format!(
                    "git {mode} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )),
            );
        }
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        if text.trim().is_empty() {
            text.push_str("(no changes)\n");
        }
        let truncated = text.len() > MAX_GIT_OUTPUT_BYTES;
        if truncated {
            let mut end = MAX_GIT_OUTPUT_BYTES;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str("\n... output truncated");
        }
        ToolResult::success(ctx.call_id, "git_diff", text)
    }
}

/// A GUI host has no console; git must not flash one for each probe.
fn quiet_git() -> tokio::process::Command {
    // Only the Windows flag needs the binding to be mutable.
    #[allow(unused_mut)]
    let mut command = tokio::process::Command::new("git");
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
}

async fn is_repository(root: &Path) -> bool {
    let mut command = quiet_git();
    command
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--is-inside-work-tree");
    match tokio::time::timeout(GIT_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output.status.success(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::{CancelToken, ToolCallId, TurnId};

    fn ctx(root: &Path) -> ToolCtx {
        ToolCtx {
            turn_id: TurnId::new(1),
            call_id: ToolCallId::new(1),
            workspace_root: root.to_path_buf(),
            cancel: CancelToken::new(),
        }
    }

    fn git(root: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        git(dir.path(), &["init", "--quiet"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("notes.txt"), "old\n").expect("a file");
        git(dir.path(), &["add", "notes.txt"]);
        git(dir.path(), &["commit", "--quiet", "-m", "init"]);
        std::fs::write(dir.path().join("notes.txt"), "new\n").expect("a change");
        dir
    }

    #[tokio::test]
    async fn shows_the_unstaged_diff() {
        let dir = repository();
        let tool = GitDiffTool::new();

        let result = tool.call(ToolArgs::new(), &ctx(dir.path())).await;

        assert!(result.is_success(), "{result:?}");
        let content = result.content().unwrap();
        assert!(content.contains("-old"), "{content}");
        assert!(content.contains("+new"), "{content}");
    }

    #[tokio::test]
    async fn shows_status_and_limits_the_path() {
        let dir = repository();
        std::fs::write(dir.path().join("other.txt"), "other\n").expect("a file");
        let tool = GitDiffTool::new();
        let mut args = ToolArgs::new();
        args.insert("mode", ArgValue::String("status".to_owned()));
        args.insert("path", ArgValue::String("notes.txt".to_owned()));

        let result = tool.call(args, &ctx(dir.path())).await;

        assert!(result.is_success(), "{result:?}");
        let content = result.content().unwrap();
        assert!(content.contains("notes.txt"), "{content}");
        assert!(!content.contains("other.txt"), "{content}");
    }

    #[tokio::test]
    async fn reports_a_directory_that_is_not_a_repository() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let tool = GitDiffTool::new();

        let result = tool.call(ToolArgs::new(), &ctx(dir.path())).await;

        assert!(result.is_success());
        assert_eq!(result.content(), Some("not a git repository"));
    }

    #[tokio::test]
    async fn rejects_an_unknown_mode() {
        let dir = repository();
        let tool = GitDiffTool::new();
        let mut args = ToolArgs::new();
        args.insert("mode", ArgValue::String("log".to_owned()));

        let result = tool.call(args, &ctx(dir.path())).await;

        assert_eq!(
            result.error().map(|error| error.code),
            Some(keel_core::ErrorCode::InvalidArguments)
        );
    }
}
