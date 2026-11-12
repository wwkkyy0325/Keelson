//! Turn execution: build the kernel from a rig and run turns.
//!
//! This module owns no output. It returns kernel outcomes; the host decides
//! what the user sees and where it goes (ADR 0004).

mod desktop;
mod git;
mod history;
mod kit;
mod screen;
mod series;
mod store;
mod video;
mod web;

pub use kit::seed_explainer_kit;

/// A GUI host has no console. A console child launched without this flag
/// opens its own black window for the duration of the command.
#[cfg(windows)]
pub(crate) fn hide_console(command: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000)
}

#[cfg(not(windows))]
pub(crate) fn hide_console(command: &mut std::process::Command) -> &mut std::process::Command {
    command
}

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use keel_core::{
    AcceptanceItem, AccessKind, ArgValue, CancelToken, Event, EvidenceOutcome, InteractionPort,
    KernelError, ModelChunk, ModelError, ModelProvider, ModelRequest, ModelStream, Policy, Session,
    TodoList, ToolArgs, ToolCallId, VerificationMode, VerificationReport, VerifySpec,
};
use keel_model_anthropic::{AnthropicConfig, AnthropicProvider};
use keel_model_openai::{OpenAiConfig, OpenAiProvider};
use keel_runtime::{
    load_profile, validate_config, AcceptanceSelector, ApplyEditsTool, BackgroundSnapshot,
    BackgroundTasks, CodeSearchTool, ContextBuilder, DeleteFileTool, EchoTool, EditFileTool,
    EventLog, EventRecord, GlobTool, HistoryStage, InteractionBroker, InterjectionPush,
    KernelOptions, LatestStage, ListDirTool, ListTrashTool, LocalBackgroundTasks, LocalTrashStore,
    MakeDirTool, MemoryStage, MockModel, MockStep, MoveFileTool, MultiEditTool, PermissionEngine,
    ProviderSummarizer, ReadFileTool, ReadLedger, RestoreFileTool, RootScope, RuntimeConfig,
    ShellTool, SystemStage, TaskStateStage, TodoTool, ToolRegistry, TurnDriver, TurnDriverConfig,
    TurnOutcome, WriteFileTool,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::rig::{self, Rig};

pub use keel_core::PermissionLevel;
pub use store::{
    Store, StoredContext, StoredEvent, StoredEvidence, StoredSession, StoredTurn, StoredUsage,
    StoredVerification, ToolLine,
};

/// Everything opening a session needs. A host supplies its own interaction
/// port and, when it wants streamed text, a channel for deltas.
pub struct OpenRequest {
    pub rig: Rig,
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    pub store_path: PathBuf,
    pub interactions: Arc<dyn InteractionPort>,
    /// Streamed model output: answer text and thinking, in arrival order.
    pub deltas: Option<mpsc::Sender<ModelDelta>>,
    /// A host-supplied model, used by the desktop onboarding. `None` keeps the
    /// profile's provider and the environment-variable key.
    pub model: Option<ModelOverride>,
    /// A host-chosen permission level. `None` keeps the profile's.
    pub permission: Option<keel_core::PermissionLevel>,
    /// Run the workspace's own checks after a turn that changed files.
    /// A host that measures the conversation, not the project, can turn them
    /// off; the desktop keeps them on, and the profile's
    /// `[verification] mode = "none"` still opts out.
    pub acceptance: bool,
    /// A SearXNG base URL, used for web search when the model provider has no
    /// search endpoint of its own. A provider endpoint always wins.
    pub search_url: Option<String>,
    /// A DeepSeek key for the provider's search and fetch tools, when the host
    /// wants them even though the chat model is a local server. The key stays
    /// in memory and never enters a prompt, a log or a stored document.
    pub search_key: Option<String>,
}

/// One piece of model output streamed to the host. Thinking is separate from
/// the answer: it is display material, never part of the turn's history.
#[derive(Debug, Clone)]
pub enum ModelDelta {
    Text(String),
    Thinking(String),
}

/// Explicit model settings from the desktop. The key never comes from a
/// profile or an environment variable in this path.
#[derive(Debug, Clone)]
pub struct ModelOverride {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: String,
    /// The selected model's hard context capacity, when the host knows it.
    /// The profile still owns the working-set budget and output reserve.
    pub context_window: Option<u32>,
}

/// Context usage at the end of one turn, for the composer's status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPoint {
    pub seq: u32,
    pub used: u32,
    pub ceiling: u32,
}

/// One finished turn as a read model. It carries the compact turn history, not
/// the kernel event stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnView {
    pub seq: u32,
    pub input: String,
    pub answer: String,
    /// Thinking the model streamed for this turn, bounded by the engine.
    #[serde(default)]
    pub thinking: String,
    pub state: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    #[serde(default)]
    pub usage: StoredUsage,
    /// Model calls spent in this turn; 0 for documents written before the
    /// field existed.
    #[serde(default)]
    pub model_calls: u32,
    pub verification_passed: Option<bool>,
    #[serde(default)]
    pub verification: Option<StoredVerification>,
    pub tools: Vec<ToolLine>,
}

impl From<&StoredTurn> for TurnView {
    fn from(turn: &StoredTurn) -> Self {
        Self {
            seq: turn.seq,
            input: turn.input.clone(),
            answer: turn.answer.clone(),
            thinking: turn.thinking.clone(),
            state: turn.state.clone(),
            error: turn.error.clone(),
            duration_ms: turn.duration_ms,
            usage: turn_usage(turn),
            model_calls: turn_model_calls(turn),
            verification_passed: turn.verification_passed,
            verification: turn.verification.clone(),
            tools: turn.tools.clone(),
        }
    }
}

/// Turns written before the usage field existed still have the numbers in
/// their `model_finished` events, so old conversations show the cost line too.
fn turn_usage(turn: &StoredTurn) -> StoredUsage {
    if turn.usage != StoredUsage::default() {
        return turn.usage;
    }
    let mut usage = StoredUsage::default();
    for event in &turn.events {
        let Some(reported) = event
            .payload
            .get("model_finished")
            .and_then(|finished| finished.get("usage"))
        else {
            continue;
        };
        let field = |name: &str| {
            reported
                .get(name)
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
        };
        usage.input_tokens = usage.input_tokens.saturating_add(field("input_tokens"));
        usage.output_tokens = usage.output_tokens.saturating_add(field("output_tokens"));
        usage.cached_input_tokens = usage
            .cached_input_tokens
            .saturating_add(field("cached_input_tokens"));
    }
    usage
}

/// Model calls in a turn. A new turn records the count; an older document
/// counts its persisted `model_finished` events instead.
fn turn_model_calls(turn: &StoredTurn) -> u32 {
    if turn.model_calls > 0 {
        return turn.model_calls;
    }
    u32::try_from(
        turn.events
            .iter()
            .filter(|event| event.payload.get("model_finished").is_some())
            .count(),
    )
    .unwrap_or(u32::MAX)
}

/// A keyset page of turns, oldest first. `next_before` is the cursor for the
/// page above this one, or `None` when the turn history starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnPage {
    pub turns: Vec<TurnView>,
    pub next_before: Option<u32>,
}

/// A long-lived host: one driver, one event log, one session document, many
/// turns. A host drives it; it knows nothing about the interface.
pub struct Engine {
    workspace: PathBuf,
    config: RuntimeConfig,
    driver: TurnDriver<DeltaModel>,
    events: Arc<EventLog>,
    todos: Arc<Mutex<TodoList>>,
    /// Session-scoped background commands; the host reads them without
    /// draining the results the driver needs.
    background: Arc<LocalBackgroundTasks>,
    store: Store,
    /// Shared with `search_history`, which reads the stored turns directly.
    session: Arc<Mutex<StoredSession>>,
    roots: Vec<String>,
    /// Thinking streamed during the current turn, persisted with it.
    thinking: Arc<Mutex<ThinkingBuffer>>,
}

enum HostModel {
    Mock(DemoMock),
    OpenAi(OpenAiProvider),
    Anthropic(AnthropicProvider),
}

/// The offline script: a tool call on even model calls, the answer on odd
/// ones. Each turn therefore shows one tool line and one streamed answer, and
/// the counter keeps that shape across turns.
struct DemoMock {
    calls: AtomicU64,
    echo: bool,
}

#[async_trait]
impl ModelProvider for DemoMock {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: &CancelToken,
    ) -> Result<ModelStream, ModelError> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        let step = if self.echo && call % 2 == 0 {
            let mut args = ToolArgs::new();
            args.insert("text", ArgValue::String("mock input".to_owned()));
            MockStep::ToolCall {
                name: "echo".to_owned(),
                args,
            }
        } else {
            MockStep::Text("mock turn complete".to_owned())
        };
        MockModel::new(vec![step]).stream(request, cancel).await
    }
}

#[async_trait]
impl ModelProvider for HostModel {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: &CancelToken,
    ) -> Result<ModelStream, ModelError> {
        match self {
            Self::Mock(model) => model.stream(request, cancel).await,
            Self::OpenAi(model) => model.stream(request, cancel).await,
            Self::Anthropic(model) => model.stream(request, cancel).await,
        }
    }
}

/// Forwards text deltas to the host while the driver consumes the stream. The
/// kernel's event stream carries boundaries, not tokens (ADR 0004).
/// Thinking kept for the transcript, bounded so a long reasoning round cannot
/// grow the session document without limit.
const MAX_TURN_THINKING_BYTES: usize = 64 * 1024;

/// What one stored turn keeps of a tool output and of the thinking text. The
/// live view is not affected; only the document is bounded.
const MAX_STORED_TOOL_OUTPUT_CHARS: usize = 8_000;
const MAX_STORED_THINKING_CHARS: usize = 8_000;

/// Keeps the head of `text` and names what was dropped, so a reader can tell a
/// short output from a clipped one.
fn cap_chars(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}\n(+{} characters)", count - limit)
}

/// Bounds what the document keeps of each tool output. The live view and the
/// kernel's event log keep the full preview.
fn cap_tool_lines(lines: &mut [ToolLine]) {
    for line in lines {
        if let Some(output) = line.output.as_deref() {
            line.output = Some(cap_chars(output, MAX_STORED_TOOL_OUTPUT_CHARS));
        }
    }
}

/// The last context point an old document's event stream carries.
fn context_from_events(turn: &StoredTurn) -> Option<StoredContext> {
    turn.events.iter().rev().find_map(|event| {
        let pressure = event.payload.get("context_pressure")?;
        Some(StoredContext {
            used: pressure
                .get("used")?
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())?,
            ceiling: pressure
                .get("ceiling")?
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())?,
        })
    })
}

/// The context point a turn reports: the stored field, or the last event in a
/// document written before the field existed.
fn context_point(turn: &StoredTurn) -> Option<ContextPoint> {
    let context = turn.context.or_else(|| context_from_events(turn))?;
    Some(ContextPoint {
        seq: turn.seq,
        used: context.used,
        ceiling: context.ceiling,
    })
}

/// Folds an old turn's event stream into the fields the host reads, then drops
/// the stream. New turns never wrote events, so this is a one-time migration
/// for documents from before the fields: the numbers stay, the bytes go.
fn absorb_events(turn: &mut StoredTurn) {
    if turn.events.is_empty() {
        return;
    }
    let usage = turn_usage(turn);
    if turn.usage == StoredUsage::default() {
        turn.usage = usage;
    }
    if turn.context.is_none() {
        turn.context = context_from_events(turn);
    }
    turn.events.clear();
}

#[derive(Default)]
struct ThinkingBuffer {
    text: String,
    truncated: bool,
}

fn record_thinking(buffer: &Arc<Mutex<ThinkingBuffer>>, delta: &str) {
    let mut buffer = buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if buffer.truncated {
        return;
    }
    buffer.text.push_str(delta);
    if buffer.text.len() > MAX_TURN_THINKING_BYTES {
        let cut = buffer
            .text
            .char_indices()
            .take_while(|(index, _)| *index <= MAX_TURN_THINKING_BYTES)
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        buffer.text.truncate(cut);
        buffer.text.push_str("\n… thinking truncated");
        buffer.truncated = true;
    }
}

struct DeltaModel {
    inner: HostModel,
    deltas: Option<mpsc::Sender<ModelDelta>>,
    thinking: Arc<Mutex<ThinkingBuffer>>,
}

#[async_trait]
impl ModelProvider for DeltaModel {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: &CancelToken,
    ) -> Result<ModelStream, ModelError> {
        let mut stream = self.inner.stream(request, cancel).await?;
        let sender = self.deltas.clone();
        let thinking = self.thinking.clone();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(chunk) = stream.recv().await {
                if let Ok(chunk) = &chunk {
                    match chunk {
                        ModelChunk::TextDelta(text) => {
                            if let Some(sender) = &sender {
                                let _ = sender.send(ModelDelta::Text(text.clone())).await;
                            }
                        }
                        ModelChunk::ThinkingDelta(text) => {
                            // Keep the thought for the transcript even when no
                            // host is listening to the live stream.
                            record_thinking(&thinking, text);
                            if let Some(sender) = &sender {
                                let _ = sender.send(ModelDelta::Thinking(text.clone())).await;
                            }
                        }
                        _ => {}
                    }
                }
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });
        Ok(ModelStream::new(rx))
    }
}

