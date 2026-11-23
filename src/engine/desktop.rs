//! Desktop process and window tools.
//!
//! `launch_app` starts an application and `wait_for_window` waits until its
//! window exists, so a verification step can look at a fresh screen. Neither
//! touches the mouse or the keyboard.

use std::path::{Path, PathBuf};
use std::process::Command;

use async_trait::async_trait;
use keel_core::{
    AccessKind, ArgValue, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool, ToolArgs,
    ToolCtx, ToolError, ToolResult, ToolSchema,
};

/// Long enough for a cold start, short enough that a missing window is
/// reported instead of hanging the turn.
const DEFAULT_WAIT_MS: u64 = 10_000;
const MAX_WAIT_MS: u64 = 30_000;
const WAIT_STEP_MS: u64 = 250;

/// Starts an application and returns its process id.
pub struct LaunchAppTool;

impl LaunchAppTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for LaunchAppTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "launch_app".to_owned(),
            description: "Start an application and return its process id. The process is not \
                          tied to the session and keeps running; the user can close it. Use \
                          wait_for_window before looking at its window."
                .to_owned(),
            risk: RiskLevel::Mutating,
            params: vec![
                ParamSpec {
                    name: "program".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "args".to_owned(),
                    kind: ParamKind::StringList,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        let program = args.require_str("program")?;
        // A relative path is resolved by the permission gate against the
        // workspace, the same way the call resolves it. A bare name has to be
        // located on PATH here, or the gate would look for it in the
        // workspace.
        let has_separator = program.contains('/') || program.contains('\\');
        let resolved = if Path::new(program).is_absolute() || has_separator {
            PathBuf::from(program)
        } else {
            resolve_program(Path::new("."), program)
                .ok_or_else(|| ToolError::not_found(format!("`{program}` was not found on PATH")))?
        };
        Ok(vec![PermissionRequest {
            tool: "launch_app".to_owned(),
            access: AccessKind::Execute,
            path: Some(resolved),
            detail: Some(program.to_owned()),
        }])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let program = match args.require_str("program") {
            Ok(program) => program,
            Err(error) => return ToolResult::failure(ctx.call_id, "launch_app", error),
        };
        let resolved = match resolve_program(&ctx.workspace_root, program) {
            Some(resolved) => resolved,
            None => {
                return ToolResult::failure(
                    ctx.call_id,
                    "launch_app",
                    ToolError::not_found(format!("`{program}` was not found")),
                );
            }
        };
        let extra: Vec<String> = match args.get("args") {
            Some(ArgValue::StringList(values)) => values.clone(),
            Some(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "launch_app",
                    ToolError::invalid_arguments("`args` must be a list of strings"),
                );
            }
            None => Vec::new(),
        };
        let mut command = Command::new(&resolved);
        command.args(&extra).current_dir(&ctx.workspace_root);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // A GUI app must not inherit the host's console, and its own
            // process group keeps a Ctrl+C in a terminal from reaching it.
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        match command.spawn() {
            Ok(child) => ToolResult::success(
                ctx.call_id,
                "launch_app",
                format!(
                    "launched {} as pid {}; use wait_for_window before capturing its window",
                    resolved.display(),
                    child.id()
                ),
            ),
            Err(error) => ToolResult::failure(
                ctx.call_id,
                "launch_app",
                ToolError::io(format!("cannot launch {}: {error}", resolved.display())),
            ),
        }
    }
}

/// Lists the UI Automation controls inside a window. The Automation API is
/// Windows-only, so the tool exists only there.
#[cfg(windows)]
pub struct ListControlsTool;

