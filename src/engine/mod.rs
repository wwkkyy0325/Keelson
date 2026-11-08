//! Turn execution: build the kernel from a rig and run turns.
//!
//! This module owns no output. It returns kernel outcomes; the host decides
//! what the user sees and where it goes (ADR 0004).

mod store;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use keel_core::{
    AcceptanceItem, AccessKind, ArgValue, CancelToken, Event, EvidenceOutcome, InteractionPort,
    KernelError, ModelChunk, ModelError, ModelProvider, ModelRequest, ModelStream, Policy, Session,
    TodoList, ToolArgs, ToolCallId, VerificationReport, VerifySpec,
};
use keel_model_anthropic::{AnthropicConfig, AnthropicProvider};
use keel_model_openai::{OpenAiConfig, OpenAiProvider};
use keel_runtime::{
    load_profile, validate_config, BackgroundTasks, CodeSearchTool, ContextBuilder, DeleteFileTool,
    EchoTool, EditFileTool, EventLog, EventRecord, GlobTool, HistoryStage, InteractionBroker,
    InterjectionPush, KernelOptions, LatestStage, ListDirTool, ListTrashTool, LocalBackgroundTasks,
    LocalTrashStore, MakeDirTool, MemoryStage, MockModel, MockStep, MoveFileTool, MultiEditTool,
    PermissionEngine, ReadFileTool, ReadLedger, RestoreFileTool, RootScope, RuntimeConfig,
    ShellTool, SystemStage, TaskStateStage, TodoTool, ToolRegistry, TurnDriver, TurnDriverConfig,
    TurnOutcome, WriteFileTool,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::rig::{self, Rig};

pub use store::{
    Store, StoredEvent, StoredEvidence, StoredSession, StoredTurn, StoredVerification, ToolLine,
};

/// Everything opening a session needs. A host supplies its own interaction
/// port and, when it wants streamed text, a channel for deltas.
pub struct OpenRequest {
    pub rig: Rig,
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    pub store_path: PathBuf,
    pub interactions: Arc<dyn InteractionPort>,
    pub deltas: Option<mpsc::Sender<String>>,
    /// A host-supplied model, used by the desktop onboarding. `None` keeps the
    /// profile's provider and the environment-variable key.
    pub model: Option<ModelOverride>,
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

/// One finished turn as a read model. It carries the compact turn history, not
/// the kernel event stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnView {
    pub seq: u32,
    pub input: String,
    pub answer: String,
    pub state: String,
    pub error: Option<String>,
    pub duration_ms: u64,
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
            state: turn.state.clone(),
            error: turn.error.clone(),
            duration_ms: turn.duration_ms,
            verification_passed: turn.verification_passed,
            verification: turn.verification.clone(),
            tools: turn.tools.clone(),
        }
    }
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
    store: Store,
    session: Mutex<StoredSession>,
    roots: Vec<String>,
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
struct DeltaModel {
    inner: HostModel,
    deltas: Option<mpsc::Sender<String>>,
}

