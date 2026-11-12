//! Per-turn workspace checkpoints in a shadow git repository.
//!
//! The user's repository is never touched: the shadow repo lives under the
//! host state directory and uses the workspace as its work tree. A checkpoint
//! pair lets the desktop restore exactly the files one turn changed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::sessions::workspace_key;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    before: String,
    after: String,
    #[serde(default)]
    undone: bool,
    /// The files the turn changed, captured with the pair. `None` on records
    /// written before this field existed; those are diffed on read.
    #[serde(default)]
    changes: Option<Vec<FileChange>>,
}

/// One path a turn changed, with its line counts. `status` is the short git
/// status letter (`A`, `M`, `D`, `T`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub status: String,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Records {
    #[serde(default)]
    turns: BTreeMap<String, Record>,
}

fn checkpoint_dir(state_dir: &Path, workspace: &Path) -> PathBuf {
    state_dir.join("checkpoints").join(workspace_key(workspace))
}

fn repo_dir(state_dir: &Path, workspace: &Path) -> PathBuf {
    checkpoint_dir(state_dir, workspace).join("repo")
}

fn records_path(state_dir: &Path, workspace: &Path) -> PathBuf {
    checkpoint_dir(state_dir, workspace).join("turns.json")
}

fn key(session_id: &str, seq: u32) -> String {
    format!("{session_id}:{seq}")
}

/// Generated artifacts are not part of a turn's source changes. A render can
/// write thousands of frames into `out/`; checkpointing them makes every
/// snapshot and diff slow and drowns the real edits.
const GENERATED: &[&str] = &[
    ".keel",
    "out",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".svelte-kit",
];

fn excludes() -> String {
    let mut text = String::from("# Generated artifacts are not part of a turn's source changes.\n");
    for name in GENERATED {
        text.push_str(name);
        text.push_str("/\n");
    }
    text
}

/// True for a path inside one of the generated top-level directories.
fn is_generated(path: &str) -> bool {
    let first = path.split(['/', '\\']).next().unwrap_or(path);
    GENERATED.contains(&first)
}

fn filter_generated(changes: Vec<FileChange>) -> Vec<FileChange> {
    changes
        .into_iter()
        .filter(|change| !is_generated(&change.path))
        .collect()
}

/// A GUI host has no console; git must not flash one for each checkpoint.
fn quiet_git() -> Command {
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Commits the current workspace and returns the commit id. The first call
/// initializes the shadow repository.
pub fn snapshot(workspace: &Path, state_dir: &Path) -> Result<String, String> {
    // A state directory inside the work tree would be added to its own
    // checkpoint (and can contain the shadow repo). Disable checkpoints for
    // that layout instead of snapshotting host state into the project.
    let state = std::fs::canonicalize(state_dir).unwrap_or_else(|_| state_dir.to_path_buf());
    let work = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    if state.starts_with(&work) {
        return Err("状态目录位于工作区内，无法建立检查点".to_owned());
    }
    let git_dir = ensure_repo(workspace, state_dir)?;
    run_git(&git_dir, workspace, &["add", "-A"])?;
    run_git(
        &git_dir,
        workspace,
        &["commit", "--allow-empty", "--quiet", "-m", "checkpoint"],
    )?;
    run_git(&git_dir, workspace, &["rev-parse", "HEAD"])
}

/// Stores the two commits that bound one turn. A checkpoint failure must not
/// fail the turn; the caller ignores the result.
pub fn record(
    state_dir: &Path,
    workspace: &Path,
    session_id: &str,
    seq: u32,
    before: &str,
    after: &str,
) -> Result<(), String> {
    let path = records_path(state_dir, workspace);
    let mut records = load_records(&path)?;
    // Capture the changed files with the pair: a history page then reads
    // them from the record instead of diffing every turn on every scroll.
    let git_dir = repo_dir(state_dir, workspace).join(".git");
    let changes = diff_changes(&git_dir, workspace, before, after).ok();
    records.turns.insert(
        key(session_id, seq),
        Record {
            before: before.to_owned(),
            after: after.to_owned(),
            undone: false,
            changes,
        },
    );
    save_records(&path, &records)
}

/// Restores every path that differs between the turn's two commits. The
/// record is marked undone only after the whole restore succeeded.
pub fn undo(state_dir: &Path, workspace: &Path, session_id: &str, seq: u32) -> Result<(), String> {
    let path = records_path(state_dir, workspace);
    let mut records = load_records(&path)?;
    let entry = records
        .turns
        .get(&key(session_id, seq))
        .cloned()
        .ok_or_else(|| "这一轮没有可撤销的检查点".to_owned())?;
    if entry.undone {
        return Err("这一轮已经撤销过了".to_owned());
    }
    let git_dir = repo_dir(state_dir, workspace).join(".git");
    let changed = run_git(
        &git_dir,
        workspace,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            &entry.before,
            &entry.after,
        ],
    )?;
    let mut restored = 0usize;
    for changed_path in changed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if is_generated(changed_path) {
            continue;
        }
        let in_before = run_git(
            &git_dir,
            workspace,
            &[
                "cat-file",
                "-e",
                &format!("{}:{changed_path}", entry.before),
            ],
        )
        .is_ok();
        if in_before {
            run_git(
                &git_dir,
                workspace,
                &["checkout", &entry.before, "--", changed_path],
            )?;
        } else {
            let target = workspace.join(changed_path);
            match std::fs::remove_file(&target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!("无法删除 {}：{error}", target.display()));
                }
            }
        }
        restored += 1;
    }
    if restored == 0 {
        return Err("本轮没有文件变化".to_owned());
    }
    if let Some(entry) = records.turns.get_mut(&key(session_id, seq)) {
        entry.undone = true;
    }
    save_records(&path, &records)
}

