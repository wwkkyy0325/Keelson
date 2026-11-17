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
    /// The rig that created the conversation. Empty in documents written
    /// before the field existed; the engine fills it on the next open.
    #[serde(default)]
    pub rig: String,
    /// Host-owned display name. Empty means "use the first user message".
    #[serde(default)]
    pub title: String,
    /// Host-owned archive flag. Archived conversations stay in the project.
    #[serde(default)]
    pub archived: bool,
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

/// One acceptance item that did not pass, kept for the transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvidence {
    pub item_id: String,
    pub verifier: String,
    pub outcome: String,
    pub detail: String,
}

/// The acceptance result of a turn. Only non-passing evidence is kept, so a
/// normal turn does not grow the session with test output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredVerification {
    pub passed: bool,
    #[serde(default)]
    pub evidence: Vec<StoredEvidence>,
}

/// Token usage summed over one turn's model calls. Absent in older sessions.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_input_tokens: u32,
}

/// Context pressure at the end of a turn. New turns keep this instead of the
/// whole event stream; older documents recover it from their events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredContext {
    pub used: u32,
    pub ceiling: u32,
}

/// One finished turn as the session file keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTurn {
    pub seq: u32,
    pub input: String,
    pub state: String,
    pub answer: String,
    /// Model thinking for this turn, bounded by the host. Absent in sessions
    /// written before the field existed.
    #[serde(default)]
    pub thinking: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    /// Input, output and cached input tokens for the whole turn.
    #[serde(default)]
    pub usage: StoredUsage,
    /// Model calls spent in this turn. Older documents recover it from their
    /// `model_finished` events.
    #[serde(default)]
    pub model_calls: u32,
    /// Acceptance result when the host ran a checklist for this turn.
    #[serde(default)]
    pub verification_passed: Option<bool>,
    /// Why the check did not pass. Absent in older sessions.
    #[serde(default)]
    pub verification: Option<StoredVerification>,
    /// Live tool rows. Absent in sessions written before the field existed.
    #[serde(default)]
    pub tools: Vec<ToolLine>,
    /// Context pressure at the end of the turn. New turns write this and no
    /// events; older documents carry events and recover the point from them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<StoredContext>,
    /// Events in emission order. Absent in sessions written before the field
    /// existed, and never written by new turns: the transcript and the status
    /// line read the fields above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<StoredEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