impl Engine {
    /// Builds the driver once and loads the session document.
    pub fn open(request: OpenRequest) -> Result<Self, KernelError> {
        let OpenRequest {
            rig,
            workspace,
            state_dir,
            store_path,
            interactions,
            deltas,
            model,
            permission,
            acceptance,
            search_url,
            search_key,
        } = request;

        let rig_name = rig.name.clone();
        let workspace = std::fs::canonicalize(&workspace).map_err(|error| KernelError::Io {
            message: format!(
                "cannot resolve workspace `{}`: {error}",
                workspace.display()
            ),
        })?;
        std::fs::create_dir_all(&state_dir).map_err(|error| KernelError::Io {
            message: format!(
                "cannot create state directory `{}`: {error}",
                state_dir.display()
            ),
        })?;
        let state_dir = std::fs::canonicalize(&state_dir).map_err(|error| KernelError::Io {
            message: format!(
                "cannot resolve state directory `{}`: {error}",
                state_dir.display()
            ),
        })?;
        // Outputs and trash belong to one project, not to the whole account.
        let project_dir = project_state_dir(&state_dir, &workspace);
        std::fs::create_dir_all(&project_dir).map_err(|error| KernelError::Io {
            message: format!(
                "cannot create project state directory `{}`: {error}",
                project_dir.display()
            ),
        })?;
        let outputs_dir = project_dir.join("outputs");

        let mut config = load_profile(&rig.profile)?;
        // A host may override the profile's permission level before the
        // permission engine is built; the rig still supplies the default.
        if let Some(level) = permission {
            config.permission_level = level;
        }
        // Project instructions are context, not a rig setting. AGENTS.md is
        // the cross-tool name; CLAUDE.md is the fallback.
        if let Some(instructions) = project_instructions(&workspace) {
            config.system_prompt.push_str("\n\n");
            config.system_prompt.push_str(&instructions);
        }
        let mut permissions = PermissionEngine::new(&workspace, config.permission_level)?;
        for root in &rig.extra_roots {
            permissions = permissions.with_root(&root.path, access_kind(root.access))?;
        }
        let roots = permissions
            .roots()
            .iter()
            .map(|scope| format!("{} [{}]", scope.path.display(), scope.access.as_str()))
            .collect();
        let permissions = Arc::new(permissions.with_outputs(outputs_dir.clone()));

        let store = Store::open(store_path.clone());
        let (restored, mut session) = match store.load(&workspace)? {
            Some(session) => (Some(session.snapshot.clone()), session),
            None => (
                None,
                StoredSession {
                    workspace: workspace.clone(),
                    rig: rig_name.clone(),
                    title: String::new(),
                    archived: false,
                    snapshot: Default::default(),
                    todos: None,
                    turns: Vec::new(),
                },
            ),
        };
        // Documents written before the field existed belong to the same rig
        // that opens them; the next persist writes the value.
        if session.rig.is_empty() {
            session.rig = rig_name;
        }
        let todos = Arc::new(Mutex::new(session.todos.clone().unwrap_or_default()));

        let events = Arc::new(EventLog::new());
        let interactions = Arc::new(InteractionBroker::new(
            interactions,
            config.interaction,
            events.clone(),
        ));

        // Background commands are session-scoped: the engine owns the
        // registry, and dropping it kills the process tree.
        let background = Arc::new(
            LocalBackgroundTasks::new(outputs_dir.join("background")).with_events(events.clone()),
        );
        // A provider that serves search and fetch tools gets them on the same
        // key as the chat API. A host-supplied search key covers the other
        // case: a local chat model with a saved cloud key for the web tools.
        // Otherwise a configured SearXNG URL is the search backend, and the
        // local fetch is always available.
        let web = match search_key.filter(|key| !key.trim().is_empty()) {
            Some(key) => Some(web::WebBackend::Provider {
                base: "https://api.deepseek.com".to_owned(),
                key,
            }),
            None => web::WebBackend::resolve(
                model
                    .as_ref()
                    .and_then(|over| over.base_url.as_deref())
                    .or(config.model_base_url.as_deref()),
                model
                    .as_ref()
                    .map(|over| over.api_key.as_str())
                    .unwrap_or(""),
                search_url.as_deref(),
            ),
        };
        let session = Arc::new(Mutex::new(session));
        // Screenshots live beside the project's outputs, outside the
        // workspace: the folder is the permission scope for a capture.
        // Captures are transient; a week of them is enough for a recheck.
        screen::prune_old_screens(&project_dir, Duration::from_secs(7 * 24 * 60 * 60));
        let screens = screen::screens_dir(&project_dir, &store_path);
        let registry = Arc::new(tools(
            &project_dir,
            &config,
            todos.clone(),
            background.clone(),
            session.clone(),
            web.clone(),
            screens,
        )?);
        if web.is_none() {
            // A profile may list web_search; without a provider endpoint the
            // tool is not registered and must not fail validation.
            config
                .visible_tool_names
                .retain(|name| name != "web_search");
        }
        let context = ContextBuilder::new(vec![
            Arc::new(SystemStage::new()),
            Arc::new(TaskStateStage::new()),
            Arc::new(MemoryStage::new()),
            Arc::new(HistoryStage::new()),
            Arc::new(LatestStage::new()),
        ]);
        validate_config(&config, &registry, &context)?;

        // A catalog model knows its own hard capacity. The profile owns the
        // working set, but a model with a smaller window must reduce it rather
        // than receive a prompt larger than the model can hold.
        if let Some(capacity) = model.as_ref().and_then(|model| model.context_window) {
            let output = config.max_output_tokens;
            let usable = capacity.saturating_sub(output);
            if usable > 0 {
                config.context.budget_tokens = config.context.budget_tokens.min(usable);
                // The ladder's ceiling follows the same capacity limit.
                config.context.max_budget_tokens = config
                    .context
                    .max_budget_tokens
                    .min(usable)
                    .max(config.context.budget_tokens);
            }
            let floor = config.context.budget_tokens.saturating_add(output);
            config.context_window = capacity.max(floor);
        }
        // The driver stamps every request with `config.model_route`; the
        // provider's own model is only a fallback. Keep the two in sync when
        // the host overrides the connection.
        if let Some(over) = model.as_ref() {
            config.model_route.provider = over.provider.clone();
            config.model_route.model = over.model.clone();
        }

        let thinking = Arc::new(Mutex::new(ThinkingBuffer::default()));
        let delta_model = DeltaModel {
            inner: build_model(&config, model.as_ref())?,
            deltas,
            thinking: thinking.clone(),
        };
        let policy: Arc<dyn Policy> = Arc::new(config.clone());
        let mut driver = TurnDriver::new(
            delta_model,
            registry,
            context,
            policy,
            TurnDriverConfig {
                events: events.clone(),
                permissions,
                interactions,
                system_prompt: config.system_prompt.clone(),
                max_output_tokens: config.max_output_tokens,
                context_window: config.context_window,
                limits: config.limits(),
                todos: Some(todos.clone()),
                background: Some(background.clone()),
                options: KernelOptions::default(),
            },
        )
        .with_outputs_dir(outputs_dir);
        // Sealed segments are compressed into records; those keep the goal,
        // result and open items, not the decisions and errors. A second model
        // instance writes the rolling summary that does. The mock provider has
        // nothing worth summarizing, so the offline rig stays deterministic.
        let summarizer_model = build_model(&config, model.as_ref())?;
        if !matches!(summarizer_model, HostModel::Mock(_)) {
            driver = driver.with_summarizer(Arc::new(ProviderSummarizer::new(summarizer_model)));
        }
        // A turn that edited the workspace is checked by the project's own
        // commands. A conversational turn skips them, and a profile that says
        // `[verification] mode = "none"` opts out even when the host asked.
        let acceptance = acceptance && !matches!(config.verification, VerificationMode::None);
        if acceptance {
            let checks = workspace_checks(&workspace);
            if !checks.is_empty() {
                let workspace_root = workspace.clone();
                let selector: AcceptanceSelector = Arc::new(move |paths: &[PathBuf]| {
                    selected_checks(&checks, &workspace_root, paths)
                });
                driver = driver.with_acceptance_selector(selector, 2);
            }
        }
        if let Some(snapshot) = restored {
            driver = driver.with_session(Session::restore(snapshot)?);
        }

        Ok(Self {
            workspace,
            config,
            driver,
            events,
            todos,
            background,
            store,
            session,
            roots,
            thinking,
        })
    }

    /// Runs one turn and persists the session once it ends.
    pub async fn run_turn(
        &self,
        input: &str,
        cancel: CancelToken,
    ) -> Result<TurnOutcome, KernelError> {
        // The log is per turn: `persist` pairs this turn's tool events, and a
        // long-lived engine would otherwise repeat every earlier call in each
        // new row. Sequence numbers stay monotonic across the clear.
        // The log is per turn: `persist` pairs this turn's tool events, and a
        // long-lived engine would otherwise repeat every earlier call in each
        // new row. Sequence numbers stay monotonic across the clear.
        self.events.clear();
        {
            let mut thinking = self
                .thinking
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            thinking.text.clear();
            thinking.truncated = false;
        }
        let outcome = self.driver.run(input, cancel).await;
        self.persist(input, &outcome)?;
        Ok(outcome)
    }

    fn persist(&self, input: &str, outcome: &TurnOutcome) -> Result<(), KernelError> {
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session.snapshot = self.driver.session_snapshot();
        session.todos = Some(
            self.todos
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        );
        let seq = session.turns.last().map(|turn| turn.seq + 1).unwrap_or(1);
        let error = outcome
            .error
            .as_ref()
            .map(|error| format!("{} ({})", error, error.code().as_str()));
        let thinking = self
            .thinking
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .text
            .clone();
        let records = self.events.snapshot();
        let mut tools = tool_lines(&records);
        // The live view has already shown the whole output; the document keeps
        // a bounded copy with a note, so a long transcript cannot grow the
        // file without limit.
        cap_tool_lines(&mut tools);
        let context = records.iter().rev().find_map(|record| match &record.event {
            Event::ContextPressure { used, ceiling, .. } => Some(StoredContext {
                used: *used,
                ceiling: *ceiling,
            }),
            _ => None,
        });
        session.turns.push(StoredTurn {
            seq,
            input: input.to_owned(),
            state: outcome.state.to_string(),
            answer: outcome.answer.body.clone(),
            thinking: cap_chars(&thinking, MAX_STORED_THINKING_CHARS),
            error,
            duration_ms: outcome.duration_ms,
            usage: StoredUsage {
                input_tokens: outcome.usage.input_tokens,
                output_tokens: outcome.usage.output_tokens,
                cached_input_tokens: outcome.usage.cached_input_tokens,
            },
            model_calls: outcome.model_calls,
            verification_passed: outcome.verification.as_ref().map(|report| report.passed()),
            verification: outcome.verification.as_ref().map(stored_verification),
            tools,
            context,
            // New turns do not write the event stream: the fields above carry
            // what the transcript and the status line read.
            events: Vec::new(),
        });
        self.save_session(&mut session)
    }

    /// Sets the host-owned title on the open conversation. Persisted now, so
    /// a later turn cannot overwrite it with the value loaded at open.
    pub fn set_title(&self, title: &str) -> Result<(), KernelError> {
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session.title = title.to_owned();
        self.save_session(&mut session)
    }

    /// Sets the host-owned archive flag on the open conversation.
    pub fn set_archived(&self, archived: bool) -> Result<(), KernelError> {
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session.archived = archived;
        self.save_session(&mut session)
    }

