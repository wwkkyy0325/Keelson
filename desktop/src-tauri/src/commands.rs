//! Tauri commands: setup, the desktop's only write path into the engine, and
//! the session read model.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

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
use crate::records;
use crate::sessions::{self, SessionMeta};
use crate::state::{Connection, CurrentSession, DesktopState, OpenConversation, RunningTurn};
use crate::taskdoc;

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

/// An emitter that names the conversation it speaks for.
///
/// Several conversations can have a turn running at once, so every event
/// carries the session it belongs to and the view drops the ones that are not
/// the conversation on screen. Wrapping the handle keeps the emit sites free of
/// that detail: `app.emit(...)` reads the same as before.
#[derive(Clone)]
struct SessionApp {
    app: AppHandle,
    session: String,
}

impl SessionApp {
    fn new(app: AppHandle, session: impl Into<String>) -> Self {
        Self {
            app,
            session: session.into(),
        }
    }

    fn emit<S: Serialize>(&self, event: &str, payload: S) {
        let mut value = serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(map) = &mut value {
            map.insert(
                "sessionId".to_owned(),
                serde_json::Value::String(self.session.clone()),
            );
        }
        let _ = self.app.emit(event, value);
    }
}

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
    /// Where the per-turn step journal goes (ADR 0062): `project`, `state` or
    /// `custom`, plus the path a `custom` journal writes under.
    pub journal_mode: String,
    pub journal_path: String,
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
    /// The turn this conversation is running right now, so a view that opens
    /// onto a live turn shows it instead of an empty transcript.
    pub running: Option<RunningTurnView>,
    pub turns: Vec<TurnWithChanges>,
    pub next_before: Option<u32>,
    /// Every conversation in the project, newest first.
    pub sessions: Vec<SessionMeta>,
    pub projects: Vec<ProjectEntry>,
}

/// A turn that has started and has not finished. The input is what the user
/// sent; the view shows it while the answer streams.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningTurnView {
    pub seq: u32,
    pub input: String,
}

fn with_changes(
    state_dir: &Path,
    workspace: &Path,
    session_id: &str,
    turns: Vec<TurnView>,
) -> Vec<TurnWithChanges> {
    let changes = checkpoints::recorded_changes_for_session(state_dir, workspace, session_id);
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

/// The provider streams one chunk per token, and every emitted event costs the
/// webview a script evaluation of its payload, so forwarding a chunk per event
/// spends a whole core re-parsing while a turn streams. A batch is held for one
/// frame and then sent as a single event, or sent early once it is large enough
/// that holding it longer would show up as a jump in the text.
const DELTA_FLUSH: std::time::Duration = std::time::Duration::from_millis(16);
const DELTA_FLUSH_BYTES: usize = 4 * 1024;

/// Streamed output of one frame window, kept per channel because the view puts
/// thinking and the answer in different places.
#[derive(Default)]
struct DeltaBatch {
    text: String,
    thinking: String,
}

impl DeltaBatch {
    fn push(&mut self, delta: ModelDelta) {
        match delta {
            ModelDelta::Text(text) => self.text.push_str(&text),
            ModelDelta::Thinking(text) => self.thinking.push_str(&text),
        }
    }

    fn len(&self) -> usize {
        self.text.len() + self.thinking.len()
    }
}

/// Drains streamed output into one batch per frame window. The caller decides
/// what a batch becomes, so the coalescing can be tested without a webview.
async fn drain_deltas(
    deltas: &mut tokio::sync::mpsc::Receiver<ModelDelta>,
    mut flush: impl FnMut(&mut DeltaBatch),
) {
    let mut batch = DeltaBatch::default();
    loop {
        // Nothing is streaming yet: wait for the first chunk.
        let Some(delta) = deltas.recv().await else {
            return;
        };
        batch.push(delta);
        let deadline = tokio::time::Instant::now() + DELTA_FLUSH;
        while batch.len() < DELTA_FLUSH_BYTES {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, deltas.recv()).await {
                Ok(Some(delta)) => batch.push(delta),
                // The turn ended under a pending batch: send the tail rather
                // than drop the end of the answer.
                Ok(None) => {
                    flush(&mut batch);
                    return;
                }
                Err(_) => break,
            }
        }
        flush(&mut batch);
        // A window never carries text into the next one, whether or not the
        // caller took the batch.
        batch = DeltaBatch::default();
    }
}