#[async_trait]
impl ModelProvider for DeltaModel {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: &CancelToken,
    ) -> Result<ModelStream, ModelError> {
        let Some(sender) = &self.deltas else {
            return self.inner.stream(request, cancel).await;
        };
        let mut stream = self.inner.stream(request, cancel).await?;
        let sender = sender.clone();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(chunk) = stream.recv().await {
                if let Ok(ModelChunk::TextDelta(text)) = &chunk {
                    let _ = sender.send(text.clone()).await;
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

        let store = Store::open(store_path);
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
        let registry = Arc::new(tools(
            &project_dir,
            &config,
            todos.clone(),
            background.clone(),
        )?);
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

        let model = DeltaModel {
            inner: build_model(&config, model.as_ref())?,
            deltas,
        };
        let policy: Arc<dyn Policy> = Arc::new(config.clone());
        let mut driver = TurnDriver::new(
            model,
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
                background: Some(background),
                options: KernelOptions::default(),
            },
        )
        .with_outputs_dir(outputs_dir);
        // A turn that edited the workspace is checked by the project's own
        // command. A conversational turn skips it.
        if let Some(check) = workspace_check(&workspace) {
            driver = driver.with_acceptance_after_changes(vec![check], 2);
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
            store,
            session: Mutex::new(session),
            roots,
        })
    }

    /// Runs one turn and persists the session once it ends.
    pub async fn run_turn(
        &self,
        input: &str,
        cancel: CancelToken,
    ) -> Result<TurnOutcome, KernelError> {
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
        let records = self.events.snapshot();
        let tools = tool_lines(&records);
        let stored_events = records
            .iter()
            .map(|record| StoredEvent {
                name: record.event.name().to_owned(),
                payload: serde_json::to_value(&record.event).unwrap_or_default(),
            })
            .collect();
        session.turns.push(StoredTurn {
            seq,
            input: input.to_owned(),
            state: outcome.state.to_string(),
            answer: outcome.answer.body.clone(),
            error,
            duration_ms: outcome.duration_ms,
            verification_passed: outcome.verification.as_ref().map(|report| report.passed()),
            verification: outcome.verification.as_ref().map(stored_verification),
            tools,
            events: stored_events,
        });
        self.store.save(&session)
    }

    /// Sets the host-owned title on the open conversation. Persisted now, so
    /// a later turn cannot overwrite it with the value loaded at open.
    pub fn set_title(&self, title: &str) -> Result<(), KernelError> {
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session.title = title.to_owned();
        self.store.save(&session)
    }

    /// Sets the host-owned archive flag on the open conversation.
    pub fn set_archived(&self, archived: bool) -> Result<(), KernelError> {
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        session.archived = archived;
        self.store.save(&session)
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
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
            .filter(|turn| before.map_or(true, |cursor| turn.seq < cursor))
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

/// The first supported project check. An unknown ecosystem stays unverified
/// instead of guessing a command. A manifest may sit in a subdirectory, so a
/// monorepo gets a check for the first project found.
fn workspace_check(workspace: &Path) -> Option<AcceptanceItem> {
    if let Some(project) = project_directories(workspace, "Cargo.toml")
        .into_iter()
        .next()
    {
        return Some(cargo_check(&project));
    }
    if let Some(project) = project_directories(workspace, "go.mod").into_iter().next() {
        return Some(go_check(&project));
    }
    if let Some(tests) = find_python_tests(workspace) {
        return Some(python_check(workspace, &tests));
    }
    if let Some(project) = node_project(workspace) {
        return Some(node_check(&project));
    }
    None
}

/// Project directories (relative to the workspace) that contain `file_name`,
/// root first, then depth 1, then depth 2.
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
                if depth < 2 && !is_skipped_test_directory(&path) {
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

fn check_item(description: String, program: String, args: Vec<String>) -> AcceptanceItem {
    AcceptanceItem {
        id: "workspace-tests".to_owned(),
        description,
        required: true,
        verify: VerifySpec::Test { program, args },
    }
}

fn cargo_check(project: &Path) -> AcceptanceItem {
    if project.as_os_str().is_empty() {
        return check_item(
            "cargo test".to_owned(),
            "cargo".to_owned(),
            vec!["test".to_owned()],
        );
    }
    let manifest = command_path(&project.join("Cargo.toml"));
    check_item(
        format!("cargo test --manifest-path {manifest}"),
        "cargo".to_owned(),
        vec!["test".to_owned(), "--manifest-path".to_owned(), manifest],
    )
}

fn go_check(project: &Path) -> AcceptanceItem {
    if project.as_os_str().is_empty() {
        return check_item(
            "go test ./...".to_owned(),
            "go".to_owned(),
            vec!["test".to_owned(), "./...".to_owned()],
        );
    }
    let directory = command_path(project);
    check_item(
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

fn node_project(workspace: &Path) -> Option<PathBuf> {
    project_directories(workspace, "package.json")
        .into_iter()
        .find(|project| has_node_test(&workspace.join(project)))
}

fn node_check(project: &Path) -> AcceptanceItem {
    if project.as_os_str().is_empty() {
        return check_item(
            "npm test".to_owned(),
            "npm".to_owned(),
            vec!["test".to_owned()],
        );
    }
    let prefix = command_path(project);
    check_item(
        format!("npm --prefix {prefix} test"),
        "npm".to_owned(),
        vec!["--prefix".to_owned(), prefix, "test".to_owned()],
    )
}

/// Directories whose test files the Python check should run, relative to the
/// workspace. Root-level test files yield an empty path.
fn find_python_tests(workspace: &Path) -> Option<PathBuf> {
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
                if depth < 2 && !is_skipped_test_directory(&path) {
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
    files
        .into_iter()
        .next()
        .map(|file| file.parent().map(Path::to_path_buf).unwrap_or_default())
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
    check_item(description, python, args)
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
        MultiEditTool::new(trash.clone()).with_ledger(ledger),
    ))?;
    registry.register(Arc::new(MoveFileTool::new(trash.clone())))?;
    registry.register(Arc::new(DeleteFileTool::new(trash.clone())))?;
    registry.register(Arc::new(ListTrashTool::new(trash.clone())))?;
    registry.register(Arc::new(RestoreFileTool::new(trash)))?;
    registry.register(Arc::new(TodoTool::new(todos)))?;
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
            session.turns[0]
                .events
                .iter()
                .any(|event| event.name == "tool_finished"),
            "the mock tool call is audited"
        );
        assert!(
            session.turns[0]
                .events
                .iter()
                .any(|event| event.name == "turn_finished"),
            "the turn is audited"
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
        })
        .expect("the engine opens");

        assert_eq!(engine.config().context.budget_tokens, 32_000 - 8192);
        assert_eq!(engine.config().context_window, 32_000);
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
        let registry =
            tools(fixture.dir.path(), &config, todos, background).expect("the registry builds");

        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("multi_edit").is_some());
        assert!(registry.get("glob").is_some());
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
    }

    #[test]
    fn a_root_manifest_wins_over_a_nested_one() {
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

        let check = workspace_check(dir.path()).expect("a check");

        assert!(matches!(
            check.verify,
            VerifySpec::Test { ref args, .. } if args == &["test".to_owned()]
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
        })
        .expect("the engine opens");

        engine
            .run_turn("hi", CancelToken::new())
            .await
            .expect("the turn");

        let mut text = String::new();
        while let Ok(Some(delta)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await
        {
            text.push_str(&delta);
        }
        assert!(text.contains("mock turn complete"), "no deltas: {text:?}");
    }
}
