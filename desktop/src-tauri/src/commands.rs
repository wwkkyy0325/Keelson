//! Tauri commands: setup, the desktop's only write path into the engine, and
//! the session read model.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use keel_core::CancelToken;
use keelson::engine::{Engine, ModelOverride, OpenRequest, TurnPage, TurnView};
use keelson::rig;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::{self, ProjectEntry};
use crate::sessions::{self, SessionMeta};
use crate::state::{CurrentSession, DesktopState};

pub const EV_TURN_STARTED: &str = "keelson:turn-started";
pub const EV_TEXT_DELTA: &str = "keelson:text-delta";
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
    pub problems: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub rig: String,
    pub workspace: String,
    pub session_id: String,
    pub turns: Vec<TurnView>,
    pub next_before: Option<u32>,
    /// Every conversation in the project, newest first.
    pub sessions: Vec<SessionMeta>,
    pub projects: Vec<ProjectEntry>,
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
    let has_key = config::load_key().ok().flatten().is_some()
        && (config.key_provider.is_empty()
            || config.provider_id.is_empty()
            || config.key_provider == config.provider_id);
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
    config::save_key(key)?;
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.key_provider = provider_id.to_owned();
    config::save_config(&config)
}

#[tauri::command]
pub fn save_model(
    state: State<'_, SharedState>,
    provider: String,
    provider_id: String,
    provider_name: String,
    base_url: String,
    model: String,
    context: Option<u32>,
) -> Result<(), String> {
    if !matches!(
        provider.as_str(),
        "anthropic-compatible" | "anthropic" | "openai-compatible" | "openai" | "mock"
    ) {
        return Err(format!("未知的模型提供方 `{provider}`"));
    }
    if provider != "mock" && (base_url.trim().is_empty() || model.trim().is_empty()) {
        return Err("接口地址和模型名不能为空".to_owned());
    }
    if provider != "mock" && provider_id.trim().is_empty() {
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
    open_inner(app, state, workspace, session_id).await
}

/// Creates a conversation and opens it.
#[tauri::command]
pub async fn new_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: Option<String>,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    ensure_idle(&state)?;
    let workspace = resolve_workspace(&state, workspace);
    let state_dir = state_dir_for(&workspace);
    let rig_name = {
        let config = state
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        rig_name_for(&config.provider)
    };
    let meta = {
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions::create(&state_dir, &workspace, rig_name)?
    };
    open_inner(app, state, workspace, Some(meta.id)).await
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
    let key = if provider == "mock" {
        None
    } else {
        if !config.key_provider.is_empty()
            && !config.provider_id.is_empty()
            && config.key_provider != config.provider_id
        {
            let provider_label = if config.provider_name.is_empty() {
                config.provider_id.clone()
            } else {
                config.provider_name.clone()
            };
            return Err(format!(
                "当前密钥属于 `{}`，请先在设置里填写 `{}` 的密钥",
                config.key_provider, provider_label
            ));
        }
        Some(config::load_key()?.ok_or_else(|| "还没有保存密钥；请重新运行初始化".to_owned())?)
    };
    let rig_name = rig_name_for(&provider);
    let discovery = rig::discover(&rig_dirs());
    let rig = discovery
        .rigs
        .into_iter()
        .find(|rig| rig.name == rig_name)
        .ok_or_else(|| format!("找不到 rig `{rig_name}`"))?;

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
                if !sessions::session_path(&state_dir, &workspace, &id).is_file() {
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

    let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel(256);
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir: state_dir.clone(),
        store_path,
        interactions: state.interactions.clone(),
        deltas: Some(delta_tx),
        model,
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
            state_dir,
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
        while let Some(text) = delta_rx.recv().await {
            let seq = seq.load(Ordering::Relaxed);
            if seq != 0 {
                let _ = app_handle.emit(EV_TEXT_DELTA, TextDelta { seq, text });
            }
        }
    });

    Ok(SessionView {
        rig: rig_name.to_owned(),
        workspace: workspace_label,
        session_id,
        turns: page.turns,
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

fn rig_name_for(provider: &str) -> &'static str {
    if provider == "mock" {
        "coding"
    } else {
        "coding-live"
    }
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
    tauri::async_runtime::spawn(async move {
        let result = engine.run_turn(&input, token).await;
        {
            let mut cancel = state
                .cancel
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *cancel = None;
        }
        match result {
            Ok(_) => {
                if let Some(current) = current {
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
pub fn load_turns(
    state: State<'_, SharedState>,
    before: Option<u32>,
    limit: Option<usize>,
) -> Result<TurnPage, String> {
    let engine = state
        .inner()
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    Ok(engine.history(before, limit.unwrap_or(30)))
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
