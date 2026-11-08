//! Run one live turn on an external workspace.
//!
//! This is a manual harness, not a CI test. Copy a real project first; the
//! engine writes only inside that copy.
//!
//! ```text
//! KEELSON_TASK_WORKSPACE=... KEELSON_TASK_PROMPT=... \
//!   cargo test --test real_task_eval -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use keel_core::CancelToken;
use keel_runtime::DeferAllInteractionPort;
use keelson::engine::{Engine, ModelOverride, OpenRequest, Store};
use keelson::rig::Rig;

#[tokio::test]
#[ignore = "runs a live turn on KEELSON_TASK_WORKSPACE"]
async fn run_the_task() {
    let Some(key) = std::env::var("DEEPSEEK_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("skipping: DEEPSEEK_API_KEY / ANTHROPIC_AUTH_TOKEN is not set");
        return;
    };
    let workspace = PathBuf::from(
        std::env::var("KEELSON_TASK_WORKSPACE").expect("KEELSON_TASK_WORKSPACE is required"),
    );
    let prompt = std::env::var("KEELSON_TASK_PROMPT").expect("KEELSON_TASK_PROMPT is required");
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
        path: root.join("rigs/coding.toml"),
    };
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
