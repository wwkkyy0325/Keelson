//! The kit's data gate: a scene can only show numbers that `data/check.py`
//! agrees with.
//!
//! No model and no browser: this runs the same scripts a film run does.

use std::path::{Path, PathBuf};
use std::process::Command;

fn kit_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("kits")
        .join("explainer")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the target dir");
    for entry in std::fs::read_dir(from).expect("a readable dir").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("a copied file");
        }
    }
}

fn python() -> Option<String> {
    for candidate in ["python", "python3"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Some(candidate.to_owned());
        }
    }
    None
}

fn run(python: &str, root: &Path, script: &str) -> std::process::Output {
    Command::new(python)
        .arg(script)
        .current_dir(root)
        .output()
        .expect("the script runs")
}

#[test]
fn the_data_gate_rejects_a_number_that_does_not_add_up() {
    let Some(python) = python() else {
        eprintln!("skipping: python is not on PATH");
        return;
    };
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let root = temp.path().join("film");
    copy_dir(&kit_root().join("data"), &root.join("data"));

    let built = run(&python, &root, "data/build.py");
    assert!(built.status.success(), "{:?}", built.stderr);
    let checked = run(&python, &root, "data/check.py");
    assert!(
        checked.status.success(),
        "the freshly built model must pass: {:?}",
        checked.stderr
    );
    assert!(
        String::from_utf8_lossy(&checked.stdout).contains("all agree"),
        "{:?}",
        checked.stdout
    );

    // Change one displayed cell and the gate has to notice.
    let path = root.join("data/model.json");
    let mut model: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("a model")).expect("json");
    let cell = model["output"][0][0].as_i64().expect("a number") + 1;
    model["output"][0][0] = serde_json::Value::from(cell);
    std::fs::write(&path, serde_json::to_string_pretty(&model).expect("json")).expect("a write");

    let tampered = run(&python, &root, "data/check.py");
    assert!(
        !tampered.status.success(),
        "a wrong feature map must fail the check"
    );
    let detail = String::from_utf8_lossy(&tampered.stderr);
    assert!(
        detail.contains("not the convolution"),
        "the failure names the disagreement: {detail}"
    );
}

#[test]
fn the_claim_gate_needs_independent_sources() {
    let Some(python) = python() else {
        eprintln!("skipping: python is not on PATH");
        return;
    };
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let root = temp.path().join("film");
    copy_dir(&kit_root().join("data"), &root.join("data"));
    let board = r#"{
  "format": { "width": 1920, "height": 1080, "fps": 30, "voice": "zh-CN-XiaoxiaoNeural" },
  "scenes": [ { "id": "01", "narration": "a claim", "claims": ["c1"],
                "source_card": "first" } ]
}"#;
    std::fs::write(root.join("storyboard.json"), board).expect("a storyboard");

    let one = r#"{
  "claims": [ {
    "id": "c1", "type": "fact", "statement": "one blog says so",
    "scenes": ["01"], "status": "supported",
    "sources": [ { "title": "blog", "publisher": "a blog",
                   "url": "https://one.example/post", "kind": "secondary",
                   "accessed": "2026-11-11", "quote": "the claim" } ]
  } ]
}"#;
    std::fs::write(root.join("claims.json"), one).expect("a ledger");
    let single = run(&python, &root, "data/check_claims.py");
    assert!(
        !single.status.success(),
        "one secondary source is not enough"
    );
    let detail = String::from_utf8_lossy(&single.stdout);
    assert!(
        detail.contains("two independent domains or a primary source"),
        "{detail}"
    );

    let two = r#"{
  "claims": [ {
    "id": "c1", "type": "fact", "statement": "two publishers say so",
    "scenes": ["01"], "status": "supported",
    "sources": [
      { "title": "report", "publisher": "first", "url": "https://one.example/report",
        "kind": "secondary", "accessed": "2026-11-11", "quote": "the claim" },
      { "title": "report", "publisher": "second", "url": "https://two.example/report",
        "kind": "secondary", "accessed": "2026-11-11", "quote": "the claim again" }
    ]
  } ]
}"#;
    std::fs::write(root.join("claims.json"), two).expect("a ledger");
    let independent = run(&python, &root, "data/check_claims.py");
    assert!(
        independent.status.success(),
        "{}",
        String::from_utf8_lossy(&independent.stdout)
    );

    // A fact on screen without a publisher is not verifiable by a viewer.
    std::fs::write(
        root.join("storyboard.json"),
        r#"{
  "format": { "width": 1920, "height": 1080, "fps": 30, "voice": "zh-CN-XiaoxiaoNeural" },
  "scenes": [ { "id": "01", "narration": "a claim", "claims": ["c1"] } ]
}"#,
    )
    .expect("a storyboard without a publisher");
    let no_card = run(&python, &root, "data/check_claims.py");
    assert!(!no_card.status.success(), "a fact needs a source card");
    assert!(
        String::from_utf8_lossy(&no_card.stdout).contains("without a `source_card`"),
        "{}",
        String::from_utf8_lossy(&no_card.stdout)
    );
    std::fs::write(root.join("storyboard.json"), board).expect("the storyboard is back");

    // A quote is verbatim; an ellipsis is a hidden cut.
    let stitched = two.replace("the claim again", "the claim ...... again");
    std::fs::write(root.join("claims.json"), &stitched).expect("a ledger");
    let ellipsis = run(&python, &root, "data/check_claims.py");
    assert!(!ellipsis.status.success(), "an ellipsis quote must fail");
    assert!(
        String::from_utf8_lossy(&ellipsis.stdout).contains("ellipsis"),
        "{}",
        String::from_utf8_lossy(&ellipsis.stdout)
    );
    std::fs::write(root.join("claims.json"), two).expect("the ledger is back");

    let unlabelled = r#"{
  "claims": [ {
    "id": "c1", "type": "value", "statement": "society is failing",
    "scenes": ["01"], "status": "supported", "sources": []
  } ]
}"#;
    std::fs::write(root.join("claims.json"), unlabelled).expect("a ledger");
    let value = run(&python, &root, "data/check_claims.py");
    assert!(!value.status.success(), "a value judgment needs a label");
    assert!(
        String::from_utf8_lossy(&value.stdout).contains("on-screen label"),
        "{}",
        String::from_utf8_lossy(&value.stdout)
    );
}

#[test]
fn the_kit_keeps_its_contract_files() {
    let root = kit_root();
    for needed in [
        "kit/design.js",
        "kit/scene.html",
        "kit/timeline.py",
        "kit/assemble.py",
        "kit/qa.py",
        "data/spec.json",
        "data/build.py",
        "data/check.py",
        "data/check_claims.py",
        "data/model.json",
        "assets/README.md",
        "claims.example.json",
        "storyboard.example.json",
    ] {
        assert!(root.join(needed).is_file(), "the kit is missing `{needed}`");
    }
    let page = std::fs::read_to_string(root.join("kit/scene.html")).expect("the page");
    assert!(
        page.contains("/data/model.json"),
        "the page loads the derived data"
    );
    let check = std::fs::read_to_string(root.join("data/check.py")).expect("the check");
    assert!(
        check.contains("model.json") && check.contains("spec.json"),
        "the check compares the model against the spec"
    );
}
