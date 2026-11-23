//! Where a turn leaves its records (ADR 0063).
//!
//! Two artifacts, two jobs:
//!
//! * the step log is a database — one row per kernel event, queryable by
//!   session, turn and time. It answers "what did the agent actually do",
//!   including the two previews the persisted event drops (`args_preview`,
//!   `output_preview`). It is high-volume, append-only and never read by a
//!   human in an editor, which is what a database is for.
//! * the progress documents are files — one per task, plus a rebuildable
//!   index of them. They are read by a person and by the next agent, and may be
//!   committed, which is what a document is for.
//!
//! Both hang off one base directory, chosen per workspace, so a checkout can
//! carry its own history or keep it out of the tree entirely.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use keel_core::Event;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::sessions;

/// The step log's file name inside a base directory.
pub const LOG_FILE: &str = "journal.db";

/// The directory the task documents go in, inside a base directory.
pub const PROGRESS_DIR: &str = "progress";

/// The index inside that directory.
pub const PROGRESS_INDEX: &str = "INDEX.md";

/// Where the records live.
///
/// `project` follows the repository, so a checkout carries its own history.
/// `state` keeps it out of the tree. `custom` is a path the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Project,
    State,
    Custom,
}

impl Mode {
    /// An unknown value falls back to the default rather than failing to open
    /// a conversation: a typo in a hand-edited config must not cost the user
    /// their session.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "state" => Mode::State,
            "custom" => Mode::Custom,
            _ => Mode::Project,
        }
    }
}

/// The directory every turn of this workspace writes under, or `None` when the
/// user chose `custom` without naming a usable path.
pub fn base(mode: Mode, custom: &str, workspace: &Path, state_dir: &Path) -> Option<PathBuf> {
    match mode {
        Mode::Project => Some(workspace.join(".keelson").join("records")),
        Mode::State => Some(
            state_dir
                .join("records")
                .join(sessions::workspace_key(workspace)),
        ),
        Mode::Custom => {
            let path = custom.trim();
            if path.is_empty() {
                return None;
            }
            // The project key keeps two projects pointed at one directory from
            // writing turns of the same number over each other.
            Some(PathBuf::from(path).join(sessions::workspace_key(workspace)))
        }
    }
}

/// The progress directory under one base.
pub fn progress_dir(base: &Path) -> PathBuf {
    base.join(PROGRESS_DIR)
}

/// The step log's path and the task index's path under one base.
pub fn paths(base: &Path) -> (PathBuf, PathBuf) {
    (base.join(LOG_FILE), progress_dir(base).join(PROGRESS_INDEX))
}

/// Keeps the log out of `git status` without hiding the documents.
///
/// `.keelson/` is not ours alone: `checks.toml` lives there and belongs to the
/// project, and a project may well track it. The step log is noise to a
/// reader — a binary file that changes on every turn — while the task
/// documents are project history the user may want to commit, so only the
/// database and its WAL sidecars are ignored. The rule is appended to an
/// existing file rather than replacing it.
pub fn ignore_in_project(workspace: &Path) -> io::Result<()> {
    const RULES: [&str; 3] = [
        "records/journal.db",
        "records/journal.db-wal",
        "records/journal.db-shm",
    ];
    let dir = workspace.join(".keelson");
    fs::create_dir_all(&dir)?;
    let path = dir.join(".gitignore");
    let mut text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => ".gitignore\n".to_owned(),
    };
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let mut added = false;
    for rule in RULES {
        if text.lines().any(|line| line.trim() == rule) {
            continue;
        }
        text.push_str(rule);
        text.push('\n');
        added = true;
    }
    if !added && path.exists() {
        return Ok(());
    }
    fs::write(&path, text)
}

/// The header of a turn's row in `turns`. Written once, before the turn runs.
#[derive(Debug, Clone)]
pub struct TurnMeta {
    pub session: String,
    pub turn: u32,
    pub model: String,
    pub provider: String,
    pub rig: String,
    pub workspace: String,
    pub input: String,
}

/// The ending of a turn's row. Written after the kernel returns.
#[derive(Debug, Clone)]
pub struct TurnEnd {
    pub ok: bool,
    pub error: Option<String>,
    pub state: Option<String>,
    pub model_calls: Option<u64>,
    pub tools: Option<usize>,
    pub duration_ms: Option<u64>,
}

/// One turn's step log, open for the length of the turn.
///
/// Every write commits on its own: a turn that is killed mid-flight must not
/// lose the steps it already took, which is the whole reason the log exists.
pub struct StepLog {
    conn: Connection,
    session: String,
    turn: u32,
}

