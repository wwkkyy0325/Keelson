//! Turn execution: build the kernel from a rig and run turns.
//!
//! This module owns no output. It returns kernel outcomes; a view decides what
//! the user sees and where it goes (ADR 0002).

mod store;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use keel_core::{
    AccessKind, ArgValue, CancelToken, InteractionPort, KernelError, ModelChunk, ModelError,
    ModelProvider, ModelRequest, ModelStream, Policy, Session, TodoList, ToolArgs, TurnState,
};
use keel_model_anthropic::{AnthropicConfig, AnthropicProvider};
use keel_model_openai::{OpenAiConfig, OpenAiProvider};
use keel_runtime::{
    load_profile, validate_config, CodeSearchTool, ContextBuilder, DeferAllInteractionPort,
    DeleteFileTool, EchoTool, EventLog, HistoryStage, InteractionBroker, InterjectionPush,
    KernelOptions, LatestStage, ListDirTool, ListTrashTool, LocalTrashStore, MakeDirTool,
    MemoryStage, MockModel, MockStep, MoveFileTool, PermissionEngine, ReadFileTool,
    RestoreFileTool, RootScope, RuntimeConfig, ShellTool, SystemStage, TaskStateStage, TodoTool,
    ToolRegistry, TurnDriver, TurnDriverConfig, TurnOutcome, WriteFileTool,
};
use tokio::sync::mpsc;

use crate::rig::{self, Rig};

pub use store::{Store, StoredEvent, StoredSession, StoredTurn};

/// Everything a one-shot run needs. The host resolves the paths; the engine
/// does not read environment variables.
pub struct RunRequest {
    pub rig: Rig,
    pub workspace: PathBuf,
    /// Host state: outputs and trash live here.
    pub state_dir: PathBuf,
    /// The session document to load and save.
    pub store_path: PathBuf,
    pub input: String,
}

/// Everything opening a session needs. A live view supplies its own
/// interaction port and a channel for text deltas; a one-shot run supplies the
/// deferring port and no channel.
pub struct OpenRequest {
    pub rig: Rig,
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    pub store_path: PathBuf,
    pub interactions: Arc<dyn InteractionPort>,
    pub deltas: Option<mpsc::Sender<String>>,
}

/// What a one-shot view needs after a turn.
pub struct TurnReport {
    pub state: TurnState,
    pub answer: String,
    pub error: Option<String>,
}

/// A long-lived host: one driver, one event log, one session document, many
/// turns. Views drive it; it knows nothing about them.
pub struct Engine {
    workspace: PathBuf,
    config: RuntimeConfig,
    driver: TurnDriver<UiModel>,
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

/// Forwards text deltas to a view while the driver consumes the stream. The
/// kernel's event stream carries boundaries, not tokens (ADR 0002).
struct UiModel {
    inner: HostModel,
    deltas: Option<mpsc::Sender<String>>,
}

#[async_trait]
impl ModelProvider for UiModel {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: &CancelToken,
    ) -> Result<ModelStream, ModelError> {
        let Some(ui) = &self.deltas else {
            return self.inner.stream(request, cancel).await;
        };
        let mut stream = self.inner.stream(request, cancel).await?;
        let ui = ui.clone();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(chunk) = stream.recv().await {
                if let Ok(ModelChunk::TextDelta(text)) = &chunk {
                    let _ = ui.send(text.clone()).await;
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
        } = request;

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
        let outputs_dir = state_dir.join("outputs");

        let config = load_profile(&rig.profile)?;
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
        let (restored, session) = match store.load(&workspace)? {
            Some(session) => (Some(session.snapshot.clone()), session),
            None => (
                None,
                StoredSession {
                    workspace: workspace.clone(),
                    snapshot: Default::default(),
                    todos: None,
                    turns: Vec::new(),
                },
            ),
        };
        let todos = Arc::new(Mutex::new(session.todos.clone().unwrap_or_default()));

        let events = Arc::new(EventLog::new());
        let interactions = Arc::new(InteractionBroker::new(
            interactions,
            config.interaction,
            events.clone(),
        ));

        let registry = Arc::new(tools(&state_dir, &config, todos.clone())?);
        let context = ContextBuilder::new(vec![
            Arc::new(SystemStage::new()),
            Arc::new(TaskStateStage::new()),
            Arc::new(MemoryStage::new()),
            Arc::new(HistoryStage::new()),
            Arc::new(LatestStage::new()),
        ]);
        validate_config(&config, &registry, &context)?;

        let model = UiModel {
            inner: build_model(&config)?,
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
                background: None,
                options: KernelOptions::default(),
            },
        )
        .with_outputs_dir(outputs_dir);
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
        let stored_events = self
            .events
            .snapshot()
            .into_iter()
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
            events: stored_events,
        });
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
}

