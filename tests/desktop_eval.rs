//! Live check: does the model actually see an image it views?
//!
//! The workspace holds a generated red circle. The model must call
//! `view_image` and name the colour and the shape; a model that cannot see
//! images cannot answer without guessing both.
//!
//! ```text
//! KEELSON_EVAL_BASE_URL=... KEELSON_EVAL_MODEL=... \
//!   cargo test --test desktop_eval -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::{Engine, OpenRequest, Store};
use keelson::rig::Rig;

mod common;

#[derive(Default)]
struct Interactions;

#[async_trait]
impl InteractionPort for Interactions {
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

fn office_rig() -> Rig {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Rig {
        name: "office".to_owned(),
        title: "Office".to_owned(),
        description: "Evaluation rig.".to_owned(),
        profile: root.join("profiles/office-live.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/office.toml"),
    }
}

/// A white canvas with a filled red circle.
fn draw_shape(path: &Path) {
    let radius = 70i32;
    let center = 120i32;
    let mut canvas = image::RgbaImage::from_pixel(240, 240, image::Rgba([255, 255, 255, 255]));
    for y in 0..240 {
        for x in 0..240 {
            let dx = x as i32 - center;
            let dy = y as i32 - center;
            if dx * dx + dy * dy <= radius * radius {
                canvas.put_pixel(x, y, image::Rgba([220, 20, 20, 255]));
            }
        }
    }
    canvas.save(path).expect("the generated image");
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn the_model_reads_an_image_it_views() {
    let Some(model) = common::eval_model() else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("the workspace");
    draw_shape(&workspace.join("shape.png"));

    let state = temp.path().join("state");
    let store_path = state.join("session.json");
    let engine = Engine::open(OpenRequest {
        rig: office_rig(),
        workspace: workspace.clone(),
        state_dir: state.clone(),
        store_path: store_path.clone(),
        interactions: Arc::new(Interactions),
        deltas: None,
        model: Some(model),
        permission: None,
        acceptance: true,
        search_url: None,
    })
    .expect("the engine opens");

    let outcome = engine
        .run_turn(
            "Use view_image on shape.png, then answer in one short sentence: what shape is it and what colour is it? Name the colour and the shape.",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    let answer = format!("{} {}", outcome.text, outcome.answer.body).to_lowercase();
    assert!(
        answer.contains("red") || answer.contains("红"),
        "the answer must name the colour that is actually in the image:\n{answer}"
    );
    assert!(
        answer.contains("circle") || answer.contains("圆") || answer.contains("round"),
        "the answer must name the shape that is actually in the image:\n{answer}"
    );

    // The tool really ran, not just the model guessing.
    let workspace = std::fs::canonicalize(&workspace).expect("a canonical workspace");
    let document = Store::open(store_path)
        .load(&workspace)
        .expect("the session loads")
        .expect("the session exists");
    let viewed = document
        .turns
        .iter()
        .flat_map(|turn| turn.tools.iter())
        .any(|tool| tool.name == "view_image");
    assert!(viewed, "the model answered without calling view_image");
}