/// The paths one turn changed between its two checkpoints. A turn with no
/// record, a failed snapshot, or an already undone turn has nothing to show.
pub fn changes(
    state_dir: &Path,
    workspace: &Path,
    session_id: &str,
    seq: u32,
) -> Result<Vec<FileChange>, String> {
    let records = load_records(&records_path(state_dir, workspace))?;
    let Some(entry) = records.turns.get(&key(session_id, seq)) else {
        return Ok(Vec::new());
    };
    if entry.undone {
        return Ok(Vec::new());
    }
    if let Some(changes) = &entry.changes {
        return Ok(filter_generated(changes.clone()));
    }
    let git_dir = repo_dir(state_dir, workspace).join(".git");
    diff_changes(&git_dir, workspace, &entry.before, &entry.after)
}

/// Every recorded turn of one session, keyed by seq. A history page reads
/// this once instead of diffing each turn separately.
pub fn changes_for_session(
    state_dir: &Path,
    workspace: &Path,
    session_id: &str,
) -> BTreeMap<u32, Vec<FileChange>> {
    let Ok(records) = load_records(&records_path(state_dir, workspace)) else {
        return BTreeMap::new();
    };
    let git_dir = repo_dir(state_dir, workspace).join(".git");
    let mut out = BTreeMap::new();
    for (key, entry) in &records.turns {
        let Some((sid, seq)) = key.rsplit_once(':') else {
            continue;
        };
        if sid != session_id {
            continue;
        }
        let Ok(seq) = seq.parse::<u32>() else {
            continue;
        };
        if entry.undone {
            continue;
        }
        let changes = match &entry.changes {
            Some(changes) => filter_generated(changes.clone()),
            None => {
                diff_changes(&git_dir, workspace, &entry.before, &entry.after).unwrap_or_default()
            }
        };
        out.insert(seq, changes);
    }
    out
}

