//! Per-project conversations.
//!
//! A project owns one directory under `<state>/sessions/<workspace-hash>/`
//! with one JSON document per conversation and an `index.json` for listing.
//! The index is a cache: it is reconciled against the directory, so a missing
//! or stale index cannot hide a document or invent one.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use keelson::engine::{Header, Store, StoredSession};
use serde::{Deserialize, Serialize};

const INDEX_FILE: &str = "index.json";
const INDEX_VERSION: u32 = 1;
const TITLE_CHARS: usize = 48;

/// One conversation row in a project list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub rig: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub turns: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionIndex {
    version: u32,
    sessions: Vec<SessionMeta>,
}

/// A stable, filesystem-safe key for a workspace, so sessions from different
/// directories cannot collide or refuse each other.
pub fn workspace_key(path: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn project_dir(state_dir: &Path, workspace: &Path) -> PathBuf {
    state_dir.join("sessions").join(workspace_key(workspace))
}

pub fn session_path(state_dir: &Path, workspace: &Path, id: &str) -> PathBuf {
    project_dir(state_dir, workspace).join(format!("{id}.json"))
}

/// A conversation id is a single filename segment. It is generated here and
/// never derived from user input.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// Conversations, most recently updated first.
pub fn list(state_dir: &Path, workspace: &Path) -> Vec<SessionMeta> {
    let mut index = load_index(&project_dir(state_dir, workspace));
    sort(&mut index.sessions);
    index.sessions
}

/// Writes an empty conversation document and its row.
///
/// The app no longer calls this: pressing 新对话 opens a blank conversation,
/// and the first turn writes the document. It stays as a fixture builder for
/// the tests below, which exercise the index against a real file.
#[cfg(test)]
pub fn create(state_dir: &Path, workspace: &Path, rig: &str) -> Result<SessionMeta, String> {
    let dir = project_dir(state_dir, workspace);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create `{}`: {error}", dir.display()))?;

    // Load the index before the document exists; otherwise the directory
    // scan would adopt the new document and the push below would duplicate it.
    let mut index = load_index(&dir);

    let id = fresh_id();
    let path = dir.join(format!("{id}.json"));
    Store::open(path)
        .save(&StoredSession {
            workspace: workspace.to_path_buf(),
            rig: rig.to_owned(),
            title: String::new(),
            archived: false,
            snapshot: Default::default(),
            todos: None,
            turns: Vec::new(),
        })
        .map_err(|error| error.to_string())?;

    let now = now_ms();
    let meta = SessionMeta {
        id,
        title: String::new(),
        archived: false,
        rig: rig.to_owned(),
        created_at: now,
        updated_at: now,
        turns: 0,
    };
    index.sessions.push(meta.clone());
    save_index(&dir, &index)?;
    Ok(meta)
}

/// Refreshes one row after a turn and returns it. `None` means the document
/// disappeared; the caller keeps the turn result either way.
pub fn touch(state_dir: &Path, workspace: &Path, id: &str) -> Option<SessionMeta> {
    let dir = project_dir(state_dir, workspace);
    let path = dir.join(format!("{id}.json"));
    let store = Store::open(path.clone());
    let document = store.read_header().ok().flatten()?;

    let now = now_ms();
    let mut index = load_index(&dir);
    let position = index.sessions.iter().position(|meta| meta.id == id);
    let meta = match position {
        Some(position) => &mut index.sessions[position],
        None => {
            index.sessions.push(SessionMeta {
                id: id.to_owned(),
                title: String::new(),
                archived: false,
                rig: String::new(),
                created_at: file_ms(&path).unwrap_or(now),
                updated_at: now,
                turns: 0,
            });
            index.sessions.last_mut().expect("a row was just pushed")
        }
    };
    meta.title = title_for(&store, &document);
    meta.archived = document.archived;
    meta.rig = document.rig.clone();
    meta.turns = turn_count(&store, &document);
    meta.updated_at = now;
    let meta = meta.clone();
    let _ = save_index(&dir, &index);
    Some(meta)
}

/// Deletes one conversation document and its row.
pub fn remove(state_dir: &Path, workspace: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("会话 id 不合法".to_owned());
    }
    let dir = project_dir(state_dir, workspace);
    let store = Store::open(dir.join(format!("{id}.json")));
    store.delete().map_err(|error| error.to_string())?;
    // The note belongs to the conversation as much as the document does: a turn
    // that was killed mid-answer leaves one, and nothing would ever read it
    // again once the document is gone (ADR 0083).
    let _ = fs::remove_file(pending_path(&dir, id));
    let mut index = load_index(&dir);
    index.sessions.retain(|meta| meta.id != id);
    save_index(&dir, &index)
}