/// Runs one turn for a rig and persists the session.
pub async fn run(request: RunRequest) -> Result<TurnReport, KernelError> {
    let engine = Engine::open(OpenRequest {
        rig: request.rig,
        workspace: request.workspace,
        state_dir: request.state_dir,
        store_path: request.store_path,
        interactions: Arc::new(DeferAllInteractionPort),
        deltas: None,
    })?;
    let outcome = engine.run_turn(&request.input, CancelToken::new()).await?;
    Ok(TurnReport {
        state: outcome.state,
        answer: outcome.answer.body,
        error: outcome
            .error
            .as_ref()
            .map(|error| format!("{} ({})", error, error.code().as_str())),
    })
}

fn tools(
    state_dir: &Path,
    config: &RuntimeConfig,
    todos: Arc<Mutex<TodoList>>,
) -> Result<ToolRegistry, KernelError> {
    let trash = Arc::new(LocalTrashStore::at(state_dir.join("trash")));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool::new()))?;
    registry.register(Arc::new(ReadFileTool::new()))?;
    registry.register(Arc::new(ListDirTool::new()))?;
    registry.register(Arc::new(MakeDirTool::new()))?;
    registry.register(Arc::new(CodeSearchTool::new()))?;
    registry.register(Arc::new(
        ShellTool::coding_defaults().with_timeout(config.limits().tool_timeout),
    ))?;
    registry.register(Arc::new(WriteFileTool::new(trash.clone())))?;
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

fn build_model(config: &RuntimeConfig) -> Result<HostModel, KernelError> {
    match config.model_route.provider.as_str() {
        "mock" => Ok(HostModel::Mock(DemoMock {
            calls: AtomicU64::new(0),
            echo: config.visible_tool_names.iter().any(|name| name == "echo"),
        })),
        "openai" | "openai-compatible" => {
            let provider = OpenAiProvider::new(OpenAiConfig {
                base_url: config
                    .model_base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_owned()),
                api_key: api_key(config, "OPENAI_API_KEY")?,
                model: config.model_route.model.clone(),
                supports_reasoning: config.supports_reasoning,
                max_retries: 2,
            })
            .map_err(model_error)?;
            Ok(HostModel::OpenAi(provider))
        }
        "anthropic" | "anthropic-compatible" => {
            let provider = AnthropicProvider::new(AnthropicConfig {
                base_url: config
                    .model_base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.anthropic.com".to_owned()),
                api_key: api_key(config, "ANTHROPIC_API_KEY")?,
                model: config.model_route.model.clone(),
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

fn api_key(config: &RuntimeConfig, default_env: &str) -> Result<String, KernelError> {
    let name = config.api_key_env.as_deref().unwrap_or(default_env);
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(KernelError::InvalidConfig {
            message: format!("{name} is not set"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::UiMode;

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
                ui: UiMode::Oneshot,
                profile: self.dir.path().join("profile.toml"),
                extra_roots: Vec::new(),
                path: self.dir.path().join("rig.toml"),
            }
        }

        fn request(&self, input: &str) -> RunRequest {
            let program = self.dir.path();
            RunRequest {
                rig: self.rig(),
                workspace: program.join("workspace"),
                state_dir: program.join("state"),
                store_path: program.join("state/sessions/test.json"),
                input: input.to_owned(),
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

        let report = run(fixture.request("hi")).await.expect("the turn runs");

        assert_eq!(report.state, TurnState::Done);
        assert_eq!(report.answer, "mock turn complete");
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
    }

    #[tokio::test]
    async fn a_second_turn_resumes_the_session() {
        let fixture = Fixture::new();

        run(fixture.request("one")).await.expect("the first turn");
        let report = run(fixture.request("two")).await.expect("the second turn");

        assert_eq!(report.state, TurnState::Done);
        let session = fixture.session();
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[1].seq, 2);
        assert_eq!(session.turns[1].input, "two");
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
    async fn a_live_view_receives_text_deltas() {
        let fixture = Fixture::new();
        let (tx, mut rx) = mpsc::channel(64);
        let engine = Engine::open(OpenRequest {
            rig: fixture.rig(),
            workspace: fixture.dir.path().join("workspace"),
            state_dir: fixture.dir.path().join("state"),
            store_path: fixture.dir.path().join("state/sessions/test.json"),
            interactions: Arc::new(DeferAllInteractionPort),
            deltas: Some(tx),
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