/// Sends what the batch holds and empties it.
fn emit_batch(app: &SessionApp, current_seq: &AtomicU32, batch: &mut DeltaBatch) {
    let text = std::mem::take(&mut batch.text);
    let thinking = std::mem::take(&mut batch.thinking);
    if text.is_empty() && thinking.is_empty() {
        return;
    }
    let seq = current_seq.load(Ordering::Relaxed);
    // No turn is running: the view drops a delta that names no sequence, so
    // there is nothing to send.
    if seq == 0 {
        return;
    }
    if !text.is_empty() {
        app.emit(EV_TEXT_DELTA, TextDelta { seq, text });
    }
    if !thinking.is_empty() {
        app.emit(
            EV_THINKING_DELTA,
            ThinkingDelta {
                seq,
                text: thinking,
            },
        );
    }
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
    /// The answer came from an identical call earlier in the same turn
    /// (ADR 0070), so this call ran no command of its own.
    reused: bool,
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
    /// The user stopped it rather than the command failing.
    stopped: bool,
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

/// The settings the window needs before it can paint. The rig discovery walks
/// directories, so it runs on the blocking pool: Tauri executes a plain `fn`
/// command on the thread that paints, and a walk there freezes the window.
#[tauri::command]
pub async fn setup_state(state: State<'_, SharedState>) -> Result<SetupState, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || setup_state_now(&state))
        .await
        .map_err(|error| error.to_string())
}

fn setup_state_now(state: &DesktopState) -> SetupState {
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
        journal_mode: config.journal_mode,
        journal_path: config.journal_path,
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
    session_id: Option<String>,
) -> Result<SessionView, String> {
    let state = state.inner().clone();
    // The source is the conversation the window names, and only that one has to
    // be idle: a branch is a copy, so a turn streaming elsewhere is no reason to
    // refuse it (ADR 0076).
    let current = conversation_named(&state, session_id.as_deref())?;
    ensure_conversation_idle(Some(&current))?;
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
    open_inner(app, state, current.workspace.clone(), Some(id), false).await
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
    // A new conversation is never the one that is running: it gets its own id
    // and its own engine, so work in progress elsewhere does not block it
    // (ADR 0076).
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

/// The index reconcile reads every conversation header, so it belongs on the
/// blocking pool rather than on the window's thread.
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, SharedState>,
    workspace: String,
) -> Result<Vec<SessionMeta>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = resolve_workspace(&state, Some(workspace));
        let state_dir = state_dir_for(&workspace);
        let _guard = state
            .session_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions::list(&state_dir, &workspace)
    })
    .await
    .map_err(|error| error.to_string())
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
    // Only the conversation being deleted has to be idle: a turn elsewhere does
    // not make this one busy.
    ensure_conversation_idle(state.inner().open_of(&session_id).as_deref())?;
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
            .open_of(&session_id)
            .ok_or_else(|| "还没有打开会话".to_owned())?
            .engine
            .clone();
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
    // Archive is the recoverable step before delete, and it is refused only
    // while the conversation being archived is streaming a turn.
    let target = state.inner().open_of(&session_id);
    ensure_conversation_idle(target.as_deref())?;
    let workspace = resolve_workspace(state.inner(), Some(workspace));
    let state_dir = state_dir_for(&workspace);
    let is_current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .is_some_and(|current| current.id == session_id.as_str() && current.workspace == workspace);
    // An open conversation carries the archive flag its engine enforces, so the
    // engine is told even when the window is showing another one.
    let meta = if let Some(target) = target {
        let engine = target.engine.clone();
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
    // Opening another conversation is allowed while one has a turn running: the
    // engine that owns the turn is kept, and its events name their
    // conversation. Refusing here keyed on `current` meant a reload during a
    // turn could not get back in at all, which is how a boot ended up on the
    // failure page.
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

    // What a conversation opened now would be built with. An idle conversation
    // whose engine was built from anything else is rebuilt when it opens, so
    // the model the status line names is the model the next turn calls — and a
    // changed key or endpoint reaches conversations that are already here
    // (ADR 0039).
    let connection = Connection {
        provider,
        provider_name: config.provider_name.clone(),
        model: config.model.clone(),
        base_url: config.base_url.trim().to_owned(),
        context_window: config.model_context,
        search_url: config
            .search_url
            .clone()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        search_key: fingerprint(search_key.as_deref()),
        key: fingerprint(key.as_deref()),
    };

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
    // Reopening a conversation the app already has open usually keeps its
    // engine; `rebuild_wanted` says when it does not.
    remember_workspace(&state, &workspace);
    // A conversation with work in flight always keeps its engine: a turn may
    // still be streaming into it and its background commands are running, so
    // rebuilding would take both down. An idle one keeps it only when nothing
    // asked for a new one: a blank conversation holds the rig it was first
    // opened with, and a connection change holds the model the engine calls —
    // keeping either would mean the setting the status line shows is not the
    // one running (ADR 0039).
    let has_document = session_list.iter().any(|meta| meta.id == session_id);
    // The send path reads this flag instead of loading the document again.
    let archived = archived_in(&session_list, &session_id);
    if let Some(open) = state.open_of(&session_id) {
        let other = open.connection != connection;
        if !rebuild_wanted(has_document, conversation_busy(&open), other) {
            // The window shows this conversation now, and every command that
            // does not name one reads this pointer: returning the view without
            // moving it is how a send ended up in the conversation the user had
            // left, with its events dropped by the view (ADR 0076).
            show_conversation(&state, &open, Some(archived));
            return Ok(open_view(
                &state,
                &open,
                &workspace,
                &session_id,
                &session_list,
            ));
        }
    }
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
        provider: connection.provider.clone(),
        model: connection.model.clone(),
        base_url: Some(config.base_url.clone()).filter(|url| !url.trim().is_empty()),
        api_key,
        context_window: connection.context_window,
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
    let open = Arc::new(OpenConversation {
        id: session_id.clone(),
        workspace: workspace.clone(),
        state_dir: state_dir.clone(),
        engine: Arc::new(engine),
        cancel: Mutex::new(None),
        seq: Arc::new(AtomicU32::new(0)),
        running: Mutex::new(None),
        connection,
    });
    state.install_open(open.clone());
    show_conversation(&state, &open, Some(archived));
    let session_app = SessionApp::new(app.clone(), session_id.clone());
    let seq = open.seq.clone();
    tauri::async_runtime::spawn(async move {
        drain_deltas(&mut delta_rx, |batch| emit_batch(&session_app, &seq, batch)).await;
    });

    Ok(open_view(
        &state,
        &open,
        &workspace,
        &session_id,
        &session_list,
    ))
}