/// Renames a conversation. An empty title clears the custom name, so the
/// first user message becomes the title again.
pub fn set_title(
    state_dir: &Path,
    workspace: &Path,
    id: &str,
    title: &str,
) -> Result<SessionMeta, String> {
    let title = clean_title(title);
    update_document(state_dir, workspace, id, |document| {
        document.title = title;
    })?;
    sync(state_dir, workspace, id).ok_or_else(|| "对话不存在".to_owned())
}

/// The title a row can show: trimmed and bounded.
pub fn clean_title(title: &str) -> String {
    title.trim().chars().take(TITLE_CHARS).collect()
}

pub fn set_archived(
    state_dir: &Path,
    workspace: &Path,
    id: &str,
    archived: bool,
) -> Result<SessionMeta, String> {
    update_document(state_dir, workspace, id, |document| {
        document.archived = archived;
    })?;
    sync(state_dir, workspace, id).ok_or_else(|| "对话不存在".to_owned())
}

/// Whether a conversation is archived. Deleting is only allowed afterwards.
pub fn is_archived(state_dir: &Path, workspace: &Path, id: &str) -> Result<bool, String> {
    if !valid_id(id) {
        return Err("会话 id 不合法".to_owned());
    }
    let store = Store::open(session_path(state_dir, workspace, id));
    let document = store
        .load(workspace)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "对话不存在".to_owned())?;
    Ok(document.archived)
}

/// Refreshes one row from its document without moving it in the list.
pub fn sync(state_dir: &Path, workspace: &Path, id: &str) -> Option<SessionMeta> {
    if !valid_id(id) {
        return None;
    }
    let dir = project_dir(state_dir, workspace);
    let path = dir.join(format!("{id}.json"));
    let store = Store::open(path.clone());
    let document = store.read_header().ok().flatten()?;

    let mut index = load_index(&dir);
    let position = index.sessions.iter().position(|meta| meta.id == id);
    let meta = match position {
        Some(position) => &mut index.sessions[position],
        None => {
            let time = file_ms(&path).unwrap_or_else(now_ms);
            index.sessions.push(SessionMeta {
                id: id.to_owned(),
                title: String::new(),
                archived: false,
                rig: String::new(),
                created_at: time,
                updated_at: time,
                turns: 0,
            });
            index.sessions.last_mut().expect("a row was just pushed")
        }
    };
    meta.title = title_for(&store, &document);
    meta.archived = document.archived;
    meta.rig = document.rig.clone();
    meta.turns = turn_count(&store, &document);
    let meta = meta.clone();
    let _ = save_index(&dir, &index);
    Some(meta)
}

/// The question a conversation was asked and has not answered with a turn.
///
/// The engine writes a turn when it ends, so a turn the app did not survive left
/// nothing behind: the question survived only as the title of an otherwise empty
/// conversation, and asking it again meant retyping it. The host notes the
/// question here from the moment it is sent and drops the note when the turn
/// lands in the document. It is a note, not a queue: a half-run turn is never
/// replayed, because the tool calls it already made had side effects. Resuming
/// means asking again (ADR 0081).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingTurn {
    pub seq: u32,
    pub input: String,
    pub started_at: u64,
    /// The answer as far as it had streamed when the note was last written.
    ///
    /// A turn reaches the document only when it ends, so a process that died
    /// mid-answer left the question and nothing else. The note carries what had
    /// already been shown, bounded and rewritten at most a few times a second
    /// (ADR 0083). Empty while the first model call has said nothing yet, and
    /// absent in a note written before this field existed.
    #[serde(default)]
    pub partial: String,
}

