//! Live checks for the research rig's evidence contract.
//!
//! The probes use a local source folder so they do not depend on a search
//! backend: what is measured is whether the answer names the files it read,
//! quotes them, reports a conflict instead of picking a side, and says when a
//! fact is absent.
//!
//! ```text
//! KEELSON_EVAL_BASE_URL=... KEELSON_EVAL_MODEL=... \
//!   cargo test --test research_eval -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
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
struct Interactions {
    seen: Mutex<Vec<String>>,
}

#[async_trait]
impl InteractionPort for Interactions {
    async fn request(
        &self,
        request: InteractionRequest,
        _cancel: &CancelToken,
    ) -> InteractionResponse {
        self.seen
            .lock()
            .unwrap()
            .push(format!("{:?} | {}", request.kind, request.prompt));
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

fn research_rig() -> Rig {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Rig {
        name: "research".to_owned(),
        title: "Research".to_owned(),
        description: "Evaluation rig.".to_owned(),
        profile: root.join("profiles/research-live.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/research.toml"),
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(path, text).expect("a file");
}

fn open(workspace: &Path, state: &Path) -> Option<Engine> {
    let model = common::eval_model()?;
    let interactions = Arc::new(Interactions::default());
    let engine = Engine::open(OpenRequest {
        rig: research_rig(),
        workspace: workspace.to_path_buf(),
        state_dir: state.to_path_buf(),
        store_path: state.join("session.json"),
        interactions,
        deltas: None,
        model: Some(model),
        permission: None,
        acceptance: true,
        search_url: None,
    })
    .expect("the engine opens");
    Some(engine)
}

/// Two sources that disagree and one that is about something else.
fn sources(workspace: &Path) {
    write(
        &workspace.join("sources/a.md"),
        "青石项目于 2021 年发布 1.0 版本。团队当时只有五个人。",
    );
    write(
        &workspace.join("sources/b.md"),
        "青石项目 1.0 版本于 2022 年 3 月发布，首个用户是本地一所学校。",
    );
    write(
        &workspace.join("sources/c.md"),
        "白鹭项目是一个独立的笔记工具，2019 年开始开发。",
    );
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_conflict_is_reported_with_both_sources() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    let state = temp.path().join("state");
    sources(&workspace);

    let Some(engine) = open(&workspace, &state) else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let outcome = engine
        .run_turn(
            "只依据 sources 目录：青石项目的 1.0 版本是哪一年发布的？来源之间有冲突时都要列出来，不要自己判断哪个对。把结论和来源台账写到 answer.md。",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    let answer = std::fs::read_to_string(workspace.join("answer.md"))
        .expect("the answer and its ledger were written to answer.md");
    for needle in ["2021", "2022", "a.md", "b.md"] {
        assert!(
            answer.contains(needle),
            "the answer must keep both sides of the conflict and name their files, missing `{needle}`:\n{answer}"
        );
    }
    assert!(
        ["quote", "引用", "原文"]
            .iter()
            .any(|marker| answer.to_lowercase().contains(marker)),
        "the ledger must quote the source text:\n{answer}"
    );
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn an_absent_fact_is_not_invented() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    let state = temp.path().join("state");
    sources(&workspace);

    let Some(engine) = open(&workspace, &state) else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let outcome = engine
        .run_turn(
            "只依据 sources 目录：青石项目使用什么开源许可证？如果材料里没有提到，就明确说没有，把答案写到 answer.md。",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    let answer = std::fs::read_to_string(workspace.join("answer.md"))
        .expect("the answer was written to answer.md");
    let lower = answer.to_lowercase();
    assert!(
        [
            "没有",
            "未提及",
            "未提到",
            "未提供",
            "未说明",
            "not mentioned",
            "no mention",
            "not stated",
            "unknown"
        ]
        .iter()
        .any(|marker| lower.contains(marker)),
        "the answer must say the sources do not state a licence:\n{answer}"
    );
    for invented in ["mit", "apache", "gpl", "bsd", "mozilla"] {
        assert!(
            !lower.contains(invented),
            "the sources do not name a licence, so `{invented}` is invented:\n{answer}"
        );
    }
}
