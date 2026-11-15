//! Tauri commands: setup, the desktop's only write path into the engine, and
//! the session read model.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use keel_core::{CancelToken, Event, PermissionLevel};
use keelson::engine::{
    seed_explainer_kit, ContextPoint, Engine, ModelDelta, ModelOverride, OpenRequest, Store,
    TurnView,
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
pub const EV_MODEL_FINISHED: &str = "keelson:model-finished";
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
    /// The selected model's context capacity, when the host knows it.
    pub model_context: Option<u32>,
    pub locale: String,
    pub last_workspace: String,
    pub key_provider: String,
    pub key_providers: Vec<String>,
    pub manual: Option<config::ManualEndpoint>,
    /// The keychain slot the current connection reads.
    pub key_slot: String,
    /// Named setups and the one the current connection came from.
    pub presets: Vec<config::Preset>,
    pub active_preset: Option<String>,
    /// A SearXNG base URL, the last web-search backend after the model's own.
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

/// One model call finished; the running turn's cost line adds this usage.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelFinishedEvent {
    seq: u32,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
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

/// One file copied into the conversation's read-only attachment folder.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    pub path: String,
    pub size: u64,
}

const MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 10;
/// A folder is a read-only snapshot, not a working copy. These bounds keep a
/// drop from turning into a multi-gigabyte copy; past them the right move is
/// to open the folder as the workspace.
const MAX_FOLDER_FILES: usize = 200;
const MAX_FOLDER_BYTES: u64 = 200 * 1024 * 1024;
const MAX_FOLDER_DEPTH: usize = 8;
const MAX_BATCH_BYTES: u64 = 400 * 1024 * 1024;

/// Attachments live in the host state directory, not in the workspace: a drop
/// must not appear as a change the turn made, and undo must not delete it.
fn attachments_dir(state_dir: &Path, workspace: &Path, session_id: &str) -> PathBuf {
    state_dir
        .join("attachments")
        .join(sessions::workspace_key(workspace))
        .join(session_id)
}

/// `name (2).ext` when the name is taken; the extension stays.
fn unique_name(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_owned());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    for count in 2u32.. {
        let next = dir.join(format!("{stem} ({count}){extension}"));
        if !next.exists() {
            return next;
        }
    }
    unreachable!("an unused name always exists")
}

async fn copy_attachment(dir: &Path, source: &Path, max_bytes: u64) -> Result<Attachment, String> {
    let metadata = tokio::fs::metadata(source)
        .await
        .map_err(|error| format!("无法读取 {}：{error}", source.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} 不是文件", source.display()));
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "{} 超过 {} MB 上限",
            source.display(),
            max_bytes / (1024 * 1024)
        ));
    }
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| format!("无法从 {} 取文件名", source.display()))?;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("无法创建附件目录 {}：{error}", dir.display()))?;
    // Re-attaching the same unchanged file reuses the copy instead of
    // numbering a duplicate. A changed file with the same name still gets a
    // new number.
    let same_name = dir.join(&name);
    if let Ok(existing) = tokio::fs::read(&same_name).await {
        if existing.len() as u64 == metadata.len() {
            if let Ok(source_bytes) = tokio::fs::read(source).await {
                if source_bytes == existing {
                    return Ok(Attachment {
                        name,
                        path: display_path(&same_name),
                        size: metadata.len(),
                    });
                }
            }
        }
    }
    let destination = unique_name(dir, &name);
    tokio::fs::copy(source, &destination)
        .await
        .map_err(|error| format!("无法复制 {}：{error}", source.display()))?;
    Ok(Attachment {
        name: destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(name),
        path: display_path(&destination),
        size: metadata.len(),
    })
}

/// A tree's bounds. `collect_tree` enforces them while walking, so an
/// oversized folder fails before anything is copied.
struct TreeLimits {
    files: usize,
    bytes: u64,
    depth: usize,
}