impl StepLog {
    /// Opens `base/journal.db`, creating it and its schema if needed.
    pub fn open(base: &Path, session: &str, turn: u32) -> rusqlite::Result<Self> {
        fs::create_dir_all(base).map_err(|error| {
            rusqlite::Error::InvalidPath(format!("{}: {error}", base.display()).into())
        })?;
        let path = base.join(LOG_FILE);
        let conn = Connection::open(&path)?;
        // WAL keeps a reader (an open `sqlite3` shell, a later telemetry
        // export) from blocking the turn's appends.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS turns (
                 session    TEXT    NOT NULL,
                 turn       INTEGER NOT NULL,
                 started_ms INTEGER NOT NULL,
                 ended_ms   INTEGER,
                 ok         INTEGER,
                 error      TEXT,
                 state      TEXT,
                 model      TEXT,
                 provider   TEXT,
                 rig        TEXT,
                 workspace  TEXT,
                 input      TEXT,
                 model_calls INTEGER,
                 tools      INTEGER,
                 duration_ms INTEGER,
                 PRIMARY KEY (session, turn)
             );
             CREATE TABLE IF NOT EXISTS steps (
                 session TEXT    NOT NULL,
                 turn    INTEGER NOT NULL,
                 step    INTEGER NOT NULL,
                 at_ms   INTEGER NOT NULL,
                 kind    TEXT    NOT NULL,
                 data    TEXT    NOT NULL
             );
             CREATE INDEX IF NOT EXISTS steps_of_turn ON steps (session, turn, step);
             CREATE INDEX IF NOT EXISTS steps_by_time ON steps (at_ms);",
        )?;
        Ok(Self {
            conn,
            session: session.to_owned(),
            turn,
        })
    }

    /// The turn's header row. A turn that fails before the kernel starts still
    /// has a row, so the log never begins with steps that belong to nothing.
    pub fn meta(&self, meta: &TurnMeta) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO turns
                 (session, turn, started_ms, model, provider, rig, workspace, input)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(session, turn) DO UPDATE SET
                 started_ms = excluded.started_ms,
                 model      = excluded.model,
                 provider   = excluded.provider,
                 rig        = excluded.rig,
                 workspace  = excluded.workspace,
                 input      = excluded.input",
            params![
                meta.session,
                i64::from(meta.turn),
                sessions::now_ms() as i64,
                meta.model,
                meta.provider,
                meta.rig,
                meta.workspace,
                meta.input,
            ],
        )?;
        Ok(())
    }

    /// One kernel event, with the kernel's own sequence number so a reader can
    /// line this row up with the event log the host polled.
    pub fn step(&self, seq: u64, event: &Event) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO steps (session, turn, step, at_ms, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                self.session,
                i64::from(self.turn),
                seq as i64,
                sessions::now_ms() as i64,
                event.name(),
                payload(event).to_string(),
            ],
        )?;
        Ok(())
    }

    /// Fills in the ending. `ok` is the gate: everything else is optional and
    /// stays `NULL` when the kernel did not report it.
    pub fn end(&self, end: &TurnEnd) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE turns SET
                 ended_ms    = ?3,
                 ok          = ?4,
                 error       = ?5,
                 state       = ?6,
                 model_calls = ?7,
                 tools       = ?8,
                 duration_ms = ?9
             WHERE session = ?1 AND turn = ?2",
            params![
                self.session,
                i64::from(self.turn),
                sessions::now_ms() as i64,
                end.ok,
                end.error,
                end.state,
                end.model_calls.map(|calls| calls as i64),
                end.tools.map(|tools| tools as i64),
                end.duration_ms.map(|ms| ms as i64),
            ],
        )?;
        Ok(())
    }
}

