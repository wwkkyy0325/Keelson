//! Every rig in `rigs/` is a contract, and the contract has to load.
//!
//! ADR 0040 splits rigs by artifact and by verification, not by subject. Two
//! files that resolve to one profile are one rig with two names; a rig whose
//! profile does not parse is a picker entry that cannot run. Neither shows up
//! in an engine unit test, so the whole directory is checked here.
//!
//! No provider is called: opening the engine parses the profile, resolves the
//! persona and validates the visible tools.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::{Engine, ModelOverride, OpenRequest};
use keelson::rig::Rig;

/// The probes never ask anything; the port exists because the engine takes one
/// at open.
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

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rig_files() -> Vec<PathBuf> {
    let dir = root().join("rigs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no rig files under {}", dir.display());
    files
}

/// Opens a rig without a model call. The override only keeps `web_search` in
/// the visible set, as `tests/rig_tools.rs` does.
fn open(rig: &Rig, workspace: &Path) -> Result<Engine, String> {
    std::fs::create_dir_all(workspace).map_err(|error| error.to_string())?;
    Engine::open(OpenRequest {
        rig: rig.clone(),
        workspace: workspace.to_path_buf(),
        state_dir: workspace.join("state"),
        store_path: workspace.join("state/session.json"),
        interactions: Arc::new(NoInteractions),
        deltas: None,
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
        search_key: None,
    })
    .map_err(|error| format!("{error:?}"))
}

#[test]
fn every_rig_loads_and_opens() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    for path in rig_files() {
        let rig = Rig::load(&path)
            .unwrap_or_else(|error| panic!("{} does not load: {error}", path.display()));
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("a file stem");
        assert_eq!(
            rig.name,
            stem,
            "{}: the rig name and the file name differ; discovery is by name",
            path.display()
        );
        assert!(
            !rig.title.trim().is_empty(),
            "{}: a rig needs a title; it is what the picker shows",
            path.display()
        );
        assert!(
            !rig.description.trim().is_empty(),
            "{}: a rig needs a description of what it produces",
            path.display()
        );
        assert!(
            rig.profile.is_file(),
            "{}: profile {} does not exist",
            path.display(),
            rig.profile.display()
        );

        let workspace = temp.path().join(&rig.name);
        let engine = open(&rig, &workspace).unwrap_or_else(|error| {
            panic!(
                "{}: the profile does not open: {error}\n\
                 a field the kernel does not know fails the whole rig; see \
                 docs/llmwiki/backlog.md",
                path.display()
            )
        });
        assert!(
            !engine.config().system_prompt.trim().is_empty(),
            "{}: the persona is empty",
            path.display()
        );
        assert!(
            !engine.config().visible_tool_names.is_empty(),
            "{}: no tool is visible, so the rig cannot do anything",
            path.display()
        );
    }
}

/// Two rig files that resolve to the same profile differ only in wording
/// (ADR 0040): one profile, one rig.
#[test]
fn no_two_rigs_share_a_profile() {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for path in rig_files() {
        let rig = Rig::load(&path)
            .unwrap_or_else(|error| panic!("{} does not load: {error}", path.display()));
        assert!(
            seen.insert(rig.profile.clone()),
            "{} and an earlier rig share {}; that is one rig with two names",
            path.display(),
            rig.profile.display()
        );
    }
}

/// Rendering without looking is a frame nobody checked. A rig that can produce
/// a picture also carries the tool that reads one.
#[test]
fn a_rig_that_renders_can_also_look() {
    let temp = tempfile::TempDir::new().expect("a temp dir");
    for path in rig_files() {
        let rig = Rig::load(&path)
            .unwrap_or_else(|error| panic!("{} does not load: {error}", path.display()));
        let workspace = temp.path().join(&rig.name);
        let engine = open(&rig, &workspace).unwrap_or_else(|error| {
            panic!("{}: the profile does not open: {error}", path.display())
        });
        let tools = &engine.config().visible_tool_names;
        for renderer in ["render_scene", "render_film", "screen_capture"] {
            if tools.iter().any(|tool| tool == renderer) {
                assert!(
                    tools.iter().any(|tool| tool == "view_image"),
                    "{} exposes `{renderer}` without `view_image`: the frame cannot be read",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn discovery_over_the_rigs_directory_is_clean() {
    let dir = root().join("rigs");
    let discovery = keelson::rig::discover(std::slice::from_ref(&dir));
    assert!(
        discovery.errors.is_empty(),
        "rigs that do not load: {:?}",
        discovery.errors
    );
    assert!(
        discovery.shadowed.is_empty(),
        "two files claim one name: {:?}",
        discovery
            .shadowed
            .iter()
            .map(|rig| rig.path.display().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        discovery.rigs.len(),
        rig_files().len(),
        "discovery did not find every rig file under {}",
        dir.display()
    );
}

/// The live profiles are five files over one fragment. If the fragment stops
/// applying, an endpoint change becomes five edits again and nothing fails:
/// every profile still loads. So the inheritance itself is pinned here, with
/// the one value each rig chose for itself beside it.
#[test]
fn a_live_profile_inherits_the_endpoint_and_keeps_its_own_choices() {
    let base = root().join("profiles/live-base.toml");
    assert!(base.is_file(), "{} is missing", base.display());

    let cases = [
        // name, temperature, verification window in milliseconds
        ("coding-live", 0.2, 300_000),
        ("ui-live", 0.5, 120_000),
        ("figure-live", 0.9, 300_000),
        ("office-live", 0.7, 60_000),
        ("research-live", 0.2, 60_000),
    ];
    for (name, temperature, window) in cases {
        let path = root().join(format!("profiles/{name}.toml"));
        let config = keel_runtime::load_profile(&path)
            .unwrap_or_else(|error| panic!("{name}: {} does not load: {error}", path.display()));

        assert_eq!(
            config.model_route.provider, "anthropic-compatible",
            "{name} should inherit the endpoint"
        );
        assert_eq!(
            config.model_route.model, "deepseek-flash",
            "{name} should inherit the model id"
        );
        assert_eq!(
            config.context.budget_tokens, 64_000,
            "{name} should inherit the context ladder"
        );
        assert_eq!(config.tool_budget, 1000, "{name} should inherit the budget");
        assert_eq!(
            config.model_route.sampling.temperature,
            Some(temperature),
            "{name} chose its own sampling"
        );
        assert_eq!(
            config.verification_idle_window,
            std::time::Duration::from_millis(window),
            "{name} should have the window it declares or the default"
        );
    }
}