fn open_view(
    state: &DesktopState,
    conversation: &OpenConversation,
    workspace: &Path,
    session_id: &str,
    session_list: &[sessions::SessionMeta],
) -> SessionView {
    let state_dir = state_dir_for(workspace);
    let engine = &conversation.engine;
    let page = engine.history(None, 30);
    SessionView {
        rig: engine.rig_name().to_owned(),
        workspace: display_path(engine.workspace()),
        session_id: session_id.to_owned(),
        running: conversation
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|running| RunningTurnView {
                seq: running.seq,
                input: running.input.clone(),
            }),
        turns: with_changes(&state_dir, workspace, session_id, page.turns),
        next_before: page.next_before,
        sessions: session_list.to_vec(),
        projects: {
            let config = state
                .config
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            config::recent_projects(&config)
        },
    }
}

/// Records the project the window opened under as the one to reopen. Switching
/// back to a conversation that is already open persists it too, so this runs on
/// both paths instead of only when an engine is built.
fn remember_workspace(state: &DesktopState, workspace: &Path) {
    let label = display_path(workspace);
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.last_workspace = label.clone();
    config::remember_project(&mut config, &label, sessions::now_ms());
    let _ = config::save_config(&config);
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
    state.each_open(|conversation| conversation.engine.set_permission(parsed));
    Ok(())
}

/// What the composer's status line shows: the configured permission and the
/// level in force now (a session grant may raise it), the connection and the
/// context pressure of recent turns.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// The conversation this answer describes. A status that arrives late must
    /// not be read as the one the window is showing now.
    pub session_id: String,
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
    /// The whole command line: the row shows what runs, not the program.
    pub command: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output_path: String,
    /// The user stopped it. While it still runs the row shows "stopping".
    pub stopped: bool,
}

#[tauri::command]
pub fn stop_background(
    state: State<'_, SharedState>,
    id: String,
    session_id: Option<String>,
) -> Result<bool, String> {
    let conversation = conversation_named(state.inner(), session_id.as_deref())?;
    Ok(conversation.engine.stop_background(&id))
}

