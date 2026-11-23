//! Session persistence.
//!
//! One session is a header, a state file, one file per segment and one file per
//! turn: the header holds what a list needs, the state file holds the session
//! planes the kernel is still working from, each segment is its own file, each
//! turn row is its own file. The kernel owns the snapshot shape; this adapter
//! only moves it in and out of files.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use keel_core::{KernelError, SegmentSnapshot, SessionSnapshot, TodoList};
use serde::{Deserialize, Serialize};

/// Where a sidecar whose header is gone is moved to, inside the project
/// directory. A dot-directory: the list's id filter never sees it.
const ORPHAN_DIR: &str = ".orphans";

/// The sidecars named after a header by directory suffix.
const SIDECAR_DIRS: [&str; 2] = [".turns", ".segments"];

/// The manifest inside the segment directory: which segments it holds, and the
/// hash of what was written for each, so a save writes only what moved.
const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_VERSION: u32 = 1;

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

/// The header file on its own: the workspace, the rig, the display name, the
/// archive flag, the todo list and whatever turn rows an old document still
/// carries inline.
///
/// This is everything a conversation list shows, and reading it never parses
/// the kernel snapshot. The snapshot is the part that grows with the
/// conversation — before it had its own file the list parsed 19 MB of headers
/// to paint nine rows (ADR 0058).
#[derive(Debug, Clone)]
pub struct Header {
    pub workspace: PathBuf,
    pub rig: String,
    pub title: String,
    pub archived: bool,
    pub todos: Option<TodoList>,
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
    /// True when an identical call earlier in the same turn answered this one
    /// (ADR 0070). Absent in sessions written before the field.
    #[serde(default)]
    pub reused: bool,
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
    /// Memory entries the `[memory]` block left out of this turn. Absent in
    /// documents written before the field existed.
    #[serde(default)]
    pub facts_clipped: u32,
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

/// One segment file, as the manifest records it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntry {
    id: u32,
    hash: String,
}

/// What the segment directory holds: the ids, and the hash of the file each
/// was written with.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SegmentManifest {
    version: u32,
    segments: Vec<ManifestEntry>,
}