/// The files under `root` as `(relative path, bytes)`, sorted, with symlinks
/// and `.git` skipped.
fn collect_tree(root: &Path, limits: &TreeLimits) -> Result<(Vec<(String, u64)>, u64), String> {
    fn walk(
        root: &Path,
        dir: &Path,
        depth: usize,
        limits: &TreeLimits,
        entries: &mut Vec<(String, u64)>,
        total: &mut u64,
    ) -> Result<(), String> {
        if depth > limits.depth {
            return Err(format!(
                "{} 的目录层级超过 {} 层",
                root.display(),
                limits.depth
            ));
        }
        let mut listing = std::fs::read_dir(dir)
            .map_err(|error| format!("无法读取 {}：{error}", dir.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("无法读取 {}：{error}", dir.display()))?;
        listing.sort_by_key(|entry| entry.file_name());
        for entry in listing {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if entry.file_name().to_string_lossy() == ".git" {
                    continue;
                }
                walk(root, &path, depth + 1, limits, entries, total)?;
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            *total += size;
            if entries.len() + 1 > limits.files {
                return Err(format!(
                    "{} 的文件数超过 {}，无法作为附件；请把该目录作为工作区打开",
                    root.display(),
                    limits.files
                ));
            }
            if *total > limits.bytes {
                return Err(format!(
                    "{} 超过 {} MB，无法作为附件；请把该目录作为工作区打开",
                    root.display(),
                    limits.bytes / (1024 * 1024)
                ));
            }
            entries.push((relative, size));
        }
        Ok(())
    }

    let mut entries = Vec::new();
    let mut total = 0u64;
    walk(root, root, 0, limits, &mut entries, &mut total)?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok((entries, total))
}

/// Two trees are the same when every relative path has the same bytes.
fn same_tree(left: &Path, right: &Path) -> bool {
    let open = TreeLimits {
        files: usize::MAX,
        bytes: u64::MAX,
        depth: usize::MAX,
    };
    let Ok((left_entries, _)) = collect_tree(left, &open) else {
        return false;
    };
    let Ok((right_entries, _)) = collect_tree(right, &open) else {
        return false;
    };
    if left_entries.len() != right_entries.len() {
        return false;
    }
    left_entries.iter().zip(right_entries.iter()).all(
        |((left_rel, left_size), (right_rel, right_size))| {
            left_rel == right_rel
                && left_size == right_size
                && std::fs::read(left.join(left_rel)).ok()
                    == std::fs::read(right.join(right_rel)).ok()
        },
    )
}

fn copy_folder(dir: &Path, source: &Path) -> Result<Attachment, String> {
    let (entries, total) = collect_tree(
        source,
        &TreeLimits {
            files: MAX_FOLDER_FILES,
            bytes: MAX_FOLDER_BYTES,
            depth: MAX_FOLDER_DEPTH,
        },
    )?;
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| format!("无法从 {} 取目录名", source.display()))?;
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("无法创建附件目录 {}：{error}", dir.display()))?;

    // The same unchanged folder dropped again reuses its snapshot; a changed
    // one gets a number, exactly like a file.
    let same_name = dir.join(&name);
    if same_name.is_dir() && same_tree(source, &same_name) {
        return Ok(Attachment {
            name,
            path: display_path(&same_name),
            size: total,
        });
    }
    let destination = unique_name(dir, &name);
    std::fs::create_dir_all(&destination)
        .map_err(|error| format!("无法创建 {}：{error}", destination.display()))?;
    for (relative, _) in &entries {
        let from = source.join(relative);
        let to = destination.join(relative);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("无法创建 {}：{error}", parent.display()))?;
        }
        std::fs::copy(&from, &to)
            .map_err(|error| format!("无法复制 {}：{error}", from.display()))?;
    }
    Ok(Attachment {
        name: destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(name),
        path: display_path(&destination),
        size: total,
    })
}

