//! Live eval on a large file with reformatting damage.
//!
//! `examples/messy-workspace` has a 1274-line Python module with two broken
//! indentation regions and a 570-line Java class with two unbalanced braces.
//! The Python check is detected from `tests/`; the Java check is declared in
//! `.keelson/checks.toml`. A single turn must repair both without touching the
//! reference test or `Main.java`, and the report must carry both checks.
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo test --test messy_eval -- --ignored --nocapture
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
async fn a_large_messy_workspace_is_repaired() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: set DEEPSEEK_API_KEY or KEELSON_EVAL_BASE_URL");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("messy-workspace");
    copy_dir(&root.join("examples/messy-workspace"), &workspace).expect("copy the example");
    let test_before =
        std::fs::read_to_string(workspace.join("tests/test_engine.py")).expect("the test");
    let main_before = std::fs::read_to_string(workspace.join("src/Main.java")).expect("main");

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Repair a large file with formatting damage.".to_owned(),
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

    let started = std::time::Instant::now();
    let outcome = engine
        .run_turn(
            "The project's own checks fail. `app/engine.py` cannot be imported because \
             indentation is broken in two places; `src/Engine.java` does not compile because \
             two braces are unbalanced. Repair both files. Do not change \
             `tests/test_engine.py` or `src/Main.java`. Report the real result.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");
    eprintln!("turn took {:.2} s", started.elapsed().as_secs_f64());

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
    let ids: Vec<&str> = report
        .evidence
        .iter()
        .map(|evidence| evidence.item_id.as_str())
        .collect();
    assert!(
        ids.contains(&"workspace-tests:python:tests"),
        "the Python check did not run: {ids:?}"
    );
    assert!(
        ids.contains(&"workspace-check:java"),
        "the declared Java check did not run: {ids:?}"
    );
    for evidence in &report.evidence {
        eprintln!(
            "{}: {:?}\n{}",
            evidence.item_id, evidence.outcome, evidence.detail
        );
    }

    let test_after =
        std::fs::read_to_string(workspace.join("tests/test_engine.py")).expect("the test");
    assert_eq!(test_before, test_after, "the model changed the test");
    let main_after = std::fs::read_to_string(workspace.join("src/Main.java")).expect("main");
    assert_eq!(main_before, main_after, "the model changed Main.java");

    let python = if cfg!(windows) { "python" } else { "python3" };
    let tests = Command::new(python)
        .args(["-m", "unittest", "discover", "-s", "tests", "-v"])
        .current_dir(&workspace)
        .output()
        .expect("run the Python tests");
    assert!(
        tests.status.success(),
        "the Python tests still fail:\n{}\n{}",
        String::from_utf8_lossy(&tests.stdout),
        String::from_utf8_lossy(&tests.stderr)
    );
    let javac = Command::new("javac")
        .args([
            "-J-Duser.language=en",
            "-J-Duser.country=US",
            "-d",
            "build",
            "src/Main.java",
            "src/Engine.java",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run javac");
    assert!(
        javac.status.success(),
        "javac still fails:\n{}\n{}",
        String::from_utf8_lossy(&javac.stdout),
        String::from_utf8_lossy(&javac.stderr)
    );
}