    /// Saves the open conversation, folding any pre-field event stream into
    /// the usage and context fields first, so an old document shrinks on its
    /// next save instead of carrying its events forever.
    fn save_session(&self, session: &mut StoredSession) -> Result<(), KernelError> {
        for turn in &mut session.turns {
            absorb_events(turn);
        }
        self.store.save(session)
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// The effective permission level, including a host override and a
    /// session grant.
    pub fn permission(&self) -> keel_core::PermissionLevel {
        self.driver.permission_level()
    }

    /// The configured permission level, without a session grant. A host
    /// shows this as the setting and `permission` as what is in force now.
    pub fn base_permission(&self) -> keel_core::PermissionLevel {
        self.driver.base_permission_level()
    }

    /// Switches the level for the open session. The desktop calls this when
    /// the composer's permission chip changes; session grants are dropped so
    /// a downgrade is real.
    pub fn set_permission(&self, level: keel_core::PermissionLevel) {
        self.driver.set_permission_level(level);
    }

    /// The rig this session was opened with.
    pub fn rig_name(&self) -> String {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .rig
            .clone()
    }

    /// The working set in force: the profile's base, or the raised value after
    /// the context ladder moved.
    pub fn context_budget(&self) -> u32 {
        self.driver.working_set()
    }

    /// The context pressure at the end of recent turns, oldest first.
    pub fn context_history(&self) -> Vec<ContextPoint> {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut points: Vec<ContextPoint> =
            session.turns.iter().filter_map(context_point).collect();
        let excess = points.len().saturating_sub(8);
        points.drain(..excess);
        points
    }

    /// Running and finished-but-unreported background commands. The driver
    /// consumes a result at the next turn; this view never does.
    pub fn background_tasks(&self) -> Vec<BackgroundSnapshot> {
        self.background.snapshot()
    }

    /// Stops one background command. Its result still reaches the model at the
    /// next safe point, so the command is not hidden from the turn.
    pub fn stop_background(&self, id: &str) -> bool {
        self.background.stop(id)
    }

    pub fn events(&self) -> &Arc<EventLog> {
        &self.events
    }

    pub fn todos(&self) -> &Arc<Mutex<TodoList>> {
        &self.todos
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Every scope, primary first, labeled with its access.
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    pub fn add_root(&self, path: &Path, access: AccessKind) -> Result<RootScope, KernelError> {
        self.driver.add_root(path, access)
    }

    pub fn escalate_full_auto(&self, ttl: Duration) {
        self.driver.escalate_full_auto(ttl);
    }

    pub fn revoke_permission_grants(&self) {
        self.driver.revoke_permission_grants();
    }

    pub fn interject(&self, text: &str) -> InterjectionPush {
        self.driver.interject(text)
    }

    /// The turn sequence the next turn will use.
    pub fn next_turn_seq(&self) -> u32 {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session
            .turns
            .last()
            .map(|turn| turn.seq.saturating_add(1))
            .unwrap_or(1)
    }

    /// One finished turn, or `None` before it is persisted.
    pub fn turn(&self, seq: u32) -> Option<TurnView> {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session
            .turns
            .iter()
            .find(|turn| turn.seq == seq)
            .map(TurnView::from)
    }

    /// A keyset page ending before `before` (exclusive), oldest first. The
    /// cursor is a turn sequence, so appends never shift an open page.
    pub fn history(&self, before: Option<u32>, limit: usize) -> TurnPage {
        let limit = limit.clamp(1, 100);
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let matching: Vec<&StoredTurn> = session
            .turns
            .iter()
            .filter(|turn| before.is_none_or(|cursor| turn.seq < cursor))
            .collect();
        let start = matching.len().saturating_sub(limit);
        let older = start > 0;
        let turns: Vec<TurnView> = matching[start..]
            .iter()
            .map(|turn| TurnView::from(*turn))
            .collect();
        let next_before = if older {
            turns.first().map(|turn| turn.seq)
        } else {
            None
        };
        TurnPage { turns, next_before }
    }
}

/// Pairs live tool events into turn rows. The kernel's serialized events
/// omit previews; these rows carry them, bounded by the kernel.
fn tool_lines(records: &[EventRecord]) -> Vec<ToolLine> {
    let mut lines: Vec<ToolLine> = Vec::new();
    // Parallel read-only calls finish out of order; the call id, not the tool
    // name, is what a finished event belongs to.
    let mut pending: HashMap<ToolCallId, usize> = HashMap::new();
    for record in records {
        match &record.event {
            Event::ToolStarted {
                call_id,
                tool,
                args_preview,
                ..
            } => {
                pending.insert(*call_id, lines.len());
                lines.push(ToolLine {
                    name: tool.clone(),
                    ok: None,
                    duration_ms: 0,
                    args: args_preview.clone(),
                    output: None,
                });
            }
            Event::ToolFinished {
                call_id,
                ok,
                duration_ms,
                output_preview,
                ..
            } => {
                if let Some(index) = pending.remove(call_id) {
                    if let Some(line) = lines.get_mut(index) {
                        line.ok = Some(*ok);
                        line.duration_ms = *duration_ms;
                        line.output = output_preview.clone();
                    }
                }
            }
            _ => {}
        }
    }
    lines
}

/// Keeps only the evidence a reader needs when a check did not pass, with the
/// tail of the output (test failures live at the end).
const MAX_VERIFICATION_DETAIL_CHARS: usize = 4000;

fn stored_verification(report: &VerificationReport) -> StoredVerification {
    let evidence = report
        .evidence
        .iter()
        .filter(|item| item.outcome != EvidenceOutcome::Passed)
        .take(8)
        .map(|item| StoredEvidence {
            item_id: item.item_id.clone(),
            verifier: item.verifier.clone(),
            outcome: match item.outcome {
                EvidenceOutcome::Passed => "passed",
                EvidenceOutcome::Failed => "failed",
                EvidenceOutcome::Unverified => "unverified",
                EvidenceOutcome::Deferred => "deferred",
            }
            .to_owned(),
            detail: tail_chars(&item.detail, MAX_VERIFICATION_DETAIL_CHARS),
        })
        .collect();
    StoredVerification {
        passed: report.passed(),
        evidence,
    }
}

fn tail_chars(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    let tail: String = text.chars().skip(count - limit).collect();
    format!("…{tail}")
}

const MAX_PROJECT_INSTRUCTIONS_BYTES: usize = 16 * 1024;

/// Reads the workspace's agent instructions. AGENTS.md wins; CLAUDE.md is the
/// fallback. The text is bounded so a large file cannot crowd out the turn.
fn project_instructions(workspace: &Path) -> Option<String> {
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let bytes = match std::fs::read(workspace.join(name)) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let Ok(mut text) = String::from_utf8(bytes) else {
            continue;
        };
        let truncated = text.len() > MAX_PROJECT_INSTRUCTIONS_BYTES;
        if truncated {
            let mut end = MAX_PROJECT_INSTRUCTIONS_BYTES;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let mut out = format!("# Project instructions ({name})\n\n{text}");
        if truncated {
            out.push_str("\n\n[instructions truncated]");
        }
        return Some(out);
    }
    None
}

/// One project check and the directory it covers. A changed path selects the
/// check of the deepest project that contains it. A declared check has no
/// directory and always runs.
struct ProjectCheck {
    project: Option<PathBuf>,
    item: AcceptanceItem,
}

/// Every supported project check. A polyglot workspace runs one check per
/// detected ecosystem, in Cargo, Go, Python, Node order. An unknown ecosystem
/// stays unverified instead of guessing a command.
fn workspace_checks(workspace: &Path) -> Vec<ProjectCheck> {
    let mut checks = Vec::new();
    for project in project_directories(workspace, "Cargo.toml") {
        checks.push(ProjectCheck {
            item: cargo_check(&project),
            project: Some(project),
        });
    }
    for project in project_directories(workspace, "go.mod") {
        checks.push(ProjectCheck {
            item: go_check(&project),
            project: Some(project),
        });
    }
    // A test root that contains another one runs both; a nested root is
    // covered by its parent and is folded into it.
    let mut python_roots: Vec<PathBuf> = Vec::new();
    for tests in find_python_tests(workspace) {
        if python_roots.iter().any(|root| tests.starts_with(root)) {
            continue;
        }
        python_roots.retain(|root| !root.starts_with(&tests));
        python_roots.push(tests);
    }
    for tests in python_roots {
        let project = python_project(workspace, &tests);
        checks.push(ProjectCheck {
            project: Some(project),
            item: python_check(workspace, &tests),
        });
    }
    for project in project_directories(workspace, "package.json") {
        if has_node_test(&workspace.join(&project)) {
            checks.push(ProjectCheck {
                item: node_check(&project),
                project: Some(project),
            });
        }
    }
    checks.extend(declared_checks(workspace));
    checks
}

/// A workspace may declare its own check in `.keelson/checks.toml` when its
/// tooling is outside the four detected ecosystems. A file that does not parse
/// becomes one required item that cannot pass, so a typo is visible instead of
/// silently leaving the workspace unchecked.
const DECLARED_CHECKS_FILE: &str = ".keelson/checks.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChecksFile {
    #[serde(default)]
    check: Vec<DeclaredCheck>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredCheck {
    name: String,
    program: String,
    #[serde(default)]
    args: Vec<String>,
}

fn declared_checks(workspace: &Path) -> Vec<ProjectCheck> {
    let Ok(bytes) = std::fs::read(workspace.join(DECLARED_CHECKS_FILE)) else {
        return Vec::new();
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return checks_file_problem("the file is not UTF-8");
    };
    match parse_declared_checks(&text) {
        Ok(checks) => checks,
        Err(message) => checks_file_problem(&message),
    }
}

fn checks_file_problem(message: &str) -> Vec<ProjectCheck> {
    vec![ProjectCheck {
        project: None,
        item: AcceptanceItem {
            id: "workspace-checks-file".to_owned(),
            description: format!("{DECLARED_CHECKS_FILE}: {message}"),
            required: true,
            verify: VerifySpec::Unverifiable {
                reason: format!("{DECLARED_CHECKS_FILE}: {message}"),
            },
        },
    }]
}

fn parse_declared_checks(text: &str) -> Result<Vec<ProjectCheck>, String> {
    let file: ChecksFile = toml::from_str(text).map_err(|error| error.to_string())?;
    let mut checks = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for declared in &file.check {
        let name = declared.name.trim();
        if name.is_empty() {
            return Err("a check needs a name".to_owned());
        }
        if !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        }) {
            return Err(format!(
                "check `{name}` may only use letters, digits, `-` and `_`"
            ));
        }
        if names.contains(&name) {
            return Err(format!("check `{name}` is declared twice"));
        }
        names.push(name);
        let program = declared.program.trim();
        if program.is_empty() {
            return Err(format!("check `{name}` needs a program"));
        }
        let mut description = program.to_owned();
        for arg in &declared.args {
            description.push(' ');
            description.push_str(arg);
        }
        checks.push(ProjectCheck {
            project: None,
            item: check_item(
                format!("workspace-check:{name}"),
                description,
                program.to_owned(),
                declared.args.clone(),
            ),
        });
    }
    Ok(checks)
}

/// Picks the checks for the deepest project that contains each changed path.
/// An empty change set runs nothing. A path outside every project selects
/// only root-level projects; a nested project is checked when its own files
/// change. A declared check has no project and runs when something changed.
fn selected_checks(
    checks: &[ProjectCheck],
    workspace: &Path,
    paths: &[PathBuf],
) -> Vec<AcceptanceItem> {
    if paths.is_empty() {
        return Vec::new();
    }
    let mut selected: Vec<AcceptanceItem> = Vec::new();
    let mut unmatched = false;
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            workspace.join(path)
        };
        let mut best = 0usize;
        let mut matched: Vec<&ProjectCheck> = Vec::new();
        for check in checks {
            let Some(relative) = &check.project else {
                continue;
            };
            let project = workspace.join(relative);
            if !absolute.starts_with(&project) {
                continue;
            }
            let depth = project.components().count();
            if depth > best {
                best = depth;
                matched.clear();
                matched.push(check);
            } else if depth == best {
                matched.push(check);
            }
        }
        if matched.is_empty() {
            unmatched = true;
        }
        for check in matched {
            if !selected.contains(&check.item) {
                selected.push(check.item.clone());
            }
        }
    }
    if unmatched {
        // A root file is covered by the root project, not by every project
        // under it. In a broad dev directory that used to run dozens of
        // unrelated suites.
        for check in checks
            .iter()
            .filter(|check| check.project.as_deref() == Some(Path::new("")))
        {
            if !selected.contains(&check.item) {
                selected.push(check.item.clone());
            }
        }
    }
    for check in checks.iter().filter(|check| check.project.is_none()) {
        if !selected.contains(&check.item) {
            selected.push(check.item.clone());
        }
    }
    selected
}

#[cfg(test)]
fn workspace_check(workspace: &Path) -> Option<AcceptanceItem> {
    workspace_checks(workspace)
        .into_iter()
        .next()
        .map(|check| check.item)
}

/// Project directories (relative to the workspace) that contain `file_name`,
/// root first, then depth 1 to 3.
fn project_directories(workspace: &Path, file_name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if workspace.join(file_name).is_file() {
        found.push(PathBuf::new());
    }
    let mut stack = vec![(workspace.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < 3 && !is_skipped_test_directory(&path) {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) != Some(file_name) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(workspace) else {
                continue;
            };
            let Some(parent) = relative.parent() else {
                continue;
            };
            found.push(parent.to_path_buf());
        }
    }
    found.sort();
    found.dedup();
    found
}

fn check_id(ecosystem: &str, project: &Path) -> String {
    let project = if project.as_os_str().is_empty() {
        "root".to_owned()
    } else {
        command_path(project)
    };
    format!("workspace-tests:{ecosystem}:{project}")
}