#[tauri::command]
pub fn setup_state(state: State<'_, SharedState>) -> SetupState {
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let discovery = rig::discover(&rig_dirs());
    let key_slot = active_key_slot(&config);
    let has_key = config::load_key(&key_slot, &key_slot)
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
        model_context: config.model_context,
        locale: config.locale,
        last_workspace: config.last_workspace,
        key_provider: config.key_provider,
        key_providers: config.key_providers,
        manual: config.manual,
        key_slot,
        presets: config.presets,
        active_preset: config.active_preset,
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
    key_slot: Option<String>,
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
    // An empty slot means "use the provider id", which is what every catalog
    // provider does; a manual endpoint may name its own slot so two
    // self-hosted services keep their keys apart.
    config.key_slot = key_slot
        .map(|slot| slot.trim().to_owned())
        .filter(|slot| !slot.is_empty());
    // Editing the connection by hand detaches it from the preset it came
    // from; the menu must stop marking that preset as active.
    config.active_preset = None;
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
            context: config.model_context,
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

/// Reopens a conversation under its current settings.
///
/// The empty-conversation rig picker needs this: a blank conversation has no
/// document yet, so `open_session` would refuse it. The id stays the same, so
/// the composer keeps whatever the user has typed.
#[tauri::command]
pub async fn reopen_session(
    app: AppHandle,
    state: State<'_, SharedState>,
    workspace: Option<String>,
    session_id: String,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    let workspace = resolve_workspace(&state, workspace);
    open_inner(app, state, workspace, Some(session_id), true).await
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

/// Copies dropped or picked files into the conversation's attachment folder.
/// Every file is validated before anything is copied, so a rejected batch
/// leaves no half-copied set behind.
#[tauri::command]
pub async fn attach_files(
    state: State<'_, SharedState>,
    workspace: Option<String>,
    session_id: String,
    paths: Vec<String>,
) -> Result<Vec<Attachment>, String> {
    if !sessions::valid_id(&session_id) {
        return Err("会话 id 不合法".to_owned());
    }
    if paths.len() > MAX_ATTACHMENTS {
        return Err(format!("一次最多添加 {MAX_ATTACHMENTS} 个文件或文件夹"));
    }
    let state = state.inner().clone();
    let workspace = resolve_workspace(&state, workspace);
    let state_dir = state_dir_for(&workspace);
    let dir = attachments_dir(&state_dir, &workspace, &session_id);

    enum Planned {
        File(PathBuf),
        Folder(PathBuf),
    }

    let mut planned = Vec::new();
    let mut batch_bytes = 0u64;
    for path in &paths {
        let source = PathBuf::from(path);
        let metadata = tokio::fs::metadata(&source)
            .await
            .map_err(|error| format!("无法读取 {}：{error}", source.display()))?;
        if source
            .file_name()
            .map(|name| name.to_string_lossy().trim().is_empty())
            .unwrap_or(true)
        {
            return Err(format!("无法从 {} 取名字", source.display()));
        }
        if metadata.is_dir() {
            // Walking now means a folder over a limit fails before anything is
            // copied, and its size counts toward the batch.
            let (_, total) = collect_tree(
                &source,
                &TreeLimits {
                    files: MAX_FOLDER_FILES,
                    bytes: MAX_FOLDER_BYTES,
                    depth: MAX_FOLDER_DEPTH,
                },
            )?;
            batch_bytes += total;
            planned.push(Planned::Folder(source));
            continue;
        }
        if !metadata.is_file() {
            return Err(format!("{} 既不是文件也不是文件夹", source.display()));
        }
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(format!(
                "{} 超过 {} MB 上限",
                source.display(),
                MAX_ATTACHMENT_BYTES / (1024 * 1024)
            ));
        }
        batch_bytes += metadata.len();
        planned.push(Planned::File(source));
    }
    if batch_bytes > MAX_BATCH_BYTES {
        return Err(format!(
            "这一批附件超过 {} MB，无法一次复制",
            MAX_BATCH_BYTES / (1024 * 1024)
        ));
    }

    let mut added = Vec::with_capacity(planned.len());
    for plan in planned {
        match plan {
            Planned::File(source) => {
                added.push(copy_attachment(&dir, &source, MAX_ATTACHMENT_BYTES).await?)
            }
            Planned::Folder(source) => added.push(copy_folder(&dir, &source)?),
        }
    }
    Ok(added)
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
    sessions::remove(&state_dir, &workspace, &session_id)?;
    // The attachments and screenshots belong to the conversation; nothing
    // else reads them.
    let _ = std::fs::remove_dir_all(attachments_dir(&state_dir, &workspace, &session_id));
    keelson::engine::remove_session_screens(&state_dir, &workspace, &session_id);
    Ok(())
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
    if is_current {
        // Keep the send-path flag in step with the document.
        if let Some(current) = state
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            current.archived = archived;
        }
    }
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
    let key_slot = active_key_slot(&config);
    let key = Some(
        config::load_key(&key_slot, &key_slot)?
            .ok_or_else(|| "还没有保存密钥；请重新运行初始化".to_owned())?,
    );
    // The connected model's own provider search runs first. A saved DeepSeek
    // key is the fallback when that search does not exist or fails; without
    // one, the configured SearXNG URL is the last resort.
    let search_key = config::load_key("deepseek", "deepseek").ok().flatten();
    let fallback = rig_name_for(&provider);
    let preferred = chosen_rig(&config, fallback);

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
    // The send path reads this flag instead of loading the document again.
    let archived = archived_in(&session_list, &session_id);
    let wanted = rig_for_session(&session_list, &session_id, &preferred);
    let discovery = rig::discover(&rig_dirs());
    let rig = discovery
        .rigs
        .iter()
        .find(|rig| rig.name == wanted)
        .or_else(|| discovery.rigs.iter().find(|rig| rig.name == fallback))
        .cloned()
        .ok_or_else(|| format!("找不到 rig `{wanted}`"))?;
    let store_path = sessions::session_path(&state_dir, &workspace, &session_id);
    // An explainer project starts from the kit: without it the renderer and
    // the data checks have nothing to run. A series keeps its kit per episode,
    // created by `new_episode`, so the series root is not seeded.
    let is_series =
        workspace.join("series.json").is_file() || workspace.join("series.md").is_file();
    if rig.name == "explainer" && !is_series {
        if let Err(error) = seed_explainer_kit(&workspace) {
            eprintln!("explainer kit was not seeded: {error}");
        }
    }

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
        search_key,
    })
    .map_err(|error| error.to_string())?;
    // Dropped files are readable by the agent and cannot be written back to:
    // the folder is a read-only root, and it is outside the workspace so it
    // never shows up in the turn's checkpoint diff.
    let attachments = attachments_dir(&state_dir, &workspace, &session_id);
    if std::fs::create_dir_all(&attachments).is_ok()
        && engine
            .add_root(&attachments, keel_core::AccessKind::Read)
            .is_err()
    {
        eprintln!("attachments root was not added: {}", attachments.display());
    }
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
            archived,
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

