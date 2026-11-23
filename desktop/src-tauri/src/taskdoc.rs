//! One task document per turn (ADR 0064).
//!
//! The step log records *how* a turn ran; these documents record *what it left
//! behind*. The reader is the next agent as much as the user: before starting
//! work it opens `INDEX.md`, finds the one-line row that matches what it was
//! asked, and reads that one file. So the unit is one file per task rather than
//! one growing document — a reader pays for the row, not for the whole history.
//!
//! Two shapes, one directory:
//!
//! * `<utc>-t<turn>-<slug>.md` — a flat `key: value` header (stable keys, so a
//!   reader parses it without guessing) and a fixed set of sections: Task,
//!   Result, Changes, Git, Next. The file name starts with UTC time, so a plain
//!   directory listing is already in chronological order.
//! * `INDEX.md` — one row per task, newest first. It is a cache: every row is
//!   read back from the task files, so deleting it costs nothing and a hand edit
//!   is not a source of truth. Its only job is to be cheap to scan.
//!
//! Nothing is deleted for age. The step log holds the events, these hold the
//! summaries, and a checkout that keeps its own records is a checkout whose
//! history survives a new conversation.
//!
//! `## Next` is the handoff slot. The host never fills it and never overwrites
//! what it finds there: rewriting a task (a retried turn) carries the section
//! over verbatim, so a note the user or an agent left is not lost.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::checkpoints::FileChange;

/// The index file's name inside the progress directory.
pub const INDEX_FILE: &str = "INDEX.md";

/// How many task files the index lists. Older ones are not lost — they are in
/// the same directory, and the index says how many it left out. A cap exists
/// because the index is read on every continuation; the directory is not.
pub const INDEX_ROWS: usize = 60;

/// The handoff slot's placeholder, replaced by whatever the user writes.
const PLACEHOLDER: &str = "\
<!-- 下一个接手的人需要知道什么：改到一半的、不能碰的、下一步做什么。 \
Keelson 重建本文件时保留这一段。 -->";

/// How much of the user's input the task section quotes.
const EXCERPT_LINES: usize = 16;
const EXCERPT_CHARS: usize = 1200;

/// How long the file name's slug may get.
const SLUG_CHARS: usize = 32;

/// How many changed paths a document lists before it summarises.
const LISTED_CHANGES: usize = 60;

/// Where the workspace's own repository stood when the turn ended.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitState {
    /// False when the workspace is not a repository at all, which is a normal
    /// state for a scratch directory and not an error.
    pub repo: bool,
    pub branch: String,
    pub head: String,
    pub subject: String,
    pub dirty: usize,
}

/// How a document stands. A turn writes its document twice under one
/// identity: `Running` the moment it starts, then `Done` or `Failed` when it
/// ends, in the same file (ADR 0064 and its amendment). A `Running` document
/// that nobody rewrote is a turn the app did not survive — the host does not
/// guess what happened to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Running,
    Done,
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Done => "done",
            Status::Failed => "failed",
        }
    }
}

/// Everything one document needs. Borrowed: the caller already owns these.
pub struct Entry<'a> {
    pub at_ms: u64,
    pub session: &'a str,
    pub turn: u32,
    pub input: &'a str,
    pub rig: &'a str,
    pub model: &'a str,
    pub provider: &'a str,
    pub status: Status,
    pub error: Option<&'a str>,
    pub model_calls: Option<u64>,
    pub tools: Option<usize>,
    pub duration_ms: Option<u64>,
    pub changes: &'a [FileChange],
    pub git: &'a GitState,
}

/// The workspace's git state, or a "no repository" value.
///
/// One `git status` per turn, asked at the moment the turn ends, because the
/// useful answer is "where is the tree *now*", not where it was.
pub fn git_state(workspace: &Path) -> GitState {
    let Some(branch) = run(workspace, &["rev-parse", "--abbrev-ref", "HEAD"]) else {
        return GitState::default();
    };
    GitState {
        repo: true,
        branch,
        head: run(workspace, &["rev-parse", "--short", "HEAD"])
            .unwrap_or_else(|| "无提交".to_owned()),
        subject: run(workspace, &["log", "-1", "--pretty=%s"]).unwrap_or_default(),
        dirty: run(workspace, &["status", "--porcelain"])
            .map(|out| out.lines().filter(|line| !line.trim().is_empty()).count())
            .unwrap_or(0),
    }
}