fn check_item(
    id: String,
    description: String,
    program: String,
    args: Vec<String>,
) -> AcceptanceItem {
    AcceptanceItem {
        id,
        description,
        required: true,
        verify: VerifySpec::Test { program, args },
    }
}

fn cargo_check(project: &Path) -> AcceptanceItem {
    let id = check_id("cargo", project);
    if project.as_os_str().is_empty() {
        return check_item(
            id,
            "cargo test".to_owned(),
            "cargo".to_owned(),
            vec!["test".to_owned()],
        );
    }
    let manifest = command_path(&project.join("Cargo.toml"));
    check_item(
        id,
        format!("cargo test --manifest-path {manifest}"),
        "cargo".to_owned(),
        vec!["test".to_owned(), "--manifest-path".to_owned(), manifest],
    )
}

fn go_check(project: &Path) -> AcceptanceItem {
    let id = check_id("go", project);
    if project.as_os_str().is_empty() {
        return check_item(
            id,
            "go test ./...".to_owned(),
            "go".to_owned(),
            vec!["test".to_owned(), "./...".to_owned()],
        );
    }
    let directory = command_path(project);
    check_item(
        id,
        format!("go -C {directory} test ./..."),
        "go".to_owned(),
        vec![
            "-C".to_owned(),
            directory,
            "test".to_owned(),
            "./...".to_owned(),
        ],
    )
}

fn node_check(project: &Path) -> AcceptanceItem {
    let id = check_id("node", project);
    if project.as_os_str().is_empty() {
        return check_item(
            id,
            "npm test".to_owned(),
            "npm".to_owned(),
            vec!["test".to_owned()],
        );
    }
    let prefix = command_path(project);
    check_item(
        id,
        format!("npm --prefix {prefix} test"),
        "npm".to_owned(),
        vec!["--prefix".to_owned(), prefix, "test".to_owned()],
    )
}

/// Directories whose test files the Python check should run, relative to the
/// workspace, shallowest first. Root-level test files yield an empty path.
fn find_python_tests(workspace: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![(workspace.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < 3 && !is_skipped_test_directory(&path) {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.ends_with(".py") && (name.starts_with("test_") || name.ends_with("_test.py")) {
                if let Ok(relative) = path.strip_prefix(workspace) {
                    files.push(relative.to_path_buf());
                }
            }
        }
    }
    files.sort_by_key(|path| (path.components().count(), path.clone()));
    let mut directories = Vec::new();
    for file in files {
        let directory = file.parent().map(Path::to_path_buf).unwrap_or_default();
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    directories
}

/// The directory a Python check belongs to. The nearest ancestor with a
/// project manifest or a virtual environment wins; otherwise the parent of a
/// `tests` directory.
fn python_project(workspace: &Path, tests: &Path) -> PathBuf {
    for base in ancestors(workspace, tests) {
        let manifest = [
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "pytest.ini",
            "tox.ini",
        ]
        .iter()
        .any(|name| base.join(name).is_file());
        let venv = [".venv", "venv"]
            .iter()
            .any(|name| base.join(name).is_dir());
        if manifest || venv {
            return base
                .strip_prefix(workspace)
                .unwrap_or(Path::new(""))
                .to_path_buf();
        }
    }
    if tests.file_name().and_then(|name| name.to_str()) == Some("tests") {
        tests.parent().map(Path::to_path_buf).unwrap_or_default()
    } else {
        tests.to_path_buf()
    }
}

fn python_check(workspace: &Path, tests: &Path) -> AcceptanceItem {
    let python = python_program(workspace, tests);
    let pytest = uses_pytest(workspace, tests);
    let target = command_path(tests);
    let mut args = if pytest {
        vec!["-m".to_owned(), "pytest".to_owned(), "-q".to_owned()]
    } else {
        vec![
            "-m".to_owned(),
            "unittest".to_owned(),
            "discover".to_owned(),
        ]
    };
    if !tests.as_os_str().is_empty() {
        if pytest {
            args.push(target);
        } else {
            args.push("-s".to_owned());
            args.push(target);
        }
    }
    if !pytest {
        args.push("-v".to_owned());
    }
    let description = format!("{python} {}", args.join(" "));
    check_item(check_id("python", tests), description, python, args)
}

/// A project-local virtual environment is the interpreter the project's
/// dependencies live in; a bare `python` often misses them. The nearest
/// ancestor of the tests wins, then the workspace root.
fn python_program(workspace: &Path, tests: &Path) -> String {
    let (subdirectory, binary) = if cfg!(windows) {
        ("Scripts", "python.exe")
    } else {
        ("bin", "python")
    };
    for base in ancestors(workspace, tests) {
        for venv in [".venv", "venv"] {
            let path = base.join(venv).join(subdirectory).join(binary);
            if path.is_file() {
                return path.to_string_lossy().into_owned();
            }
        }
    }
    if cfg!(windows) {
        "python".to_owned()
    } else {
        "python3".to_owned()
    }
}

/// Pytest-style tests exit `unittest discover` with code 0 and no tests, which
/// would count as a pass. Run pytest when the project or an ancestor declares
/// it.
fn uses_pytest(workspace: &Path, tests: &Path) -> bool {
    ancestors(workspace, tests)
        .iter()
        .any(|base| pytest_marker(base))
}

fn pytest_marker(directory: &Path) -> bool {
    for name in [
        "pytest.ini",
        "conftest.py",
        "pyproject.toml",
        "setup.cfg",
        "tox.ini",
    ] {
        if let Ok(text) = std::fs::read_to_string(directory.join(name)) {
            if mentions_pytest(&text) {
                return true;
            }
        }
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.starts_with("requirements") || !name.ends_with(".txt") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            if mentions_pytest(&text) {
                return true;
            }
        }
    }
    false
}

/// The tests directory and its parents up to the workspace root.
fn ancestors(workspace: &Path, tests: &Path) -> Vec<PathBuf> {
    let mut bases = Vec::new();
    let mut current = if tests.as_os_str().is_empty() {
        workspace.to_path_buf()
    } else {
        workspace.join(tests)
    };
    loop {
        bases.push(current.clone());
        if current == workspace {
            break;
        }
        match current.parent() {
            Some(parent) if parent.starts_with(workspace) => current = parent.to_path_buf(),
            _ => break,
        }
    }
    if !bases.iter().any(|base| base == workspace) {
        bases.push(workspace.to_path_buf());
    }
    bases
}

fn command_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn mentions_pytest(text: &str) -> bool {
    text.lines()
        .any(|line| !line.trim_start().starts_with('#') && line.contains("pytest"))
}

fn is_skipped_test_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            ".git"
                | "target"
                | "node_modules"
                | "dist"
                | "build"
                | ".venv"
                | "venv"
                | ".tox"
                | "__pycache__"
                | "site-packages"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
                | ".cache"
                | "__pypackages__"
                | ".next"
                | ".svelte-kit"
                | "coverage"
        )
    )
}

fn has_node_test(workspace: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(workspace.join("package.json")) else {
        return false;
    };
    let Ok(package) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    package
        .get("scripts")
        .and_then(|scripts| scripts.get("test"))
        .and_then(|test| test.as_str())
        .is_some_and(|test| {
            let test = test.trim();
            !test.is_empty() && !test.contains("no test specified")
        })
}

/// One project's outputs and trash. The account state root is shared, but a
/// project must not list or restore another project's files.
/// Removes the screenshots of one conversation. The host calls this when the
/// conversation is deleted; the engine owns the folder layout, so the host
/// does not have to know it.
pub fn remove_session_screens(state_dir: &Path, workspace: &Path, session_id: &str) {
    let workspace = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let state_dir = std::fs::canonicalize(state_dir).unwrap_or_else(|_| state_dir.to_path_buf());
    let project = project_state_dir(&state_dir, &workspace);
    let _ = std::fs::remove_dir_all(screen::screens_dir_for(&project, session_id));
}

fn project_state_dir(state_dir: &Path, workspace: &Path) -> PathBuf {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in workspace.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    state_dir.join("projects").join(format!("{hash:016x}"))
}

fn tools(
    project_dir: &Path,
    config: &RuntimeConfig,
    todos: Arc<Mutex<TodoList>>,
    background: Arc<dyn BackgroundTasks>,
    session: Arc<Mutex<StoredSession>>,
    web: Option<web::WebBackend>,
    screens: PathBuf,
) -> Result<ToolRegistry, KernelError> {
    let trash = Arc::new(LocalTrashStore::at(project_dir.join("trash")));
    // One ledger per engine: a write or edit must follow a current read.
    let ledger = ReadLedger::new();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool::new()))?;
    registry.register(Arc::new(ReadFileTool::new().with_ledger(ledger.clone())))?;
    registry.register(Arc::new(ListDirTool::new()))?;
    registry.register(Arc::new(MakeDirTool::new()))?;
    registry.register(Arc::new(CodeSearchTool::new()))?;
    registry.register(Arc::new(GlobTool::new()))?;
    registry.register(Arc::new(git::GitDiffTool::new()))?;
    registry.register(Arc::new(
        ShellTool::coding_defaults()
            .with_timeout(config.limits().tool_timeout)
            .with_background(background),
    ))?;
    registry.register(Arc::new(
        WriteFileTool::new(trash.clone()).with_ledger(ledger.clone()),
    ))?;
    registry.register(Arc::new(
        EditFileTool::new(trash.clone()).with_ledger(ledger.clone()),
    ))?;
    registry.register(Arc::new(
        ApplyEditsTool::new(trash.clone()).with_ledger(ledger.clone()),
    ))?;
    registry.register(Arc::new(
        MultiEditTool::new(trash.clone()).with_ledger(ledger),
    ))?;
    registry.register(Arc::new(MoveFileTool::new(trash.clone())))?;
    registry.register(Arc::new(DeleteFileTool::new(trash.clone())))?;
    registry.register(Arc::new(ListTrashTool::new(trash.clone())))?;
    registry.register(Arc::new(RestoreFileTool::new(trash)))?;
    registry.register(Arc::new(TodoTool::new(todos)))?;
    registry.register(Arc::new(history::SearchHistoryTool::new(session)))?;
    registry.register(Arc::new(web::WebFetchTool::new(web.clone())))?;
    if let Some(backend) = web {
        registry.register(Arc::new(web::WebSearchTool::new(backend)))?;
    }
    registry.register(Arc::new(screen::ListWindowsTool::new()))?;
    registry.register(Arc::new(screen::ScreenCaptureTool::new(screens)))?;
    registry.register(Arc::new(screen::ViewImageTool::new()))?;
    registry.register(Arc::new(video::RenderSceneTool::new()))?;
    registry.register(Arc::new(video::RenderFilmTool::new()))?;
    registry.register(Arc::new(video::ProbeMediaTool::new()))?;
    registry.register(Arc::new(series::NewEpisodeTool::new()))?;
    registry.register(Arc::new(series::ArchiveEpisodeTool::new()))?;
    registry.register(Arc::new(desktop::LaunchAppTool::new()))?;
    registry.register(Arc::new(desktop::WaitForWindowTool::new()))?;
    registry.register(Arc::new(desktop::ListControlsTool::new()))?;
    Ok(registry)
}

fn access_kind(access: rig::Access) -> AccessKind {
    match access {
        rig::Access::ReadOnly => AccessKind::Read,
        rig::Access::ReadWrite => AccessKind::Write,
    }
}

fn build_model(
    config: &RuntimeConfig,
    over: Option<&ModelOverride>,
) -> Result<HostModel, KernelError> {
    let provider = over
        .map(|over| over.provider.as_str())
        .unwrap_or(config.model_route.provider.as_str());
    let model = over
        .map(|over| over.model.clone())
        .unwrap_or_else(|| config.model_route.model.clone());
    let base_url = over
        .and_then(|over| over.base_url.clone())
        .or_else(|| config.model_base_url.clone());
    let key = |default: &str, alternatives: &[&str]| -> Result<String, KernelError> {
        match over {
            Some(over) => Ok(over.api_key.clone()),
            None => api_key(config, default, alternatives),
        }
    };

    match provider {
        "mock" => Ok(HostModel::Mock(DemoMock {
            calls: AtomicU64::new(0),
            echo: config.visible_tool_names.iter().any(|name| name == "echo"),
        })),
        "openai" | "openai-compatible" => {
            let provider = OpenAiProvider::new(OpenAiConfig {
                base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_owned()),
                api_key: key("OPENAI_API_KEY", &[])?,
                model,
                supports_reasoning: config.supports_reasoning,
                max_retries: 2,
            })
            .map_err(model_error)?;
            Ok(HostModel::OpenAi(provider))
        }
        "anthropic" | "anthropic-compatible" => {
            let provider = AnthropicProvider::new(AnthropicConfig {
                base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_owned()),
                api_key: key("ANTHROPIC_API_KEY", &["ANTHROPIC_AUTH_TOKEN"])?,
                model,
                max_retries: 2,
            })
            .map_err(model_error)?;
            Ok(HostModel::Anthropic(provider))
        }
        other => Err(KernelError::InvalidConfig {
            message: format!("unknown model provider `{other}`"),
        }),
    }
}