/// The rig a new conversation starts with: the chosen file when it is set,
/// otherwise the host's default.
fn chosen_rig(config: &config::DesktopConfig, fallback: &str) -> String {
    let chosen = config.rig.trim();
    if chosen.is_empty() {
        fallback.to_owned()
    } else {
        chosen.to_owned()
    }
}

/// The archive flag of one conversation, read from the project list at open.
/// The send path uses it instead of parsing the document.
fn archived_in(session_list: &[sessions::SessionMeta], session_id: &str) -> bool {
    session_list
        .iter()
        .find(|meta| meta.id == session_id)
        .is_some_and(|meta| meta.archived)
}

/// A conversation keeps the rig it started with. The setting only chooses the
/// rig for a conversation that has none yet; a recorded rig whose file is gone
/// falls back to the setting.
fn rig_for_session(
    session_list: &[sessions::SessionMeta],
    session_id: &str,
    preferred: &str,
) -> String {
    session_list
        .iter()
        .find(|meta| meta.id == session_id)
        .map(|meta| meta.rig.trim())
        .filter(|rig| !rig.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| preferred.to_owned())
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

/// A preset id is opaque and stable; the name is what the user edits.
fn new_preset_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("p{now}")
}

/// Saves the current connection as a preset, replacing one with the same
/// name. Keys are not copied: the preset records the slot it reads, so two
/// presets can share one key and one key can serve several models.
#[tauri::command]
pub fn save_preset(state: State<'_, SharedState>, name: String) -> Result<(), String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err("预设名不能为空".to_owned());
    }
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let rig = chosen_rig(&config, rig_name_for(&config.provider));
    let existing = config.presets.iter().position(|preset| preset.name == name);
    let id = existing
        .map(|index| config.presets[index].id.clone())
        .unwrap_or_else(new_preset_id);
    let preset = config::Preset {
        id: id.clone(),
        name,
        rig,
        provider: config.provider.clone(),
        provider_id: config.provider_id.clone(),
        provider_name: config.provider_name.clone(),
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        model_context: config.model_context,
        key_slot: config.key_slot.clone(),
        permission: config.permission.clone(),
        search_url: config.search_url.clone(),
    };
    match existing {
        Some(index) => config.presets[index] = preset,
        None => config.presets.push(preset),
    }
    config.active_preset = Some(id);
    config::save_config(&config)
}

