//! Tauri commands: setup, the desktop's only write path into the engine, and
//! the session read model.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use keel_core::{CancelToken, Event, PermissionLevel};
use keelson::engine::{
    ContextPoint, Engine, ModelDelta, ModelOverride, OpenRequest, Store, TurnView,
};
use keelson::rig;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::checkpoints;
use crate::config::{self, ProjectEntry};
use crate::sessions::{self, SessionMeta};
use crate::state::{CurrentSession, DesktopState};

pub const EV_TURN_STARTED: &str = "keelson:turn-started";
pub const EV_TEXT_DELTA: &str = "keelson:text-delta";
pub const EV_THINKING_DELTA: &str = "keelson:thinking-delta";
pub const EV_TOOL_STARTED: &str = "keelson:tool-started";
pub const EV_TOOL_FINISHED: &str = "keelson:tool-finished";
pub const EV_BACKGROUND_STARTED: &str = "keelson:background-started";
pub const EV_BACKGROUND_FINISHED: &str = "keelson:background-finished";
pub const EV_TURN_VERIFYING: &str = "keelson:turn-verifying";
pub const EV_TURN_CHANGES: &str = "keelson:turn-changes";
pub const EV_TURN_FINISHED: &str = "keelson:turn-finished";
pub const EV_TURN_FAILED: &str = "keelson:turn-failed";
pub const EV_SESSION_UPDATED: &str = "keelson:session-updated";

type SharedState = Arc<DesktopState>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub onboarded: bool,
    pub has_key: bool,
    pub provider: String,
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub model: String,
    pub locale: String,
    pub last_workspace: String,
    pub key_provider: String,
    pub key_providers: Vec<String>,
    pub manual: Option<config::ManualEndpoint>,
    /// A SearXNG base URL used when the provider has no search endpoint.
    pub search_url: Option<String>,
    /// The rig that will run, and every rig the host found.
    pub rig: String,
    pub rigs: Vec<RigOption>,
    pub problems: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigOption {
    pub name: String,
    pub title: String,
    pub description: String,
}

