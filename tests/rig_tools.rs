//! The rig files expose the tools their output contract needs.
//!
//! No provider is called: the engine builds its effective tool set and loads
//! the persona when it opens. This is the wiring check the live evals cannot
//! run without a key.

use std::path::Path;

use std::sync::Arc;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::{Engine, ModelOverride, OpenRequest};
use keelson::rig::Rig;

/// The probes never ask anything; the port exists because the engine takes
/// one at open.
struct NoInteractions;

#[async_trait]
impl InteractionPort for NoInteractions {
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

fn open(rig_name: &str, profile: &str, workspace: &Path) -> Engine {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rig = Rig {
        name: rig_name.to_owned(),
        title: rig_name.to_owned(),
        description: String::new(),
        profile: root.join(profile),
        extra_roots: Vec::new(),
        path: root.join("rigs").join(format!("{rig_name}.toml")),
    };
    Engine::open(OpenRequest {
        rig,
        workspace: workspace.to_path_buf(),
        state_dir: workspace.join("state"),
        store_path: workspace.join("state/session.json"),
        interactions: Arc::new(NoInteractions),
        deltas: None,
        // Only the host is read, to keep `web_search` in the visible set; no
        // request is made.
        model: Some(ModelOverride {
            provider: "openai-compatible".to_owned(),
            model: "unused".to_owned(),
            base_url: Some("https://api.deepseek.com".to_owned()),
            api_key: "unused".to_owned(),
            context_window: None,
        }),
        permission: None,
        acceptance: true,
        search_url: None,
    })
    .expect("the engine opens")
}

#[test]
fn the_office_rig_exposes_document_tools() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let engine = open("office", "profiles/office-live.toml", temp.path());
    let tools = &engine.config().visible_tool_names;

    for needed in [
        "read_file",
        "write_file",
        "edit_file",
        "multi_edit",
        "code_search",
        "list_dir",
        "glob",
        "move_file",
        "delete_file",
        "list_trash",
        "restore_file",
        "shell",
        "todo",
        "search_history",
        "web_search",
        "web_fetch",
        "list_windows",
        "screen_capture",
    ] {
        assert!(
            tools.iter().any(|tool| tool == needed),
            "office is missing `{needed}`: {tools:?}"
        );
    }
    for code_only in ["git_diff", "apply_edits"] {
        assert!(
            !tools.iter().any(|tool| tool == code_only),
            "`{code_only}` belongs to the coding rig: {tools:?}"
        );
    }
    assert!(
        engine.config().system_prompt.contains("office assistant"),
        "the office persona was not loaded"
    );
}

#[test]
fn the_research_rig_exposes_search_and_cites_sources() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    let engine = open("research", "profiles/research-live.toml", temp.path());
    let tools = &engine.config().visible_tool_names;

    for needed in [
        "web_search",
        "web_fetch",
        "code_search",
        "read_file",
        "write_file",
        "edit_file",
        "list_dir",
        "glob",
        "shell",
        "todo",
        "search_history",
        "list_windows",
        "screen_capture",
    ] {
        assert!(
            tools.iter().any(|tool| tool == needed),
            "research is missing `{needed}`: {tools:?}"
        );
    }
    // Research creates notes; reorganising the user's files is office work.
    for office_only in [
        "move_file",
        "delete_file",
        "multi_edit",
        "git_diff",
        "apply_edits",
    ] {
        assert!(
            !tools.iter().any(|tool| tool == office_only),
            "`{office_only}` does not belong to the research rig: {tools:?}"
        );
    }
    assert!(
        engine.config().system_prompt.contains("Sources"),
        "the research persona did not load its evidence contract"
    );
}
