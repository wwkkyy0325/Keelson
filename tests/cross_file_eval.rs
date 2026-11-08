//! Live cross-file edit eval.
//!
//! `examples/cross-file-workspace` exports `add` from `src/math.js` and uses
//! it in `src/report.js`. The task renames it to `sum` in both files, and the
//! test asserts the model used `apply_edits` and left `test.js` alone.
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo test --test cross_file_eval -- --ignored --nocapture
//! ```

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use keel_core::CancelToken;
use keel_runtime::DeferAllInteractionPort;
use keelson::engine::{Engine, ModelOverride, OpenRequest, Store};
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

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_cross_file_rename_uses_apply_edits() {
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
    let workspace = temp.path().join("cross-file-workspace");
    copy_dir(&root.join("examples/cross-file-workspace"), &workspace).expect("copy the example");
    let test_before = std::fs::read_to_string(workspace.join("test.js")).expect("the test");

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Rename a function across files.".to_owned(),
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
            "Rename the exported `add` function to `sum` in `src/math.js` and \
             `src/report.js`. Use `apply_edits` so both replacements are validated \
             together. Do not change `test.js`. Report the real result.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

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

    let math = std::fs::read_to_string(workspace.join("src/math.js")).expect("math");
    let report = std::fs::read_to_string(workspace.join("src/report.js")).expect("report");
    assert!(math.contains("function sum"), "{math}");
    assert!(!math.contains("function add"), "{math}");
    assert!(report.contains("sum"), "{report}");
    assert!(!report.contains("add"), "{report}");
    let test_after = std::fs::read_to_string(workspace.join("test.js")).expect("the test");
    assert_eq!(test_before, test_after, "the model changed the test");

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
    assert!(
        tools.contains(&"apply_edits"),
        "the model did not use apply_edits: {tools:?}"
    );

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let check = Command::new(npm)
        .arg("test")
        .current_dir(&workspace)
        .output()
        .expect("run the test");
    assert!(
        check.status.success(),
        "the test still fails:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}