/// One stored turn plus the files its checkpoint pair changed. The changes
/// belong to the turn that made them, so a later turn cannot take them over.
#[derive(Serialize)]
pub struct TurnWithChanges {
    #[serde(flatten)]
    pub turn: TurnView,
    pub changes: Vec<checkpoints::FileChange>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnPageWithChanges {
    pub turns: Vec<TurnWithChanges>,
    pub next_before: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub rig: String,
    pub workspace: String,
    pub session_id: String,
    pub turns: Vec<TurnWithChanges>,
    pub next_before: Option<u32>,
    /// Every conversation in the project, newest first.
    pub sessions: Vec<SessionMeta>,
    pub projects: Vec<ProjectEntry>,
}

fn with_changes(
    state_dir: &Path,
    workspace: &Path,
    session_id: &str,
    turns: Vec<TurnView>,
) -> Vec<TurnWithChanges> {
    let changes = checkpoints::changes_for_session(state_dir, workspace, session_id);
    turns
        .into_iter()
        .map(|turn| {
            let changes = changes.get(&turn.seq).cloned().unwrap_or_default();
            TurnWithChanges { turn, changes }
        })
        .collect()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnStarted {
    seq: u32,
    input: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TextDelta {
    seq: u32,
    text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThinkingDelta {
    seq: u32,
    text: String,
}

/// A tool call the running turn just started. The previews are bounded and
/// redacted by the kernel.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolStartedEvent {
    seq: u32,
    call_id: u64,
    name: String,
    args: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolFinishedEvent {
    seq: u32,
    call_id: u64,
    name: String,
    ok: bool,
    duration_ms: u64,
    error: Option<String>,
    output: Option<String>,
}

/// A shell command the turn moved to the background. The result arrives later
/// as a user message in the conversation; this pair lets the view show it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackgroundStartedEvent {
    seq: u32,
    id: String,
    program: String,
    pid: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackgroundFinishedEvent {
    seq: u32,
    id: String,
    program: String,
    exit_code: Option<i32>,
    duration_ms: u64,
    output_path: String,
}

/// The answer is written and the turn is running its acceptance checks. The
/// composer still waits for the turn; this only gives the view a state to show.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnVerifyingEvent {
    seq: u32,
    checks: u32,
}

/// The checkpoint pair for a finished turn is recorded after the turn row is
/// shown; this tells the view to reload the diff and undo state.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnChanges {
    seq: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnFinished {
    turn: TurnView,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnFailed {
    seq: u32,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionUpdated {
    meta: SessionMeta,
}

#[tauri::command]
pub fn setup_state(state: State<'_, SharedState>) -> SetupState {
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let discovery = rig::discover(&rig_dirs());
    let has_key = config::load_key(&config.provider_id, &config.key_provider)
        .ok()
        .flatten()
        .is_some();
    let rigs: Vec<RigOption> = discovery
        .rigs
        .iter()
        .map(|rig| RigOption {
            name: rig.name.clone(),
            title: rig.title.clone(),
            description: rig.description.clone(),
        })
        .collect();
    let rig = chosen_rig(&config, rig_name_for(&config.provider));
    SetupState {
        onboarded: config.onboarded,
        has_key,
        provider: config.provider,
        provider_id: config.provider_id,
        provider_name: config.provider_name,
        base_url: config.base_url,
        model: config.model,
        locale: config.locale,
        last_workspace: config.last_workspace,
        key_provider: config.key_provider,
        key_providers: config.key_providers,
        manual: config.manual,
        search_url: config.search_url,
        rig,
        rigs,
        problems: discovery
            .errors
            .iter()
            .map(|error| error.to_string())
            .collect(),
    }
}

#[tauri::command]
pub async fn model_catalog(refresh: bool) -> Result<crate::catalog::ModelCatalog, String> {
    let state_dir = rig::state_dir().unwrap_or_else(|| default_workspace().join(".keel"));
    crate::catalog::load(&state_dir, refresh).await
}

#[tauri::command]
pub fn save_key(
    state: State<'_, SharedState>,
    key: String,
    provider_id: String,
) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("密钥不能为空".to_owned());
    }
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Err("密钥必须对应一个提供方".to_owned());
    }
    config::save_key(provider_id, key)?;
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.key_provider = provider_id.to_owned();
    if !config.key_providers.iter().any(|id| id == provider_id) {
        config.key_providers.push(provider_id.to_owned());
    }
    config::save_config(&config)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_model(
    state: State<'_, SharedState>,
    provider: String,
    provider_id: String,
    provider_name: String,
    base_url: String,
    model: String,
    context: Option<u32>,
    search_url: Option<String>,
) -> Result<(), String> {
    if !matches!(
        provider.as_str(),
        "anthropic-compatible" | "anthropic" | "openai-compatible" | "openai"
    ) {
        return Err(format!("未知的模型提供方 `{provider}`"));
    }
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err("接口地址和模型名不能为空".to_owned());
    }
    if provider_id.trim().is_empty() {
        return Err("提供方不能为空".to_owned());
    }
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.provider = provider;
    config.provider_id = provider_id.trim().to_owned();
    config.provider_name = provider_name.trim().to_owned();
    config.base_url = base_url.trim().to_owned();
    config.model = model.trim().to_owned();
    config.model_context = context.filter(|value| *value > 0);
    // `Some("")` clears the setting; `None` leaves it as it was, so a caller
    // that does not own the field (the wizard) cannot erase it.
    if let Some(url) = search_url.map(|url| url.trim().to_owned()) {
        if !url.is_empty() {
            let parsed = reqwest::Url::parse(&url)
                .map_err(|error| format!("搜索服务地址不合法：{error}"))?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err("搜索服务地址必须是 http(s) URL".to_owned());
            }
            config.search_url = Some(url);
        } else {
            config.search_url = None;
        }
    }
    if config.provider_id == "custom" {
        config.manual = Some(config::ManualEndpoint {
            provider: config.provider.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
        });
    }
    config::save_config(&config)
}

#[tauri::command]
pub fn save_locale(state: State<'_, SharedState>, locale: String) -> Result<(), String> {
    if !matches!(locale.as_str(), "zh" | "en") {
        return Err("语言只能是 zh 或 en".to_owned());
    }
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.locale = locale;
    config::save_config(&config)
}

#[tauri::command]
pub fn complete_onboarding(state: State<'_, SharedState>) -> Result<(), String> {
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.onboarded = true;
    if config.last_workspace.trim().is_empty() {
        config.last_workspace = default_workspace().display().to_string();
    }
    config::save_config(&config)
}

/// Opens the requested conversation, or the most recently updated one in a
/// project. A project with no conversations opens a blank one without writing
/// a document; the first turn creates it.
#[tauri::command]
pub async fn open_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: Option<String>,
    session_id: Option<String>,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    let workspace = resolve_workspace(&state, workspace);
    open_inner(app, state, workspace, session_id, false).await
}