/// A session document at one path.
///
/// The path is the header: `s-<id>.json` holds the workspace, the rig, the
/// title, the archive flag and the todo list. The planes the kernel is still
/// working from are beside it in `s-<id>.snapshot.json`, each segment is its own
/// file under `s-<id>.segments/`, and each turn is its own file under
/// `s-<id>.turns/`, named by sequence number.
///
/// Four shapes rather than one for the same reason three times over: a turn row
/// is written once and never rewritten (ADR 0057), the state sits apart from the
/// header so listing conversations reads a few hundred bytes per conversation
/// instead of the whole conversation state (ADR 0058), and the segments sit
/// apart from the state so a save writes the segment that grew, not the whole
/// conversation (ADR 0060).
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

    /// Where the kernel snapshot lives. Beside the header, named from it, so a
    /// caller that knows the session path still finds it.
    pub fn snapshot_path(&self) -> PathBuf {
        self.path.with_extension("snapshot.json")
    }

    /// Where the segment files live: one per segment, beside the state file
    /// that used to carry every one of them.
    pub fn segments_dir(&self) -> PathBuf {
        self.path.with_extension("segments")
    }

    /// The manifest naming the segment files, and the hash of each.
    fn manifest_path(&self) -> PathBuf {
        self.segments_dir().join(MANIFEST_FILE)
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

    /// Reads the header and the rows a document written before the split still
    /// carries inline, without opening a single row file.
    ///
    /// For a caller that needs the workspace, the flags and the title and
    /// nothing else — the desktop index, and the row it refreshes after a turn.
    pub fn read_header(&self) -> Result<Option<Header>, KernelError> {
        let Some(header) = self.read_header_json()? else {
            return Ok(None);
        };
        Ok(Some(Header {
            workspace: header.workspace,
            rig: header.rig,
            title: header.title,
            archived: header.archived,
            todos: header.todos,
            turns: header.turns,
        }))
    }

    /// The header and the kernel snapshot, without a single turn row.
    ///
    /// A document written before the snapshot had its own file carries it
    /// inline and is read from there; the next save writes the separate file.
    pub fn read_document(&self) -> Result<Option<StoredSession>, KernelError> {
        let Some(header) = self.read_header_json()? else {
            return Ok(None);
        };
        let snapshot = match header.snapshot {
            Some(snapshot) => snapshot,
            None => self.read_snapshot()?,
        };
        Ok(Some(StoredSession {
            workspace: header.workspace,
            rig: header.rig,
            title: header.title,
            archived: header.archived,
            snapshot,
            todos: header.todos,
            turns: header.turns,
        }))
    }

    fn read_header_json(&self) -> Result<Option<HeaderJson>, KernelError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage_error(&self.path, error)),
        };
        let header: HeaderJson =
            serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                message: format!(
                    "session `{}` is not valid JSON: {error}",
                    self.path.display()
                ),
            })?;
        Ok(Some(header))
    }

    fn read_snapshot(&self) -> Result<SessionSnapshot, KernelError> {
        let path = self.snapshot_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(KernelError::InvalidConfig {
                    message: format!(
                        "session `{}` has no snapshot: `{}` is missing",
                        self.path.display(),
                        path.display()
                    ),
                })
            }
            Err(error) => return Err(storage_error(&path, error)),
        };
        let mut snapshot: SessionSnapshot =
            serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                message: format!("snapshot `{}` is not valid JSON: {error}", path.display()),
            })?;
        // A snapshot written before the segments had files of their own still
        // carries them inline. It is read as it is and written out file by file
        // on the next save, which is the whole migration.
        if snapshot.segments.is_empty() {
            snapshot.segments = self.read_segment_files()?;
        }
        Ok(snapshot)
    }

    /// Reads the segment files the manifest names, in id order.
    fn read_segment_files(&self) -> Result<Vec<SegmentSnapshot>, KernelError> {
        let dir = self.segments_dir();
        let mut entries = self.read_manifest()?;
        entries.sort_by_key(|entry| entry.id);
        let mut segments = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = dir.join(segment_file_name(entry.id));
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(KernelError::InvalidConfig {
                        message: format!(
                            "session `{}` has no segment {}: `{}` is missing",
                            self.path.display(),
                            entry.id,
                            path.display()
                        ),
                    })
                }
                Err(error) => return Err(storage_error(&path, error)),
            };
            let segment: SegmentSnapshot =
                serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                    message: format!("segment `{}` is not valid JSON: {error}", path.display()),
                })?;
            segments.push(segment);
        }
        Ok(segments)
    }

    /// The manifest as written, or nothing when there is none.
    ///
    /// A manifest that cannot be read is treated as empty: it carries only the
    /// hashes that let a save skip an unchanged segment, so losing it costs one
    /// rewrite of segments that are already on disk, never a segment.
    fn read_manifest(&self) -> Result<Vec<ManifestEntry>, KernelError> {
        let path = self.manifest_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage_error(&path, error)),
        };
        let Ok(manifest) = serde_json::from_str::<SegmentManifest>(&text) else {
            return Ok(Vec::new());
        };
        Ok(manifest.segments)
    }

    /// Writes one file per segment, then the manifest that names them.
    ///
    /// A segment file is written only when its content changed: the manifest
    /// keeps the hash of every segment that was written, so a save costs the
    /// segments that moved. A sealed segment is never touched again, and the
    /// file grows with its own segment rather than with the conversation.
    fn save_segments(&self, snapshot: &SessionSnapshot) -> Result<(), KernelError> {
        let dir = self.segments_dir();
        fs::create_dir_all(&dir).map_err(|error| storage_error(&dir, error))?;
        let written = self.read_manifest()?;
        let mut entries = Vec::with_capacity(snapshot.segments.len());
        for segment in &snapshot.segments {
            let text = serde_json::to_string(segment).map_err(|error| KernelError::Internal {
                message: format!("cannot encode segment {}: {error}", segment.id),
            })?;
            let hash = content_hash(&text);
            if !written
                .iter()
                .any(|entry| entry.id == segment.id && entry.hash == hash)
            {
                write_atomic(&dir.join(segment_file_name(segment.id)), &text)?;
            }
            entries.push(ManifestEntry {
                id: segment.id,
                hash,
            });
        }
        // A segment file the new manifest does not name is one the kernel
        // absorbed into a summary, or one a save that never finished left
        // behind. Neither belongs to the conversation any more.
        self.prune_segments(&entries)?;
        let manifest = SegmentManifest {
            version: MANIFEST_VERSION,
            segments: entries,
        };
        let text = serde_json::to_string(&manifest).map_err(|error| KernelError::Internal {
            message: format!(
                "cannot encode the segment manifest of `{}`: {error}",
                self.path.display()
            ),
        })?;
        write_atomic(&self.manifest_path(), &text)
    }

    fn prune_segments(&self, entries: &[ManifestEntry]) -> Result<(), KernelError> {
        let dir = self.segments_dir();
        let keep: BTreeSet<String> = entries
            .iter()
            .map(|entry| segment_file_name(entry.id))
            .collect();
        let files = match fs::read_dir(&dir) {
            Ok(files) => files,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(storage_error(&dir, error)),
        };
        for file in files {
            let path = file.map_err(|error| storage_error(&dir, error))?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name == MANIFEST_FILE || keep.contains(name) {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(storage_error(&path, error)),
            }
        }
        Ok(())
    }

    /// Loads the document without checking which workspace it belongs to.
    ///
    /// For a caller that already knows the path belongs to the project — the
    /// desktop index derives the directory from the workspace hash — and needs
    /// the rows themselves.
    pub fn load_unchecked(&self) -> Result<Option<StoredSession>, KernelError> {
        let Some(mut session) = self.read_document()? else {
            return Ok(None);
        };
        self.read_turns(&mut session.turns)?;
        Ok(Some(session))
    }

    /// How many rows the conversation has. Counts the names, not the contents:
    /// a row the header still carries is counted once.
    pub fn turn_count(&self, header: &Header) -> Result<u32, KernelError> {
        let mut seqs: BTreeSet<u32> = header.turns.iter().map(|turn| turn.seq).collect();
        for (seq, _) in self.row_files()? {
            seqs.insert(seq);
        }
        Ok(seqs.len() as u32)
    }

    /// The earliest row, which is where a conversation without a custom title
    /// takes its name from. Reads one file, not the conversation.
    pub fn first_turn(&self, header: &Header) -> Result<Option<StoredTurn>, KernelError> {
        let inline = header.turns.iter().min_by_key(|turn| turn.seq);
        let Some((seq, path)) = self.row_files()?.into_iter().next() else {
            return Ok(inline.cloned());
        };
        if inline.is_some_and(|turn| turn.seq < seq) {
            return Ok(inline.cloned());
        }
        let text = fs::read_to_string(&path).map_err(|error| storage_error(&path, error))?;
        let turn: StoredTurn =
            serde_json::from_str(&text).map_err(|error| KernelError::InvalidConfig {
                message: format!("turn `{}` is not valid JSON: {error}", path.display()),
            })?;
        Ok(Some(turn))
    }

    /// The row files in sequence order. Names only; nothing is parsed.
    fn row_files(&self) -> Result<Vec<(u32, PathBuf)>, KernelError> {
        let dir = self.turns_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
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
            rows.push((seq, path));
        }
        rows.sort_by_key(|(seq, _)| *seq);
        Ok(rows)
    }

    /// Writes the state, the segments, the header and any turn row that does
    /// not have a file yet.
    ///
    /// The parts go in that order and the header goes last: the header names
    /// the sidecars, so a crash before it leaves the previous header pointing
    /// at a state that is one turn ahead — the state the resume path reads —
    /// rather than a header that points at a file that is not there.
    ///
    /// A crash leaves a conversation that is missing its last row, not one with
    /// a half-written row: every file is written through a temporary name and
    /// renamed.
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
        self.save_segments(&session.snapshot)?;
        // The segments live in their own files; what is left is the state a
        // save has to rewrite — the memo, the facts and the counters.
        let mut state = session.snapshot.clone();
        state.segments.clear();
        let snapshot = serde_json::to_string(&state).map_err(|error| KernelError::Internal {
            message: format!(
                "cannot encode the state of `{}`: {error}",
                self.path.display()
            ),
        })?;
        write_atomic(&self.snapshot_path(), &snapshot)?;
        let header = SessionHeader {
            workspace: &session.workspace,
            rig: &session.rig,
            title: &session.title,
            archived: session.archived,
            todos: session.todos.as_ref(),
        };
        let text =
            serde_json::to_string_pretty(&header).map_err(|error| KernelError::Internal {
                message: format!("cannot encode session `{}`: {error}", self.path.display()),
            })?;
        write_atomic(&self.path, &text)
    }

    /// Removes the header, the state, the segments and every turn row it owns.
    pub fn delete(&self) -> Result<(), KernelError> {
        for path in [self.path.clone(), self.snapshot_path()] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(storage_error(&path, error)),
            }
        }
        for dir in [self.segments_dir(), self.turns_dir()] {
            match fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(storage_error(&dir, error)),
            }
        }
        Ok(())
    }

    /// Moves the sidecars of a header that is gone into `<dir>/.orphans/`.
    ///
    /// The state file, the segment directory and the row directory are all
    /// named after their header, so a header removed without them — by a build
    /// that predates the split, or by a crash between the two — leaves the
    /// largest files in the directory behind and no row in the list that would
    /// ever remove them.
    ///
    /// Nothing is deleted. Each orphan is renamed into `.orphans/`, where it
    /// stays readable until a person confirms the loss and removes it.
    pub fn sweep_orphans(dir: &Path) -> Result<u32, KernelError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(storage_error(dir, error)),
        };
        let mut orphans = Vec::new();
        for entry in entries {
            let path = entry.map_err(|error| storage_error(dir, error))?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let header = if let Some(prefix) = name.strip_suffix(".snapshot.json") {
                dir.join(format!("{prefix}.json"))
            } else if path.is_dir() {
                match SIDECAR_DIRS
                    .iter()
                    .find_map(|suffix| name.strip_suffix(suffix))
                {
                    Some(prefix) => dir.join(format!("{prefix}.json")),
                    None => continue,
                }
            } else {
                continue;
            };
            if !header.exists() {
                orphans.push(path);
            }
        }
        if orphans.is_empty() {
            return Ok(0);
        }

        let aside = dir.join(ORPHAN_DIR);
        fs::create_dir_all(&aside).map_err(|error| storage_error(&aside, error))?;
        let mut moved = 0;
        for path in orphans {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let mut target = aside.join(name);
            let mut suffix = 1;
            while target.exists() {
                target = aside.join(format!("{name}.{suffix}"));
                suffix += 1;
            }
            fs::rename(&path, &target).map_err(|error| storage_error(&path, error))?;
            moved += 1;
        }
        Ok(moved)
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