/// Switches the connection to a preset. The rig part lands with the next
/// conversation; a conversation that already has turns keeps its rig.
#[tauri::command]
pub fn apply_preset(state: State<'_, SharedState>, id: String) -> Result<(), String> {
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let preset = config
        .presets
        .iter()
        .find(|preset| preset.id == id)
        .cloned()
        .ok_or_else(|| "找不到这个预设".to_owned())?;
    if !preset.rig.trim().is_empty() {
        let discovery = rig::discover(&rig_dirs());
        if !discovery.rigs.iter().any(|rig| rig.name == preset.rig) {
            return Err(format!(
                "预设 `{}` 使用的 rig `{}` 不存在",
                preset.name, preset.rig
            ));
        }
    }
    let slot = preset
        .key_slot
        .clone()
        .map(|slot| slot.trim().to_owned())
        .filter(|slot| !slot.is_empty())
        .unwrap_or_else(|| preset.provider_id.clone());
    if config::load_key(&slot, &slot)?.is_none() {
        return Err(format!("预设 `{}` 的密钥还没有保存", preset.name));
    }
    copy_preset(&mut config, &preset, slot);
    config::save_config(&config)
}

/// Copies a preset's connection into the config. The caller has already
/// validated the rig and the key; this part is pure and tested.
fn copy_preset(config: &mut config::DesktopConfig, preset: &config::Preset, slot: String) {
    config.provider = preset.provider.clone();
    config.provider_id = preset.provider_id.clone();
    config.provider_name = preset.provider_name.clone();
    config.base_url = preset.base_url.clone();
    config.model = preset.model.clone();
    config.model_context = preset.model_context;
    config.key_slot = Some(slot);
    config.permission = preset.permission.clone();
    config.search_url = preset.search_url.clone();
    // An empty rig means "whatever is configured"; a preset saved before the
    // rig existed must not blank it.
    if !preset.rig.trim().is_empty() {
        config.rig = preset.rig.clone();
    }
    config.active_preset = Some(preset.id.clone());
}

#[tauri::command]
pub fn delete_preset(state: State<'_, SharedState>, id: String) -> Result<(), String> {
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.presets.retain(|preset| preset.id != id);
    if config.active_preset.as_deref() == Some(id.as_str()) {
        config.active_preset = None;
    }
    config::save_config(&config)
}

fn rig_name_for(_provider: &str) -> &'static str {
    "coding-live"
}