fn model_error(error: ModelError) -> KernelError {
    KernelError::InvalidConfig {
        message: error.to_string(),
    }
}

fn api_key(
    config: &RuntimeConfig,
    default_env: &str,
    alternatives: &[&str],
) -> Result<String, KernelError> {
    let mut names: Vec<&str> = Vec::new();
    if let Some(name) = config.api_key_env.as_deref() {
        names.push(name);
    }
    names.push(default_env);
    names.extend_from_slice(alternatives);
    for name in &names {
        if let Ok(value) = std::env::var(name) {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
    }
    Err(KernelError::InvalidConfig {
        message: format!("{} is not set", names.join(" or ")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::TurnState;
    use keel_runtime::DeferAllInteractionPort;

    #[test]
    fn old_turns_recover_usage_from_their_events() {
        let turn = StoredTurn {
            seq: 1,
            input: "hi".to_owned(),
            state: "Done".to_owned(),
            answer: "hello".to_owned(),
            thinking: String::new(),
            error: None,
            duration_ms: 2,
            usage: StoredUsage::default(),
            model_calls: 0,
            verification_passed: None,
            verification: None,
            tools: Vec::new(),
            context: None,
            events: vec![StoredEvent {
                name: "model_finished".to_owned(),
                payload: serde_json::json!({
                    "model_finished": {
                        "usage": {
                            "input_tokens": 100,
                            "output_tokens": 7,
                            "cached_input_tokens": 60
                        }
                    }
                }),
            }],
        };

        let view = TurnView::from(&turn);

        assert_eq!(
            view.usage,
            StoredUsage {
                input_tokens: 100,
                output_tokens: 7,
                cached_input_tokens: 60,
            }
        );
        assert_eq!(view.model_calls, 1, "the event count is recovered");
    }

    #[test]
    fn thinking_is_bounded_for_storage() {
        let buffer = Arc::new(Mutex::new(ThinkingBuffer::default()));
        record_thinking(&buffer, &"字".repeat(MAX_TURN_THINKING_BYTES));
        let buffer = buffer.lock().unwrap();
        assert!(buffer.truncated, "the cap was reached");
        assert!(buffer.text.ends_with("… thinking truncated"));
        assert!(
            buffer.text.len() <= MAX_TURN_THINKING_BYTES + "… thinking truncated".len() + 1,
            "the stored text stays near the cap"
        );
    }

    #[test]
    fn a_stored_usage_wins_over_the_events() {
        let mut turn = StoredTurn {
            seq: 1,
            input: "hi".to_owned(),
            state: "Done".to_owned(),
            answer: "hello".to_owned(),
            thinking: String::new(),
            error: None,
            duration_ms: 2,
            usage: StoredUsage {
                input_tokens: 3,
                output_tokens: 1,
                cached_input_tokens: 0,
            },
            model_calls: 1,
            verification_passed: None,
            verification: None,
            tools: Vec::new(),
            context: None,
            events: Vec::new(),
        };
        turn.events.push(StoredEvent {
            name: "model_finished".to_owned(),
            payload: serde_json::json!({
                "model_finished": {
                    "usage": { "input_tokens": 100, "output_tokens": 7 }
                }
            }),
        });

        let view = TurnView::from(&turn);

        assert_eq!(view.usage.input_tokens, 3);
    }

    fn check_programs(items: &[AcceptanceItem]) -> Vec<&str> {
        items
            .iter()
            .map(|item| match &item.verify {
                VerifySpec::Test { program, .. } => program.as_str(),
                _ => panic!("unexpected check"),
            })
            .collect()
    }

    const PROFILE: &str = r#"
[profile]
name = "test"

[model]
provider = "mock"
model = "mock"
thinking_budget = 0

[context]
stages = ["system", "latest"]
budget_tokens = 2048

[tools]
visible = ["echo"]
budget = 4

[verification]
mode = "none"

[permission]
level = "workspace_auto"

[persona]
system = "prompt.md"
"#;

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::TempDir::new().expect("a temp dir");
            std::fs::write(dir.path().join("prompt.md"), "You are a test host.").expect("a prompt");
            std::fs::write(dir.path().join("profile.toml"), PROFILE).expect("a profile");
            std::fs::create_dir(dir.path().join("workspace")).expect("a workspace");
            Self { dir }
        }

        fn rig(&self) -> Rig {
            Rig {
                name: "test".to_owned(),
                title: "Test".to_owned(),
                description: String::new(),
                profile: self.dir.path().join("profile.toml"),
                extra_roots: Vec::new(),
                path: self.dir.path().join("rig.toml"),
            }
        }

        fn engine(&self, interactions: Arc<dyn InteractionPort>) -> Engine {
            let program = self.dir.path();
            Engine::open(OpenRequest {
                rig: self.rig(),
                workspace: program.join("workspace"),
                state_dir: program.join("state"),
                store_path: program.join("state/sessions/test.json"),
                interactions,
                deltas: None,
                model: None,
                permission: None,
                acceptance: true,
                search_url: None,
                search_key: None,
            })
            .expect("the engine opens")
        }

        fn session(&self) -> StoredSession {
            let workspace = std::fs::canonicalize(self.dir.path().join("workspace"))
                .expect("a canonical workspace");
            Store::open(self.dir.path().join("state/sessions/test.json"))
                .load(&workspace)
                .expect("the session loads")
                .expect("the session exists")
        }
    }

    #[tokio::test]
    async fn a_mock_turn_returns_and_persists_the_answer() {
        let fixture = Fixture::new();
        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));

        let outcome = engine
            .run_turn("hi", CancelToken::new())
            .await
            .expect("the turn runs");

        assert_eq!(outcome.state, TurnState::Done);
        assert_eq!(outcome.answer.body, "mock turn complete");
        let session = fixture.session();
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].seq, 1);
        assert_eq!(session.turns[0].input, "hi");
        assert!(
            session.turns[0].events.is_empty(),
            "a new turn keeps its tool lines, not the event stream"
        );
        assert_eq!(session.turns[0].tools.len(), 1);
        assert_eq!(session.turns[0].tools[0].name, "echo");
        assert_eq!(session.turns[0].tools[0].ok, Some(true));
        assert_eq!(
            session.turns[0].tools[0].args.as_deref(),
            Some("text=mock input")
        );
    }

    #[tokio::test]
    async fn saving_an_old_document_absorbs_its_events() {
        let fixture = Fixture::new();
        let workspace = std::fs::canonicalize(fixture.dir.path().join("workspace"))
            .expect("a canonical workspace");
        let mut old = stored_turn();
        old.events = vec![
            StoredEvent {
                name: "model_finished".to_owned(),
                payload: serde_json::json!({
                    "model_finished": {
                        "usage": {
                            "input_tokens": 100,
                            "output_tokens": 7,
                            "cached_input_tokens": 60
                        }
                    }
                }),
            },
            StoredEvent {
                name: "context_pressure".to_owned(),
                payload: serde_json::json!({
                    "context_pressure": { "used": 40, "ceiling": 64 }
                }),
            },
        ];
        Store::open(fixture.dir.path().join("state/sessions/test.json"))
            .save(&StoredSession {
                workspace,
                rig: "test".to_owned(),
                title: String::new(),
                archived: false,
                snapshot: keel_core::SessionSnapshot::default(),
                todos: None,
                turns: vec![old],
            })
            .expect("the old document saves");

        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));
        engine
            .run_turn("next", CancelToken::new())
            .await
            .expect("the turn runs");

        let session = fixture.session();
        assert_eq!(session.turns.len(), 2);
        assert!(
            session.turns[0].events.is_empty(),
            "the old stream is gone after one save"
        );
        assert_eq!(
            session.turns[0].usage,
            StoredUsage {
                input_tokens: 100,
                output_tokens: 7,
                cached_input_tokens: 60,
            }
        );
        assert_eq!(
            session.turns[0].context,
            Some(StoredContext {
                used: 40,
                ceiling: 64,
            })
        );
    }

    #[tokio::test]
    async fn each_turn_records_only_its_own_tool_calls() {
        let fixture = Fixture::new();
        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));

        engine
            .run_turn("one", CancelToken::new())
            .await
            .expect("the first turn");
        engine
            .run_turn("two", CancelToken::new())
            .await
            .expect("the second turn");

        let session = fixture.session();
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[0].tools.len(), 1);
        assert_eq!(
            session.turns[1].tools.len(),
            1,
            "the second turn keeps only its own tool line"
        );
        assert!(
            session.turns[1].events.is_empty(),
            "a new turn writes no events"
        );
        assert_eq!(
            session.turns[1].tools.len(),
            1,
            "the second turn keeps one tool line, not both"
        );
    }

    #[tokio::test]
    async fn a_second_turn_resumes_the_session() {
        let fixture = Fixture::new();
        fixture
            .engine(Arc::new(DeferAllInteractionPort))
            .run_turn("one", CancelToken::new())
            .await
            .expect("the first turn");
        let outcome = fixture
            .engine(Arc::new(DeferAllInteractionPort))
            .run_turn("two", CancelToken::new())
            .await
            .expect("the second turn");

        assert_eq!(outcome.state, TurnState::Done);
        let session = fixture.session();
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[1].seq, 2);
        assert_eq!(session.turns[1].input, "two");
    }

    #[tokio::test]
    async fn host_metadata_survives_a_turn() {
        let fixture = Fixture::new();
        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));
        engine.set_title("renamed").expect("set the title");
        engine.set_archived(true).expect("archive");

        engine
            .run_turn("hi", CancelToken::new())
            .await
            .expect("the turn runs");

        let session = fixture.session();
        assert_eq!(session.title, "renamed");
        assert_eq!(session.rig, "test");
        assert!(session.archived);

        let reopened = fixture.engine(Arc::new(DeferAllInteractionPort));
        reopened
            .run_turn("again", CancelToken::new())
            .await
            .expect("the reopened engine runs");
        let session = fixture.session();
        assert_eq!(session.title, "renamed");
        assert!(session.archived);
        assert_eq!(session.turns.len(), 2);
    }

    #[tokio::test]
    async fn a_model_override_sets_the_context_capacity() {
        let fixture = Fixture::new();
        let engine = Engine::open(OpenRequest {
            rig: fixture.rig(),
            workspace: fixture.dir.path().join("workspace"),
            state_dir: fixture.dir.path().join("state"),
            store_path: fixture.dir.path().join("state/sessions/test.json"),
            interactions: Arc::new(DeferAllInteractionPort),
            deltas: None,
            model: Some(ModelOverride {
                provider: "mock".to_owned(),
                model: "override-model".to_owned(),
                base_url: None,
                api_key: String::new(),
                context_window: Some(1_000_000),
            }),
            permission: None,
            acceptance: true,
            search_url: None,
            search_key: None,
        })
        .expect("the engine opens");

        assert_eq!(engine.config().context_window, 1_000_000);
        assert_eq!(engine.config().model_route.provider, "mock");
        assert_eq!(engine.config().model_route.model, "override-model");
    }

    #[test]
    fn parallel_tool_results_are_paired_by_call_id() {
        let turn_id = keel_core::TurnId::new(1);
        let records = vec![
            EventRecord {
                seq: 1,
                event: Event::ToolStarted {
                    turn_id,
                    call_id: ToolCallId::new(1),
                    tool: "code_search".to_owned(),
                    risk: keel_core::RiskLevel::ReadOnly,
                    args_preview: Some("first".to_owned()),
                },
            },
            EventRecord {
                seq: 2,
                event: Event::ToolStarted {
                    turn_id,
                    call_id: ToolCallId::new(2),
                    tool: "code_search".to_owned(),
                    risk: keel_core::RiskLevel::ReadOnly,
                    args_preview: Some("second".to_owned()),
                },
            },
            EventRecord {
                seq: 3,
                event: Event::ToolFinished {
                    turn_id,
                    call_id: ToolCallId::new(2),
                    tool: "code_search".to_owned(),
                    ok: true,
                    error: None,
                    duration_ms: 2,
                    output_preview: Some("second output".to_owned()),
                },
            },
            EventRecord {
                seq: 4,
                event: Event::ToolFinished {
                    turn_id,
                    call_id: ToolCallId::new(1),
                    tool: "code_search".to_owned(),
                    ok: false,
                    error: None,
                    duration_ms: 1,
                    output_preview: Some("first output".to_owned()),
                },
            },
        ];

        let lines = tool_lines(&records);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].args.as_deref(), Some("first"));
        assert_eq!(lines[0].output.as_deref(), Some("first output"));
        assert_eq!(lines[0].ok, Some(false));
        assert_eq!(lines[1].args.as_deref(), Some("second"));
        assert_eq!(lines[1].output.as_deref(), Some("second output"));
        assert_eq!(lines[1].ok, Some(true));
    }

    #[test]
    fn verification_evidence_keeps_failures_and_bounds_the_tail() {
        let long = "x".repeat(MAX_VERIFICATION_DETAIL_CHARS + 10);
        let report = VerificationReport {
            evidence: vec![
                keel_core::Evidence {
                    item_id: "tests".to_owned(),
                    verifier: "test".to_owned(),
                    outcome: EvidenceOutcome::Passed,
                    detail: "ok".to_owned(),
                },
                keel_core::Evidence {
                    item_id: "lint".to_owned(),
                    verifier: "test".to_owned(),
                    outcome: EvidenceOutcome::Failed,
                    detail: long,
                },
            ],
            required_failed: vec!["lint".to_owned()],
            required_unverified: Vec::new(),
            optional_failed: Vec::new(),
        };

        let stored = stored_verification(&report);

        assert!(!stored.passed);
        assert_eq!(stored.evidence.len(), 1);
        assert_eq!(stored.evidence[0].item_id, "lint");
        assert_eq!(stored.evidence[0].outcome, "failed");
        assert!(stored.evidence[0].detail.starts_with('…'));
        assert_eq!(
            stored.evidence[0].detail.chars().count(),
            MAX_VERIFICATION_DETAIL_CHARS + 1
        );
    }

    #[tokio::test]
    async fn a_small_model_window_reduces_the_working_set() {
        let fixture = Fixture::new();
        let profile = PROFILE
            .replace("budget_tokens = 2048", "budget_tokens = 64000")
            .replace(
                "thinking_budget = 0",
                "thinking_budget = 0\nmax_output_tokens = 8192",
            );
        std::fs::write(fixture.dir.path().join("large-profile.toml"), profile).expect("a profile");
        let mut rig = fixture.rig();
        rig.profile = fixture.dir.path().join("large-profile.toml");

        let engine = Engine::open(OpenRequest {
            rig,
            workspace: fixture.dir.path().join("workspace"),
            state_dir: fixture.dir.path().join("state"),
            store_path: fixture.dir.path().join("state/sessions/test.json"),
            interactions: Arc::new(DeferAllInteractionPort),
            deltas: None,
            model: Some(ModelOverride {
                provider: "mock".to_owned(),
                model: "small-window".to_owned(),
                base_url: None,
                api_key: String::new(),
                context_window: Some(32_000),
            }),
            permission: None,
            acceptance: true,
            search_url: None,
            search_key: None,
        })
        .expect("the engine opens");

        assert_eq!(engine.config().context.budget_tokens, 32_000 - 8192);
        assert_eq!(engine.config().context_window, 32_000);
    }

    #[test]
    fn web_search_is_visible_only_with_a_provider_endpoint() {
        for (base_url, expected) in [
            ("https://api.deepseek.com", true),
            ("http://192.168.3.132:18081/v1", false),
        ] {
            let fixture = Fixture::new();
            let profile = PROFILE.replace(
                "visible = [\"echo\"]",
                "visible = [\"echo\", \"web_search\", \"web_fetch\"]",
            );
            std::fs::write(fixture.dir.path().join("web-profile.toml"), profile)
                .expect("a profile");
            let mut rig = fixture.rig();
            rig.profile = fixture.dir.path().join("web-profile.toml");
            let engine = Engine::open(OpenRequest {
                rig,
                workspace: fixture.dir.path().join("workspace"),
                state_dir: fixture.dir.path().join("state"),
                store_path: fixture.dir.path().join("state/sessions/test.json"),
                interactions: Arc::new(DeferAllInteractionPort),
                deltas: None,
                model: Some(ModelOverride {
                    provider: "mock".to_owned(),
                    model: "mock".to_owned(),
                    base_url: Some(base_url.to_owned()),
                    api_key: "test".to_owned(),
                    context_window: None,
                }),
                permission: None,
                acceptance: false,
                search_url: None,
                search_key: None,
            })
            .expect("the engine opens");
            let visible = engine
                .config()
                .visible_tool_names
                .iter()
                .any(|name| name == "web_search");
            assert_eq!(visible, expected, "{base_url}");
            assert!(
                engine
                    .config()
                    .visible_tool_names
                    .iter()
                    .any(|name| name == "web_fetch"),
                "the local fetch is always visible when listed"
            );
        }
    }

    #[test]
    fn a_search_key_serves_the_web_tools_for_a_local_model() {
        let fixture = Fixture::new();
        let profile = PROFILE.replace(
            "visible = [\"echo\"]",
            "visible = [\"echo\", \"web_search\", \"web_fetch\"]",
        );
        std::fs::write(fixture.dir.path().join("web-profile.toml"), profile).expect("a profile");
        let mut rig = fixture.rig();
        rig.profile = fixture.dir.path().join("web-profile.toml");
        let engine = Engine::open(OpenRequest {
            rig,
            workspace: fixture.dir.path().join("workspace"),
            state_dir: fixture.dir.path().join("state"),
            store_path: fixture.dir.path().join("state/sessions/test.json"),
            interactions: Arc::new(DeferAllInteractionPort),
            deltas: None,
            model: Some(ModelOverride {
                provider: "openai-compatible".to_owned(),
                model: "q2_0".to_owned(),
                base_url: Some("http://192.168.3.132:18081/v1".to_owned()),
                api_key: "local".to_owned(),
                context_window: None,
            }),
            permission: None,
            acceptance: false,
            search_url: None,
            search_key: Some("search-key".to_owned()),
        })
        .expect("the engine opens");

        assert!(
            engine
                .config()
                .visible_tool_names
                .iter()
                .any(|name| name == "web_search"),
            "a saved search key registers web_search for a local model"
        );
    }

    #[tokio::test]
    async fn one_engine_runs_many_turns() {
        let fixture = Fixture::new();
        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));

        let first = engine
            .run_turn("one", CancelToken::new())
            .await
            .expect("the first turn");
        let second = engine
            .run_turn("two", CancelToken::new())
            .await
            .expect("the second turn");

        assert_eq!(first.state, TurnState::Done);
        assert_eq!(second.state, TurnState::Done);
        assert_eq!(fixture.session().turns.len(), 2);
    }

    #[tokio::test]
    async fn history_pages_by_turn_sequence() {
        let fixture = Fixture::new();
        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));
        for input in ["one", "two", "three"] {
            engine
                .run_turn(input, CancelToken::new())
                .await
                .expect("the turn");
        }

        let page = engine.history(None, 2);
        assert_eq!(
            page.turns.iter().map(|turn| turn.seq).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(page.next_before, Some(2));

        let older = engine.history(page.next_before, 2);
        assert_eq!(
            older.turns.iter().map(|turn| turn.seq).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(older.next_before, None);
    }

    #[test]
    fn the_tool_registry_exposes_edit_file() {
        let fixture = Fixture::new();
        let config = load_profile(&fixture.rig().profile).expect("the profile loads");
        let todos = Arc::new(Mutex::new(TodoList::default()));

        let background: Arc<dyn BackgroundTasks> = Arc::new(LocalBackgroundTasks::new(
            fixture.dir.path().join("outputs"),
        ));
        let session = Arc::new(Mutex::new(StoredSession {
            workspace: fixture.dir.path().to_path_buf(),
            rig: "test".to_owned(),
            title: String::new(),
            archived: false,
            snapshot: Default::default(),
            todos: None,
            turns: Vec::new(),
        }));
        let registry = tools(
            fixture.dir.path(),
            &config,
            todos,
            background,
            session,
            None,
            fixture.dir.path().join("screens"),
        )
        .expect("the registry builds");

        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("multi_edit").is_some());
        assert!(registry.get("apply_edits").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("git_diff").is_some());
        assert!(registry.get("search_history").is_some());
    }

    #[test]
    fn agents_md_is_appended_to_the_system_prompt() {
        let fixture = Fixture::new();
        std::fs::write(
            fixture.dir.path().join("workspace/AGENTS.md"),
            "Run cargo test before claiming done.\n",
        )
        .expect("project instructions");

        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));

        let prompt = &engine.config().system_prompt;
        assert!(
            prompt.contains("Project instructions (AGENTS.md)"),
            "{prompt}"
        );
        assert!(
            prompt.contains("Run cargo test before claiming done."),
            "{prompt}"
        );
    }

    #[test]
    fn claude_md_is_the_instructions_fallback() {
        let fixture = Fixture::new();
        std::fs::write(
            fixture.dir.path().join("workspace/CLAUDE.md"),
            "Use the project formatter.\n",
        )
        .expect("project instructions");

        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));

        let prompt = &engine.config().system_prompt;
        assert!(
            prompt.contains("Project instructions (CLAUDE.md)"),
            "{prompt}"
        );
        assert!(prompt.contains("Use the project formatter."), "{prompt}");
    }

    #[test]
    fn agents_md_wins_over_claude_md() {
        let fixture = Fixture::new();
        std::fs::write(
            fixture.dir.path().join("workspace/AGENTS.md"),
            "agents instructions\n",
        )
        .expect("agents");
        std::fs::write(
            fixture.dir.path().join("workspace/CLAUDE.md"),
            "claude instructions\n",
        )
        .expect("claude");

        let engine = fixture.engine(Arc::new(DeferAllInteractionPort));

        let prompt = &engine.config().system_prompt;
        assert!(prompt.contains("agents instructions"), "{prompt}");
        assert!(!prompt.contains("claude instructions"), "{prompt}");
    }

    #[test]
    fn a_cargo_workspace_gets_a_test_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
            .expect("a manifest");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "node test.js"}}"#,
        )
        .expect("a node manifest");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(check.required);
        assert_eq!(check.id, "workspace-tests:cargo:root");
        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "cargo" && args == &["test".to_owned()]
        ));

        let python = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(python.path().join("test_stats.py"), "pass\n").expect("a test file");
        let check = workspace_check(python.path()).expect("a python check");
        let expected = if cfg!(windows) { "python" } else { "python3" };
        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == expected && args.iter().any(|arg| arg == "unittest")
        ));

        let empty = tempfile::TempDir::new().expect("a temp dir");
        assert!(workspace_check(empty.path()).is_none());
    }

    #[test]
    fn a_pytest_project_gets_a_pytest_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            dir.path().join("test_stats.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\ntestpaths = [\"tests\"]\n",
        )
        .expect("a pytest config");

        let check = workspace_check(dir.path()).expect("a python check");
        let expected = if cfg!(windows) { "python" } else { "python3" };
        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == expected && args == &["-m".to_owned(), "pytest".to_owned(), "-q".to_owned()]
        ));
        assert_eq!(check.description, format!("{expected} -m pytest -q"));
    }

    #[test]
    fn a_pytest_dependency_selects_pytest() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("tests")).expect("a test directory");
        std::fs::write(
            dir.path().join("tests/test_stats.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(dir.path().join("requirements-dev.txt"), "pytest==7.4.4\n")
            .expect("a requirements file");

        let check = workspace_check(dir.path()).expect("a python check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref args, .. }
                if args.iter().any(|arg| arg == "pytest")
        ));
    }

    #[test]
    fn a_commented_pytest_mention_keeps_unittest() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(dir.path().join("test_stats.py"), "pass\n").expect("a test file");
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "# pytest would also work here\n[project]\nname = \"x\"\n",
        )
        .expect("a manifest");

        let check = workspace_check(dir.path()).expect("a python check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref args, .. }
                if args.iter().any(|arg| arg == "unittest")
        ));
    }

    #[test]
    fn a_project_venv_is_preferred() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(dir.path().join("test_stats.py"), "pass\n").expect("a test file");
        let (subdirectory, binary) = if cfg!(windows) {
            ("Scripts", "python.exe")
        } else {
            ("bin", "python")
        };
        let python = dir.path().join(".venv").join(subdirectory).join(binary);
        std::fs::create_dir_all(python.parent().expect("parent")).expect("a venv directory");
        std::fs::write(&python, "").expect("an interpreter");

        let check = workspace_check(dir.path()).expect("a python check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, .. }
                if program == &python.to_string_lossy()
        ));
    }

    #[test]
    fn a_nested_cargo_project_gets_a_manifest_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend")).expect("a project directory");
        std::fs::write(
            dir.path().join("backend/Cargo.toml"),
            "[package]\nname = \"x\"\n",
        )
        .expect("a manifest");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "cargo"
                    && args == &[
                        "test".to_owned(),
                        "--manifest-path".to_owned(),
                        "backend/Cargo.toml".to_owned(),
                    ]
        ));
        assert_eq!(
            check.description,
            "cargo test --manifest-path backend/Cargo.toml"
        );
        assert_eq!(check.id, "workspace-tests:cargo:backend");
    }

    #[test]
    fn the_root_project_check_comes_first() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .expect("a root manifest");
        std::fs::create_dir_all(dir.path().join("backend")).expect("a project directory");
        std::fs::write(
            dir.path().join("backend/Cargo.toml"),
            "[package]\nname = \"nested\"\n",
        )
        .expect("a nested manifest");

        let checks = workspace_checks(dir.path());

        assert_eq!(checks.len(), 2);
        assert!(matches!(
            &checks[0].item.verify,
            VerifySpec::Test { args, .. } if args == &["test".to_owned()]
        ));
        assert!(matches!(
            &checks[1].item.verify,
            VerifySpec::Test { args, .. }
                if args == &[
                    "test".to_owned(),
                    "--manifest-path".to_owned(),
                    "backend/Cargo.toml".to_owned(),
                ]
        ));
    }

    #[test]
    fn a_cargo_monorepo_runs_each_crate() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        for name in ["api", "worker"] {
            std::fs::create_dir_all(dir.path().join(name)).expect("a project directory");
            std::fs::write(
                dir.path().join(name).join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\n"),
            )
            .expect("a manifest");
        }

        let checks = workspace_checks(dir.path());
        let items: Vec<AcceptanceItem> = checks.iter().map(|check| check.item.clone()).collect();
        assert_eq!(check_programs(&items), ["cargo", "cargo"]);

        let api = selected_checks(&checks, dir.path(), &[PathBuf::from("api/src/lib.rs")]);
        assert_eq!(check_programs(&api), ["cargo"]);
        assert!(matches!(
            &api[0].verify,
            VerifySpec::Test { args, .. }
                if args.iter().any(|arg| arg == "api/Cargo.toml")
        ));

        let worker = selected_checks(&checks, dir.path(), &[PathBuf::from("worker/src/lib.rs")]);
        assert!(matches!(
            &worker[0].verify,
            VerifySpec::Test { args, .. }
                if args.iter().any(|arg| arg == "worker/Cargo.toml")
        ));
    }

    #[test]
    fn a_node_monorepo_runs_each_package() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        for name in ["admin", "web"] {
            std::fs::create_dir_all(dir.path().join(name)).expect("a project directory");
            std::fs::write(
                dir.path().join(name).join("package.json"),
                r#"{"scripts": {"test": "node test.js"}}"#,
            )
            .expect("a manifest");
        }

        let checks = workspace_checks(dir.path());
        let items: Vec<AcceptanceItem> = checks.iter().map(|check| check.item.clone()).collect();
        assert_eq!(check_programs(&items), ["npm", "npm"]);

        let web = selected_checks(&checks, dir.path(), &[PathBuf::from("web/src/app.ts")]);
        assert!(matches!(
            &web[0].verify,
            VerifySpec::Test { args, .. } if args.iter().any(|arg| arg == "web")
        ));

        let admin = selected_checks(&checks, dir.path(), &[PathBuf::from("admin/src/app.ts")]);
        assert!(matches!(
            &admin[0].verify,
            VerifySpec::Test { args, .. } if args.iter().any(|arg| arg == "admin")
        ));
    }

    #[test]
    fn a_python_monorepo_runs_each_project() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        for name in ["api", "worker"] {
            std::fs::create_dir_all(dir.path().join(name).join("tests")).expect("a test directory");
            std::fs::write(
                dir.path().join(name).join("tests/test_stats.py"),
                "def test_ok():\n    pass\n",
            )
            .expect("a test file");
            std::fs::write(
                dir.path().join(name).join("pyproject.toml"),
                "[tool.pytest.ini_options]\n",
            )
            .expect("a pytest config");
        }

        let checks = workspace_checks(dir.path());
        let items: Vec<AcceptanceItem> = checks.iter().map(|check| check.item.clone()).collect();
        let python = if cfg!(windows) { "python" } else { "python3" };
        assert_eq!(check_programs(&items), [python, python]);
        let ids: Vec<&str> = checks.iter().map(|check| check.item.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "workspace-tests:python:api/tests",
                "workspace-tests:python:worker/tests",
            ]
        );

        let api = selected_checks(&checks, dir.path(), &[PathBuf::from("api/app.py")]);
        assert!(matches!(
            &api[0].verify,
            VerifySpec::Test { args, .. } if args.iter().any(|arg| arg == "api/tests")
        ));

        let worker = selected_checks(&checks, dir.path(), &[PathBuf::from("worker/app.py")]);
        assert!(matches!(
            &worker[0].verify,
            VerifySpec::Test { args, .. } if args.iter().any(|arg| arg == "worker/tests")
        ));
    }

    #[test]
    fn a_python_project_runs_each_test_root() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("api/tests")).expect("a test directory");
        std::fs::create_dir_all(dir.path().join("api/integration")).expect("a test directory");
        std::fs::write(
            dir.path().join("api/tests/test_a.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("api/integration/test_b.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("api/pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .expect("a pytest config");
        let checks = workspace_checks(dir.path());
        let items: Vec<AcceptanceItem> = checks.iter().map(|check| check.item.clone()).collect();
        let python = if cfg!(windows) { "python" } else { "python3" };
        assert_eq!(check_programs(&items), [python, python]);
        assert_ne!(items[0].id, items[1].id);

        let selected = selected_checks(&checks, dir.path(), &[PathBuf::from("api/app.py")]);

        assert_eq!(selected.len(), 2);
        let args: Vec<String> = selected
            .iter()
            .flat_map(|item| match &item.verify {
                VerifySpec::Test { args, .. } => args.clone(),
                _ => Vec::new(),
            })
            .collect();
        assert!(args.iter().any(|arg| arg == "api/tests"));
        assert!(args.iter().any(|arg| arg == "api/integration"));
    }

    #[test]
    fn a_nested_test_root_folds_into_its_parent() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("tests/unit")).expect("a test directory");
        std::fs::write(
            dir.path().join("tests/test_a.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("tests/unit/test_b.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .expect("a pytest config");

        let checks = workspace_checks(dir.path());
        let items: Vec<AcceptanceItem> = checks.iter().map(|check| check.item.clone()).collect();

        let python = if cfg!(windows) { "python" } else { "python3" };
        assert_eq!(check_programs(&items), [python]);
        assert!(matches!(
            &items[0].verify,
            VerifySpec::Test { args, .. }
                if args.iter().any(|arg| arg == "tests")
                    && !args.iter().any(|arg| arg == "tests/unit")
        ));
    }

    #[test]
    fn a_deep_cargo_project_gets_a_manifest_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("services/api/backend"))
            .expect("a project directory");
        std::fs::write(
            dir.path().join("services/api/backend/Cargo.toml"),
            "[package]\nname = \"api\"\n",
        )
        .expect("a manifest");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "cargo"
                    && args.iter().any(|arg| arg == "services/api/backend/Cargo.toml")
        ));
    }

    #[test]
    fn a_deep_python_test_gets_a_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend/src/tests")).expect("a test directory");
        std::fs::write(
            dir.path().join("backend/src/tests/test_stats.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("backend/pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .expect("a pytest config");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref args, .. }
                if args.iter().any(|arg| arg == "pytest")
                    && args.iter().any(|arg| arg == "backend/src/tests")
        ));
    }

    #[test]
    fn a_nested_go_project_gets_a_directory_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("server")).expect("a project directory");
        std::fs::write(dir.path().join("server/go.mod"), "module example.com/x\n")
            .expect("a manifest");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "go"
                    && args == &[
                        "-C".to_owned(),
                        "server".to_owned(),
                        "test".to_owned(),
                        "./...".to_owned(),
                    ]
        ));
    }

    #[test]
    fn a_nested_node_project_gets_a_prefix_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("frontend")).expect("a project directory");
        std::fs::write(
            dir.path().join("frontend/package.json"),
            r#"{"scripts": {"test": "vitest run"}}"#,
        )
        .expect("a manifest");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "npm"
                    && args == &[
                        "--prefix".to_owned(),
                        "frontend".to_owned(),
                        "test".to_owned(),
                    ]
        ));
    }

    #[test]
    fn a_nested_python_project_uses_its_venv() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend/tests")).expect("a test directory");
        std::fs::write(
            dir.path().join("backend/tests/test_stats.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        let (subdirectory, binary) = if cfg!(windows) {
            ("Scripts", "python.exe")
        } else {
            ("bin", "python")
        };
        let python = dir
            .path()
            .join("backend")
            .join(".venv")
            .join(subdirectory)
            .join(binary);
        std::fs::create_dir_all(python.parent().expect("parent")).expect("a venv directory");
        std::fs::write(&python, "").expect("an interpreter");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == &python.to_string_lossy()
                    && args.iter().any(|arg| arg == "backend/tests")
        ));
    }

    #[test]
    fn a_nested_python_project_runs_pytest() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend/tests")).expect("a test directory");
        std::fs::write(
            dir.path().join("backend/tests/test_stats.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("backend/pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .expect("a pytest config");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref args, .. }
                if args.iter().any(|arg| arg == "pytest")
                    && args.iter().any(|arg| arg == "backend/tests")
        ));
    }

    #[test]
    fn a_polyglot_workspace_gets_every_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
            .expect("a cargo manifest");
        std::fs::write(dir.path().join("test_stats.py"), "pass\n").expect("a test file");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "node test.js"}}"#,
        )
        .expect("a node manifest");

        let checks = workspace_checks(dir.path());
        let programs: Vec<&str> = checks
            .iter()
            .map(|check| match &check.item.verify {
                VerifySpec::Test { program, .. } => program.as_str(),
                _ => panic!("unexpected check"),
            })
            .collect();
        let python = if cfg!(windows) { "python" } else { "python3" };

        assert_eq!(programs, ["cargo", python, "npm"]);
        let ids: Vec<&str> = checks.iter().map(|check| check.item.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "workspace-tests:cargo:root",
                "workspace-tests:python:root",
                "workspace-tests:node:root",
            ]
        );
        assert!(checks.iter().all(|check| check.item.required));
    }

    #[tokio::test]
    async fn a_polyglot_workspace_runs_every_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"polyglot\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("a cargo manifest");
        std::fs::create_dir_all(dir.path().join("src")).expect("a source directory");
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "#[test]\nfn ok() {\n    assert_eq!(2 + 2, 4);\n}\n",
        )
        .expect("a test");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "node test.js"}}"#,
        )
        .expect("a node manifest");
        std::fs::write(dir.path().join("test.js"), "console.log('ok');\n").expect("a node test");

        let items: Vec<AcceptanceItem> = workspace_checks(dir.path())
            .into_iter()
            .map(|check| check.item)
            .collect();
        let report = keel_runtime::AcceptanceVerifier::new(dir.path())
            .verify(&items, &CancelToken::new())
            .await;

        assert_eq!(report.evidence.len(), 2, "{report:?}");
        assert!(report.passed(), "{report:?}");
    }

    #[test]
    fn a_changed_file_selects_its_project() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend")).expect("a project directory");
        std::fs::write(
            dir.path().join("backend/Cargo.toml"),
            "[package]\nname = \"x\"\n",
        )
        .expect("a cargo manifest");
        std::fs::create_dir_all(dir.path().join("frontend")).expect("a project directory");
        std::fs::write(
            dir.path().join("frontend/package.json"),
            r#"{"scripts": {"test": "node test.js"}}"#,
        )
        .expect("a node manifest");
        std::fs::write(dir.path().join("README.md"), "root\n").expect("a readme");
        let checks = workspace_checks(dir.path());

        let backend = selected_checks(&checks, dir.path(), &[PathBuf::from("backend/src/lib.rs")]);
        assert_eq!(check_programs(&backend), ["cargo"]);

        let frontend =
            selected_checks(&checks, dir.path(), &[PathBuf::from("frontend/src/app.ts")]);
        assert_eq!(check_programs(&frontend), ["npm"]);

        // A root file belongs to the root project, not to every project under
        // it. Running all nested suites for it turned a broad dev directory
        // into a seventeen-minute verification.
        let root = selected_checks(&checks, dir.path(), &[PathBuf::from("README.md")]);
        assert!(root.is_empty(), "{:?}", check_programs(&root));

        let none = selected_checks(&checks, dir.path(), &[]);
        assert!(none.is_empty(), "{:?}", check_programs(&none));
    }

    #[test]
    fn site_packages_are_not_discovered_as_tests() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let packages = dir.path().join("anaconda3/Lib/site-packages");
        std::fs::create_dir_all(&packages).expect("a site-packages directory");
        std::fs::write(
            packages.join("test_vendor.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a vendored test");

        assert!(workspace_checks(dir.path()).is_empty());
    }

    #[test]
    fn a_root_project_yields_to_a_nested_one() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .expect("a cargo manifest");
        std::fs::create_dir_all(dir.path().join("frontend")).expect("a project directory");
        std::fs::write(
            dir.path().join("frontend/package.json"),
            r#"{"scripts": {"test": "node test.js"}}"#,
        )
        .expect("a node manifest");
        let checks = workspace_checks(dir.path());

        let frontend =
            selected_checks(&checks, dir.path(), &[PathBuf::from("frontend/src/app.ts")]);
        assert_eq!(check_programs(&frontend), ["npm"]);

        let rust = selected_checks(&checks, dir.path(), &[PathBuf::from("src/lib.rs")]);
        assert_eq!(check_programs(&rust), ["cargo"]);

        let root = selected_checks(&checks, dir.path(), &[PathBuf::from("README.md")]);
        assert_eq!(check_programs(&root), ["cargo"]);
    }

    #[test]
    fn a_nested_python_change_selects_the_python_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend/tests")).expect("a test directory");
        std::fs::write(
            dir.path().join("backend/tests/test_stats.py"),
            "def test_ok():\n    pass\n",
        )
        .expect("a test file");
        std::fs::write(
            dir.path().join("backend/pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .expect("a pytest config");
        let checks = workspace_checks(dir.path());

        let selected = selected_checks(&checks, dir.path(), &[PathBuf::from("backend/app.py")]);

        let python = if cfg!(windows) { "python" } else { "python3" };
        assert_eq!(check_programs(&selected), [python]);
    }

    #[test]
    fn a_go_workspace_gets_a_test_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(dir.path().join("go.mod"), "module example.com/x\n").expect("a manifest");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "go" && args == &vec!["test".to_owned(), "./...".to_owned()]
        ));
    }

    #[test]
    fn node_needs_a_test_script() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "vitest run"}}"#,
        )
        .expect("a manifest");

        let check = workspace_check(dir.path()).expect("a check");
        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "npm" && args == &["test".to_owned()]
        ));

        let empty = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            empty.path().join("package.json"),
            r#"{"scripts": {"build": "vite build"}}"#,
        )
        .expect("a manifest");
        assert!(workspace_check(empty.path()).is_none());

        let placeholder = tempfile::TempDir::new().expect("a temp dir");
        std::fs::write(
            placeholder.path().join("package.json"),
            r#"{"scripts": {"test": "echo \"Error: no test specified\" && exit 1"}}"#,
        )
        .expect("a manifest");
        assert!(workspace_check(placeholder.path()).is_none());
    }

    #[test]
    fn a_nested_python_test_gets_a_check() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("tests/unit")).expect("directories");
        std::fs::write(dir.path().join("tests/unit/test_stats.py"), "pass\n").expect("a test file");

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == if cfg!(windows) { "python" } else { "python3" }
                    && args.iter().any(|arg| arg == "unittest")
        ));
    }

    #[test]
    fn each_workspace_gets_its_own_project_state_directory() {
        let state = Path::new("state");
        let first = project_state_dir(state, Path::new("work/first"));
        let second = project_state_dir(state, Path::new("work/second"));

        assert_ne!(first, second);
        assert_eq!(first, project_state_dir(state, Path::new("work/first")));
        assert!(first.starts_with(state.join("projects")));
    }

    #[tokio::test]
    async fn a_host_receives_text_deltas() {
        let fixture = Fixture::new();
        let (tx, mut rx) = mpsc::channel(64);
        let engine = Engine::open(OpenRequest {
            rig: fixture.rig(),
            workspace: fixture.dir.path().join("workspace"),
            state_dir: fixture.dir.path().join("state"),
            store_path: fixture.dir.path().join("state/sessions/test.json"),
            interactions: Arc::new(DeferAllInteractionPort),
            deltas: Some(tx),
            model: None,
            permission: None,
            acceptance: true,
            search_url: None,
            search_key: None,
        })
        .expect("the engine opens");

        engine
            .run_turn("hi", CancelToken::new())
            .await
            .expect("the turn");

        let mut text = String::new();
        while let Ok(Some(delta)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await
        {
            if let ModelDelta::Text(part) = delta {
                text.push_str(&part);
            }
        }
        assert!(text.contains("mock turn complete"), "no deltas: {text:?}");
    }

    fn declare_checks(dir: &Path, contents: &str) {
        std::fs::create_dir_all(dir.join(".keelson")).expect("a config directory");
        std::fs::write(dir.join(".keelson/checks.toml"), contents).expect("a checks file");
    }

    #[test]
    fn a_declared_check_is_a_candidate() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        declare_checks(
            dir.path(),
            "[[check]]\nname = \"jvm\"\nprogram = \"java\"\nargs = [\"-version\"]\n",
        );

        let check = workspace_check(dir.path()).expect("a check");

        assert_eq!(check.id, "workspace-check:jvm");
        assert_eq!(check.description, "java -version");
        assert!(check.required);
        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref program, ref args }
                if program == "java" && args == &["-version".to_owned()]
        ));
    }

    #[test]
    fn a_declared_check_always_runs() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("backend")).expect("a project directory");
        std::fs::write(
            dir.path().join("backend/Cargo.toml"),
            "[package]\nname = \"x\"\n",
        )
        .expect("a cargo manifest");
        declare_checks(
            dir.path(),
            "[[check]]\nname = \"jvm\"\nprogram = \"java\"\nargs = [\"-version\"]\n",
        );

        let checks = workspace_checks(dir.path());
        let selected = selected_checks(&checks, dir.path(), &[PathBuf::from("backend/src/lib.rs")]);

        let ids: Vec<&str> = selected.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            ["workspace-tests:cargo:backend", "workspace-check:jvm"]
        );
    }

    #[tokio::test]
    async fn a_declared_check_runs_and_fails_by_its_exit_code() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        declare_checks(
            dir.path(),
            "[[check]]\nname = \"toolchain\"\nprogram = \"rustc\"\nargs = [\"--version\"]\n\
             [[check]]\nname = \"broken\"\nprogram = \"rustc\"\nargs = [\"--no-such-flag\"]\n",
        );

        let items: Vec<AcceptanceItem> = workspace_checks(dir.path())
            .into_iter()
            .map(|check| check.item)
            .collect();
        let report = keel_runtime::AcceptanceVerifier::new(dir.path())
            .verify(&items, &CancelToken::new())
            .await;

        assert_eq!(report.evidence.len(), 2, "{report:?}");
        assert_eq!(report.required_failed, vec!["workspace-check:broken"]);
    }

    #[test]
    fn the_messy_example_gets_a_python_check_and_its_declared_java_check() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/messy-workspace");
        let ids: Vec<String> = workspace_checks(&workspace)
            .into_iter()
            .map(|check| check.item.id)
            .collect();

        assert!(
            ids.contains(&"workspace-tests:python:tests".to_owned()),
            "{ids:?}"
        );
        assert!(ids.contains(&"workspace-check:java".to_owned()), "{ids:?}");
    }

    #[test]
    fn a_checks_file_that_does_not_parse_is_a_failing_item() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        declare_checks(dir.path(), "[[check]\nname = \"jvm\"\n");

        let check = workspace_check(dir.path()).expect("a check");

        assert_eq!(check.id, "workspace-checks-file");
        assert!(check.required);
        assert!(matches!(
            check.verify,
            VerifySpec::Unverifiable { ref reason }
                if reason.starts_with(".keelson/checks.toml: ")
        ));
    }

    #[test]
    fn a_declared_check_needs_a_unique_name_and_program() {
        for contents in [
            "[[check]]\nprogram = \"java\"\n",
            "[[check]]\nname = \"bad name\"\nprogram = \"java\"\n",
            "[[check]]\nname = \"jvm\"\nprogram = \"\"\n",
            "[[check]]\nname = \"jvm\"\nprogram = \"java\"\n[[check]]\nname = \"jvm\"\nprogram = \"java\"\n",
            "[[check]]\nname = \"jvm\"\nprogram = \"java\"\narg = [\"typo\"]\n",
        ] {
            let dir = tempfile::TempDir::new().expect("a temp dir");
            declare_checks(dir.path(), contents);

            let check = workspace_check(dir.path()).expect("a check");

            assert_eq!(check.id, "workspace-checks-file", "{contents}");
            assert!(check.required);
            assert!(matches!(
                check.verify,
                VerifySpec::Unverifiable { ref reason }
                    if reason.starts_with(".keelson/checks.toml: ")
            ));
        }
    }

    fn stored_turn() -> StoredTurn {
        StoredTurn {
            seq: 1,
            input: "hi".to_owned(),
            state: "Done".to_owned(),
            answer: "hello".to_owned(),
            thinking: String::new(),
            error: None,
            duration_ms: 2,
            usage: StoredUsage::default(),
            model_calls: 0,
            verification_passed: None,
            verification: None,
            tools: Vec::new(),
            context: None,
            events: Vec::new(),
        }
    }

    #[test]
    fn a_stored_context_point_wins_over_the_events() {
        let mut turn = stored_turn();
        turn.context = Some(StoredContext {
            used: 7,
            ceiling: 9,
        });
        turn.events.push(StoredEvent {
            name: "context_pressure".to_owned(),
            payload: serde_json::json!({
                "context_pressure": { "used": 1, "ceiling": 2 }
            }),
        });

        assert_eq!(
            context_point(&turn),
            Some(ContextPoint {
                seq: 1,
                used: 7,
                ceiling: 9,
            })
        );
    }

    #[test]
    fn an_old_turn_recovers_its_context_point_from_the_events() {
        let mut turn = stored_turn();
        turn.events.push(StoredEvent {
            name: "context_pressure".to_owned(),
            payload: serde_json::json!({
                "context_pressure": { "used": 40, "ceiling": 64 }
            }),
        });

        assert_eq!(
            context_point(&turn),
            Some(ContextPoint {
                seq: 1,
                used: 40,
                ceiling: 64,
            })
        );
    }

    #[test]
    fn absorbing_an_old_turns_events_keeps_the_numbers() {
        let mut turn = stored_turn();
        turn.events = vec![
            StoredEvent {
                name: "model_finished".to_owned(),
                payload: serde_json::json!({
                    "model_finished": {
                        "usage": {
                            "input_tokens": 100,
                            "output_tokens": 7,
                            "cached_input_tokens": 60
                        }
                    }
                }),
            },
            StoredEvent {
                name: "context_pressure".to_owned(),
                payload: serde_json::json!({
                    "context_pressure": { "used": 40, "ceiling": 64 }
                }),
            },
        ];

        absorb_events(&mut turn);

        assert!(turn.events.is_empty(), "the stream is gone");
        assert_eq!(
            turn.usage,
            StoredUsage {
                input_tokens: 100,
                output_tokens: 7,
                cached_input_tokens: 60,
            }
        );
        assert_eq!(
            turn.context,
            Some(StoredContext {
                used: 40,
                ceiling: 64,
            })
        );
        assert_eq!(TurnView::from(&turn).usage, turn.usage);
        assert_eq!(
            context_point(&turn),
            Some(ContextPoint {
                seq: 1,
                used: 40,
                ceiling: 64,
            })
        );
    }

    #[test]
    fn absorbing_events_keeps_a_stored_value() {
        let mut turn = stored_turn();
        turn.usage = StoredUsage {
            input_tokens: 3,
            output_tokens: 1,
            cached_input_tokens: 0,
        };
        turn.context = Some(StoredContext {
            used: 7,
            ceiling: 9,
        });
        turn.events = vec![
            StoredEvent {
                name: "model_finished".to_owned(),
                payload: serde_json::json!({
                    "model_finished": {
                        "usage": {
                            "input_tokens": 100,
                            "output_tokens": 7,
                            "cached_input_tokens": 60
                        }
                    }
                }),
            },
            StoredEvent {
                name: "context_pressure".to_owned(),
                payload: serde_json::json!({
                    "context_pressure": { "used": 40, "ceiling": 64 }
                }),
            },
        ];

        absorb_events(&mut turn);

        assert!(turn.events.is_empty(), "the stream is gone");
        assert_eq!(
            turn.usage,
            StoredUsage {
                input_tokens: 3,
                output_tokens: 1,
                cached_input_tokens: 0,
            }
        );
        assert_eq!(
            turn.context,
            Some(StoredContext {
                used: 7,
                ceiling: 9,
            })
        );
    }

    #[test]
    fn a_megabyte_tool_output_is_stored_capped() {
        let mut lines = vec![ToolLine {
            name: "read_file".to_owned(),
            ok: Some(true),
            duration_ms: 5,
            args: None,
            output: Some("x".repeat(1024 * 1024)),
        }];

        cap_tool_lines(&mut lines);

        let output = lines[0].output.as_deref().expect("the output");
        assert!(
            output.chars().count() <= MAX_STORED_TOOL_OUTPUT_CHARS + 40,
            "the stored output stays near the cap"
        );
        assert!(
            output.contains("(+") && output.ends_with("characters)"),
            "the dropped character count is named"
        );
    }

    #[test]
    fn a_new_turn_omits_the_event_stream() {
        let mut turn = stored_turn();
        turn.context = Some(StoredContext {
            used: 1,
            ceiling: 2,
        });

        let text = serde_json::to_string(&turn).expect("the turn serializes");

        assert!(!text.contains("\"events\""), "{text}");
        assert!(text.contains("\"context\""), "{text}");
    }
}