/// What the session document keeps once each turn has its own file and the
/// kernel snapshot has its own: everything the list and the resume path need,
/// and nothing that grows with the length of the conversation.
#[derive(serde::Serialize)]
struct SessionHeader<'a> {
    workspace: &'a Path,
    rig: &'a str,
    title: &'a str,
    archived: bool,
    todos: Option<&'a TodoList>,
}

/// The header as it is read back.
///
/// `snapshot` is `None` in every document written since the snapshot moved to
/// its own file, and `Some` in a document written before it did; that is the
/// whole migration (ADR 0058).
#[derive(serde::Deserialize)]
struct HeaderJson {
    workspace: PathBuf,
    #[serde(default)]
    rig: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    snapshot: Option<SessionSnapshot>,
    #[serde(default)]
    todos: Option<TodoList>,
    #[serde(default)]
    turns: Vec<StoredTurn>,
}

fn turn_file_name(seq: u32) -> String {
    format!("{seq:08}.json")
}

fn segment_file_name(id: u32) -> String {
    format!("{id:08}.json")
}

/// A fingerprint of one segment's JSON, so a save can tell an unchanged segment
/// from one that grew without reading the file back.
fn content_hash(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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

    use keel_core::{EntrySnapshot, SegmentState};

    fn segment(id: u32, topic: &str) -> SegmentSnapshot {
        SegmentSnapshot {
            id,
            topic: topic.to_owned(),
            state: SegmentState::Sealed,
            entries: vec![EntrySnapshot {
                role: "user".to_owned(),
                content: format!("about {topic}"),
            }],
            turns: 1,
            terms: vec![topic.to_lowercase()],
            artifacts: Vec::new(),
            pending: Vec::new(),
            verified: false,
            record: None,
        }
    }

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
    fn the_header_carries_no_turns_and_no_snapshot() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));

        store.save(&session(workspace.clone())).expect("save");

        let header = fs::read_to_string(&store.path).expect("the header");
        assert!(!header.contains("\"turns\""), "{header}");
        assert!(!header.contains("\"snapshot\""), "{header}");
        assert!(
            store.snapshot_path().is_file(),
            "{}",
            store.snapshot_path().display()
        );
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
    fn the_list_reads_a_count_and_a_title_without_the_conversation() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        let mut stored = session(workspace.clone());
        let second = StoredTurn {
            seq: 2,
            input: "second question".to_owned(),
            ..stored.turns[0].clone()
        };
        stored.turns.push(second);
        store.save(&stored).expect("save");

        // What a list row opens: the header alone, with no row in it.
        let header = store.read_header().expect("read").expect("a header");
        assert!(header.turns.is_empty());

        assert_eq!(store.turn_count(&header).expect("count"), 2);
        let first = store.first_turn(&header).expect("first").expect("a row");
        assert_eq!(first.seq, 1);
        assert_eq!(first.input, "hello");
    }

    #[test]
    fn a_row_the_header_still_carries_is_counted_once() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        // A document written before the split: its rows are inline, and a later
        // save has already written one of them out as a file.
        let store = Store::open(dir.path().join("sessions/s-old.json"));
        let legacy = session(workspace.clone());
        fs::create_dir_all(store.path.parent().expect("a parent")).expect("the session dir");
        fs::write(
            &store.path,
            serde_json::to_string(&legacy).expect("encode the document"),
        )
        .expect("the legacy document");
        let row = StoredTurn {
            seq: 2,
            input: "second question".to_owned(),
            ..legacy.turns[0].clone()
        };
        fs::create_dir_all(store.turns_dir()).expect("the turn dir");
        fs::write(
            store.turns_dir().join(turn_file_name(2)),
            serde_json::to_string(&row).expect("encode the row"),
        )
        .expect("the row file");

        let header = store.read_header().expect("read").expect("a header");
        assert_eq!(header.turns.len(), 1, "the inline row is still there");

        assert_eq!(store.turn_count(&header).expect("count"), 2);
        let first = store.first_turn(&header).expect("first").expect("a row");
        assert_eq!(first.seq, 1);
    }

    #[test]
    fn the_snapshot_round_trips_through_its_own_file() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        let mut stored = session(workspace.clone());
        stored.snapshot = SessionSnapshot {
            next_segment_id: 12,
            last_verified: true,
            ..SessionSnapshot::default()
        };

        store.save(&stored).expect("save");
        let loaded = store.load(&workspace).expect("load").expect("a session");

        assert_eq!(loaded.snapshot.next_segment_id, 12);
        assert!(loaded.snapshot.last_verified);
    }

    #[test]
    fn a_document_that_still_carries_its_snapshot_moves_it_out() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-old.json"));
        let mut legacy = session(workspace.clone());
        legacy.snapshot.next_segment_id = 7;
        fs::create_dir_all(store.path.parent().expect("a parent")).expect("the session dir");
        fs::write(
            &store.path,
            serde_json::to_string(&legacy).expect("encode the document"),
        )
        .expect("the legacy document");

        let loaded = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(loaded.snapshot.next_segment_id, 7, "read from the header");

        store.save(&loaded).expect("save");
        let header = fs::read_to_string(&store.path).expect("the header");
        assert!(!header.contains("\"snapshot\""), "{header}");
        let again = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(
            again.snapshot.next_segment_id, 7,
            "read from the snapshot file"
        );
    }

    #[test]
    fn a_session_without_its_snapshot_file_is_an_error() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        store.save(&session(workspace.clone())).expect("save");
        fs::remove_file(store.snapshot_path()).expect("remove the snapshot");

        let error = store.load(&workspace).expect_err("refused");

        assert!(error.to_string().contains("no snapshot"), "{error}");
    }

    #[test]
    fn every_segment_is_a_file_and_the_state_keeps_none() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        let mut session = session(workspace.clone());
        session.snapshot.segments = vec![segment(1, "alpha"), segment(2, "beta")];

        store.save(&session).expect("save");

        let state = fs::read_to_string(store.snapshot_path()).expect("the state file");
        assert!(state.contains("\"segments\":[]"), "{state}");
        assert!(store.segments_dir().join("00000001.json").is_file());
        assert!(store.segments_dir().join("00000002.json").is_file());
        let loaded = store.load(&workspace).expect("load").expect("a session");
        let topics: Vec<&str> = loaded
            .snapshot
            .segments
            .iter()
            .map(|segment| segment.topic.as_str())
            .collect();
        assert_eq!(topics, ["alpha", "beta"]);
    }

    #[test]
    fn an_unchanged_segment_is_not_written_again() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        let mut session = session(workspace.clone());
        session.snapshot.segments = vec![segment(1, "alpha")];
        store.save(&session).expect("save");
        let file = store.segments_dir().join("00000001.json");
        let before = fs::metadata(&file)
            .expect("the segment file")
            .modified()
            .expect("a time");

        std::thread::sleep(std::time::Duration::from_millis(20));
        store.save(&session).expect("save again");

        let after = fs::metadata(&file)
            .expect("the segment file")
            .modified()
            .expect("a time");
        assert_eq!(before, after, "a sealed segment was written again");

        // A segment that grew is written, and only it.
        let mut grown = session.clone();
        grown.snapshot.segments[0].topic = "alpha grown".to_owned();
        grown.snapshot.segments.push(segment(2, "beta"));
        store.save(&grown).expect("save a grown segment");

        let text = fs::read_to_string(&file).expect("the segment file");
        assert!(text.contains("alpha grown"), "{text}");
        assert!(store.segments_dir().join("00000002.json").is_file());
    }

    #[test]
    fn a_segment_the_kernel_dropped_loses_its_file() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        let mut session = session(workspace.clone());
        session.snapshot.segments = vec![segment(1, "alpha"), segment(2, "beta")];
        store.save(&session).expect("save");
        let dropped = store.segments_dir().join("00000001.json");

        // Absorbed into a summary, so the snapshot no longer lists it.
        session.snapshot.segments.remove(0);
        store.save(&session).expect("save");

        assert!(!dropped.exists(), "a segment the kernel dropped is gone");
        let loaded = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(loaded.snapshot.segments.len(), 1);
        assert_eq!(loaded.snapshot.segments[0].topic, "beta");
    }

    #[test]
    fn a_missing_segment_file_is_an_error() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        let mut session = session(workspace.clone());
        session.snapshot.segments = vec![segment(1, "alpha")];
        store.save(&session).expect("save");
        fs::remove_file(store.segments_dir().join("00000001.json")).expect("remove the segment");

        let error = store.load(&workspace).expect_err("refused");

        assert!(error.to_string().contains("no segment 1"), "{error}");
    }

    #[test]
    fn a_snapshot_written_inline_is_moved_into_files() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let store = Store::open(dir.path().join("sessions/s-1.json"));
        store.save(&session(workspace.clone())).expect("save");
        // The shape a build before the split left behind: the whole snapshot,
        // segments included, in one file, and no segment directory.
        let inline = SessionSnapshot {
            segments: vec![segment(1, "alpha")],
            ..SessionSnapshot::default()
        };
        fs::write(
            store.snapshot_path(),
            serde_json::to_string(&inline).expect("json"),
        )
        .expect("an inline snapshot");
        fs::remove_dir_all(store.segments_dir()).expect("remove the segment files");

        let loaded = store.load(&workspace).expect("load").expect("a session");
        assert_eq!(loaded.snapshot.segments.len(), 1);
        store.save(&loaded).expect("save");

        let state = fs::read_to_string(store.snapshot_path()).expect("the state file");
        assert!(state.contains("\"segments\":[]"), "{state}");
        assert!(store.segments_dir().join("00000001.json").is_file());
    }

    #[test]
    fn a_sidecar_without_a_header_is_moved_aside() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let dir = dir.path().join("sessions");
        let store = Store::open(dir.join("s-1.json"));
        store.save(&session(workspace.clone())).expect("save");
        let kept = Store::open(dir.join("s-2.json"));
        kept.save(&session(workspace)).expect("save");

        // A build that predates the split deleted the header and left the rest.
        fs::remove_file(&store.path).expect("remove the header");

        assert_eq!(Store::sweep_orphans(&dir).expect("sweep"), 3);
        assert!(!store.snapshot_path().exists(), "the state moved");
        assert!(!store.turns_dir().exists(), "the rows moved");
        assert!(!store.segments_dir().exists(), "the segments moved");
        assert!(kept.snapshot_path().exists(), "a live sidecar stays");
        assert!(kept.turns_dir().is_dir(), "a live row dir stays");
        assert!(kept.segments_dir().is_dir(), "a live segment dir stays");
        let aside = dir.join(ORPHAN_DIR);
        let moved: Vec<String> = fs::read_dir(&aside)
            .expect("the orphan dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(moved.iter().any(|name| name == "s-1.snapshot.json"));
        assert!(moved.iter().any(|name| name == "s-1.turns"));
    }

    #[test]
    fn a_second_orphan_of_the_same_name_does_not_replace_the_first() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).expect("a workspace");
        let dir = dir.path().join("sessions");
        let store = Store::open(dir.join("s-1.json"));

        for round in 1..=2 {
            store.save(&session(workspace.clone())).expect("save");
            fs::remove_file(&store.path).expect("remove the header");
            assert_eq!(
                Store::sweep_orphans(&dir).expect("sweep"),
                3,
                "round {round}"
            );
        }

        let moved: Vec<String> = fs::read_dir(dir.join(ORPHAN_DIR))
            .expect("the orphan dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(moved.len(), 6, "{moved:?}");
        assert!(moved.iter().any(|name| name == "s-1.snapshot.json"));
        assert!(
            moved
                .iter()
                .any(|name| name.starts_with("s-1.snapshot.json.")),
            "{moved:?}"
        );
        assert!(moved.iter().any(|name| name == "s-1.segments"));
        assert!(
            moved.iter().any(|name| name.starts_with("s-1.segments.")),
            "{moved:?}"
        );
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
        assert!(!store.segments_dir().exists());
        assert!(store.load(&workspace).expect("load").is_none());
        // Deleting what is already gone is not an error.
        store.delete().expect("delete again");
    }
}
