//! Live polyglot check-selection eval.
//!
//! `examples/polyglot-workspace` has a passing Node test and a deliberately
//! failing Python test. A frontend-only edit must select the Node check and
//! skip Python; the report has exactly one passing item.
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo test --test polyglot_eval -- --ignored --nocapture
//! ```

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use keel_core::CancelToken;
use keel_runtime::DeferAllInteractionPort;
use keelson::engine::{Engine, OpenRequest};
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

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_frontend_change_skips_the_python_check() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: set DEEPSEEK_API_KEY or KEELSON_EVAL_BASE_URL");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("polyglot-workspace");
    copy_dir(&root.join("examples/polyglot-workspace"), &workspace).expect("copy the example");
    let frontend_test =
        std::fs::read_to_string(workspace.join("frontend/test.js")).expect("the frontend test");
    let backend_test = std::fs::read_to_string(workspace.join("backend/tests/test_fail.py"))
        .expect("the backend test");

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Fix the frontend test.".to_owned(),
        profile: root.join("profiles/coding.toml"),
        extra_roots: Vec::new(),
        path: root.join("tests/fixtures/coding-rig.toml"),
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
        model: Some(model),
        permission: None,
        acceptance: true,
        search_url: None,
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(
            "The Node test in `frontend/` fails: `add(2, 3)` returns the wrong value. \
             Fix `frontend/app.js` so the test passes. Do not change `frontend/test.js` \
             and do not touch `backend/`. Report the real result.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    let report = outcome
        .verification
        .as_ref()
        .expect("a verification report");
    assert!(report.passed(), "verification failed: {report:?}");
    assert_eq!(
        report.evidence.len(),
        1,
        "only the frontend check should run: {report:?}"
    );
    let detail = &report.evidence[0].detail;
    assert!(
        detail.contains("ok"),
        "the Node check did not run:\n{detail}"
    );
    assert!(
        !detail.contains("the backend check must not run"),
        "the Python check ran:\n{detail}"
    );

    let frontend_after =
        std::fs::read_to_string(workspace.join("frontend/test.js")).expect("the frontend test");
    assert_eq!(
        frontend_test, frontend_after,
        "the model changed the frontend test"
    );
    let backend_after = std::fs::read_to_string(workspace.join("backend/tests/test_fail.py"))
        .expect("the backend test");
    assert_eq!(
        backend_test, backend_after,
        "the model changed the backend test"
    );

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let check = Command::new(npm)
        .args(["--prefix", "frontend", "test"])
        .current_dir(&workspace)
        .output()
        .expect("run the frontend test");
    assert!(
        check.status.success(),
        "the frontend test still fails:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}
