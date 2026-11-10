//! Live larger-workspace eval: does the model use regex search and multi-edit?
//!
//! The workspace is `examples/shop-workspace`: a dozen Python modules whose
//! tests describe a missing `discount` parameter at several call sites. The
//! prompt asks for the parameter, a regex search for the call sites and a
//! multi-edit in `pricing.py`.
//!
//! This test needs a real provider key; it is ignored by default.
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo test --test shop_eval -- --ignored --nocapture
//! ```

use std::path::Path;
use std::process::Command;
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

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn the_discount_feature_crosses_the_shop() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: set DEEPSEEK_API_KEY or KEELSON_EVAL_BASE_URL");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Keep the workspace after a failed run so the transcript and tool trace
    // can be inspected; the next run replaces it.
    let base = root.join("target/eval/shop");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("eval directory");
    let workspace = base.join("workspace");
    copy_dir(&root.join("examples/shop-workspace"), &workspace).expect("copy the example");
    let tests_before =
        std::fs::read_to_string(workspace.join("tests/test_discount.py")).expect("the tests");
    eprintln!("workspace: {}", workspace.display());

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Add the discount feature across the shop.".to_owned(),
        profile: root.join("profiles/coding.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/coding.toml"),
    };
    let state_dir = base.join("state");
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
            "The tests in this directory fail because the discount feature is missing. \
             Add a `discount` parameter to `pricing.subtotal`, `pricing.total`, \
             `pricing.category_total` and `pricing.price_breakdown`, and propagate it \
             through `reporting.render`, `reporting.invoice`, `reporting.breakdown`, \
             `checkout.checkout`, `api.quote`, `wholesale.wholesale_total`, \
             `inventory.stock_value` and `shipping.shipping_quote`. Use `code_search` \
             with a regex to find every `total(` and `subtotal(` call site, and use \
             `multi_edit` for the several related changes in `pricing.py`. Do not \
             change the test files. Run the tests and report the real result.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    eprintln!("turn state: {:?}", outcome.state);
    eprintln!("turn error: {:?}", outcome.error);
    if let Some(report) = &outcome.verification {
        for evidence in &report.evidence {
            eprintln!(
                "verify {} ({}): {}",
                evidence.item_id, evidence.verifier, evidence.detail
            );
        }
        eprintln!("required failed: {:?}", report.required_failed);
        eprintln!("required unverified: {:?}", report.required_unverified);
    } else {
        eprintln!("no verification report");
    }
    let canonical = std::fs::canonicalize(&workspace).expect("canonical workspace");
    let session = Store::open(store_for_check).load(&canonical).ok().flatten();
    let tools: Vec<&str> = session
        .as_ref()
        .map(|session| {
            session
                .turns
                .iter()
                .flat_map(|turn| turn.tools.iter().map(|tool| tool.name.as_str()))
                .collect()
        })
        .unwrap_or_default();
    eprintln!("tools used: {tools:?}");
    eprintln!(
        "adoption: code_search={} multi_edit={} edit_file={} write_file={}",
        tools.contains(&"code_search"),
        tools.contains(&"multi_edit"),
        tools.contains(&"edit_file"),
        tools.contains(&"write_file")
    );
    eprintln!("model calls: {}", outcome.model_calls);

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
        std::fs::read_to_string(workspace.join("tests/test_discount.py")).expect("the tests");
    assert_eq!(
        tests_before, tests_after,
        "the model changed the visible tests"
    );

    for file in [
        "shop/pricing.py",
        "shop/reporting.py",
        "shop/checkout.py",
        "shop/api.py",
        "shop/wholesale.py",
        "shop/inventory.py",
        "shop/shipping.py",
    ] {
        let text = std::fs::read_to_string(workspace.join(file)).expect("a changed file");
        assert!(text.contains("discount"), "{file} was not changed:\n{text}");
    }
}