/// Copies the conversation up to a turn into a new one and opens it.
///
/// A branch keeps the source intact: the turns before `seq` are copied, the
/// turn's own input is not, so the caller can reopen the branch with that text
/// in the composer and edit it. The workspace is not touched, and the source
/// conversation keeps its turns.
#[tauri::command]
pub async fn branch_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    seq: u32,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    ensure_idle(&state)?;
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    let source = sessions::session_path(&current.state_dir, &current.workspace, &current.id);
    let mut document = Store::open(source)
        .load(&current.workspace)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "对话不存在".to_owned())?;
    if !document.turns.iter().any(|turn| turn.seq == seq) {
        return Err("找不到这一轮".to_owned());
    }
    document.turns.retain(|turn| turn.seq < seq);
    let id = sessions::fresh_id();
    let path = sessions::session_path(&current.state_dir, &current.workspace, &id);
    Store::open(path)
        .save(&document)
        .map_err(|error| error.to_string())?;
    sessions::touch(&current.state_dir, &current.workspace, &id);
    open_inner(app, state, current.workspace, Some(id), false).await
}

/// Opens a blank conversation without writing a document. The row appears in
/// the project's history only when the first turn writes the document, so
/// pressing 新对话 and leaving does not litter the list.
#[tauri::command]
pub async fn new_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: Option<String>,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    ensure_idle(&state)?;
    let workspace = resolve_workspace(&state, workspace);
    open_inner(app, state, workspace, Some(sessions::fresh_id()), true).await
}

#[tauri::command]
pub fn list_sessions(
    state: State<'_, SharedState>,
    workspace: String,
) -> Result<Vec<SessionMeta>, String> {
    let workspace = resolve_workspace(state.inner(), Some(workspace));
    let state_dir = state_dir_for(&workspace);
    let _guard = state
        .session_io
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(sessions::list(&state_dir, &workspace))
}

#[tauri::command]
pub fn delete_session(
    state: State<'_, SharedState>,
    workspace: String,
    session_id: String,
) -> Result<(), String> {
    if !sessions::valid_id(&session_id) {
        return Err("会话 id 不合法".to_owned());
    }
    ensure_idle(state.inner())?;
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|current| current.id.clone());
    if current.as_deref() == Some(session_id.as_str()) {
        return Err("请先切换到其他对话".to_owned());
    }
    let workspace = resolve_workspace(state.inner(), Some(workspace));
    let state_dir = state_dir_for(&workspace);
    let _guard = state
        .session_io
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Deleting removes the document; archive is the recoverable step before
    // it. The rule is enforced here, not only by hiding the button.
    if !sessions::is_archived(&state_dir, &workspace, &session_id)? {
        return Err("请先归档再删除".to_owned());
    }
    sessions::remove(&state_dir, &workspace, &session_id)
}

