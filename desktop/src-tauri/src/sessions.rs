//! Per-project conversations.
//!
//! A project owns one directory under `<state>/sessions/<workspace-hash>/`
//! with one JSON document per conversation and an `index.json` for listing.
//! The index is a cache: it is reconciled against the directory, so a missing
//! or stale index cannot hide a document or invent one.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use keelson::engine::{Store, StoredSession};
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

/// Creates an empty conversation and writes its document, so the row survives
/// a restart and the index always points at a real file.
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
    let text = fs::read_to_string(&path).ok()?;
    let document: StoredSession = serde_json::from_str(&text).ok()?;

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
    meta.title = title_for(&document);
    meta.archived = document.archived;
    meta.rig = document.rig.clone();
    meta.turns = document.turns.len() as u32;
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
    let path = dir.join(format!("{id}.json"));
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot delete `{}`: {error}", path.display())),
    }
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
    let text = fs::read_to_string(&path).ok()?;
    let document: StoredSession = serde_json::from_str(&text).ok()?;

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
    meta.title = title_for(&document);
    meta.archived = document.archived;
    meta.rig = document.rig.clone();
    meta.turns = document.turns.len() as u32;
    let meta = meta.clone();
    let _ = save_index(&dir, &index);
    Some(meta)
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

/// A custom title wins; otherwise the first user message is the title.
fn title_for(document: &StoredSession) -> String {
    if !document.title.trim().is_empty() {
        return document.title.trim().to_owned();
    }
    document
        .turns
        .first()
        .map(|turn| title_from(&turn.input))
        .unwrap_or_default()
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
        sessions.push(meta_from_file(&path, &id));
    }
    SessionIndex {
        version: INDEX_VERSION,
        sessions,
    }
}

/// Drops rows whose document is gone, adopts documents the index does not
/// know about, and collapses duplicate rows. A crashed write or a manually
/// copied file stays visible.
fn reconcile(dir: &Path, index: &mut SessionIndex) -> bool {
    let present: HashSet<String> = documents(dir).into_iter().map(|(id, _)| id).collect();
    let mut changed = false;
    let before = index.sessions.len();
    index.sessions.retain(|meta| present.contains(&meta.id));
    changed |= index.sessions.len() != before;

    let known: HashSet<String> = index.sessions.iter().map(|meta| meta.id.clone()).collect();
    let added: Vec<SessionMeta> = present
        .into_iter()
        .filter(|id| !known.contains(id))
        .map(|id| meta_from_file(&dir.join(format!("{id}.json")), &id))
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
    let document = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<StoredSession>(&text).ok());
    let time = file_ms(path).unwrap_or_else(now_ms);
    let (title, archived, rig, turns) = document
        .as_ref()
        .map(|document| {
            (
                title_for(document),
                document.archived,
                document.rig.clone(),
                document.turns.len() as u32,
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
                    error: None,
                    duration_ms: 1,
                    verification_passed: None,
                    tools: Vec::new(),
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
    fn a_created_conversation_has_a_document_and_a_row() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");

        let meta = create(&state, &workspace, "coding").expect("create");
        assert!(valid_id(&meta.id));
        assert!(session_path(&state, &workspace, &meta.id).is_file());

        let rows = list(&state, &workspace);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, meta.id);
        assert_eq!(rows[0].rig, "coding");
        assert_eq!(rows[0].turns, 0);
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
    fn a_rename_lives_in_the_document() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = workspace(&dir);
        let state = dir.path().join("state");
        let meta = create(&state, &workspace, "coding").expect("create");

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
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = create(&state, &workspace, "coding").expect("second");

        let rows = list(&state, &workspace);

        assert_eq!(rows[0].id, second.id);
        assert_eq!(rows[1].id, first.id);
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
        let project = project_dir(&state, &workspace);
        let mut index = read_index(&project).expect("an index");
        let mut older = meta.clone();
        older.title = "old".to_owned();
        older.updated_at = meta.updated_at.saturating_sub(10);
        let mut newer = meta.clone();
        newer.title = "new".to_owned();
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
}