#[cfg(windows)]
impl ListControlsTool {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
const MAX_CONTROLS: usize = 200;
#[cfg(windows)]
const MAX_CONTROL_DEPTH: usize = 6;

#[cfg(windows)]
#[async_trait]
impl Tool for ListControlsTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "list_controls".to_owned(),
            description: "List the UI Automation controls inside a window: control type, \
                          name, class and screen bounds, up to 200 elements. Use it to find a \
                          labelled control or read structured state instead of guessing from \
                          pixels. `window` is a title fragment from list_windows."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![ParamSpec {
                name: "window".to_owned(),
                kind: ParamKind::String,
                required: true,
            }],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(Vec::new())
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let title = match args.require_str("window") {
            Ok(title) => title.trim().to_owned(),
            Err(error) => return ToolResult::failure(ctx.call_id, "list_controls", error),
        };
        if title.is_empty() {
            return ToolResult::failure(
                ctx.call_id,
                "list_controls",
                ToolError::invalid_arguments("`window` must not be empty"),
            );
        }
        match list_controls(&title) {
            Ok(text) => ToolResult::success(ctx.call_id, "list_controls", text),
            Err(error) => ToolResult::failure(ctx.call_id, "list_controls", ToolError::io(error)),
        }
    }
}

#[cfg(windows)]
fn list_controls(title: &str) -> Result<String, String> {
    use uiautomation::types::ControlType;
    use uiautomation::{UIAutomation, UIElement, UITreeWalker};

    fn walk(walker: &UITreeWalker, element: &UIElement, depth: usize, out: &mut Vec<String>) {
        if depth > MAX_CONTROL_DEPTH || out.len() >= MAX_CONTROLS {
            return;
        }
        let Some(children) = walker.get_children(element) else {
            return;
        };
        for child in children {
            if out.len() >= MAX_CONTROLS {
                return;
            }
            let control = child
                .get_control_type()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_default();
            let name = child.get_name().unwrap_or_default();
            let class = child.get_classname().unwrap_or_default();
            let bounds = child
                .get_bounding_rectangle()
                .map(|rect| {
                    format!(
                        "{},{} {}x{}",
                        rect.get_left(),
                        rect.get_top(),
                        rect.get_right() - rect.get_left(),
                        rect.get_bottom() - rect.get_top()
                    )
                })
                .unwrap_or_default();
            out.push(format!(
                "{}{} name=\"{name}\" class={class} bounds={bounds}",
                "  ".repeat(depth),
                if control.is_empty() {
                    "?"
                } else {
                    control.as_str()
                }
            ));
            walk(walker, &child, depth + 1, out);
        }
    }

    let automation =
        UIAutomation::new().map_err(|error| format!("cannot start UI Automation: {error}"))?;
    let window = automation
        .create_matcher()
        .control_type(ControlType::Window)
        .contains_name(title)
        .find_first()
        .map_err(|error| format!("no window matches `{title}`: {error}"))?;
    let walker = automation
        .get_control_view_walker()
        .map_err(|error| format!("cannot read the control tree: {error}"))?;
    let mut lines = Vec::new();
    walk(&walker, &window, 0, &mut lines);
    if lines.is_empty() {
        return Ok(format!("no controls found in `{title}`"));
    }
    Ok(lines.join("\n"))
}

/// Waits until a window whose title contains `title` is visible.
pub struct WaitForWindowTool;

impl WaitForWindowTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WaitForWindowTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "wait_for_window".to_owned(),
            description: "Wait until a visible window whose title contains `title` exists, then \
                          return its id and size. Use it after launch_app, before screen_capture."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![
                ParamSpec {
                    name: "title".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "timeout_ms".to_owned(),
                    kind: ParamKind::Integer,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(Vec::new())
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let title = match args.require_str("title") {
            Ok(title) => title.trim().to_lowercase(),
            Err(error) => return ToolResult::failure(ctx.call_id, "wait_for_window", error),
        };
        if title.is_empty() {
            return ToolResult::failure(
                ctx.call_id,
                "wait_for_window",
                ToolError::invalid_arguments("`title` must not be empty"),
            );
        }
        let timeout = match args.get("timeout_ms") {
            Some(ArgValue::Integer(value)) => (*value).clamp(500, MAX_WAIT_MS as i64) as u64,
            Some(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "wait_for_window",
                    ToolError::invalid_arguments("`timeout_ms` must be an integer"),
                );
            }
            None => DEFAULT_WAIT_MS,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout);
        loop {
            if ctx.cancel.is_cancelled() {
                return ToolResult::failure(ctx.call_id, "wait_for_window", ToolError::cancelled());
            }
            if let Ok(found) = find_window(&title) {
                return ToolResult::success(ctx.call_id, "wait_for_window", found);
            }
            if std::time::Instant::now() >= deadline {
                return ToolResult::failure(
                    ctx.call_id,
                    "wait_for_window",
                    ToolError::deadline(std::time::Duration::from_millis(timeout)),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(WAIT_STEP_MS)).await;
        }
    }
}