fn pending_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.pending.json"))
}

/// Notes the question before the first model call, so a quit cannot lose it.
pub fn mark_pending(
    state_dir: &Path,
    workspace: &Path,
    id: &str,
    seq: u32,
    input: &str,
) -> Option<()> {
    if !valid_id(id) || input.trim().is_empty() {
        return None;
    }
    let dir = project_dir(state_dir, workspace);
    fs::create_dir_all(&dir).ok()?;
    let pending = PendingTurn {
        seq,
        input: input.to_owned(),
        started_at: now_ms(),
        partial: String::new(),
    };
    let text = serde_json::to_vec(&pending).ok()?;
    // Beside the document and moved into place: a reader never sees half a
    // question, and the name fails `valid_id`, so listing cannot mistake it for
    // a conversation of its own.
    let temporary = dir.join(format!("{id}.pending.json.tmp"));
    fs::write(&temporary, text).ok()?;
    fs::rename(&temporary, pending_path(&dir, id)).ok()?;
    Some(())
}

/// Drops the note: the turn it named is in the document now.
pub fn clear_pending(state_dir: &Path, workspace: &Path, id: &str) {
    if !valid_id(id) {
        return;
    }
    let _ = fs::remove_file(pending_path(&project_dir(state_dir, workspace), id));
}

/// Records how far the answer had got, so a restart shows the text as well as
/// the question (ADR 0083).
///
/// A read-modify-write of a small file: the other fields are the ones
/// `mark_pending` wrote, and this may be the last thing the process does. A note
/// that names another turn is left alone — a note belongs to exactly one turn,
/// and the tail of an older answer must never land on a newer question.
pub fn note_partial(
    state_dir: &Path,
    workspace: &Path,
    id: &str,
    seq: u32,
    partial: &str,
) -> Option<()> {
    if !valid_id(id) {
        return None;
    }
    let dir = project_dir(state_dir, workspace);
    let path = pending_path(&dir, id);
    let mut note: PendingTurn = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
    // The kept text is a head that only grows, so a buffer no longer than what
    // the note already holds is an older copy of it: a slower writer must not
    // put the shorter text back (ADR 0083).
    if note.seq != seq || note.partial.len() >= partial.len() {
        return None;
    }
    note.partial = partial.to_owned();
    let text = serde_json::to_vec(&note).ok()?;
    let temporary = dir.join(format!("{id}.pending.json.tmp"));
    fs::write(&temporary, text).ok()?;
    fs::rename(&temporary, path).ok()?;
    Some(())
}