#[tauri::command]
pub fn rename_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: String,
    session_id: String,
    title: String,
) -> Result<SessionMeta, String> {
    if !sessions::valid_id(&session_id) {
        return Err("会话 id 不合法".to_owned());
    }
    let workspace = resolve_workspace(state.inner(), Some(workspace));
    let state_dir = state_dir_for(&workspace);
    let title = sessions::clean_title(&title);
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let is_current = current
        .as_ref()
        .is_some_and(|current| current.id == session_id.as_str() && current.workspace == workspace);
    let meta = if is_current {
        let engine = state
            .engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| "还没有打开会话".to_owned())?;
        engine
            .set_title(&title)
            .map_err(|error| error.to_string())?;
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions::sync(&state_dir, &workspace, &session_id)
            .ok_or_else(|| "对话不存在".to_owned())?
    } else {
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions::set_title(&state_dir, &workspace, &session_id, &title)?
    };
    let _ = app.emit(EV_SESSION_UPDATED, SessionUpdated { meta: meta.clone() });
    Ok(meta)
}

#[tauri::command]
pub fn archive_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: String,
    session_id: String,
    archived: bool,
) -> Result<SessionMeta, String> {
    if !sessions::valid_id(&session_id) {
        return Err("会话 id 不合法".to_owned());
    }
    ensure_idle(state.inner())?;
    let workspace = resolve_workspace(state.inner(), Some(workspace));
    let state_dir = state_dir_for(&workspace);
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let is_current = current
        .as_ref()
        .is_some_and(|current| current.id == session_id.as_str() && current.workspace == workspace);
    let meta = if is_current {
        let engine = state
            .engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| "还没有打开会话".to_owned())?;
        engine
            .set_archived(archived)
            .map_err(|error| error.to_string())?;
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions::sync(&state_dir, &workspace, &session_id)
            .ok_or_else(|| "对话不存在".to_owned())?
    } else {
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions::set_archived(&state_dir, &workspace, &session_id, archived)?
    };
    let _ = app.emit(EV_SESSION_UPDATED, SessionUpdated { meta: meta.clone() });
    Ok(meta)
}

async fn open_inner(
    app: AppHandle,
    state: Arc<DesktopState>,
    workspace: PathBuf,
    session_id: Option<String>,
    allow_missing: bool,
) -> Result<SessionView, String> {
    ensure_idle(&state)?;
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if !config.onboarded {
        return Err("请先完成初始化".to_owned());
    }

    let provider = config.provider.clone();
    let key = Some(
        config::load_key(&config.provider_id, &config.key_provider)?
            .ok_or_else(|| "还没有保存密钥；请重新运行初始化".to_owned())?,
    );
    let fallback = rig_name_for(&provider);
    let preferred = chosen_rig(&config, fallback);
    let discovery = rig::discover(&rig_dirs());
    let rig = discovery
        .rigs
        .iter()
        .find(|rig| rig.name == preferred)
        .or_else(|| discovery.rigs.iter().find(|rig| rig.name == fallback))
        .cloned()
        .ok_or_else(|| format!("找不到 rig `{preferred}`"))?;

    let state_dir = state_dir_for(&workspace);
    let (session_id, session_list) = {
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session_id = match session_id {
            Some(id) => {
                if !sessions::valid_id(&id) {
                    return Err("会话 id 不合法".to_owned());
                }
                if !allow_missing && !sessions::session_path(&state_dir, &workspace, &id).is_file()
                {
                    return Err("对话不存在".to_owned());
                }
                id
            }
            None => match sessions::list(&state_dir, &workspace)
                .into_iter()
                .find(|meta| !meta.archived)
            {
                Some(meta) => meta.id,
                None => sessions::fresh_id(),
            },
        };
        let list = sessions::list(&state_dir, &workspace);
        (session_id, list)
    };
    let store_path = sessions::session_path(&state_dir, &workspace, &session_id);

    let model = key.map(|api_key| ModelOverride {
        provider,
        model: config.model.clone(),
        base_url: Some(config.base_url.clone()).filter(|url| !url.trim().is_empty()),
        api_key,
        context_window: config.model_context,
    });

    let rig_label = rig.name.clone();
    let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel(256);
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir: state_dir.clone(),
        store_path,
        interactions: state.interactions.clone(),
        deltas: Some(delta_tx),
        model,
        permission: config.permission.as_deref().and_then(permission_level),
        acceptance: true,
        search_url: config.search_url.clone(),
    })
    .map_err(|error| error.to_string())?;
    let engine = Arc::new(engine);
    let workspace_label = display_path(engine.workspace());
    let page = engine.history(None, 30);
    state.current_seq.store(0, Ordering::Relaxed);
    {
        let mut slot = state
            .engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(engine);
    }
    {
        let mut current = state
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = Some(CurrentSession {
            id: session_id.clone(),
            workspace: workspace.clone(),
            state_dir: state_dir.clone(),
        });
    }
    let projects = {
        let mut config = state
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        config.last_workspace = workspace_label.clone();
        config::remember_project(&mut config, &workspace_label, sessions::now_ms());
        let _ = config::save_config(&config);
        config::recent_projects(&config)
    };

    let app_handle = app.clone();
    let seq = state.current_seq.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(delta) = delta_rx.recv().await {
            let seq = seq.load(Ordering::Relaxed);
            if seq == 0 {
                continue;
            }
            match delta {
                ModelDelta::Text(text) => {
                    let _ = app_handle.emit(EV_TEXT_DELTA, TextDelta { seq, text });
                }
                ModelDelta::Thinking(text) => {
                    let _ = app_handle.emit(EV_THINKING_DELTA, ThinkingDelta { seq, text });
                }
            }
        }
    });

    Ok(SessionView {
        rig: rig_label,
        workspace: workspace_label,
        session_id: session_id.clone(),
        turns: with_changes(&state_dir, &workspace, &session_id, page.turns),
        next_before: page.next_before,
        sessions: session_list,
        projects,
    })
}

