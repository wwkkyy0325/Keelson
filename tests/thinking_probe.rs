//! Live probe: a thinking-mode model survives a tool follow-up.
//!
//! DeepSeek's `deepseek-flash` runs in thinking mode. It returns a signed
//! thinking block with the tool call and rejects the next request unless that
//! block is sent back, so this turn fails if the kernel drops it.
//!
//! Run with:
//!   cargo test --test thinking_probe -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse, TurnState,
};
use keelson::engine::{Engine, OpenRequest};
use keelson::rig::Rig;

struct AutoApprove;

#[async_trait]
impl InteractionPort for AutoApprove {
    async fn request(
        &self,
        request: InteractionRequest,
        _cancel: &CancelToken,
    ) -> InteractionResponse {
        match request.kind {
            InteractionKind::PermissionApproval => InteractionResponse::Approved {
                scope: GrantScope::Once,
            },
            _ => InteractionResponse::Selected {
                option: request
                    .options
                    .first()
                    .map(|option| option.id.clone())
                    .unwrap_or_else(|| "ok".to_owned()),
            },
        }
    }
}

#[tokio::test]
#[ignore = "needs a live thinking-mode provider key"]
async fn a_thinking_model_survives_a_tool_followup() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("docs")).expect("the docs dir");
    std::fs::write(root.join("docs/one.md"), "one").expect("a file");
    std::fs::write(root.join("docs/two.md"), "two").expect("a file");

    let rig = Rig {
        name: "explainer".to_owned(),
        title: "Explainer".to_owned(),
        description: String::new(),
        profile: repo.join("profiles/explainer-live.toml"),
        extra_roots: Vec::new(),
        path: repo.join("rigs/explainer.toml"),
    };
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: root.clone(),
        state_dir: root.join(".state"),
        store_path: root.join(".state/session.json"),
        interactions: Arc::new(AutoApprove),
        deltas: None,
        // The rig's own model: `deepseek-flash`, thinking mode, no override.
        model: None,
        permission: None,
        acceptance: false,
        search_url: None,
        search_key: None,
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(
            "Use list_dir on the `docs` directory once. Then reply with exactly the number of files you saw, and nothing else.",
            CancelToken::new(),
        )
        .await
        .expect("the turn runs");

    eprintln!("usage: {:?}", outcome.usage);
    assert_eq!(
        outcome.state,
        TurnState::Done,
        "the tool follow-up failed: {:?}",
        outcome.error
    );
    assert!(
        outcome.answer.body.contains('2'),
        "the answer should report two files: {}",
        outcome.answer.body
    );
}