/// Runs one read-only git command in `workspace`. `--no-optional-locks` keeps
/// the status call from taking `index.lock` away from a build in progress.
fn run(workspace: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Writes this turn's document and refreshes the index.
///
/// `dir` is the progress directory. A turn written twice (the window reopened,
/// the hook retried) replaces its own file instead of leaving two, and the
/// handoff note survives the replacement.
pub fn write_task(dir: &Path, entry: &Entry<'_>) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let id = task_id(entry.session, entry.turn);
    let previous = find_task(dir, &id);
    let path = dir.join(file_name(entry));
    let notes = previous.as_ref().map(|old| old.next.as_str());
    fs::write(&path, render(entry, notes))?;
    if let Some(old) = previous {
        if old.path != path {
            // Same task, new time in the name: the older file is the same
            // document under a name that sorts before the current one.
            fs::remove_file(&old.path)?;
        }
    }
    rebuild_index(dir)?;
    Ok(path)
}

/// Rebuilds `INDEX.md` from the task files, newest first.
///
/// The index is derived, so it is safe to delete: this call recreates it, and
/// nothing else reads it as a source of truth.
pub fn rebuild_index(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut rows = scan(dir);
    rows.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then(b.turn.cmp(&a.turn)));

    let mut text = String::from("# 任务索引\n\n");
    text.push_str(
        "本目录每个回合一个文件，文件名以 UTC 时间开头。本索引由 Keelson 在每次任务结束时\
从这些文件重建，手改会丢；要留下交接说明，请写进对应文件的 `## Next` 段。完整的事件\
日志是同级的 `../journal.db`。\n\n",
    );
    if let Some(newest) = rows.first() {
        text.push_str(&format!("最新任务：`{}`\n\n", newest.name));
    }
    text.push_str(&format!(
        "共 {} 个任务，下面列出最近 {} 个，新的在上面。\n\n",
        rows.len(),
        rows.len().min(INDEX_ROWS)
    ));

    if rows.is_empty() {
        text.push_str("还没有记录。\n");
        fs::write(dir.join(INDEX_FILE), text)?;
        return Ok(dir.join(INDEX_FILE));
    }

    text.push_str("| UTC | 轮 | 状态 | 任务 | 文件 | 文档 |\n|---|---|---|---|---|---|\n");
    for row in rows.iter().take(INDEX_ROWS) {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | [t{}]({}) |\n",
            stamp(row.at_ms),
            row.turn,
            row.status,
            cell(&row.goal),
            row.files_changed,
            row.turn,
            row.name
        ));
    }
    if rows.len() > INDEX_ROWS {
        text.push_str(&format!(
            "\n更早的 {} 个任务仍在本目录里，文件名早于上面最后一个。\n",
            rows.len() - INDEX_ROWS
        ));
    }

    let path = dir.join(INDEX_FILE);
    fs::write(&path, text)?;
    Ok(path)
}

/// One task file as the index needs it: the header keys plus the title line.
struct Row {
    name: String,
    at_ms: u64,
    turn: u32,
    status: String,
    goal: String,
    files_changed: usize,
}

fn scan(dir: &Path) -> Vec<Row> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == INDEX_FILE {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        rows.push(Row {
            name,
            at_ms: header(&text, "at_ms")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            turn: header(&text, "turn")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            status: header(&text, "status").unwrap_or_default(),
            goal: title(&text),
            files_changed: header(&text, "files_changed")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        });
    }
    rows
}

/// The value of one `key: value` line in a document's header block, unquoted.
fn header(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}: ");
    for line in text.lines() {
        if line == "---" {
            continue;
        }
        if let Some(value) = line.strip_prefix(&prefix) {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .unwrap_or(value);
            return Some(value.replace("\\\"", "\"").replace("\\\\", "\\"));
        }
        if line.starts_with("# ") {
            // The header is over: a later `key: value` would be prose.
            return None;
        }
    }
    None
}

/// The `# ` title of a document, which is the task in one line.
fn title(text: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("# "))
        .unwrap_or("")
        .trim()
        .to_owned()
}

/// What an earlier write of the same task left in `## Next`.
struct Previous {
    path: PathBuf,
    next: String,
}

/// The file already holding this task, with its handoff note.
fn find_task(dir: &Path, id: &str) -> Option<Previous> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == INDEX_FILE {
            continue;
        }
        let path = entry.path();
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if header(&text, "task").as_deref() == Some(id) {
            return Some(Previous {
                path,
                next: next_section(&text),
            });
        }
    }
    None
}