fn resolve_workspace(state: &DesktopState, workspace: Option<String>) -> PathBuf {
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let wanted = match workspace.filter(|path| !path.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => {
            let last = config.last_workspace.trim();
            if last.is_empty() || is_home_dir(Path::new(last)) {
                default_workspace()
            } else {
                PathBuf::from(last)
            }
        }
    };
    std::fs::canonicalize(&wanted).unwrap_or(wanted)
}

fn state_dir_for(workspace: &Path) -> PathBuf {
    rig::state_dir().unwrap_or_else(|| workspace.join(".keel"))
}

/// The rig to run: the chosen file when it is set, otherwise the one the
/// provider implies (offline runs the mock profile).
fn chosen_rig(config: &config::DesktopConfig, fallback: &str) -> String {
    let chosen = config.rig.trim();
    if chosen.is_empty() {
        fallback.to_owned()
    } else {
        chosen.to_owned()
    }
}

#[tauri::command]
pub fn save_rig(state: State<'_, SharedState>, name: String) -> Result<(), String> {
    let name = name.trim();
    let discovery = rig::discover(&rig_dirs());
    if !discovery.rigs.iter().any(|rig| rig.name == name) {
        return Err(format!("找不到 rig `{name}`"));
    }
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.rig = name.to_owned();
    config::save_config(&config)
}

fn rig_name_for(_provider: &str) -> &'static str {
    "coding-live"
}

fn permission_level(level: &str) -> Option<PermissionLevel> {
    match level {
        "workspace_auto" => Some(PermissionLevel::WorkspaceAuto),
        "full_auto" => Some(PermissionLevel::FullAuto),
        _ => None,
    }
}

/// The permission override lives in the desktop config, not in the rig file.
/// It reaches the open engine immediately and is persisted for the next open.
#[tauri::command]
pub fn save_permission(state: State<'_, SharedState>, level: String) -> Result<(), String> {
    let parsed = permission_level(level.trim()).ok_or_else(|| "未知的权限级别".to_owned())?;
    {
        let mut config = state
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        config.permission = Some(parsed.as_str().to_owned());
        config::save_config(&config)?;
    }
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(engine) = engine {
        engine.set_permission(parsed);
    }
    Ok(())
}