/// The question a conversation was left asking, if no turn answers it.
pub fn pending(state_dir: &Path, workspace: &Path, id: &str) -> Option<PendingTurn> {
    if !valid_id(id) {
        return None;
    }
    let text = fs::read_to_string(pending_path(&project_dir(state_dir, workspace), id)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Makes a conversation part of the project's history when its first message is
/// sent, instead of when the turn that message started ends.
///
/// A blank conversation still writes nothing: pressing 新对话 and leaving must
/// not litter the list. A sent message is the difference — the document is
/// created named after what was asked, with no turns yet, and `is_listed` keeps
/// a named empty document visible for the same reason a branch relies on it. A
/// first turn that ran for a minute used to leave the conversation out of the
/// history for that whole minute, and a first turn that failed left no trace of
/// what was asked.
pub fn begin(
    state_dir: &Path,
    workspace: &Path,
    id: &str,
    rig: &str,
    input: &str,
) -> Option<SessionMeta> {
    if !valid_id(id) {
        return None;
    }
    let dir = project_dir(state_dir, workspace);
    let path = dir.join(format!("{id}.json"));
    let store = Store::open(path);
    if store.read_header().ok().flatten().is_none() {
        let title = clean_title(input);
        if title.is_empty() {
            return None;
        }
        fs::create_dir_all(&dir).ok()?;
        store
            .save(&StoredSession {
                workspace: workspace.to_path_buf(),
                rig: rig.to_owned(),
                title,
                archived: false,
                snapshot: Default::default(),
                todos: None,
                turns: Vec::new(),
            })
            .ok()?;
    }
    // Whether this call made the document or found one, the row reports what is
    // on disk: a conversation that already has turns keeps its count.
    sync(state_dir, workspace, id)
}

fn update_document(
    state_dir: &Path,
    workspace: &Path,
    id: &str,
    change: impl FnOnce(&mut StoredSession),
) -> Result<(), String> {
    if !valid_id(id) {
        return Err("会话 id 不合法".to_owned());
    }
    let store = Store::open(session_path(state_dir, workspace, id));
    let mut document = store
        .load(workspace)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "对话不存在".to_owned())?;
    change(&mut document);
    store.save(&document).map_err(|error| error.to_string())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// A fresh conversation id. Opening a project with no conversations uses one
/// without writing anything; the first persisted turn gives it a document.
pub fn fresh_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("s-{nanos:x}")
}

fn title_from(input: &str) -> String {
    let line = input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    line.chars().take(TITLE_CHARS).collect()
}

/// A custom title wins; otherwise the first user message is the title. The
/// first row is one file, not the conversation: a list row must not read the
/// whole history to write one string.
fn title_for(store: &Store, document: &Header) -> String {
    if !document.title.trim().is_empty() {
        return document.title.trim().to_owned();
    }
    store
        .first_turn(document)
        .ok()
        .flatten()
        .map(|turn| title_from(&turn.input))
        .unwrap_or_default()
}

/// How many rows a document has. Counted from the row file names, so listing a
/// project reads headers rather than conversations. A document written before
/// the split still carries its rows inline, and those count too.
fn turn_count(store: &Store, document: &Header) -> u32 {
    store
        .turn_count(document)
        .unwrap_or(document.turns.len() as u32)
}

fn sort(sessions: &mut [SessionMeta]) {
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then(right.created_at.cmp(&left.created_at))
            .then(right.id.cmp(&left.id))
    });
}

fn load_index(dir: &Path) -> SessionIndex {
    let mut index = read_index(dir).unwrap_or_else(|| rebuild(dir));
    if reconcile(dir, &mut index) {
        let _ = save_index(dir, &index);
    }
    // A header removed without its sidecars leaves the largest files in the
    // project behind and no row that would ever remove them; listing is the one
    // moment they are found. The sweep moves them aside, never deletes them.
    let _ = Store::sweep_orphans(dir);
    index
}

fn read_index(dir: &Path) -> Option<SessionIndex> {
    let text = fs::read_to_string(dir.join(INDEX_FILE)).ok()?;
    let index: SessionIndex = serde_json::from_str(&text).ok()?;
    (index.version == INDEX_VERSION).then_some(index)
}

fn rebuild(dir: &Path) -> SessionIndex {
    let mut sessions = Vec::new();
    for (id, path) in documents(dir) {
        let meta = meta_from_file(&path, &id);
        if is_listed(&meta) {
            sessions.push(meta);
        }
    }
    SessionIndex {
        version: INDEX_VERSION,
        sessions,
    }
}

/// Whether a row belongs in the list. A document with no turns and no name is
/// not a conversation: pressing 新对话 and leaving used to write one, and the
/// row then sat in the history forever. A named empty document is different —
/// a branch starts with no turns and a name — so it stays visible from the
/// moment it exists.
fn is_listed(meta: &SessionMeta) -> bool {
    meta.turns > 0 || !meta.title.trim().is_empty()
}

/// Drops rows whose document is gone, adopts documents the index does not
/// know about, and collapses duplicate rows. A crashed write or a manually
/// copied file stays visible.
fn reconcile(dir: &Path, index: &mut SessionIndex) -> bool {
    let present: HashSet<String> = documents(dir).into_iter().map(|(id, _)| id).collect();
    let mut changed = false;
    let before = index.sessions.len();
    index
        .sessions
        .retain(|meta| present.contains(&meta.id) && is_listed(meta));
    changed |= index.sessions.len() != before;

    let known: HashSet<String> = index.sessions.iter().map(|meta| meta.id.clone()).collect();
    let added: Vec<SessionMeta> = present
        .into_iter()
        .filter(|id| !known.contains(id))
        .map(|id| meta_from_file(&dir.join(format!("{id}.json")), &id))
        .filter(is_listed)
        .collect();
    changed |= !added.is_empty();
    index.sessions.extend(added);

    // The newest row wins, so an older duplicate cannot hide a title or a
    // turn count written after it.
    index
        .sessions
        .sort_by_key(|meta| std::cmp::Reverse(meta.updated_at));
    let before_dedup = index.sessions.len();
    let mut seen = HashSet::new();
    index.sessions.retain(|meta| seen.insert(meta.id.clone()));
    changed |= index.sessions.len() != before_dedup;

    changed
}

