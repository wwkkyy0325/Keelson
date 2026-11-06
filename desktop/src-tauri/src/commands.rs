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

use crate::config;
use crate::state::DesktopState;

pub const EV_TURN_STARTED: &str = "keelson:turn-started";
pub const EV_TEXT_DELTA: &str = "keelson:text-delta";
pub const EV_TURN_FINISHED: &str = "keelson:turn-finished";
pub const EV_TURN_FAILED: &str = "keelson:turn-failed";

type SharedState = Arc<DesktopState>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub onboarded: bool,
    pub has_key: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub locale: String,
    pub last_workspace: String,
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

#[tauri::command]
pub fn setup_state(state: State<'_, SharedState>) -> SetupState {
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let discovery = rig::discover(&rig_dirs());
    SetupState {
        onboarded: config.onboarded,
        has_key: config::load_key().ok().flatten().is_some(),
        provider: config.provider,
        base_url: config.base_url,
        model: config.model,
        locale: config.locale,
        last_workspace: config.last_workspace,
        problems: discovery
            .errors
            .iter()
            .map(|error| error.to_string())
            .collect(),
    }
}

#[tauri::command]
pub fn save_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("密钥不能为空".to_owned());
    }
    config::save_key(key)
}

#[tauri::command]
pub fn save_model(
    state: State<'_, SharedState>,
    provider: String,
    base_url: String,
    model: String,
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
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.provider = provider;
    config.base_url = base_url.trim().to_owned();
    config.model = model.trim().to_owned();
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

#[tauri::command]
pub async fn open_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: Option<String>,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    if state
        .cancel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some()
    {
        return Err("当前回合还在运行，请先取消".to_owned());
    }
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
        Some(config::load_key()?.ok_or_else(|| "还没有保存密钥；请重新运行初始化".to_owned())?)
    };
    let rig_name = if provider == "mock" {
        "coding"
    } else {
        "coding-live"
    };
    let discovery = rig::discover(&rig_dirs());
    let rig = discovery
        .rigs
        .into_iter()
        .find(|rig| rig.name == rig_name)
        .ok_or_else(|| format!("找不到 rig `{rig_name}`"))?;

    let wanted = workspace
        .filter(|path| !path.trim().is_empty())
        .or_else(|| Some(config.last_workspace.clone()).filter(|path| !path.trim().is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(default_workspace);
    let workspace = std::fs::canonicalize(&wanted).unwrap_or(wanted);
    let state_dir = rig::state_dir().unwrap_or_else(|| workspace.join(".keel"));
    let store_path = state_dir
        .join("sessions")
        .join(workspace_key(&workspace))
        .join("default.json");

    let model = key.map(|api_key| ModelOverride {
        provider,
        model: config.model.clone(),
        base_url: Some(config.base_url.clone()).filter(|url| !url.trim().is_empty()),
        api_key,
    });

    let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel(256);
    let engine = Engine::open(OpenRequest {
        rig,
        workspace: workspace.clone(),
        state_dir,
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
        let mut config = state
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        config.last_workspace = workspace_label.clone();
        let _ = config::save_config(&config);
    }

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
        session_id: "default".to_owned(),
        turns: page.turns,
        next_before: page.next_before,
    })
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
            Ok(_) => match engine.turn(seq) {
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
            },
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

fn default_workspace() -> PathBuf {
    rig::home_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// A stable, filesystem-safe key for a workspace, so sessions from different
/// directories cannot collide or refuse each other.
fn workspace_key(path: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_keys_are_stable_and_distinct() {
        let first = workspace_key(Path::new("C:/work/one"));
        assert_eq!(first, workspace_key(Path::new("C:/work/one")));
        assert_ne!(first, workspace_key(Path::new("C:/work/two")));
        assert_eq!(first.len(), 16);
    }
}