/// A session document at one path.
///
/// The path is the header: `s-<id>.json` holds the workspace, the rig, the
/// title, the archive flag, the kernel snapshot and the todo list. Each turn is
/// its own file under `s-<id>.turns/`, named by sequence number.
///
/// A turn row is written once and never rewritten. That is what keeps a save
/// proportional to the new turn instead of the whole conversation: before this
/// layout a 10.5 MB document was rewritten at every turn, so bytes written grew
/// with the square of the turn count (ADR 0057).
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where the turn rows live. Derived from the header path so every caller
    /// that already knows the session path keeps working.
    pub fn turns_dir(&self) -> PathBuf {
        self.path.with_extension("turns")
    }

    /// Loads the session, or `None` when the file does not exist yet.
    ///
    /// A document written before the turn files existed still carries its rows
    /// inline; they are read as they are and, from the next save on, written
    /// out one file per turn.
    pub fn load(&self, workspace: &Path) -> Result<Option<StoredSession>, KernelError> {
        let Some(session) = self.load_unchecked()? else {
            return Ok(None);
        };
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

    /// Loads the document without checking which workspace it belongs to.
    ///
    /// For a caller that already knows the path belongs to the project — the
    /// desktop index derives the directory from the workspace hash — and needs
    /// the rows that the header no longer carries.
    pub fn load_unchecked(&self) -> Result<Option<StoredSession>, KernelError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage_error(&self.path, error)),
        };
        let mut session: StoredSession =
            serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                message: format!(
                    "session `{}` is not valid JSON: {error}",
                    self.path.display()
                ),
            })?;
        self.read_turns(&mut session.turns)?;
        Ok(Some(session))
    }

    /// Writes the header and any turn row that does not have a file yet.
    ///
    /// A crash between the two leaves a conversation that is missing its last
    /// row, not one with a half-written row: every file is written through a
    /// temporary name and renamed.
    pub fn save(&self, session: &StoredSession) -> Result<(), KernelError> {
        self.save_with(session, &[])
    }

    /// Writes the header, any turn row that does not have a file yet, and the
    /// rows named in `rewrite`.
    ///
    /// A row is written once. `rewrite` exists for the one migration that
    /// changes an old row in place — folding a stored event stream into the
    /// fields — so it happens once per document instead of every save, which
    /// would read and write the whole conversation again.
    pub fn save_with(&self, session: &StoredSession, rewrite: &[u32]) -> Result<(), KernelError> {
        let dir = self.turns_dir();
        fs::create_dir_all(&dir).map_err(|error| storage_error(&dir, error))?;
        for turn in &session.turns {
            let path = dir.join(turn_file_name(turn.seq));
            if path.exists() && !rewrite.contains(&turn.seq) {
                continue;
            }
            let text = serde_json::to_string(turn).map_err(|error| KernelError::Internal {
                message: format!("cannot encode turn {}: {error}", turn.seq),
            })?;
            write_atomic(&path, &text)?;
        }
        let header = SessionHeader {
            workspace: &session.workspace,
            rig: &session.rig,
            title: &session.title,
            archived: session.archived,
            snapshot: &session.snapshot,
            todos: session.todos.as_ref(),
        };
        let text =
            serde_json::to_string_pretty(&header).map_err(|error| KernelError::Internal {
                message: format!("cannot encode session `{}`: {error}", self.path.display()),
            })?;
        write_atomic(&self.path, &text)
    }

    /// Removes the header and every turn row it owns.
    pub fn delete(&self) -> Result<(), KernelError> {
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage_error(&self.path, error)),
        }
        match fs::remove_dir_all(self.turns_dir()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(storage_error(&self.turns_dir(), error)),
        }
    }

    /// Appends the turn rows on disk to `turns`, keeping the document's own
    /// rows when both exist (a document written before the split).
    fn read_turns(&self, turns: &mut Vec<StoredTurn>) -> Result<(), KernelError> {
        let dir = self.turns_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(storage_error(&dir, error)),
        };
        let mut rows: Vec<(u32, PathBuf)> = Vec::new();
        for entry in entries {
            let path = entry.map_err(|error| storage_error(&dir, error))?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let seq = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<u32>().ok());
            let Some(seq) = seq else { continue };
            if turns.iter().any(|turn| turn.seq == seq) {
                continue;
            }
            rows.push((seq, path));
        }
        rows.sort_by_key(|(seq, _)| *seq);
        for (_, path) in rows {
            let text = fs::read_to_string(&path).map_err(|error| storage_error(&path, error))?;
            let turn: StoredTurn =
                serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                    message: format!("turn `{}` is not valid JSON: {error}", path.display()),
                })?;
            turns.push(turn);
        }
        turns.sort_by_key(|turn| turn.seq);
        Ok(())
    }
}

/// What the session document keeps once each turn has its own file:
/// everything the list and the resume path need, and nothing that grows with
/// the length of the conversation.
#[derive(serde::Serialize)]
struct SessionHeader<'a> {
    workspace: &'a Path,
    rig: &'a str,
    title: &'a str,
    archived: bool,
    snapshot: &'a SessionSnapshot,
    todos: Option<&'a TodoList>,
}

fn turn_file_name(seq: u32) -> String {
    format!("{seq:08}.json")
}

