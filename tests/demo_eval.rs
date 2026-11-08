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
use keelson::engine::{Engine, ModelOverride, OpenRequest};
use keelson::rig::Rig;

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
    let Some(key) = std::env::var("DEEPSEEK_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("skipping: DEEPSEEK_API_KEY / ANTHROPIC_AUTH_TOKEN is not set");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("demo-workspace");
    copy_dir(&root.join("examples/demo-workspace"), &workspace).expect("copy the demo");

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
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir,
        store_path,
        interactions: Arc::new(DeferAllInteractionPort),
        deltas: None,
        model: Some(ModelOverride {
            provider: "openai-compatible".to_owned(),
            model: "deepseek-flash".to_owned(),
            base_url: Some("https://api.deepseek.com".to_owned()),
            api_key: key,
            context_window: Some(1_048_576),
        }),
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

    let repaired = std::fs::read_to_string(workspace.join("stats.py")).expect("read stats.py");
    assert!(
        !repaired.contains("len(values) - 1"),
        "stats.py was not repaired:\n{repaired}"
    );
}
