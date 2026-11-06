//! Session persistence.
//!
//! One session is one JSON document: the kernel's opaque snapshot, the task
//! list and the turn rows this host has run. The kernel owns the
//! snapshot shape; this adapter only moves it in and out of a file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use keel_core::{KernelError, SessionSnapshot, TodoList};
use serde::{Deserialize, Serialize};

/// A stored session. The workspace is recorded so a session id cannot be
/// resumed against a different project by accident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub workspace: PathBuf,
    pub snapshot: SessionSnapshot,
    #[serde(default)]
    pub todos: Option<TodoList>,
    #[serde(default)]
    pub turns: Vec<StoredTurn>,
}

/// One tool call as a turn row. The previews are bounded by the kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLine {
    pub name: String,
    /// `None` while the call is still running.
    pub ok: Option<bool>,
    pub duration_ms: u64,
    /// Bounded, redacted call arguments.
    pub args: Option<String>,
    /// Bounded, redacted output.
    pub output: Option<String>,
}

/// One finished turn as the session file keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTurn {
    pub seq: u32,
    pub input: String,
    pub state: String,
    pub answer: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    /// Live tool rows. Absent in sessions written before the field existed.
    #[serde(default)]
    pub tools: Vec<ToolLine>,
    /// Events in emission order. The payload is the kernel's serialized event;
    /// previews are already skipped by the kernel.
    #[serde(default)]
    pub events: Vec<StoredEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

/// A session document at one path.
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    /// Loads the session, or `None` when the file does not exist yet.
    pub fn load(&self, workspace: &Path) -> Result<Option<StoredSession>, KernelError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage_error(&self.path, error)),
        };
        let session: StoredSession =
            serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                message: format!(
                    "session `{}` is not valid JSON: {error}",
                    self.path.display()
                ),
            })?;
        if session.workspace != workspace {
            return Err(KernelError::InvalidConfig {
                message: format!(
                    "session `{}` belongs to `{}`, not `{}`",
                    self.path.display(),
                    session.workspace.display(),
                    workspace.display()
                ),
            });
        }
        Ok(Some(session))
    }

    /// Writes the session through a temporary file so a crash cannot leave a
    /// half-written document in place.
    pub fn save(&self, session: &StoredSession) -> Result<(), KernelError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| storage_error(&self.path, error))?;
        }
        let text =
            serde_json::to_string_pretty(session).map_err(|error| KernelError::Internal {
                message: format!("cannot encode session `{}`: {error}", self.path.display()),
            })?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, text).map_err(|error| storage_error(&temporary, error))?;
        fs::rename(&temporary, &self.path).map_err(|error| storage_error(&self.path, error))?;
        Ok(())
    }
}

fn storage_error(path: &Path, error: io::Error) -> KernelError {
    KernelError::Io {
        message: format!("session store `{}`: {error}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(workspace: PathBuf) -> StoredSession {
        StoredSession {
            workspace,
            snapshot: SessionSnapshot::default(),
            todos: Some(TodoList::default()),
            turns: vec![StoredTurn {
                seq: 1,
                input: "hello".to_owned(),
                state: "Done".to_owned(),
                answer: "hi".to_owned(),
                error: None,
                duration_ms: 3,
                tools: Vec::new(),
                events: vec![StoredEvent {
                    name: "turn_started".to_owned(),
                    payload: serde_json::json!({ "input": "hello" }),
                }],
            }],
        }
    }

    #[test]
    fn a_session_round_trips_through_the_store() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/test.json"));

        store.save(&session(workspace.clone())).expect("save");
        let loaded = store
            .load(&workspace)
            .expect("load")
            .expect("the session exists");

        assert_eq!(loaded.turns.len(), 1);
        assert_eq!(loaded.turns[0].answer, "hi");
        assert_eq!(loaded.turns[0].events[0].name, "turn_started");
        assert!(loaded.todos.is_some());
    }

    #[test]
    fn a_missing_session_is_not_an_error() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let store = Store::open(dir.path().join("sessions/test.json"));

        assert!(store.load(dir.path()).expect("load").is_none());
    }

    #[test]
    fn a_session_from_another_workspace_is_refused() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::create_dir(&first).expect("a workspace");
        fs::create_dir(&second).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/test.json"));
        store.save(&session(first)).expect("save");

        let error = store
            .load(&second)
            .expect_err("another workspace is refused");

        assert!(error.to_string().contains("belongs to"), "{error}");
    }
}