#[tauri::command]
pub async fn background_tasks(
    state: State<'_, SharedState>,
    session_id: Option<String>,
) -> Result<Vec<BackgroundTaskView>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let engine = conversation_named(&state, session_id.as_deref())?
            .engine
            .clone();
        Ok(engine
            .background_tasks()
            .into_iter()
            .map(|task| BackgroundTaskView {
                id: task.id,
                program: task.program,
                command: task.command,
                running: task.running,
                exit_code: task.exit_code,
                duration_ms: task.duration_ms,
                output_path: task.output_path,
                stopped: task.stopped,
            })
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn session_status(
    state: State<'_, SharedState>,
    session_id: Option<String>,
) -> Result<SessionStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // The window names the conversation the status describes. Falling back
        // to the pointer is only for a boot that has not opened one yet.
        let session_id = match session_id.filter(|id| !id.is_empty()) {
            Some(id) => id,
            None => state
                .current_id()
                .ok_or_else(|| "还没有打开会话".to_owned())?,
        };
        let conversation = state
            .open_of(&session_id)
            .ok_or_else(|| "会话已关闭，请重新打开".to_owned())?;
        let engine = conversation.engine.clone();
        Ok(SessionStatus {
            session_id,
            permission: engine.base_permission().as_str().to_owned(),
            permission_effective: engine.permission().as_str().to_owned(),
            rig: engine.rig_name(),
            // What this engine calls, not what the settings now hold: an
            // engine cannot change connection under itself, and naming the
            // setting would describe a turn that never runs.
            model: conversation.connection.model.clone(),
            provider: conversation.connection.provider.clone(),
            provider_name: conversation.connection.provider_name.clone(),
            context_budget: engine.context_budget(),
            context_window: Some(engine.config().context_window).filter(|window| *window > 0),
            recent: engine.context_history(),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The records' home (ADRs 0062, 0063): `project` follows the repository,
/// `state` keeps them beside the sessions, `custom` writes under a path the
/// user names.
#[tauri::command]
pub fn save_journal(
    state: State<'_, SharedState>,
    mode: String,
    path: String,
) -> Result<(), String> {
    let mode = match mode.trim() {
        "state" => "state",
        "custom" => "custom",
        _ => "project",
    };
    let path = path.trim().to_owned();
    if mode == "custom" && path.is_empty() {
        return Err("自定义目录需要一个路径".to_owned());
    }
    let mut config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    config.journal_mode = mode.to_owned();
    config.journal_path = path;
    config::save_config(&config)
}

/// The two places a turn writes (ADR 0063, 0064), so the settings panel can
/// show the user where their records actually go: the step log's database and
/// the directory of task documents.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaths {
    pub log: String,
    pub progress_dir: String,
}

/// What the window spent on one turn, recorded beside the turn's own records
/// (ADR 0071).
///
/// The account is written by the window, because only the window knows how many
/// streamed batches it received and how long a markdown sample took. The host's
/// part is to put it somewhere bounded and inspectable.
#[tauri::command]
pub async fn perf_report(
    workspace: String,
    session_id: String,
    seq: u32,
    phase: String,
    report: String,
) -> Result<String, String> {
    let dir = crate::perf::dir(&state_dir_for(Path::new(&workspace)));
    tauri::async_runtime::spawn_blocking(move || {
        crate::perf::write(&dir, &session_id, seq, &phase, &report)
            .map(|path| path.display().to_string())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn record_paths(state: State<'_, SharedState>) -> Result<RecordPaths, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let empty = || RecordPaths {
            log: String::new(),
            progress_dir: String::new(),
        };
        let config = state
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let current = state
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(current) = current else {
            return Ok(empty());
        };
        let Some(base) = records::base(
            records::Mode::parse(&config.journal_mode),
            &config.journal_path,
            &current.workspace,
            &current.state_dir,
        ) else {
            return Ok(empty());
        };
        let (log, index) = records::paths(&base);
        Ok(RecordPaths {
            log: log.display().to_string(),
            progress_dir: index
                .parent()
                .map_or_else(String::new, |dir| dir.display().to_string()),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Refuses a change while the conversation on screen has a turn running.
///
/// Drops the window's grip on every open conversation and cancels the turns
/// still streaming into them. The next open builds a fresh engine, so a turn
/// that will not unwind can no longer keep a workspace closed. This is the
/// boot screen's last resort; the app menu reaches it through `recover`.
#[tauri::command]
pub fn force_recover(state: State<'_, SharedState>) -> bool {
    state.inner().forget_open()
}

/// The conversation a command acts on: the one it names, else the one the
/// window shows (ADR 0076).
///
/// A command that names a conversation must reach it even after the window has
/// moved on, because the pointer is a view of the window, not the address of
/// the work.
fn conversation_named(
    state: &DesktopState,
    session_id: Option<&str>,
) -> Result<Arc<OpenConversation>, String> {
    match session_id.filter(|id| !id.is_empty()) {
        Some(id) => state
            .open_of(id)
            .ok_or_else(|| "这个对话没有打开，请重新选择".to_owned()),
        None => state.current_open(),
    }
}

/// Whether opening a conversation has to replace the engine it already holds.
///
/// Work in flight is never interrupted: its engine started it. An idle
/// conversation is rebuilt when it has no document yet — a blank one carries
/// the rig it was first opened with, and the picker's choice has to become the
/// one that runs — or when the connection it was built with is no longer the
/// one the settings describe (ADR 0039).
fn rebuild_wanted(has_document: bool, busy: bool, connection_changed: bool) -> bool {
    !busy && (!has_document || connection_changed)
}

/// A stable number for a secret, so a changed key can be told from an unchanged
/// one without the snapshot holding the key itself.
fn fingerprint(secret: Option<&str>) -> u64 {
    let mut hasher = DefaultHasher::new();
    secret.hash(&mut hasher);
    hasher.finish()
}

/// Whether a conversation is working right now: a turn streaming into it, or a
/// background command it started. Both belong to the engine that started them,
/// so that engine may not be replaced under them.
fn conversation_busy(conversation: &OpenConversation) -> bool {
    let streaming = conversation
        .cancel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some()
        || conversation
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some();
    streaming || !conversation.engine.background_tasks().is_empty()
}

/// Makes a conversation the one the window shows.
///
/// The pointer is the window's: it moves whenever the window moves, not only
/// when an engine is built. `archived` is the flag the send path reads; `None`
/// asks the index, which is what a command naming a conversation that is already
/// open but no longer on screen has to do.
fn show_conversation(
    state: &DesktopState,
    conversation: &OpenConversation,
    archived: Option<bool>,
) {
    let archived = match archived {
        Some(archived) => archived,
        None => {
            let known = {
                let current = state
                    .current
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                current
                    .as_ref()
                    .filter(|current| {
                        current.id == conversation.id
                            && current.workspace == conversation.workspace
                            && current.state_dir == conversation.state_dir
                    })
                    .map(|current| current.archived)
            };
            known.unwrap_or_else(|| {
                // One document, not the whole project: the flag lives in the
                // conversation's own file, and listing every session to read
                // one boolean is a scan per send.
                sessions::is_archived(
                    &conversation.state_dir,
                    &conversation.workspace,
                    &conversation.id,
                )
                .unwrap_or(false)
            })
        }
    };
    let mut current = state
        .current
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *current = Some(CurrentSession {
        id: conversation.id.clone(),
        workspace: conversation.workspace.clone(),
        state_dir: conversation.state_dir.clone(),
        archived,
    });
}

/// No conversation open is idle, not an error: the boot path asks this before
/// anything is current, and answering "还没有打开会话" here made the first
/// open of every launch fail.
fn ensure_conversation_idle(conversation: Option<&OpenConversation>) -> Result<(), String> {
    let Some(conversation) = conversation else {
        return Ok(());
    };
    if conversation
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
    session_id: Option<String>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let input = input.trim().to_owned();
    if input.is_empty() {
        return Err("消息不能为空".to_owned());
    }
    // The window names the conversation it shows (ADR 0076). Following the
    // window's pointer instead meant that after a switch the pointer had left
    // behind, the turn ran in the conversation the user was not looking at:
    // its events carry that id and the view drops them, so the message appeared
    // only after a reload, and only in the other conversation.
    let conversation = match session_id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => {
            let conversation = state
                .open_of(id)
                .ok_or_else(|| "这个对话没有打开，请重新选择".to_owned())?;
            // A send is proof of what the window shows, so the pointer follows
            // it: the record, the checkpoint and the status line all read it.
            show_conversation(&state, &conversation, None);
            conversation
        }
        None => state.current_open()?,
    };
    let engine = conversation.engine.clone();
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
    // A turn belongs to its conversation: another conversation may still be
    // running when this one starts.
    let token = {
        let mut cancel = conversation
            .cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cancel.is_some() {
            return Err("这个对话的回合还在运行".to_owned());
        }
        let token = CancelToken::new();
        *cancel = Some(token.clone());
        token
    };
    let seq = engine.next_turn_seq();
    conversation.seq.store(seq, Ordering::Relaxed);
    {
        let mut running = conversation
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *running = Some(RunningTurn {
            seq,
            input: input.clone(),
        });
    }
    // Every event this turn emits names the conversation, so a window showing
    // another one drops it.
    let session_app = SessionApp::new(app.clone(), conversation.id.clone());
    session_app.emit(
        EV_TURN_STARTED,
        TurnStarted {
            seq,
            input: input.clone(),
        },
    );
    let checkpoint_session = current.clone();
    // The conversation lands in the project's history now, not when the turn
    // ends (ADR 0078). A new conversation used to stay out of the list for the
    // whole first turn — a minute is an ordinary turn here — and a first turn
    // that failed left nothing at all.
    {
        let workspace = conversation.workspace.clone();
        let id = conversation.id.clone();
        let rig = engine.rig_name().to_owned();
        let first = input.clone();
        let app = app.clone();
        let state = state.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let state_dir = state_dir_for(&workspace);
            let meta = {
                let _guard = state
                    .session_io
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                sessions::begin(&state_dir, &workspace, &id, &rig, &first)
            };
            if let Some(meta) = meta {
                SessionApp::new(app, id).emit(EV_SESSION_UPDATED, SessionUpdated { meta });
            }
        });
    }
    // Where this turn's records go (ADR 0063): the setting decides, and the
    // turn's workspace decides which project owns them.
    let config = state
        .config
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let records_base = current.as_ref().and_then(|current| {
        let mode = records::Mode::parse(&config.journal_mode);
        if mode == records::Mode::Project {
            let _ = records::ignore_in_project(&current.workspace);
        }
        records::base(
            mode,
            &config.journal_path,
            &current.workspace,
            &current.state_dir,
        )
    });
    // The turn's header, kept for the log's row and the document's block.
    let log_meta = records::TurnMeta {
        session: conversation.id.clone(),
        turn: seq,
        model: config.model.clone(),
        provider: config.provider.clone(),
        rig: engine.rig_name().to_owned(),
        workspace: current
            .as_ref()
            .map(|current| current.workspace.display().to_string())
            .unwrap_or_default(),
        input: input.clone(),
    };
    // The document's identity from here on: the moment the turn started, so the
    // write at the end replaces the running document instead of adding a second
    // file for one task.
    let started_ms = sessions::now_ms();
    tauri::async_runtime::spawn(async move {
        let step_log = records_base
            .as_ref()
            .and_then(|base| records::StepLog::open(base, &log_meta.session, log_meta.turn).ok())
            .map(|log| {
                // The row exists before the first step, so the log never starts
                // with steps that belong to nothing.
                let _ = log.meta(&log_meta);
                Arc::new(Mutex::new(log))
            });
        // The turn's document exists before its first step (ADR 0064, amended by
        // ADR 0077). A turn that is still running used to be nowhere in
        // `progress/`: the row under `{progress}/` appeared only once the turn
        // had ended, so a reader looking for work in flight found nothing, and a
        // turn the app did not survive left no trace at all. The status says
        // `running`, and the same identity rewrites the file when the turn ends.
        if let Some(base) = records_base.as_ref() {
            let dir = records::progress_dir(base);
            let workspace = log_meta.workspace.clone();
            // Where the tree stood before the turn touched it. Cheap enough to
            // ask twice per turn, and the end write replaces it.
            let git = tauri::async_runtime::spawn_blocking(move || {
                taskdoc::git_state(Path::new(&workspace))
            })
            .await
            .unwrap_or_default();
            let entry = taskdoc::Entry {
                at_ms: started_ms,
                session: &log_meta.session,
                turn: log_meta.turn,
                input: &log_meta.input,
                rig: &log_meta.rig,
                model: &log_meta.model,
                provider: &log_meta.provider,
                status: taskdoc::Status::Running,
                error: None,
                model_calls: None,
                tools: None,
                duration_ms: None,
                changes: &[],
                git: &git,
            };
            let _ = taskdoc::write_task(&dir, &entry);
        }
        let before = match checkpoint_session.as_ref() {
            Some(current) => take_snapshot(current).await,
            None => None,
        };

        // Live tool activity: the kernel keeps this turn's events in the log
        // while the turn runs, so a short poll is enough to show each call as
        // it starts and finishes. Sequence numbers stay monotonic across the
        // per-turn clear, so `live_last` filters out the previous turn.
        let live_events = engine.events().clone();
        let live_app = session_app.clone();
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
            let step_log = step_log.clone();
            let live_poll = live_events.clone();
            tauri::async_runtime::spawn(async move {
                while !live_done_flag.load(Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    // Only the new tail: cloning the whole turn's event log
                    // every 120 ms costs more as a tool-heavy turn grows.
                    for record in live_poll.since(live_last.load(Ordering::Relaxed)) {
                        live_last.store(record.seq, Ordering::Relaxed);
                        // Every step lands in the log before the view sees it:
                        // a UI can drop an event, a committed row cannot be
                        // unmade.
                        if let Some(log) = step_log.as_ref() {
                            let log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                            let _ = log.step(record.seq, &record.event);
                        }
                        match record.event {
                            Event::ToolStarted {
                                call_id,
                                tool,
                                args_preview,
                                ..
                            } => {
                                live_app.emit(
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
                                reused,
                                ..
                            } => {
                                live_app.emit(
                                    EV_TOOL_FINISHED,
                                    ToolFinishedEvent {
                                        seq,
                                        call_id: call_id.get(),
                                        name: tool,
                                        ok,
                                        duration_ms,
                                        error: error.map(|code| code.as_str().to_owned()),
                                        output: output_preview,
                                        reused,
                                    },
                                );
                            }
                            Event::BackgroundStarted {
                                id, program, pid, ..
                            } => {
                                live_app.emit(
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
                                stopped,
                                ..
                            } => {
                                live_app.emit(
                                    EV_BACKGROUND_FINISHED,
                                    BackgroundFinishedEvent {
                                        seq,
                                        id,
                                        program,
                                        exit_code,
                                        duration_ms,
                                        output_path,
                                        stopped,
                                    },
                                );
                            }
                            Event::ModelFinished { usage, .. } => {
                                live_app.emit(
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
                                live_app
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

        // The poller can stop between two ticks; whatever it did not reach is
        // still in the kernel's log, and the step log must be whole.
        if let Some(log) = step_log.as_ref() {
            let log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for record in live_events.since(live_last.load(Ordering::Relaxed)) {
                live_last.store(record.seq, Ordering::Relaxed);
                let _ = log.step(record.seq, &record.event);
            }
            let turn = engine.turn(seq);
            let _ = log.end(&records::TurnEnd {
                ok: result.is_ok(),
                error: result.as_ref().err().map(|error| error.to_string()),
                state: turn.as_ref().map(|turn| turn.state.clone()),
                model_calls: turn.as_ref().map(|turn| u64::from(turn.model_calls)),
                tools: turn.as_ref().map(|turn| turn.tools.len()),
                duration_ms: turn.as_ref().map(|turn| turn.duration_ms),
            });
        }

        // The persisted turn is ready now. Show it and release the composer
        // before the checkpoint snapshot: that git commit can take seconds on
        // a large project and must not hold the raw streaming view.
        {
            let mut cancel = conversation
                .cancel
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *cancel = None;
        }
        {
            let mut running = conversation
                .running
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *running = None;
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
                        session_app.emit(EV_SESSION_UPDATED, SessionUpdated { meta });
                    }
                }
                match engine.turn(seq) {
                    Some(turn) => {
                        session_app.emit(EV_TURN_FINISHED, TurnFinished { turn });
                    }
                    None => {
                        session_app.emit(
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
                session_app.emit(
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
                    session_app.emit(EV_TURN_CHANGES, TurnChanges { seq });
                }
            }
        }

        // The task document (ADR 0064): what this task left behind and where
        // git stands now. Written after the checkpoint, so the change list is
        // already recorded and reading it back is exact.
        if let (Some(base), Some(current)) = (records_base.as_ref(), current.as_ref()) {
            let changes =
                checkpoints::changes(&current.state_dir, &current.workspace, &current.id, seq)
                    .unwrap_or_default();
            let git = taskdoc::git_state(&current.workspace);
            let failure = result.as_ref().err().map(|error| error.to_string());
            let turn = engine.turn(seq);
            let entry = taskdoc::Entry {
                at_ms: started_ms,
                session: &log_meta.session,
                turn: seq,
                input: &log_meta.input,
                rig: &log_meta.rig,
                model: &log_meta.model,
                provider: &log_meta.provider,
                status: if result.is_ok() {
                    taskdoc::Status::Done
                } else {
                    taskdoc::Status::Failed
                },
                error: failure.as_deref(),
                model_calls: turn.as_ref().map(|turn| u64::from(turn.model_calls)),
                tools: turn.as_ref().map(|turn| turn.tools.len()),
                duration_ms: turn.as_ref().map(|turn| turn.duration_ms),
                changes: &changes,
                git: &git,
            };
            let _ = taskdoc::write_task(&records::progress_dir(base), &entry);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_turn(state: State<'_, SharedState>, session_id: Option<String>) -> bool {
    let state = state.inner();
    // The cancel belongs to the conversation the window shows, which names
    // itself; the pointer is only the fallback for a boot that has none yet.
    let conversation = match session_id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => state.open_of(id),
        None => state.current_open().ok(),
    };
    let Some(conversation) = conversation else {
        return false;
    };
    let token = conversation
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
pub async fn undo_last_turn(
    state: State<'_, SharedState>,
    session_id: Option<String>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let conversation = conversation_named(&state, session_id.as_deref())?;
    ensure_conversation_idle(Some(&conversation))?;
    let engine = conversation.engine.clone();
    let seq = engine
        .history(None, 1)
        .turns
        .first()
        .map(|turn| turn.seq)
        .ok_or_else(|| "还没有可撤销的回合".to_owned())?;
    let (state_dir, workspace, id) = (
        conversation.state_dir.clone(),
        conversation.workspace.clone(),
        conversation.id.clone(),
    );
    tauri::async_runtime::spawn_blocking(move || {
        checkpoints::undo(&state_dir, &workspace, &id, seq)
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
    session_id: Option<String>,
) -> Result<Vec<checkpoints::FileChange>, String> {
    let state = state.inner().clone();
    let Ok(conversation) = conversation_named(&state, session_id.as_deref()) else {
        return Ok(Vec::new());
    };
    let engine = conversation.engine.clone();
    let Some(seq) = seq.or_else(|| engine.history(None, 1).turns.first().map(|turn| turn.seq))
    else {
        return Ok(Vec::new());
    };
    let (state_dir, workspace, id) = (
        conversation.state_dir.clone(),
        conversation.workspace.clone(),
        conversation.id.clone(),
    );
    let changes = tauri::async_runtime::spawn_blocking(move || {
        checkpoints::changes(&state_dir, &workspace, &id, seq)
    })
    .await
    .map_err(|error| error.to_string())?
    .unwrap_or_default();
    Ok(changes)
}

#[tauri::command]
pub async fn load_turns(
    state: State<'_, SharedState>,
    before: Option<u32>,
    limit: Option<usize>,
    session_id: Option<String>,
) -> Result<TurnPageWithChanges, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conversation = conversation_named(&state, session_id.as_deref())?;
        let page = conversation.engine.history(before, limit.unwrap_or(30));
        let turns = with_changes(
            &conversation.state_dir,
            &conversation.workspace,
            &conversation.id,
            page.turns,
        );
        Ok(TurnPageWithChanges {
            turns,
            next_before: page.next_before,
        })
    })
    .await
    .map_err(|error| error.to_string())?
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
        collect_tree, copy_attachment, copy_folder, copy_preset, drain_deltas, rig_dirs,
        rig_for_session, DELTA_FLUSH_BYTES,
    };
    use crate::config::{DesktopConfig, Preset};
    use crate::sessions::SessionMeta;
    use keelson::engine::ModelDelta;

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

    /// The four cases of the open decision, in the order they were wrong: a
    /// blank conversation could not take the rig that was picked, a changed
    /// model never reached the conversation on screen, and work in flight must
    /// outrank both.
    #[test]
    fn an_engine_is_rebuilt_only_when_nothing_is_working_in_it() {
        let build = super::rebuild_wanted;
        // Blank and idle: the rig picked in the composer has to become the one
        // that runs (ADR 0039).
        assert!(build(false, false, false));
        // A document and an unchanged connection: the engine stays, so a turn
        // that streams in is not taken down with it.
        assert!(!build(true, false, false));
        // A changed connection reaches an idle conversation, document or not.
        assert!(build(true, false, true));
        // Nothing interrupts a running turn or a live background command.
        assert!(!build(true, true, true));
        assert!(!build(false, true, false));
    }

    #[test]
    fn a_changed_key_is_a_different_connection() {
        let first = super::fingerprint(Some("sk-a"));
        assert_eq!(first, super::fingerprint(Some("sk-a")));
        assert_ne!(first, super::fingerprint(Some("sk-b")));
        assert_ne!(first, super::fingerprint(None));
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

    /// A chunk per event is a script evaluation per token in the webview, which
    /// is what pinned a core while a turn streamed. One window is one event.
    #[tokio::test]
    async fn a_burst_of_chunks_becomes_one_event() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let sender = tokio::spawn(async move {
            for _ in 0..50 {
                tx.send(ModelDelta::Text("ab".to_owned()))
                    .await
                    .expect("the receiver is alive");
            }
        });

        let mut flushes = Vec::new();
        drain_deltas(&mut rx, |batch| flushes.push(batch.text.len())).await;
        sender.await.expect("the sender finished");

        assert_eq!(flushes, vec![100], "fifty chunks arrive as one batch");
    }

    /// Holding a large batch for the rest of the window would read as a jump in
    /// the text, so size wins over the clock.
    #[tokio::test]
    async fn a_large_batch_is_sent_before_the_window_closes() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let sender = tokio::spawn(async move {
            for _ in 0..3 {
                tx.send(ModelDelta::Text("x".repeat(2048)))
                    .await
                    .expect("the receiver is alive");
            }
        });

        let mut flushes = Vec::new();
        drain_deltas(&mut rx, |batch| flushes.push(batch.text.len())).await;
        sender.await.expect("the sender finished");

        assert_eq!(flushes, vec![DELTA_FLUSH_BYTES, 2048]);
    }

    /// Thinking and the answer are shown in different places, so a batch keeps
    /// them apart even when they interleave.
    #[tokio::test]
    async fn thinking_stays_separate_from_the_answer() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let sender = tokio::spawn(async move {
            for delta in [
                ModelDelta::Thinking("why".to_owned()),
                ModelDelta::Text("so".to_owned()),
                ModelDelta::Thinking(" not".to_owned()),
            ] {
                tx.send(delta).await.expect("the receiver is alive");
            }
        });

        let mut flushes = Vec::new();
        drain_deltas(&mut rx, |batch| {
            flushes.push((batch.thinking.len(), batch.text.len()))
        })
        .await;
        sender.await.expect("the sender finished");

        assert_eq!(flushes, vec![(7, 2)]);
    }
}