/// The body of the `## Next` section, placeholder removed.
fn next_section(text: &str) -> String {
    let mut body = String::new();
    let mut inside = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            inside = rest.trim() == "Next";
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    let body = body.trim();
    let body = body.trim_start_matches(PLACEHOLDER).trim();
    if body == "—" {
        return String::new();
    }
    body.to_owned()
}

/// One document as markdown. Separated from the file work so it can be tested
/// without a workspace.
pub fn render(entry: &Entry<'_>, notes: Option<&str>) -> String {
    let mut out = String::from("---\n");
    let fields: [(&str, String); 18] = [
        ("task", quote(&task_id(entry.session, entry.turn))),
        ("turn", entry.turn.to_string()),
        ("session", quote(entry.session)),
        ("at", quote(&iso(entry.at_ms))),
        ("at_ms", entry.at_ms.to_string()),
        ("status", quote(entry.status.as_str())),
        ("rig", quote(entry.rig)),
        ("model", quote(entry.model)),
        ("provider", quote(entry.provider)),
        ("model_calls", number(entry.model_calls)),
        ("tools", number(entry.tools.map(|tools| tools as u64))),
        ("duration_ms", number(entry.duration_ms)),
        ("files_changed", entry.changes.len().to_string()),
        ("workspace_git", entry.git.repo.to_string()),
        ("git_branch", quote(&entry.git.branch)),
        ("git_head", quote(&entry.git.head)),
        ("git_subject", quote(&entry.git.subject)),
        ("git_dirty", entry.git.dirty.to_string()),
    ];
    for (key, value) in fields {
        out.push_str(&format!("{key}: {value}\n"));
    }
    out.push_str("---\n\n");

    out.push_str(&format!("# {}\n", headline(entry.input)));

    out.push_str("\n## Task\n\n");
    out.push_str(&excerpt(entry.input));

    out.push_str("\n## Result\n\n");
    out.push_str(&outcome(entry));
    out.push('\n');

    out.push_str("\n## Changes\n\n");
    if entry.status == Status::Running {
        // The turn has not touched anything yet, and the empty list would be
        // read as "this turn changed nothing".
        out.push_str("回合还在跑，变更在结束时记录。\n");
    } else if entry.changes.is_empty() {
        out.push_str("这轮没有留下文件变更。\n");
    } else {
        out.push_str("| 文件 | 状态 | 增 | 删 |\n|---|---|---|---|\n");
        for change in entry.changes.iter().take(LISTED_CHANGES) {
            out.push_str(&format!(
                "| `{}` | {} | +{} | -{} |\n",
                change.path, change.status, change.added, change.removed
            ));
        }
        if entry.changes.len() > LISTED_CHANGES {
            out.push_str(&format!(
                "\n另有 {} 个文件未列出。\n",
                entry.changes.len() - LISTED_CHANGES
            ));
        }
    }

    out.push_str("\n## Git\n\n");
    out.push_str(&git_line(entry.git, entry.status == Status::Running));
    out.push('\n');

    out.push_str("\n## Next\n\n");
    match notes.map(str::trim) {
        Some(note) if !note.is_empty() => {
            out.push_str(note);
            out.push('\n');
        }
        _ => {
            out.push_str(PLACEHOLDER);
            out.push('\n');
        }
    }
    out
}

/// `session:turn`, the identity a document is found by.
fn task_id(session: &str, turn: u32) -> String {
    format!("{session}:{turn}")
}

/// `20261117T0342Z-t007-fix-the-stop-button.md`.
///
/// Time first so `ls` is chronological; then the turn, so two documents can be
/// tied back to the turn numbers in the step log; then the goal, so a reader
/// scanning a directory can pick without opening anything.
fn file_name(entry: &Entry<'_>) -> String {
    format!(
        "{}-t{:03}-{}.md",
        stamp_file(entry.at_ms),
        entry.turn,
        slug(entry.input)
    )
}

/// The goal reduced to something a file system accepts on every platform.
///
/// Spaces, quotes, slashes, colons and control characters become `-`; letters
/// and digits survive in any script, so a Chinese ask keeps its words instead
/// of collapsing to `task`.
fn slug(input: &str) -> String {
    if input.trim().is_empty() {
        return "task".to_owned();
    }
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in headline(input).chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
        if slug.chars().count() > SLUG_CHARS {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        return "task".to_owned();
    }
    let mut kept: String = slug.chars().take(SLUG_CHARS).collect();
    kept = kept.trim_end_matches('-').to_owned();
    kept
}