/// The event body without its single-key wrapper, plus the previews the kernel
/// skips when it serializes an event for the store.
///
/// `args` and `output` are bounded and already redacted by the dispatcher; they
/// are the two fields a reader needs to understand what a step actually did, so
/// they belong in the log even though the persisted event omits them.
fn payload(event: &Event) -> Value {
    let mut value = serde_json::to_value(event).unwrap_or(Value::Null);
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    let Some(kind) = object.keys().next().cloned() else {
        return value;
    };
    let mut body = object.remove(&kind).unwrap_or(Value::Null);
    let Some(fields) = body.as_object_mut() else {
        return body;
    };
    match event {
        Event::ToolStarted { args_preview, .. } => {
            fields.insert("args".to_owned(), json!(args_preview));
        }
        Event::ToolFinished { output_preview, .. } => {
            fields.insert("output".to_owned(), json!(output_preview));
        }
        _ => {}
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::{RiskLevel, ToolCallId, TurnId};

    #[test]
    fn the_project_mode_follows_the_repository() {
        let workspace = Path::new("/work/app");
        let state = Path::new("/state");

        assert_eq!(
            base(Mode::Project, "", workspace, state),
            Some(PathBuf::from("/work/app/.keelson/records"))
        );
        // The state and custom modes are per project, so two workspaces can
        // share one root without writing over each other.
        let key = sessions::workspace_key(workspace);
        assert_eq!(
            base(Mode::State, "", workspace, state),
            Some(Path::new("/state/records").join(&key))
        );
        assert_eq!(
            base(Mode::Custom, "/media/archive", workspace, state),
            Some(Path::new("/media/archive").join(&key))
        );
        // An empty path is a configuration mistake, not a place to write.
        assert_eq!(base(Mode::Custom, "   ", workspace, state), None);
    }

    #[test]
    fn an_unknown_mode_falls_back_to_the_project() {
        assert_eq!(Mode::parse(""), Mode::Project);
        assert_eq!(Mode::parse("Project"), Mode::Project);
        assert_eq!(Mode::parse("state"), Mode::State);
        assert_eq!(Mode::parse("elsewhere"), Mode::Project);
    }

    #[test]
    fn the_log_keeps_the_arguments_and_the_output() {
        let dir = tempfile::tempdir().unwrap();
        let log = StepLog::open(dir.path(), "s-1", 7).unwrap();
        assert!(dir.path().join(LOG_FILE).exists());

        let call = ToolCallId::new(3);
        log.step(
            1,
            &Event::ToolStarted {
                turn_id: TurnId::new(7),
                call_id: call,
                tool: "shell".to_owned(),
                args_preview: Some("cargo test".to_owned()),
                risk: RiskLevel::ReadOnly,
            },
        )
        .unwrap();
        log.step(
            2,
            &Event::ToolFinished {
                turn_id: TurnId::new(7),
                call_id: call,
                tool: "shell".to_owned(),
                ok: true,
                error: None,
                duration_ms: 12,
                output_preview: Some("ok. 3 passed".to_owned()),
                reused: true,
            },
        )
        .unwrap();

        let conn = Connection::open(dir.path().join(LOG_FILE)).unwrap();
        let (kind, data): (String, String) = conn
            .query_row("SELECT kind, data FROM steps WHERE step = 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(kind, "tool_started");
        assert!(data.contains("cargo test"));

        let count: i64 = conn
            .query_row("SELECT count(*) FROM steps", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        // The reuse flag is a fact about the turn, so the step record keeps it.
        let (_, finished): (String, String) = conn
            .query_row("SELECT kind, data FROM steps WHERE step = 2", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert!(finished.contains("\"reused\":true"));
    }

    #[test]
    fn a_turn_row_carries_its_header_and_its_ending() {
        let dir = tempfile::tempdir().unwrap();
        let log = StepLog::open(dir.path(), "s-1", 2).unwrap();
        log.meta(&TurnMeta {
            session: "s-1".to_owned(),
            turn: 2,
            model: "m".to_owned(),
            provider: "p".to_owned(),
            rig: "coding".to_owned(),
            workspace: "/work/app".to_owned(),
            input: "fix the button".to_owned(),
        })
        .unwrap();
        log.end(&TurnEnd {
            ok: true,
            error: None,
            state: Some("Done".to_owned()),
            model_calls: Some(4),
            tools: Some(9),
            duration_ms: Some(1500),
        })
        .unwrap();

        let conn = Connection::open(dir.path().join(LOG_FILE)).unwrap();
        let (ok, calls, tools, input): (bool, i64, i64, String) = conn
            .query_row(
                "SELECT ok, model_calls, tools, input FROM turns",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert!(ok);
        assert_eq!((calls, tools), (4, 9));
        assert_eq!(input, "fix the button");

        // A re-run of the same turn replaces its row instead of failing.
        log.meta(&TurnMeta {
            session: "s-1".to_owned(),
            turn: 2,
            model: "m2".to_owned(),
            provider: "p".to_owned(),
            rig: "coding".to_owned(),
            workspace: "/work/app".to_owned(),
            input: "again".to_owned(),
        })
        .unwrap();
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM turns", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn a_project_log_hides_itself_from_git_but_the_document_stays() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".keelson/.gitignore");

        ignore_in_project(dir.path()).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("records/journal.db\n"));
        // The task documents are the user's, and a project may commit them.
        assert!(!text.contains(PROGRESS_DIR));

        // The project's own rules survive, and a second call adds nothing.
        fs::write(&file, "*.log\n").unwrap();
        ignore_in_project(dir.path()).unwrap();
        ignore_in_project(dir.path()).unwrap();
        let text = fs::read_to_string(&file).unwrap();

        assert_eq!(
            text,
            "*.log\nrecords/journal.db\nrecords/journal.db-wal\nrecords/journal.db-shm\n"
        );
    }
}