/// What the composer's status line shows: the configured permission and the
/// level in force now (a session grant may raise it), the connection and the
/// context pressure of recent turns.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// The persisted setting.
    pub permission: String,
    /// The level actually in force; differs while a full-auto grant is live.
    pub permission_effective: String,
    pub rig: String,
    pub model: String,
    pub provider: String,
    pub provider_name: String,
    pub context_budget: u32,
    pub context_window: Option<u32>,
    pub recent: Vec<ContextPoint>,
}

/// Running and finished-but-unreported background commands. The view polls
/// this while it shows a background row; it never consumes a result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTaskView {
    pub id: String,
    pub program: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output_path: String,
}

#[tauri::command]
pub fn background_tasks(state: State<'_, SharedState>) -> Result<Vec<BackgroundTaskView>, String> {
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    Ok(engine
        .background_tasks()
        .into_iter()
        .map(|task| BackgroundTaskView {
            id: task.id,
            program: task.program,
            running: task.running,
            exit_code: task.exit_code,
            duration_ms: task.duration_ms,
            output_path: task.output_path,
        })
        .collect())
}

#[tauri::command]
pub fn session_status(state: State<'_, SharedState>) -> Result<SessionStatus, String> {
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    Ok(SessionStatus {
        permission: engine.base_permission().as_str().to_owned(),
        permission_effective: engine.permission().as_str().to_owned(),
        rig: engine.rig_name(),
        model: config.model.clone(),
        provider: config.provider.clone(),
        provider_name: config.provider_name.clone(),
        context_budget: engine.context_budget(),
        context_window: Some(engine.config().context_window).filter(|window| *window > 0),
        recent: engine.context_history(),
    })
}

fn ensure_idle(state: &DesktopState) -> Result<(), String> {
    if state
        .cancel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some()
    {
        return Err("当前回合还在运行，请先取消".to_owned());
    }
    Ok(())
}

async fn take_snapshot(current: &CurrentSession) -> Option<String> {
    let workspace = current.workspace.clone();
    let state_dir = current.state_dir.clone();
    tauri::async_runtime::spawn_blocking(move || checkpoints::snapshot(&workspace, &state_dir).ok())
        .await
        .ok()
        .flatten()
}