/// The task in one line: the first line of the ask, bounded.
fn headline(input: &str) -> String {
    let line = input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();
    if line.is_empty() {
        return "（空输入）".to_owned();
    }
    if line.chars().count() <= 80 {
        return line.to_owned();
    }
    let mut short: String = line.chars().take(80).collect();
    short.push('…');
    short
}

/// A quoted header value. Quoted so a `:` in a model name or a `#` in a subject
/// cannot break the reader, and so the block is still valid YAML if a tool
/// reads it as such.
fn quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' | '\r' => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

fn number(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn outcome(entry: &Entry<'_>) -> String {
    let mut parts = Vec::new();
    match (entry.status, entry.error) {
        (Status::Running, _) => parts.push("进行中".to_owned()),
        (Status::Done, _) => parts.push("完成".to_owned()),
        (Status::Failed, Some(error)) => parts.push(format!("失败 — {error}")),
        (Status::Failed, None) => parts.push("未完成".to_owned()),
    }
    if let Some(calls) = entry.model_calls {
        parts.push(format!("{calls} 次模型调用"));
    }
    if let Some(tools) = entry.tools {
        parts.push(format!("{tools} 次工具调用"));
    }
    if let Some(ms) = entry.duration_ms {
        parts.push(duration(ms));
    }
    parts.join(" · ")
}

fn git_line(git: &GitState, running: bool) -> String {
    if !git.repo {
        return "工作区不是 git 仓库。".to_owned();
    }
    let mut line = format!("`{}` @ `{}`", git.branch, git.head);
    if !git.subject.is_empty() {
        line.push_str(&format!(" \"{}\"", git.subject));
    }
    line.push_str(&if git.dirty == 0 {
        " · 工作区干净。".to_owned()
    } else {
        format!(" · {} 个文件未提交。", git.dirty)
    });
    if running {
        // Measured before the turn touched anything, so it says so.
        return format!("回合开始时：{line}");
    }
    line
}

/// The user's ask, quoted, bounded. A turn's input can be a pasted file; the
/// document is a summary, not a second copy of it.
fn excerpt(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "> （空输入）\n".to_owned();
    }
    let mut text: String = trimmed
        .lines()
        .take(EXCERPT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if text.chars().count() > EXCERPT_CHARS {
        text = text.chars().take(EXCERPT_CHARS).collect();
        text.push('…');
    } else if trimmed.lines().count() > EXCERPT_LINES {
        text.push_str("\n…");
    }
    text.lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>()
}

/// A table cell that cannot break the table.
fn cell(text: &str) -> String {
    let flat = text.replace('|', "\\|").replace('\n', " ");
    if flat.chars().count() <= 60 {
        return flat;
    }
    let mut short: String = flat.chars().take(60).collect();
    short.push('…');
    short
}

fn duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let secs = ms / 1000;
    if secs < 60 {
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return format!("{minutes}m{}s", secs % 60);
    }
    format!("{}h{}m", minutes / 60, minutes % 60)
}

