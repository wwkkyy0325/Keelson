//! Live check: does the rig ask when the repository cannot answer?
//!
//! The brief names a choice the repository cannot settle (which currency to
//! print). The kernel offers the control tool on every non-final call; the
//! provider turns it into an `InteractionRequest`; this port answers with the
//! first option. The turn must ask once with concrete options and then finish.
//!
//! ```text
//! KEELSON_EVAL_BASE_URL=... KEELSON_EVAL_MODEL=... //!   cargo test --test ask_eval -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::{Engine, OpenRequest};
use keelson::rig::Rig;

mod common;

#[derive(Default)]
struct AnswerChoices {
    seen: Mutex<Vec<String>>,
}

#[async_trait]
impl InteractionPort for AnswerChoices {
    async fn request(
        &self,
        request: InteractionRequest,
        _cancel: &CancelToken,
    ) -> InteractionResponse {
        let first = request
            .options
            .first()
            .map(|option| option.id.clone())
            .unwrap_or_else(|| "ok".to_owned());
        self.seen.lock().unwrap().push(format!(
            "{:?} | {} | options={:?} | recommended={:?}",
            request.kind,
            request.prompt,
            request
                .options
                .iter()
                .map(|option| option.id.clone())
                .collect::<Vec<_>>(),
            request.recommended
        ));
        match request.kind {
            InteractionKind::PermissionApproval => InteractionResponse::Approved {
                scope: GrantScope::Once,
            },
            _ => InteractionResponse::Selected { option: first },
        }
    }
}

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
async fn an_ambiguous_brief_makes_the_model_ask() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("inventory-workspace");
    copy_dir(&root.join("examples/inventory-workspace"), &workspace).expect("copy");
    let interactions = Arc::new(AnswerChoices::default());

    let rig = Rig {
        name: "coding".to_owned(),
        title: "Coding".to_owned(),
        description: "Diagnose a discount report.".to_owned(),
        profile: root.join("profiles/coding.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/coding.toml"),
    };
    let state_dir = temp.path().join("state");
    let store_path = state_dir.join("session.json");
    let engine = Engine::open(OpenRequest {
        rig,
        workspace,
        state_dir,
        store_path,
        interactions: interactions.clone(),
        deltas: None,
        model: Some(model),
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(
            "The report should print money with a currency symbol. Use dollars or euros — the product team has not decided. Wire it up.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    let seen = interactions.seen.lock().unwrap().clone();
    for line in &seen {
        eprintln!("interaction: {line}");
    }
    eprintln!("turn done: {}", outcome.is_done());
    eprintln!("model calls: {}", outcome.model_calls);

    let asked = seen
        .iter()
        .any(|line| line.starts_with("Choice") || line.starts_with("Clarify"));
    assert!(
        asked,
        "the repository could not answer the choice, so the turn should have asked: {seen:?}"
    );
    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
}