/// The keychain slot the current connection reads. Catalog providers use
/// their id; a manual endpoint may name its own slot so two self-hosted
/// services do not overwrite each other's key.
fn active_key_slot(config: &config::DesktopConfig) -> String {
    config
        .key_slot
        .clone()
        .map(|slot| slot.trim().to_owned())
        .filter(|slot| !slot.is_empty())
        .unwrap_or_else(|| config.provider_id.clone())
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
pub fn stop_background(state: State<'_, SharedState>, id: String) -> Result<bool, String> {
    let engine = state
        .engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "还没有打开会话".to_owned())?;
    Ok(engine.stop_background(&id))
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
    // An archived conversation is read-only. The flag was read when the
    // conversation opened and is kept current by the archive command, so the
    // send path does not parse the document.
    if let Some(current) = current.as_ref() {
        if current.archived {
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
                    // Only the new tail: cloning the whole turn's event log
                    // every 120 ms costs more as a tool-heavy turn grows.
                    for record in live_events.since(live_last.load(Ordering::Relaxed)) {
                        live_last.store(record.seq, Ordering::Relaxed);
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
                            Event::ModelFinished { usage, .. } => {
                                let _ = live_app.emit(
                                    EV_MODEL_FINISHED,
                                    ModelFinishedEvent {
                                        seq,
                                        input_tokens: usage.input_tokens,
                                        output_tokens: usage.output_tokens,
                                        cached_input_tokens: usage.cached_input_tokens,
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

#[tauri::command]
pub fn dismiss_interaction(state: State<'_, SharedState>, id: String) -> Result<(), String> {
    state.inner().interactions.dismiss(&id)
}

#[tauri::command]
pub fn pending_interaction(
    state: State<'_, SharedState>,
) -> Option<crate::state::InteractionRequested> {
    state.inner().interactions.current()
}

fn rig_dirs() -> Vec<PathBuf> {
    let mut dirs = rig::search_dirs();
    // A run from the source tree (debug or release) finds the repository's
    // rigs next to the crate. A packaged install does not have that path, so
    // it is skipped there and rigs come from `~/.keelson/rigs` instead.
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rigs");
    if source.is_dir() && !dirs.iter().any(|dir| dir == &source) {
        dirs.insert(0, source);
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        collect_tree, copy_attachment, copy_folder, copy_preset, rig_dirs, rig_for_session,
    };
    use crate::config::{DesktopConfig, Preset};
    use crate::sessions::SessionMeta;

    fn meta(id: &str, rig: &str) -> SessionMeta {
        SessionMeta {
            id: id.to_owned(),
            title: String::new(),
            archived: false,
            rig: rig.to_owned(),
            created_at: 0,
            updated_at: 0,
            turns: 0,
        }
    }

    #[test]
    fn the_open_conversation_reports_its_archived_flag() {
        let mut archived = meta("s-1", "coding-live");
        archived.archived = true;
        assert!(super::archived_in(&[archived], "s-1"));
        assert!(!super::archived_in(&[meta("s-2", "")], "s-2"));
        assert!(!super::archived_in(&[], "s-3"));
    }

    #[test]
    fn a_conversation_keeps_the_rig_it_started_with() {
        let list = vec![meta("s-1", "general"), meta("s-2", "")];
        assert_eq!(rig_for_session(&list, "s-1", "coding-live"), "general");
    }

    #[test]
    fn a_conversation_without_a_rig_takes_the_setting() {
        let list = vec![meta("s-1", "  ")];
        assert_eq!(rig_for_session(&list, "s-1", "coding-live"), "coding-live");
        assert_eq!(rig_for_session(&[], "s-9", "general"), "general");
    }

    #[tokio::test]
    async fn an_unchanged_file_is_reused_and_a_changed_one_is_numbered() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let dir = temp.path().join("attachments");
        let source = temp.path().join("notes.txt");
        std::fs::write(&source, "hello").expect("the source");

        let first = copy_attachment(&dir, &source, 1024)
            .await
            .expect("the first copy");
        let again = copy_attachment(&dir, &source, 1024)
            .await
            .expect("the same content");
        assert_eq!(first.name, "notes.txt");
        assert_eq!(again.name, "notes.txt");
        assert_eq!(again.path, first.path);
        assert_eq!(
            std::fs::read_to_string(&first.path).expect("the copy"),
            "hello"
        );

        std::fs::write(&source, "hello again").expect("the edit");
        let changed = copy_attachment(&dir, &source, 1024)
            .await
            .expect("the changed file");
        assert_eq!(changed.name, "notes (2).txt");
        assert_eq!(
            std::fs::read_to_string(&changed.path).expect("the copy"),
            "hello again"
        );
    }

    #[test]
    fn a_folder_is_copied_with_its_structure_and_reused() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let source = temp.path().join("docs");
        std::fs::create_dir_all(source.join("sub")).expect("the sub folder");
        std::fs::write(source.join("a.md"), "alpha").expect("the note");
        std::fs::write(source.join("sub/b.txt"), "beta").expect("the leaf");
        let dir = temp.path().join("attachments");

        let first = copy_folder(&dir, &source).expect("the first copy");
        assert_eq!(first.name, "docs");
        assert_eq!(first.size, 9);
        assert_eq!(
            std::fs::read_to_string(Path::new(&first.path).join("a.md")).expect("the note"),
            "alpha"
        );
        assert_eq!(
            std::fs::read_to_string(Path::new(&first.path).join("sub/b.txt")).expect("the leaf"),
            "beta"
        );

        let again = copy_folder(&dir, &source).expect("the same folder");
        assert_eq!(again.path, first.path);

        std::fs::write(source.join("a.md"), "alpha changed").expect("the edit");
        let changed = copy_folder(&dir, &source).expect("the changed folder");
        assert_eq!(changed.name, "docs (2)");
        assert_eq!(
            std::fs::read_to_string(Path::new(&changed.path).join("a.md")).expect("the note"),
            "alpha changed"
        );
    }

    #[test]
    fn a_folder_over_the_file_limit_is_refused() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let source = temp.path().join("many");
        std::fs::create_dir_all(&source).expect("the folder");
        for index in 0..3 {
            std::fs::write(source.join(format!("{index}.txt")), "x").expect("a file");
        }

        let error = collect_tree(
            &source,
            &super::TreeLimits {
                files: 2,
                bytes: u64::MAX,
                depth: 8,
            },
        )
        .expect_err("over the file limit");

        assert!(error.contains("文件数"), "{error}");
    }

    #[test]
    fn a_source_run_finds_the_repository_rigs() {
        let dirs = rig_dirs();
        assert!(
            dirs.iter().any(|dir| dir.join("figure.toml").is_file()),
            "the repository rigs are on the search path: {dirs:?}"
        );
    }

    fn preset(rig: &str) -> Preset {
        Preset {
            id: "p1".to_owned(),
            name: "火山".to_owned(),
            rig: rig.to_owned(),
            provider: "openai-compatible".to_owned(),
            provider_id: "custom".to_owned(),
            provider_name: "Volcengine".to_owned(),
            base_url: "https://ark.example/api/v3".to_owned(),
            model: "doubao-pro".to_owned(),
            model_context: Some(131_072),
            key_slot: Some("volcengine".to_owned()),
            permission: Some("workspace_auto".to_owned()),
            search_url: None,
        }
    }

    #[test]
    fn applying_a_preset_replaces_the_whole_connection() {
        let mut config = DesktopConfig {
            model: "old-model".to_owned(),
            rig: "coding-live".to_owned(),
            ..DesktopConfig::default()
        };

        copy_preset(&mut config, &preset("office-live"), "volcengine".to_owned());

        assert_eq!(config.model, "doubao-pro");
        assert_eq!(config.base_url, "https://ark.example/api/v3");
        assert_eq!(config.model_context, Some(131_072));
        assert_eq!(config.key_slot.as_deref(), Some("volcengine"));
        assert_eq!(config.permission.as_deref(), Some("workspace_auto"));
        assert_eq!(config.rig, "office-live");
        assert_eq!(config.active_preset.as_deref(), Some("p1"));
    }

    #[test]
    fn a_preset_without_a_rig_keeps_the_current_one() {
        let mut config = DesktopConfig {
            rig: "coding-live".to_owned(),
            ..DesktopConfig::default()
        };

        copy_preset(&mut config, &preset(""), "custom".to_owned());

        assert_eq!(config.rig, "coding-live");
        assert_eq!(config.active_preset.as_deref(), Some("p1"));
    }

    #[tokio::test]
    async fn an_oversized_attachment_is_refused() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let source = temp.path().join("big.txt");
        std::fs::write(&source, "0123456789").expect("the source");

        let error = copy_attachment(&temp.path().join("attachments"), &source, 5)
            .await
            .expect_err("a file over the cap is refused");

        assert!(error.contains("上限"), "{error}");
    }
}