fn diff_changes(
    git_dir: &Path,
    workspace: &Path,
    before: &str,
    after: &str,
) -> Result<Vec<FileChange>, String> {
    let numstat = run_git(
        git_dir,
        workspace,
        &["diff", "--numstat", "--no-renames", before, after],
    )?;
    let names = run_git(
        git_dir,
        workspace,
        &["diff", "--name-status", "--no-renames", before, after],
    )?;
    let statuses: BTreeMap<String, String> = names
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(status, path)| (path.to_owned(), status.to_owned()))
        .collect();
    Ok(filter_generated(
        numstat
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                let added = parts.next()?;
                let removed = parts.next()?;
                let path = parts.next()?;
                Some(FileChange {
                    path: path.to_owned(),
                    status: statuses
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| "M".to_owned()),
                    // Binary files report `-`; they still count as changed.
                    added: added.parse().unwrap_or(0),
                    removed: removed.parse().unwrap_or(0),
                })
            })
            .collect(),
    ))
}

fn ensure_repo(workspace: &Path, state_dir: &Path) -> Result<PathBuf, String> {
    let repo = repo_dir(state_dir, workspace);
    let git_dir = repo.join(".git");
    if git_dir.is_dir() {
        write_excludes(&git_dir, workspace)?;
        return Ok(git_dir);
    }
    std::fs::create_dir_all(&repo)
        .map_err(|error| format!("无法创建检查点目录 {}：{error}", repo.display()))?;
    let output = quiet_git()
        .arg("init")
        .arg("--quiet")
        .arg(&repo)
        .output()
        .map_err(|error| format!("无法运行 git：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git init 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    for (name, value) in [
        ("user.name", "Keelson"),
        ("user.email", "keelson@localhost"),
        ("core.autocrlf", "false"),
        ("core.safecrlf", "false"),
        ("core.quotepath", "false"),
    ] {
        run_git(&git_dir, workspace, &["config", name, value])?;
    }
    write_excludes(&git_dir, workspace)?;
    Ok(git_dir)
}

/// Writes the shadow repo's exclude list, and drops newly excluded paths from
/// the index once. The working tree is untouched.
fn write_excludes(git_dir: &Path, workspace: &Path) -> Result<(), String> {
    let info = git_dir.join("info");
    std::fs::create_dir_all(&info).map_err(|error| format!("无法创建检查点规则目录：{error}"))?;
    let path = info.join("exclude");
    let text = excludes();
    if std::fs::read_to_string(&path).unwrap_or_default() == text {
        return Ok(());
    }
    std::fs::write(&path, text).map_err(|error| format!("无法写入检查点排除规则：{error}"))?;
    for ignored in GENERATED {
        run_git(
            git_dir,
            workspace,
            &[
                "rm",
                "-r",
                "--cached",
                "--quiet",
                "--ignore-unmatch",
                "--",
                ignored,
            ],
        )?;
    }
    Ok(())
}

fn run_git(git_dir: &Path, work_tree: &Path, args: &[&str]) -> Result<String, String> {
    let output = quiet_git()
        .arg(format!("--git-dir={}", git_dir.display()))
        .arg(format!("--work-tree={}", work_tree.display()))
        .args(args)
        .output()
        .map_err(|error| format!("无法运行 git：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} 失败：{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn load_records(path: &Path) -> Result<Records, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| format!("检查点索引损坏：{error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Records::default()),
        Err(error) => Err(format!("无法读取检查点索引：{error}")),
    }
}

fn save_records(path: &Path, records: &Records) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建检查点目录：{error}"))?;
    }
    let text = serde_json::to_string_pretty(records)
        .map_err(|error| format!("无法序列化检查点索引：{error}"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|error| format!("无法写入检查点索引：{error}"))?;
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&temporary, path).map_err(|error| format!("无法替换检查点索引：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn a_turn_can_be_undone() {
        if !git_available() {
            return;
        }
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        let state_dir = dir.path().join("state");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::write(workspace.join("notes.txt"), "one\n").expect("notes");
        std::fs::write(workspace.join("old.txt"), "old\n").expect("old");

        let before = snapshot(&workspace, &state_dir).expect("the first snapshot");
        std::fs::write(workspace.join("notes.txt"), "two\n").expect("edit");
        std::fs::remove_file(workspace.join("old.txt")).expect("delete");
        std::fs::write(workspace.join("new.txt"), "new\n").expect("create");
        let after = snapshot(&workspace, &state_dir).expect("the second snapshot");
        record(&state_dir, &workspace, "s-1", 1, &before, &after).expect("record");

        undo(&state_dir, &workspace, "s-1", 1).expect("undo");

        assert_eq!(
            std::fs::read_to_string(workspace.join("notes.txt")).expect("notes"),
            "one\n"
        );
        assert_eq!(
            std::fs::read_to_string(workspace.join("old.txt")).expect("old"),
            "old\n"
        );
        assert!(!workspace.join("new.txt").exists());
        assert!(undo(&state_dir, &workspace, "s-1", 1).is_err());
    }

    #[test]
    fn generated_directories_are_not_checkpointed() {
        if !git_available() {
            return;
        }
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        let state_dir = dir.path().join("state");
        std::fs::create_dir_all(workspace.join("out/frames")).expect("workspace");
        std::fs::write(workspace.join("out/frames/0000.png"), "one").expect("a frame");
        std::fs::write(workspace.join("notes.txt"), "one\n").expect("notes");

        let before = snapshot(&workspace, &state_dir).expect("the first snapshot");
        std::fs::write(workspace.join("out/frames/0000.png"), "two").expect("the frame edit");
        std::fs::write(workspace.join("notes.txt"), "two\n").expect("the edit");
        let after = snapshot(&workspace, &state_dir).expect("the second snapshot");
        record(&state_dir, &workspace, "s-1", 1, &before, &after).expect("record");

        let stats = changes(&state_dir, &workspace, "s-1", 1).expect("the diff stats");
        assert_eq!(stats.len(), 1, "{stats:?}");
        assert_eq!(stats[0].path, "notes.txt");
    }

    #[test]
    fn a_turn_reports_the_files_it_changed() {
        if !git_available() {
            return;
        }
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        let state_dir = dir.path().join("state");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::write(workspace.join("notes.txt"), "one\n").expect("notes");
        std::fs::write(workspace.join("old.txt"), "old\n").expect("old");

        let before = snapshot(&workspace, &state_dir).expect("the first snapshot");
        std::fs::write(workspace.join("notes.txt"), "two\nthree\n").expect("edit");
        std::fs::remove_file(workspace.join("old.txt")).expect("delete");
        std::fs::write(workspace.join("new.txt"), "new\n").expect("create");
        let after = snapshot(&workspace, &state_dir).expect("the second snapshot");
        record(&state_dir, &workspace, "s-1", 1, &before, &after).expect("record");

        let stats = changes(&state_dir, &workspace, "s-1", 1).expect("the diff stats");
        let by_path: BTreeMap<&str, &FileChange> = stats
            .iter()
            .map(|change| (change.path.as_str(), change))
            .collect();
        assert_eq!(stats.len(), 3);
        let notes = by_path.get("notes.txt").expect("the edit");
        assert_eq!(notes.status, "M");
        assert_eq!((notes.added, notes.removed), (2, 1));
        assert_eq!(by_path.get("new.txt").expect("the create").status, "A");
        assert_eq!(by_path.get("old.txt").expect("the delete").status, "D");

        undo(&state_dir, &workspace, "s-1", 1).expect("undo");
        assert!(
            changes(&state_dir, &workspace, "s-1", 1)
                .expect("the diff stats")
                .is_empty(),
            "an undone turn has nothing left to show"
        );
    }

    #[test]
    fn a_state_dir_inside_the_workspace_is_refused() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        let state_dir = workspace.join(".keel");
        std::fs::create_dir_all(&state_dir).expect("state");

        let error = snapshot(&workspace, &state_dir).unwrap_err();

        assert!(error.contains("状态目录"), "{error}");
    }

    #[test]
    fn undo_without_a_record_is_refused() {
        if !git_available() {
            return;
        }
        let dir = tempfile::TempDir::new().expect("a temp dir");
        let workspace = dir.path().join("workspace");
        let state_dir = dir.path().join("state");
        std::fs::create_dir_all(&workspace).expect("workspace");

        let error = undo(&state_dir, &workspace, "s-1", 1).unwrap_err();

        assert!(error.contains("没有可撤销"), "{error}");
    }
}
