//! Live multi-file coding eval.
//!
//! The workspace is `examples/inventory-workspace`: a small Python package
//! whose tests already describe a missing discount feature. The model must
//! change `service.py` and `report.py` without touching the tests, and the
//! automatic Python check must pass.
//!
//! This test needs a real provider key; it is ignored by default.
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo test --test coding_eval -- --ignored --nocapture
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

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_discount_is_added_across_two_modules() {
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
    let workspace = temp.path().join("inventory-workspace");
    copy_dir(&root.join("examples/inventory-workspace"), &workspace).expect("copy the example");
    let tests_before =
        std::fs::read_to_string(workspace.join("tests/test_service.py")).expect("the tests");

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Add the discount feature.".to_owned(),
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
            "The tests in this directory fail because the discount feature is missing. \
             Add a `discount` parameter to `total` and `render` so a 10% discount is \
             applied before tax. Do not change the test files. Run the tests and report \
             the real result.",
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

    let check = Command::new(python())
        .args(["-m", "unittest", "discover", "-v"])
        .current_dir(&workspace)
        .output()
        .expect("run the tests");
    assert!(
        check.status.success(),
        "tests still fail:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let tests_after =
        std::fs::read_to_string(workspace.join("tests/test_service.py")).expect("the tests");
    assert_eq!(
        tests_before, tests_after,
        "the model changed the visible tests"
    );

    let service = std::fs::read_to_string(workspace.join("inventory/service.py")).expect("service");
    let report = std::fs::read_to_string(workspace.join("inventory/report.py")).expect("report");
    assert!(
        service.contains("discount"),
        "service.py was not changed:\n{service}"
    );
    assert!(
        report.contains("discount"),
        "report.py was not changed:\n{report}"
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
    eprintln!("model calls: {}", outcome.model_calls);
    assert!(
        tools.contains(&"read_file"),
        "the model did not read a file: {tools:?}"
    );
    assert!(
        tools
            .iter()
            .any(|name| matches!(*name, "edit_file" | "multi_edit" | "write_file")),
        "the model did not use an edit tool: {tools:?}"
    );
}