/// The broken-down UTC clock. UTC on purpose: the user's machine clock changes
/// with travel, a record's time should not.
fn parts(ms: u64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

/// `YYYY-MM-DD HH:MM UTC`, for a table cell.
fn stamp(ms: u64) -> String {
    let (year, month, day, hour, minute, _) = parts(ms);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// `20261117T0342Z`, the prefix of a file name.
fn stamp_file(ms: u64) -> String {
    let (year, month, day, hour, minute, _) = parts(ms);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}Z")
}

/// `2026-11-17T03:42:11Z`, the header's `at`.
fn iso(ms: u64) -> String {
    let (year, month, day, hour, minute, second) = parts(ms);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`: the calendar date a signed day count
/// from 1970-01-01 lands on.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { y + 1 } else { y }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry<'a>(changes: &'a [FileChange], git: &'a GitState) -> Entry<'a> {
        Entry {
            at_ms: 1_700_000_000_000,
            session: "s-1",
            turn: 7,
            input: "修一下后台任务的停止按钮",
            rig: "ui",
            model: "doubao-1",
            provider: "volcengine",
            status: Status::Done,
            error: None,
            model_calls: Some(4),
            tools: Some(9),
            duration_ms: Some(81_000),
            changes,
            git,
        }
    }

    fn git() -> GitState {
        GitState {
            repo: true,
            branch: "main".to_owned(),
            head: "a1b2c3d".to_owned(),
            subject: "fix: stop button".to_owned(),
            dirty: 2,
        }
    }

    fn change() -> FileChange {
        FileChange {
            path: "src/app.rs".to_owned(),
            status: "M".to_owned(),
            added: 12,
            removed: 3,
        }
    }

    #[test]
    fn a_document_names_the_task_its_result_and_where_git_stood() {
        let changes = vec![change()];
        let git = git();
        let text = render(&entry(&changes, &git), None);

        assert!(text.starts_with("---\ntask: \"s-1:7\"\n"));
        assert!(text.contains("\nstatus: \"done\"\n"));
        assert!(text.contains("\nat: \"2023-11-14T22:13:20Z\"\n"));
        assert!(text.contains("\nmodel_calls: 4\n"));
        assert!(text.contains("\ngit_dirty: 2\n"));
        assert!(text.contains("\n# 修一下后台任务的停止按钮\n"));
        assert!(text.contains("\n## Task\n\n> 修一下后台任务的停止按钮\n"));
        assert!(text.contains("完成 · 4 次模型调用 · 9 次工具调用 · 1m21s"));
        assert!(text.contains("| `src/app.rs` | M | +12 | -3 |"));
        assert!(text.contains("`main` @ `a1b2c3d` \"fix: stop button\" · 2 个文件未提交"));
        assert!(text.contains("## Next"));
        assert!(text.contains(PLACEHOLDER));
    }

    #[test]
    fn a_failed_turn_is_marked_failed_and_keeps_its_error() {
        let git = GitState::default();
        let mut item = entry(&[], &git);
        item.status = Status::Failed;
        item.error = Some("provider returned 400");
        let text = render(&item, None);

        assert!(text.contains("\nstatus: \"failed\"\n"));
        assert!(text.contains("失败 — provider returned 400"));
        assert!(text.contains("\nworkspace_git: false\n"));
        assert!(text.contains("工作区不是 git 仓库。"));
    }

    #[test]
    fn every_turn_gets_its_own_file_and_the_index_lists_them_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        let mut first = entry(&[], &git);
        first.turn = 7;
        write_task(dir.path(), &first).unwrap();

        let mut second = entry(&[], &git);
        second.turn = 8;
        second.at_ms += 60_000;
        second.input = "继续推进";
        let path = write_task(dir.path(), &second).unwrap();

        assert!(path.ends_with("20231114T2214Z-t008-继续推进.md"));
        assert!(dir
            .path()
            .join("20231114T2213Z-t007-修一下后台任务的停止按钮.md")
            .exists());

        let index = fs::read_to_string(dir.path().join(INDEX_FILE)).unwrap();
        assert!(index.starts_with("# 任务索引\n"));
        assert!(index.contains(&format!(
            "最新任务：`{}`",
            "20231114T2214Z-t008-继续推进.md"
        )));
        assert!(index.contains("共 2 个任务"));
        // Newest first: the turn-8 row is above the turn-7 row.
        let eight = index.find("t008-继续推进.md").unwrap();
        let seven = index.find("t007-修一下后台任务的停止按钮.md").unwrap();
        assert!(eight < seven);
        assert!(index.contains("| 2023-11-14 22:14 UTC | 8 | done | 继续推进 | 0 |"));
    }

    #[test]
    fn the_index_is_a_cache_and_rebuilds_from_the_task_files() {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        write_task(dir.path(), &entry(&[change()], &git)).unwrap();
        fs::remove_file(dir.path().join(INDEX_FILE)).unwrap();

        rebuild_index(dir.path()).unwrap();
        let index = fs::read_to_string(dir.path().join(INDEX_FILE)).unwrap();

        assert!(index.contains("共 1 个任务"));
        assert!(index.contains("| M |") || index.contains("| 1 |"));
        assert!(index.contains("t007-修一下后台任务的停止按钮.md"));
    }

    #[test]
    fn a_rewritten_task_replaces_its_file_and_keeps_the_handoff_note() {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        // A first write, then a hand edit into the handoff slot.
        let path = write_task(dir.path(), &entry(&[], &git)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(PLACEHOLDER));
        fs::write(
            &path,
            text.replace(PLACEHOLDER, "下一步：把设置面板的两行路径截个图。"),
        )
        .unwrap();

        // The same turn is written again, a minute later: one file, not two,
        // and the note is still there.
        let mut again = entry(&[], &git);
        again.at_ms += 60_000;
        let path = write_task(dir.path(), &again).unwrap();

        let remaining: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".md") && name != INDEX_FILE)
            .collect();
        assert_eq!(remaining.len(), 1);

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("下一步：把设置面板的两行路径截个图。"));
        assert!(!text.contains(PLACEHOLDER));
    }

    #[test]
    fn a_placeholder_note_is_not_carried_over_as_if_it_were_written() {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        write_task(dir.path(), &entry(&[], &git)).unwrap();
        let mut again = entry(&[], &git);
        again.at_ms += 1_000;
        let path = write_task(dir.path(), &again).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches(PLACEHOLDER).count(), 1);
    }

    #[test]
    fn a_file_name_survives_the_characters_a_file_system_reacts_to() {
        assert_eq!(slug("修一下后台任务的停止按钮"), "修一下后台任务的停止按钮");
        assert_eq!(slug("fix: the /stop/ button"), "fix-the-stop-button");
        assert_eq!(slug("   "), "task");
        assert_eq!(slug("a\"b\\c|d"), "a-b-c-d");
        assert!(slug(&"长".repeat(200)).chars().count() <= SLUG_CHARS);
        assert_eq!(headline("##  一件小事\n第二行"), "一件小事");
        assert_eq!(headline(""), "（空输入）");
    }

    #[test]
    fn a_document_understands_what_it_wrote() {
        let changes = vec![change()];
        let git = git();
        let text = render(&entry(&changes, &git), Some("接下来跑 clippy"));

        assert_eq!(header(&text, "task").as_deref(), Some("s-1:7"));
        assert_eq!(header(&text, "turn").as_deref(), Some("7"));
        assert_eq!(header(&text, "status").as_deref(), Some("done"));
        assert_eq!(header(&text, "files_changed").as_deref(), Some("1"));
        // A key that is not there is not invented.
        assert_eq!(header(&text, "missing"), None);
        assert_eq!(title(&text), "修一下后台任务的停止按钮");
        assert_eq!(next_section(&text), "接下来跑 clippy");
    }

    #[test]
    fn a_turn_has_its_document_from_the_first_moment() {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        // What `send_message` writes when the turn starts: no result yet.
        let mut started = entry(&[], &git);
        started.status = Status::Running;
        started.model_calls = None;
        started.tools = None;
        started.duration_ms = None;
        let path = write_task(dir.path(), &started).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\nstatus: \"running\"\n"));
        assert!(text.contains("\nmodel_calls: null\n"));
        assert!(text.contains("进行中"));
        assert!(text.contains("回合还在跑，变更在结束时记录。"));
        assert!(text.contains("回合开始时：`main` @ `a1b2c3d`"));
        let index = fs::read_to_string(dir.path().join(INDEX_FILE)).unwrap();
        assert!(index.contains("| running |"));

        // The end of the same turn rewrites that one file, not a second one.
        let changed = vec![change()];
        let finished = entry(&changed, &git);
        let path_again = write_task(dir.path(), &finished).unwrap();
        assert_eq!(path, path_again);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\nstatus: \"done\"\n"));
        assert!(!text.contains("running"));
        let files: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|item| item.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".md") && name != INDEX_FILE)
            .collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn a_timestamp_renders_in_utc() {
        assert_eq!(stamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(stamp(1_700_000_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(stamp_file(1_700_000_000_000), "20231114T2213Z");
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn a_directory_outside_a_repository_reports_none() {
        let dir = tempfile::tempdir().unwrap();
        // A temp dir is not inside a checkout on CI, and the answer must be
        // "no repository" rather than a panic.
        let state = git_state(dir.path());
        if state.repo {
            // Some sandboxes put $TMPDIR inside a repository; then only the
            // branch is knowable and the test has nothing to assert.
            return;
        }
        assert_eq!(state, GitState::default());
    }

    #[test]
    fn the_records_base_holds_a_log_and_a_progress_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (log, index) = crate::records::paths(dir.path());
        assert!(log.ends_with("journal.db"));
        assert!(index.ends_with("INDEX.md"));
        assert!(index.starts_with(crate::records::progress_dir(dir.path())));
    }
}
