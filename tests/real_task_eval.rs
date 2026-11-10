//! Run one live turn on an external workspace.
//!
//! This is a manual harness, not a CI test. Copy a real project first; the
//! engine writes only inside that copy.
//!
//! ```text
//! KEELSON_TASK_WORKSPACE=... KEELSON_TASK_PROMPT=... \
//! KEELSON_TASK_PYTHON=.../.venv/Scripts/python.exe \
//!   cargo test --test real_task_eval -- --ignored --nocapture
//! ```
//!
//! `KEELSON_TASK_PYTHON` is optional; when set, its directory goes to the
//! front of `PATH` so the project's interpreter is used by both the model and
//! the host check.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::{Engine, OpenRequest, Store};
use keelson::rig::Rig;

mod common;

/// Simulates a user clicking "allow once" for an execution request. Only use
/// this on a trusted copy; a `shell` call is not sandboxed.
struct ApproveExecution;

/// A turn must not change the machine outside the copied workspace. Package
/// managers, process killers and installers are denied even though the rest of
/// the execution is approved.
fn changes_the_host(prompt: &str) -> bool {
    const BLOCKED: [&str; 11] = [
        "-m,pip",
        "pip install",
        "pip,install",
        "conda",
        "taskkill",
        "wmic",
        "cargo install",
        "npm install -g",
        "rustup",
        "winget",
        "choco",
    ];
    BLOCKED.iter().any(|needle| prompt.contains(needle))
}

#[async_trait]
impl InteractionPort for ApproveExecution {
    async fn request(
        &self,
        request: InteractionRequest,
        _cancel: &CancelToken,
    ) -> InteractionResponse {
        if request.kind != InteractionKind::PermissionApproval {
            return InteractionResponse::Deferred;
        }
        if changes_the_host(&request.prompt) {
            return InteractionResponse::Rejected {
                reason: "the harness denies commands that change the host".to_owned(),
            };
        }
        InteractionResponse::Approved {
            scope: GrantScope::Once,
        }
    }
}

#[test]
fn host_changing_commands_are_blocked() {
    assert!(changes_the_host(
        "Allow `shell` to execute `python -m pip install x`? command: args=-m,pip,install"
    ));
    assert!(changes_the_host("command: program=taskkill args=/PID,1,/F"));
    assert!(!changes_the_host(
        "command: program=python args=-m,pytest,-q"
    ));
}

#[tokio::test]
#[ignore = "runs a live turn on KEELSON_TASK_WORKSPACE"]
async fn run_the_task() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: set DEEPSEEK_API_KEY or KEELSON_EVAL_BASE_URL");
        return;
    };
    let workspace = PathBuf::from(
        std::env::var("KEELSON_TASK_WORKSPACE").expect("KEELSON_TASK_WORKSPACE is required"),
    );
    let prompt = std::env::var("KEELSON_TASK_PROMPT").expect("KEELSON_TASK_PROMPT is required");
    if let Some(python) = std::env::var_os("KEELSON_TASK_PYTHON") {
        let directory = Path::new(&python)
            .parent()
            .expect("KEELSON_TASK_PYTHON must name an interpreter")
            .to_path_buf();
        let mut paths = vec![directory];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        std::env::set_var("PATH", std::env::join_paths(paths).expect("a valid PATH"));
    }
    match std::process::Command::new("python")
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
    {
        Ok(output) if output.status.success() => {
            eprintln!("python: {}", String::from_utf8_lossy(&output.stdout).trim())
        }
        Ok(output) => eprintln!("python: exit code {:?}", output.status.code()),
        Err(error) => eprintln!("python: {error}"),
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state_dir = root.join("target/real-task-state");
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("state directory");
    let store_path = state_dir.join("session.json");
    let store_for_check = store_path.clone();

    eprintln!("workspace: {}", workspace.display());
    eprintln!("prompt: {prompt}");

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Run a real task.".to_owned(),
        profile: root.join("profiles/coding.toml"),
        extra_roots: Vec::new(),
        path: root.join("tests/fixtures/coding-rig.toml"),
    };
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir,
        store_path,
        interactions: Arc::new(ApproveExecution),
        deltas: None,
        model: Some(model),
        permission: None,
        acceptance: true,
        search_url: None,
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(&prompt, CancelToken::new())
        .await
        .expect("the turn completes");

    eprintln!("turn state: {:?}", outcome.state);
    eprintln!("turn error: {:?}", outcome.error);
    eprintln!("model calls: {}", outcome.model_calls);
    if let Some(report) = &outcome.verification {
        eprintln!("verification passed: {}", report.passed());
        for evidence in &report.evidence {
            eprintln!(
                "verify {} ({}): {}",
                evidence.item_id, evidence.verifier, evidence.detail
            );
        }
    } else {
        eprintln!("no verification report");
    }

    let canonical = std::fs::canonicalize(&workspace).expect("canonical workspace");
    let session = Store::open(store_for_check).load(&canonical).ok().flatten();
    if let Some(session) = &session {
        for turn in &session.turns {
            for tool in &turn.tools {
                let args: String = tool
                    .args
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(180)
                    .collect();
                eprintln!("tool {} ok={:?} args={args}", tool.name, tool.ok);
            }
        }
    }

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    assert!(
        outcome
            .verification
            .as_ref()
            .is_some_and(|report| report.passed()),
        "automatic verification did not pass"
    );
}
