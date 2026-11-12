//! Live checks for the office rig's output contract.
//!
//! Three probes: a targeted edit leaves the rest of the document verbatim, a
//! summary is written to the file the user named, and an ambiguous request
//! makes the model ask before it acts.
//!
//! ```text
//! KEELSON_EVAL_BASE_URL=... KEELSON_EVAL_MODEL=... \
//!   cargo test --test office_eval -- --ignored --nocapture
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
        self.seen.lock().unwrap().push(format!(
            "{:?} | {} | options={:?}",
            request.kind,
            request.prompt,
            request
                .options
                .iter()
                .map(|option| option.id.clone())
                .collect::<Vec<_>>()
        ));
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

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn office_rig() -> Rig {
    let root = root();
    Rig {
        name: "office".to_owned(),
        title: "Office".to_owned(),
        description: "Evaluation rig.".to_owned(),
        profile: root.join("profiles/office-live.toml"),
        extra_roots: Vec::new(),
        path: root.join("rigs/office.toml"),
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(path, text).expect("a file");
}

/// Opens the office rig on `workspace` and returns the engine with the
/// interaction recorder, or `None` when no endpoint is configured.
fn open(workspace: &Path, state: &Path) -> Option<(Engine, Arc<Interactions>)> {
    let model = common::eval_model()?;
    let interactions = Arc::new(Interactions::default());
    let engine = Engine::open(OpenRequest {
        rig: office_rig(),
        workspace: workspace.to_path_buf(),
        state_dir: state.to_path_buf(),
        store_path: state.join("session.json"),
        interactions: interactions.clone(),
        deltas: None,
        model: Some(model),
        permission: None,
        acceptance: true,
        search_url: None,
        search_key: None,
    })
    .expect("the engine opens");
    Some((engine, interactions))
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn an_edit_changes_only_the_requested_text() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    let state = temp.path().join("state");
    let memo = "\
# 项目备忘

评审会日期：2024-03-01。

参与人：张三、李四、王五。
地点：三楼会议室。

待办：
- 确认会议室
- 准备演示材料
- 通知外部顾问

备注：会后一周内发出纪要。
";
    write(&workspace.join("memo.md"), memo);

    let Some((engine, _)) = open(&workspace, &state) else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let outcome = engine
        .run_turn(
            "把 memo.md 里的日期 2024-03-01 改成 2024-11-20，其他内容保持原样，不要改动格式。",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    let actual = std::fs::read_to_string(workspace.join("memo.md")).expect("the memo");
    let expected = memo.replace("2024-03-01", "2024-11-20");
    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    assert_eq!(
        actual, expected,
        "an edit must change the date and leave every other byte alone"
    );
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn a_summary_is_written_to_the_named_file() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    let state = temp.path().join("state");
    write(
        &workspace.join("notes/alpha.md"),
        "Alpha 项目于 2023 年启动，负责人是王五。首年预算 120 万元。",
    );
    write(
        &workspace.join("notes/beta.md"),
        "Beta 项目于 2024 年启动，负责人是赵六。目标是覆盖三个省份。",
    );
    write(
        &workspace.join("notes/gamma.md"),
        "Gamma 项目于 2025 年启动，负责人是孙七。它和 Alpha 共用同一套数据平台。",
    );

    let Some((engine, _)) = open(&workspace, &state) else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let outcome = engine
        .run_turn(
            "读 notes/ 下的三份材料，写一份不超过 200 字的摘要到 summary.md，并注明每部分来自哪个文件。",
            CancelToken::new(),
        )
        .await
        .expect("the turn completes");

    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    let summary = std::fs::read_to_string(workspace.join("summary.md"))
        .expect("the deliverable was written to the file the user named");
    for needle in ["alpha.md", "beta.md", "gamma.md", "王五", "赵六", "孙七"] {
        assert!(
            summary.contains(needle),
            "summary is missing `{needle}`:\n{summary}"
        );
    }
    assert!(
        summary.chars().count() <= 400,
        "the summary should stay near 200 characters, got {}:\n{summary}",
        summary.chars().count()
    );
}

#[tokio::test]
#[ignore = "needs a live provider key"]
async fn an_ambiguous_request_asks_before_acting() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let workspace = temp.path().join("workspace");
    let state = temp.path().join("state");
    write(
        &workspace.join("notes/meeting.md"),
        "周会纪要：下周三讨论预算。",
    );
    write(
        &workspace.join("notes/idea.md"),
        "想法：把季度报告改成图表。",
    );
    write(
        &workspace.join("drafts/letter.txt"),
        "草稿：致客户的说明信。",
    );
    write(&workspace.join("report-2024.md"), "年度报告初稿。");

    let Some((engine, interactions)) = open(&workspace, &state) else {
        eprintln!("skipping: no endpoint");
        return;
    };
    let outcome = engine
        .run_turn("把这里的文件整理一下。", CancelToken::new())
        .await
        .expect("the turn completes");

    let seen = interactions.seen.lock().unwrap().clone();
    for line in &seen {
        eprintln!("interaction: {line}");
    }
    assert!(
        outcome.is_done(),
        "turn did not finish: {:?}",
        outcome.error
    );
    assert!(
        seen.iter().any(|line| line.starts_with("Choice") || line.starts_with("Clarify")),
        "organising a mixed directory has several valid answers, so the model should have asked: {seen:?}"
    );
}