async fn record_checkpoint(current: &CurrentSession, seq: u32, before: &str, after: &str) {
    let workspace = current.workspace.clone();
    let state_dir = current.state_dir.clone();
    let session_id = current.id.clone();
    let before = before.to_owned();
    let after = after.to_owned();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        checkpoints::record(&state_dir, &workspace, &session_id, seq, &before, &after)
    })
    .await;
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, SharedState>,
    input: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    let input = input.trim().to_owned();
    if input.is_empty() {
        return Err("消息不能为空".to_owned());
    }
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    // The turn belongs to the conversation that was open when it started,
    // even if the window switches before the row is refreshed.
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    // An archived conversation is read-only. A blank conversation has no
    // document yet, so a missing document is not an archive block.
    if let Some(current) = current.as_ref() {
        if matches!(
            sessions::is_archived(&current.state_dir, &current.workspace, &current.id),
            Ok(true)
        ) {
            return Err("已归档的对话不能继续；请先取消归档".to_owned());
        }
    }
    let token = {
        let mut cancel = state
            .cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cancel.is_some() {
            return Err("当前回合还在运行".to_owned());
        }
        let token = CancelToken::new();
        *cancel = Some(token.clone());
        token
    };
    let seq = engine.next_turn_seq();
    state.current_seq.store(seq, Ordering::Relaxed);
    let _ = app.emit(
        EV_TURN_STARTED,
        TurnStarted {
            seq,
            input: input.clone(),
        },
    );
    let checkpoint_session = current.clone();
    tauri::async_runtime::spawn(async move {
        let before = match checkpoint_session.as_ref() {
            Some(current) => take_snapshot(current).await,
            None => None,
        };

        // Live tool activity: the kernel keeps this turn's events in the log
        // while the turn runs, so a short poll is enough to show each call as
        // it starts and finishes. Sequence numbers stay monotonic across the
        // per-turn clear, so `live_last` filters out the previous turn.
        let live_events = engine.events().clone();
        let live_app = app.clone();
        let live_done = Arc::new(AtomicBool::new(false));
        let live_done_flag = live_done.clone();
        let live_last = Arc::new(std::sync::atomic::AtomicU64::new(
            live_events
                .snapshot()
                .last()
                .map(|record| record.seq)
                .unwrap_or(0),
        ));
        let poller = {
            let live_last = live_last.clone();
            tauri::async_runtime::spawn(async move {
                while !live_done_flag.load(Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    for record in live_events.snapshot() {
                        let mut last = live_last.load(Ordering::Relaxed);
                        if record.seq <= last {
                            continue;
                        }
                        last = record.seq;
                        live_last.store(last, Ordering::Relaxed);
                        match record.event {
                            Event::ToolStarted {
                                call_id,
                                tool,
                                args_preview,
                                ..
                            } => {
                                let _ = live_app.emit(
                                    EV_TOOL_STARTED,
                                    ToolStartedEvent {
                                        seq,
                                        call_id: call_id.get(),
                                        name: tool,
                                        args: args_preview,
                                    },
                                );
                            }
                            Event::ToolFinished {
                                call_id,
                                tool,
                                ok,
                                error,
                                duration_ms,
                                output_preview,
                                ..
                            } => {
                                let _ = live_app.emit(
                                    EV_TOOL_FINISHED,
                                    ToolFinishedEvent {
                                        seq,
                                        call_id: call_id.get(),
                                        name: tool,
                                        ok,
                                        duration_ms,
                                        error: error.map(|code| code.as_str().to_owned()),
                                        output: output_preview,
                                    },
                                );
                            }
                            Event::BackgroundStarted {
                                id, program, pid, ..
                            } => {
                                let _ = live_app.emit(
                                    EV_BACKGROUND_STARTED,
                                    BackgroundStartedEvent {
                                        seq,
                                        id,
                                        program,
                                        pid,
                                    },
                                );
                            }
                            Event::BackgroundResult {
                                id,
                                program,
                                output_path,
                                exit_code,
                                duration_ms,
                                ..
                            } => {
                                let _ = live_app.emit(
                                    EV_BACKGROUND_FINISHED,
                                    BackgroundFinishedEvent {
                                        seq,
                                        id,
                                        program,
                                        exit_code,
                                        duration_ms,
                                        output_path,
                                    },
                                );
                            }
                            Event::VerificationStarted { checks, .. } => {
                                let _ = live_app
                                    .emit(EV_TURN_VERIFYING, TurnVerifyingEvent { seq, checks });
                            }
                            _ => {}
                        }
                    }
                }
            })
        };

        let result = engine.run_turn(&input, token).await;
        live_done.store(true, Ordering::Relaxed);
        poller.abort();

        // The persisted turn is ready now. Show it and release the composer
        // before the checkpoint snapshot: that git commit can take seconds on
        // a large project and must not hold the raw streaming view.
        {
            let mut cancel = state
                .cancel
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *cancel = None;
        }
        match &result {
            Ok(_) => {
                if let Some(current) = current.as_ref() {
                    let meta = {
                        let _guard = state
                            .session_io
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        sessions::touch(&current.state_dir, &current.workspace, &current.id)
                    };
                    if let Some(meta) = meta {
                        let _ = app.emit(EV_SESSION_UPDATED, SessionUpdated { meta });
                    }
                }
                match engine.turn(seq) {
                    Some(turn) => {
                        let _ = app.emit(EV_TURN_FINISHED, TurnFinished { turn });
                    }
                    None => {
                        let _ = app.emit(
                            EV_TURN_FAILED,
                            TurnFailed {
                                seq,
                                message: "回合没有写入会话".to_owned(),
                            },
                        );
                    }
                }
            }
            Err(error) => {
                let _ = app.emit(
                    EV_TURN_FAILED,
                    TurnFailed {
                        seq,
                        message: error.to_string(),
                    },
                );
            }
        }

        // Undo and the change summary are post-turn artifacts; the view
        // reloads them when the pair is recorded.
        if let (Some(before), Some(current)) = (before, current.as_ref()) {
            if let Some(after) = take_snapshot(current).await {
                record_checkpoint(current, seq, &before, &after).await;
                if result.is_ok() {
                    let _ = app.emit(EV_TURN_CHANGES, TurnChanges { seq });
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_turn(state: State<'_, SharedState>) -> bool {
    let state = state.inner();
    let token = state
        .cancel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(token) = token {
        token.cancel();
        true
    } else {
        false
    }
}

#[tauri::command]
pub async fn undo_last_turn(state: State<'_, SharedState>) -> Result<(), String> {
    let state = state.inner().clone();
    ensure_idle(&state)?;
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    let seq = engine
        .history(None, 1)
        .turns
        .first()
        .map(|turn| turn.seq)
        .ok_or_else(|| "还没有可撤销的回合".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        checkpoints::undo(&current.state_dir, &current.workspace, &current.id, seq)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The files the latest turn changed, from its checkpoint pair. Empty when the
/// turn changed nothing, has no checkpoint, or was already undone.
#[tauri::command]
pub async fn turn_changes(
    state: State<'_, SharedState>,
    seq: Option<u32>,
) -> Result<Vec<checkpoints::FileChange>, String> {
    let state = state.inner().clone();
    let current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let (Some(current), Some(engine)) = (current, engine) else {
        return Ok(Vec::new());
    };
    let Some(seq) = seq.or_else(|| engine.history(None, 1).turns.first().map(|turn| turn.seq))
    else {
        return Ok(Vec::new());
    };
    let changes = tauri::async_runtime::spawn_blocking(move || {
        checkpoints::changes(&current.state_dir, &current.workspace, &current.id, seq)
    })
    .await
    .map_err(|error| error.to_string())?
    .unwrap_or_default();
    Ok(changes)
}

#[tauri::command]
pub fn load_turns(
    state: State<'_, SharedState>,
    before: Option<u32>,
    limit: Option<usize>,
) -> Result<TurnPageWithChanges, String> {
    let engine = state
        .inner()
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    let current = state
        .inner()
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let page = engine.history(before, limit.unwrap_or(30));
    let turns = match current {
        Some(current) => with_changes(
            &current.state_dir,
            &current.workspace,
            &current.id,
            page.turns,
        ),
        None => page
            .turns
            .into_iter()
            .map(|turn| TurnWithChanges {
                turn,
                changes: Vec::new(),
            })
            .collect(),
    };
    Ok(TurnPageWithChanges {
        turns,
        next_before: page.next_before,
    })
}

#[tauri::command]
pub fn respond_interaction(
    state: State<'_, SharedState>,
    id: String,
    option: String,
) -> Result<(), String> {
    state.inner().interactions.respond(&id, &option)
}

fn rig_dirs() -> Vec<PathBuf> {
    let mut dirs = rig::search_dirs();
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rigs");
        if dev.is_dir() && !dirs.iter().any(|dir| dir == &dev) {
            dirs.insert(0, dev);
        }
    }
    dirs
}

/// The Windows canonical form carries a `\?\` prefix; people do not.
fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

/// A safe landing workspace, never the whole user home. The header picker
/// switches to a real project.
fn default_workspace() -> PathBuf {
    let Some(home) = rig::home_dir() else {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    };
    let documents = home.join("Documents");
    let parent = if documents.is_dir() { documents } else { home };
    let workspace = parent.join("Keelson");
    let _ = std::fs::create_dir_all(&workspace);
    workspace
}

fn is_home_dir(path: &Path) -> bool {
    let Some(home) = rig::home_dir() else {
        return false;
    };
    normalized(path) == normalized(&home)
}

fn normalized(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