fn documents(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut documents = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return documents;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if id == "index" || !valid_id(id) {
            continue;
        }
        documents.push((id.to_owned(), path));
    }
    documents
}

fn meta_from_file(path: &Path, id: &str) -> SessionMeta {
    let store = Store::open(path.to_path_buf());
    let document = store.read_header().ok().flatten();
    let time = file_ms(path).unwrap_or_else(now_ms);
    let (title, archived, rig, turns) = document
        .as_ref()
        .map(|document| {
            (
                title_for(&store, document),
                document.archived,
                document.rig.clone(),
                turn_count(&store, document),
            )
        })
        .unwrap_or_default();
    SessionMeta {
        id: id.to_owned(),
        title,
        archived,
        rig,
        created_at: time,
        updated_at: time,
        turns,
    }
}

fn file_ms(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn save_index(dir: &Path, index: &SessionIndex) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("cannot create `{}`: {error}", dir.display()))?;
    let text = serde_json::to_string_pretty(index).map_err(|error| error.to_string())?;
    let path = dir.join(INDEX_FILE);
    let temporary = dir.join("index.json.tmp");
    fs::write(&temporary, text)
        .map_err(|error| format!("cannot write `{}`: {error}", temporary.display()))?;
    fs::rename(&temporary, &path)
        .map_err(|error| format!("cannot replace `{}`: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::SessionSnapshot;

    fn workspace(dir: &tempfile::TempDir) -> PathBuf {
        let path = dir.path().join("workspace");
        fs::create_dir_all(&path).expect("a workspace");
        fs::canonicalize(path).expect("a canonical workspace")
    }

    fn document(workspace: &Path, first: &str, turns: u32) -> StoredSession {
        StoredSession {
            workspace: workspace.to_path_buf(),
            rig: "coding".to_owned(),
            title: String::new(),
            archived: false,
            snapshot: SessionSnapshot::default(),
            todos: None,
            turns: (1..=turns)
                .map(|seq| keelson::engine::StoredTurn {
                    seq,
                    input: if seq == 1 {
                        first.to_owned()
                    } else {
                        format!("turn {seq}")
                    },
                    state: "Done".to_owned(),
                    answer: "ok".to_owned(),
                    thinking: String::new(),
                    error: None,
                    duration_ms: 1,
                    usage: keelson::engine::StoredUsage::default(),
                    model_calls: 0,
                    verification_passed: None,
                    verification: None,
                    tools: Vec::new(),
                    context: None,
                    events: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn workspace_keys_are_stable_and_distinct() {
        let first = workspace_key(Path::new("C:/work/one"));
        assert_eq!(first, workspace_key(Path::new("C:/work/one")));
        assert_ne!(first, workspace_key(Path::new("C:/work/two")));
        assert_eq!(first.len(), 16);
    }

    #[test]
    fn a_document_with_no_turns_is_not_listed() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");

        let meta = create(&state, &workspace, "coding").expect("create");
        assert!(valid_id(&meta.id));
        assert!(session_path(&state, &workspace, &meta.id).is_file());
        // No turn yet: the document exists but the history stays empty.
        assert!(list(&state, &workspace).is_empty());

        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 1))
            .expect("save");
        let rows = list(&state, &workspace);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, meta.id);
        assert_eq!(rows[0].turns, 1);
    }

    #[test]
    fn the_old_default_document_is_adopted() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let project = project_dir(&state, &workspace);
        fs::create_dir_all(&project).expect("a project dir");
        Store::open(project.join("default.json"))
            .save(&document(&workspace, "first question", 2))
            .expect("save");

        let rows = list(&state, &workspace);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "default");
        assert_eq!(rows[0].title, "first question");
        assert_eq!(rows[0].turns, 2);
    }

    #[test]
    fn touching_a_conversation_sets_its_title_and_count() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "  a title  ", 3))
            .expect("save");

        let touched = touch(&state, &workspace, &meta.id).expect("touch");

        assert_eq!(touched.title, "a title");
        assert_eq!(touched.turns, 3);
        assert!(touched.updated_at >= meta.updated_at);
        assert_eq!(list(&state, &workspace)[0].title, "a title");
    }

    #[test]
    fn a_first_message_lists_the_conversation_before_any_turn_is_written() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();

        // The turn has not written anything yet; this is the moment the message
        // is sent.
        let row = begin(&state, &workspace, &id, "coding", "  修一下  按钮  ").expect("begin");

        assert_eq!(row.id, id);
        assert_eq!(row.title, "修一下  按钮");
        assert_eq!(row.turns, 0);
        assert_eq!(row.rig, "coding");

        // The list agrees, and stays agreed after a reconcile: a named document
        // with no turns is a conversation, not litter.
        let rows = list(&state, &workspace);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert!(session_path(&state, &workspace, &id).is_file());
    }

    #[test]
    fn beginning_a_conversation_that_has_turns_keeps_its_count() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 4))
            .expect("save");

        let row = begin(&state, &workspace, &meta.id, "coding", "second question").expect("begin");

        assert_eq!(row.turns, 4);
        assert_eq!(row.title, "first question");
        // One row, not two: begin never appends a second row for one document.
        assert_eq!(list(&state, &workspace).len(), 1);
    }

    #[test]
    fn a_message_with_nothing_in_it_writes_nothing() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();

        assert!(begin(&state, &workspace, &id, "coding", "   \n ").is_none());
        assert!(list(&state, &workspace).is_empty());
        assert!(!session_path(&state, &workspace, &id).is_file());
    }

    #[test]
    fn a_question_is_noted_until_its_turn_is_written() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();

        mark_pending(&state, &workspace, &id, 1, "  sweep the sidecars  ").expect("a note");
        let note = pending(&state, &workspace, &id).expect("the question");
        assert_eq!(note.seq, 1);
        assert_eq!(note.input, "  sweep the sidecars  ");

        // The note is not a conversation of its own: its name must fail the id
        // check, and listing must not adopt it.
        assert!(!valid_id(&format!("{id}.pending")));
        assert!(list(&state, &workspace).is_empty());

        clear_pending(&state, &workspace, &id);
        assert!(pending(&state, &workspace, &id).is_none());
    }

    #[test]
    fn an_empty_question_is_not_noted() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();

        assert!(mark_pending(&state, &workspace, &id, 1, "   ").is_none());
        assert!(pending(&state, &workspace, &id).is_none());
    }

    /// The answer that was streaming when the process stopped is kept beside the
    /// question, so a restart shows how far it got (ADR 0083).
    #[test]
    fn the_note_keeps_the_answer_that_had_streamed() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();

        mark_pending(&state, &workspace, &id, 7, "explain the sweep").expect("a note");
        assert_eq!(
            pending(&state, &workspace, &id).expect("the note").partial,
            ""
        );

        note_partial(&state, &workspace, &id, 7, "The sweep ").expect("the first part");
        let note = pending(&state, &workspace, &id).expect("the note");
        // The question and the sequence it belongs to are the ones that were
        // written; only the answer was added.
        assert_eq!(note.seq, 7);
        assert_eq!(note.input, "explain the sweep");
        assert_eq!(note.partial, "The sweep ");

        // A note that names another turn is not this turn's to change, and
        // writing what it already holds is not a write.
        assert!(note_partial(&state, &workspace, &id, 8, "no").is_none());
        assert!(note_partial(&state, &workspace, &id, 7, "The sweep ").is_none());
        assert_eq!(
            pending(&state, &workspace, &id).expect("the note").partial,
            "The sweep "
        );
    }

    /// A note written before the field existed still reads, and can be given an
    /// answer: the file outlives the build that wrote it.
    #[test]
    fn a_note_from_before_the_field_existed_still_reads() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();
        let project = project_dir(&state, &workspace);
        fs::create_dir_all(&project).expect("the project dir");
        fs::write(
            pending_path(&project, &id),
            r#"{"seq":4,"input":"earlier","startedAt":1}"#,
        )
        .expect("an old note");

        let note = pending(&state, &workspace, &id).expect("the note");
        assert_eq!(note.seq, 4);
        assert_eq!(note.partial, "");

        note_partial(&state, &workspace, &id, 4, "half an answer").expect("the answer");
        let note = pending(&state, &workspace, &id).expect("the note");
        assert_eq!(note.partial, "half an answer");
        assert_eq!(note.input, "earlier");
    }

    #[test]
    fn a_deleted_conversation_leaves_no_note_behind() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        mark_pending(&state, &workspace, &meta.id, 1, "sweep the sidecars").expect("a note");

        remove(&state, &workspace, &meta.id).expect("remove");

        assert!(pending(&state, &workspace, &meta.id).is_none());
        assert!(!session_path(&state, &workspace, &meta.id).is_file());
    }

    #[test]
    fn a_rename_lives_in_the_document() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 1))
            .expect("save");

        let renamed = set_title(&state, &workspace, &meta.id, "  my name  ").expect("rename");
        assert_eq!(renamed.title, "my name");

        fs::remove_file(project_dir(&state, &workspace).join(INDEX_FILE)).expect("drop the index");
        let rows = list(&state, &workspace);
        assert_eq!(rows[0].title, "my name");
    }

    #[test]
    fn clearing_a_title_falls_back_to_the_first_message() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 1))
            .expect("save");

        set_title(&state, &workspace, &meta.id, "custom").expect("rename");
        let reset = set_title(&state, &workspace, &meta.id, "").expect("clear");

        assert_eq!(reset.title, "first question");
    }

    #[test]
    fn an_archive_flag_lives_in_the_document() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 1))
            .expect("save");

        let archived = set_archived(&state, &workspace, &meta.id, true).expect("archive");
        assert!(archived.archived);

        fs::remove_file(project_dir(&state, &workspace).join(INDEX_FILE)).expect("drop the index");
        assert!(list(&state, &workspace)[0].archived);

        let restored = set_archived(&state, &workspace, &meta.id, false).expect("unarchive");
        assert!(!restored.archived);
    }

    #[test]
    fn only_archived_conversations_are_deletable() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");

        assert!(!is_archived(&state, &workspace, &meta.id).expect("read the flag"));

        set_archived(&state, &workspace, &meta.id, true).expect("archive");
        assert!(is_archived(&state, &workspace, &meta.id).expect("read the flag"));
    }

    #[test]
    fn a_first_turn_gives_a_blank_conversation_a_row() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let id = fresh_id();
        let project = project_dir(&state, &workspace);
        fs::create_dir_all(&project).expect("a project dir");
        Store::open(project.join(format!("{id}.json")))
            .save(&document(&workspace, "lazy first turn", 1))
            .expect("save");

        let meta = touch(&state, &workspace, &id).expect("touch");

        assert_eq!(meta.title, "lazy first turn");
        assert_eq!(meta.turns, 1);
        assert_eq!(list(&state, &workspace)[0].id, id);
    }

    #[test]
    fn conversations_are_listed_newest_first() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let first = create(&state, &workspace, "coding").expect("first");
        Store::open(session_path(&state, &workspace, &first.id))
            .save(&document(&workspace, "first question", 1))
            .expect("save");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = create(&state, &workspace, "coding").expect("second");
        Store::open(session_path(&state, &workspace, &second.id))
            .save(&document(&workspace, "second question", 1))
            .expect("save");

        let rows = list(&state, &workspace);

        assert_eq!(rows[0].id, second.id);
        assert_eq!(rows[1].id, first.id);
    }

    #[test]
    fn a_named_branch_is_listed_before_its_first_turn() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");

        set_title(&state, &workspace, &meta.id, "首条（分支）").expect("name it");
        let rows = list(&state, &workspace);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "首条（分支）");
        assert_eq!(rows[0].turns, 0);
    }

    #[test]
    fn deleting_a_conversation_removes_its_row_and_document() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");

        remove(&state, &workspace, &meta.id).expect("remove");

        assert!(!session_path(&state, &workspace, &meta.id).exists());
        assert!(list(&state, &workspace).is_empty());
    }

    #[test]
    fn an_external_document_is_adopted() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        create(&state, &workspace, "coding").expect("create");
        let project = project_dir(&state, &workspace);
        Store::open(project.join("s-external.json"))
            .save(&document(&workspace, "copied in", 1))
            .expect("save");

        let rows = list(&state, &workspace);

        let copied = rows
            .iter()
            .find(|meta| meta.id == "s-external")
            .expect("the copied document is listed");
        assert_eq!(copied.title, "copied in");
    }

    #[test]
    fn a_stale_index_entry_is_dropped() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        fs::remove_file(session_path(&state, &workspace, &meta.id)).expect("remove the document");

        assert!(list(&state, &workspace).is_empty());
    }

    #[test]
    fn duplicate_rows_are_collapsed_to_the_newest() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 1))
            .expect("save");
        touch(&state, &workspace, &meta.id).expect("touch");
        let project = project_dir(&state, &workspace);
        let mut index = read_index(&project).expect("an index");
        let mut older = meta.clone();
        older.title = "old".to_owned();
        older.turns = 1;
        older.updated_at = meta.updated_at.saturating_sub(10);
        let mut newer = meta.clone();
        newer.title = "new".to_owned();
        newer.turns = 1;
        newer.updated_at = meta.updated_at.saturating_add(10);
        index.sessions = vec![older, newer];
        save_index(&project, &index).expect("save");

        let rows = list(&state, &workspace);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "new");
    }

    #[test]
    fn ids_that_could_escape_the_project_directory_are_refused() {
        assert!(!valid_id("../index"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(""));
        assert!(valid_id("s-123abc"));
    }

    #[test]
    fn a_row_counts_the_turns_that_live_in_their_own_files() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        Store::open(session_path(&state, &workspace, &meta.id))
            .save(&document(&workspace, "first question", 3))
            .expect("save");

        // The index is dropped so the row is derived from the files, which is
        // the path a migrated conversation takes.
        fs::remove_file(project_dir(&state, &workspace).join(INDEX_FILE)).expect("drop the index");
        let rows = list(&state, &workspace);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].turns, 3);
        assert_eq!(rows[0].title, "first question");
        assert_eq!(touch(&state, &workspace, &meta.id).expect("touch").turns, 3);
    }

    #[test]
    fn listing_moves_the_sidecars_of_a_removed_header_aside() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        let path = session_path(&state, &workspace, &meta.id);
        Store::open(path.clone())
            .save(&document(&workspace, "first question", 2))
            .expect("save");
        // A build before the split removed the header and left the sidecars.
        fs::remove_file(&path).expect("remove the header");

        let rows = list(&state, &workspace);

        assert!(rows.is_empty());
        assert!(!path.with_extension("snapshot.json").exists());
        assert!(!path.with_extension("turns").exists());
        assert!(!path.with_extension("segments").exists());
        assert!(project_dir(&state, &workspace)
            .join(".orphans")
            .join(format!("{}.snapshot.json", meta.id))
            .exists());
    }

    #[test]
    fn deleting_a_conversation_removes_its_turn_files() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");
        let path = session_path(&state, &workspace, &meta.id);
        Store::open(path.clone())
            .save(&document(&workspace, "first question", 2))
            .expect("save");
        let turns = path.with_extension("turns");
        assert!(turns.is_dir());
        set_archived(&state, &workspace, &meta.id, true).expect("archive");

        remove(&state, &workspace, &meta.id).expect("delete");

        assert!(!path.exists());
        assert!(!turns.exists(), "{}", turns.display());
        assert!(!path.with_extension("segments").exists());
        assert!(list(&state, &workspace).is_empty());
    }
}