fn find_window(title: &str) -> Result<String, String> {
    let windows = xcap::Window::all().map_err(|error| format!("cannot list windows: {error}"))?;
    for window in windows {
        let candidate = window.title().unwrap_or_default();
        let minimized = window.is_minimized().unwrap_or(false);
        if minimized || !candidate.to_lowercase().contains(title) {
            continue;
        }
        return Ok(format!(
            "window \"{candidate}\" id={} app={} size={}x{}",
            window.id().unwrap_or(0),
            window.app_name().unwrap_or_default(),
            window.width().unwrap_or(0),
            window.height().unwrap_or(0)
        ));
    }
    Err(format!("no window matches `{title}`"))
}

/// An absolute path for `program`. A bare name is looked up on `PATH`, with
/// the executable extensions on Windows.
fn resolve_program(workspace: &Path, program: &str) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    if program.contains('/') || program.contains('\\') {
        let relative = workspace.join(candidate);
        return relative.is_file().then_some(relative);
    }
    let path = std::env::var_os("PATH")?;
    // On Windows a bare name is looked up as a program, not as a file: an
    // extensionless `notepad` on PATH is often a shell script that cannot be
    // launched. A name that already carries an extension is taken literally.
    let named_with_extension = candidate.extension().is_some();
    let extensions: &[&str] = if cfg!(windows) && !named_with_extension {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for directory in std::env::split_paths(&path) {
        for extension in extensions {
            let candidate = directory.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_resolved_absolutely() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(temp.path().join("work")).expect("the folder");
        let script = temp.path().join("work").join("app.bat");
        std::fs::write(&script, "@echo off\n").expect("the program");

        assert_eq!(
            resolve_program(temp.path(), script.to_str().expect("a utf-8 path")),
            Some(script.clone())
        );
        assert_eq!(
            resolve_program(temp.path(), "work/app.bat"),
            Some(script.clone())
        );
        assert!(resolve_program(temp.path(), "missing-app-xyz").is_none());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "needs an interactive desktop session"]
    fn the_control_tree_of_a_window_is_listed() {
        let windows = xcap::Window::all().expect("windows");
        let title = windows
            .iter()
            .filter_map(|window| window.title().ok())
            .find(|title| !title.trim().is_empty())
            .expect("a visible window");

        let listing = list_controls(&title).expect("the control tree");

        assert!(!listing.is_empty(), "{title}");
    }

    #[test]
    fn a_launch_asks_to_execute_the_resolved_program() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let script = temp.path().join("app.bat");
        std::fs::write(&script, "@echo off\n").expect("the program");

        let mut args = ToolArgs::new();
        args.insert(
            "program",
            ArgValue::String(script.to_str().expect("a utf-8 path").to_owned()),
        );
        let requests = LaunchAppTool::new()
            .permission_requests(&args)
            .expect("a request");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].access, AccessKind::Execute);
        assert_eq!(requests[0].path.as_deref(), Some(script.as_path()));
    }

    fn ctx(workspace: &std::path::Path) -> ToolCtx {
        ToolCtx {
            turn_id: keel_core::TurnId::new(1),
            call_id: keel_core::ToolCallId::new(1),
            workspace_root: workspace.to_path_buf(),
            cancel: keel_core::CancelToken::new(),
        }
    }

    fn between<'a>(line: &'a str, start: &str, end: &str) -> Option<&'a str> {
        let from = line.find(start)? + start.len();
        let rest = &line[from..];
        let to = rest.find(end).unwrap_or(rest.len());
        Some(rest[..to].trim())
    }

    struct ClosePid(u32);

    impl Drop for ClosePid {
        fn drop(&mut self) {
            // The guard only has work on Windows, where the probe launches a
            // program it must close; elsewhere the id is still read so the
            // field is not dead code on those targets.
            #[cfg(not(windows))]
            let _ = self.0;
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &self.0.to_string(), "/F"])
                    .status();
            }
        }
    }

    #[tokio::test]
    #[ignore = "needs an interactive desktop session"]
    async fn the_desktop_tools_work_end_to_end() {
        use crate::engine::screen::{ListWindowsTool, ScreenCaptureTool, ViewImageTool};

        let temp = tempfile::TempDir::new().expect("a temp dir");
        let screens = temp.path().join("screens");
        let context = ctx(temp.path());

        // What is visible before the launch, so the new window is unambiguous.
        let before = ListWindowsTool::new()
            .call(ToolArgs::new(), &context)
            .await
            .content()
            .expect("the window list")
            .to_owned();
        let before_ids: Vec<String> = before
            .lines()
            .filter_map(|line| between(line, "id=", " app=").map(str::to_owned))
            .collect();

        let mut launch_args = ToolArgs::new();
        // Paint opens a blank canvas with no session state, so closing it
        // cannot lose anything.
        launch_args.insert("program", ArgValue::String("mspaint".to_owned()));
        let launched = LaunchAppTool::new().call(launch_args, &context).await;
        if !launched.is_success() {
            eprintln!(
                "skipping: mspaint is not available: {:?}",
                launched.content()
            );
            return;
        }
        let pid = launched
            .content()
            .and_then(|text| text.split("as pid ").nth(1))
            .and_then(|rest| rest.split(';').next())
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            .expect("a pid");
        let _close = ClosePid(pid);

        // Wait for the new window, up to 20 s.
        let mut found: Option<(String, String)> = None;
        for _ in 0..80 {
            let listing = ListWindowsTool::new()
                .call(ToolArgs::new(), &context)
                .await
                .content()
                .unwrap_or_default()
                .to_owned();
            for line in listing.lines() {
                let Some(id) = between(line, "id=", " app=") else {
                    continue;
                };
                if before_ids.iter().any(|known| known == id) {
                    continue;
                }
                let title = between(line, "title=", " size=")
                    .unwrap_or_default()
                    .to_owned();
                if !title.trim().is_empty() {
                    found = Some((id.to_owned(), title));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let (window_id, title) = found.expect("a new window shortly after the launch");

        // wait_for_window finds the same window by title.
        let mut wait_args = ToolArgs::new();
        wait_args.insert("title", ArgValue::String(title.clone()));
        let waited = WaitForWindowTool::new().call(wait_args, &context).await;
        assert!(waited.is_success(), "{waited:?}");

        // The capture carries an image and a saved file.
        let mut capture_args = ToolArgs::new();
        capture_args.insert("target", ArgValue::String("window".to_owned()));
        capture_args.insert("window", ArgValue::String(window_id.clone()));
        let captured = ScreenCaptureTool::new(screens.clone())
            .call(capture_args, &context)
            .await;
        assert!(captured.is_success(), "{captured:?}");
        assert_eq!(captured.images.len(), 1, "the model gets the pixels");
        let saved = std::fs::read_dir(&screens)
            .expect("the screens folder")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .next()
            .expect("a saved screenshot");
        assert!(saved.is_file());

        // The control tree of the new window is readable and non-empty.
        // `list_controls` reads Windows UI Automation; the type does not exist
        // on other platforms, so neither does this assertion.
        #[cfg(windows)]
        {
            let mut control_args = ToolArgs::new();
            control_args.insert("window", ArgValue::String(title.clone()));
            let controls = ListControlsTool::new().call(control_args, &context).await;
            assert!(controls.is_success(), "{controls:?}");
            assert!(
                !controls.content().unwrap_or_default().trim().is_empty(),
                "the control list is empty for `{title}`"
            );
        }

        // view_image reopens the saved screenshot.
        let mut view_args = ToolArgs::new();
        view_args.insert(
            "path",
            ArgValue::String(saved.to_string_lossy().into_owned()),
        );
        let viewed = ViewImageTool::new().call(view_args, &context).await;
        assert!(viewed.is_success(), "{viewed:?}");
        assert_eq!(viewed.images.len(), 1);
    }
}