fn write_atomic(path: &Path, text: &str) -> Result<(), KernelError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| storage_error(path, error))?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text).map_err(|error| storage_error(&temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| storage_error(path, error))
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
            rig: "test".to_owned(),
            title: String::new(),
            archived: false,
            snapshot: SessionSnapshot::default(),
            todos: Some(TodoList::default()),
            turns: vec![StoredTurn {
                seq: 1,
                input: "hello".to_owned(),
                state: "Done".to_owned(),
                answer: "hi".to_owned(),
                thinking: String::new(),
                error: None,
                duration_ms: 3,
                usage: StoredUsage::default(),
                model_calls: 0,
                verification_passed: None,
                verification: None,
                tools: Vec::new(),
                context: None,
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
    fn a_named_row_is_written_again_and_no_other_is() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        store.save(&session(workspace.clone())).expect("save");
        let row = store.turns_dir().join("00000001.json");
        let before = fs::read(&row).expect("the row");

        // An ordinary save leaves an existing row alone even when the caller's
        // copy differs: rows are written once.
        let mut changed = session(workspace.clone());
        changed.turns[0].answer = "rewritten".to_owned();
        store.save(&changed).expect("save");
        assert_eq!(fs::read(&row).expect("the row"), before);

        // The event migration is the one caller that names a row to replace.
        store.save_with(&changed, &[1]).expect("rewrite");

        assert_ne!(fs::read(&row).expect("the row"), before);
        let loaded = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(loaded.turns[0].answer, "rewritten");
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

    #[test]
    fn the_header_carries_no_turns_and_each_row_is_one_file() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));

        store.save(&session(workspace.clone())).expect("save");

        let header = fs::read_to_string(&store.path).expect("the header");
        assert!(!header.contains("\"turns\""), "{header}");
        assert!(header.contains("\"snapshot\""), "{header}");
        let row = store.turns_dir().join("00000001.json");
        assert!(row.is_file(), "{}", row.display());

        // The second save writes only the row it has not seen; the first file
        // is not touched.
        let before = fs::read(&row).expect("the row");
        let mut grown = session(workspace.clone());
        let second = StoredTurn {
            seq: 2,
            ..grown.turns[0].clone()
        };
        grown.turns.push(second);
        store.save(&grown).expect("save again");

        assert_eq!(fs::read(&row).expect("the row"), before);
        assert!(store.turns_dir().join("00000002.json").is_file());
        let loaded = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(loaded.turns.len(), 2);
        assert_eq!(loaded.turns[1].seq, 2);
    }

    #[test]
    fn a_document_that_still_carries_its_turns_migrates_one_file_each() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        fs::create_dir_all(store.path.parent().expect("a parent")).expect("the sessions dir");

        // What a document written before the split looks like: one file, all
        // rows inside it.
        let legacy = serde_json::to_string_pretty(&session(workspace.clone())).expect("encode");
        assert!(legacy.contains("\"turns\""));
        fs::write(&store.path, legacy).expect("write the legacy document");

        let loaded = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(loaded.turns.len(), 1);

        store.save(&loaded).expect("save");

        assert!(store.turns_dir().join("00000001.json").is_file());
        let header = fs::read_to_string(&store.path).expect("the header");
        assert!(!header.contains("\"turns\""), "{header}");
        // The row is not lost on the way out, and it is not written twice.
        let again = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(again.turns.len(), 1);
        assert_eq!(again.turns[0].answer, "hi");
    }

    #[test]
    fn a_temporary_file_left_by_a_crash_is_ignored() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        store.save(&session(workspace.clone())).expect("save");
        fs::write(store.turns_dir().join("00000002.json.tmp"), "{ half")
            .expect("a leftover temporary file");

        let loaded = store.load(&workspace).expect("load").expect("a session");

        assert_eq!(loaded.turns.len(), 1);
    }

    #[test]
    fn deleting_a_session_takes_every_turn_file_with_it() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        store.save(&session(workspace.clone())).expect("save");
        assert!(store.turns_dir().is_dir());

        store.delete().expect("delete");

        assert!(!store.path.exists());
        assert!(!store.turns_dir().exists());
        assert!(store.load(&workspace).expect("load").is_none());
        // Deleting what is already gone is not an error.
        store.delete().expect("delete again");
    }
}
