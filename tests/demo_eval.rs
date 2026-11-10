//! M4 acceptance eval: repair the demo workspace with a live model.
//!
//! This test needs a real provider key; it is ignored by default.
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo test --test demo_eval -- --ignored --nocapture
//! ```
//!
//! `ANTHROPIC_AUTH_TOKEN` is accepted as a fallback. The eval uses the
//! OpenAI-compatible endpoint, the same wire path as the desktop app; the
//! Anthropic-compatible DeepSeek path currently rejects thinking blocks on a
//! tool round.

use std::path::Path;
use std::sync::Arc;

use keel_core::CancelToken;
use keel_runtime::DeferAllInteractionPort;
use keelson::engine::{Engine, OpenRequest, Store};
use keelson::rig::Rig;

mod common;

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &target)?;
        } else {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

/// Makes the demo look like a Windows checkout: every Python file uses CRLF.
/// The model's snippets normally use LF, so this exercises the line-ending
/// fallback and the read-before-write guard.
fn use_crlf(directory: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            use_crlf(&path)?;
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("py") {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        let crlf = text.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(&path, crlf)?;
    }
    Ok(())
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn demo_workspace_is_repaired_by_the_coding_rig() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: set DEEPSEEK_API_KEY or KEELSON_EVAL_BASE_URL");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("demo-workspace");
    copy_dir(&root.join("examples/demo-workspace"), &workspace).expect("copy the demo");
    use_crlf(&workspace).expect("convert the demo to CRLF");

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Repair the demo workspace.".to_owned(),
        profile: root.join("profiles/coding.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/coding.toml"),
    };
    let state_dir = temp.path().join("state");
    let store_path = state_dir.join("session.json");
    let store_for_check = store_path.clone();
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir,
        store_path,
        interactions: Arc::new(DeferAllInteractionPort),
        deltas: None,
        model: Some(model),
        permission: None,
        acceptance: true,
        search_url: None,
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(
            "The tests in this directory fail. Run them, fix the code, and run them again.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );

    let check = std::process::Command::new(python())
        .args(["-m", "unittest", "discover", "-v"])
        .current_dir(&workspace)
        .output()
        .expect("run the demo tests");
    assert!(
        check.status.success(),
        "demo tests still fail:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let canonical = std::fs::canonicalize(&workspace).expect("canonical workspace");
    let session = Store::open(store_for_check)
        .load(&canonical)
        .expect("load the session")
        .expect("the session exists");
    let tools: Vec<&str> = session
        .turns
        .iter()
        .flat_map(|turn| turn.tools.iter().map(|tool| tool.name.as_str()))
        .collect();
    eprintln!("tools used: {tools:?}");

    let repaired = std::fs::read_to_string(workspace.join("stats.py")).expect("read stats.py");
    eprintln!(
        "stats.py line endings after the turn: {}",
        if repaired.contains("\r\n") {
            "CRLF"
        } else {
            "LF"
        }
    );
    assert!(
        !repaired.contains("len(values) - 1"),
        "stats.py was not repaired:\n{repaired}"
    );
}
