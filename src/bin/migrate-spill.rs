//! Moves spills written before the per-conversation split into the folder the
//! conversation they belong to now owns, and points the references at the new
//! place.
//!
//! Before the split, every conversation of one workspace wrote
//! `outputs/turn-<n>-call-<m>.txt`, so a later turn overwrote an earlier
//! conversation's file. The rows that mention such a path still resolve (the
//! project's `outputs/` stays the permission root), but two conversations
//! cannot both keep the content. This reads the session files, finds who
//! mentions each old file, copies it into every owner's folder and rewrites the
//! mention. A file nobody mentions is left where it is and reported.
//!
//! A conversation that already holds a file of that name keeps its own copy: the
//! original then stays in place as well, because it is still the only copy of
//! what an older session mentioned. Nothing is overwritten.
//!
//! Dry run by default; `--apply` writes.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Summary {
    moved: usize,
    fresh: usize,
    identical: usize,
    taken: usize,
    rewrites: usize,
    kept: usize,
}

/// What the destination folder already holds under that name.
enum Place {
    /// Nothing there: the copy has to be written.
    Fresh,
    /// The same bytes are already in place: only the mention needs updating.
    Identical,
    /// A different file of that name lives there: keep both.
    Taken,
}

fn main() {
    let mut apply = false;
    let mut state: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--apply" => apply = true,
            "--state" => match args.next() {
                Some(value) => state = Some(PathBuf::from(value)),
                None => fail("--state needs a directory"),
            },
            "--help" | "-h" => {
                println!("usage: migrate-spill [--apply] [--state <dir>]");
                return;
            }
            other => fail(&format!("unknown argument `{other}`")),
        }
    }
    let state = state.unwrap_or_else(default_state_dir);
    if !state.is_dir() {
        fail(&format!("{} is not a directory", state.display()));
    }

    let plan = scan(&state);
    if plan.is_empty() {
        println!("no pre-split spill found under {}", state.display());
        return;
    }
    let summary = if apply { write(&plan) } else { report(&plan) };
    println!(
        "\n{} spill file(s): {} moved, {} already in place, {} name clash(es) left as they are, \
         {} unmentioned, {} mention(s) rewritten",
        plan.len(),
        summary.moved,
        summary.identical,
        summary.taken,
        summary.kept,
        summary.rewrites
    );
    if !apply {
        println!("dry run: pass --apply to move these files");
    } else if summary.fresh + summary.identical + summary.taken > 0 {
        println!(
            "{} copy(ies) written into conversation folders",
            summary.fresh
        );
    }
}

fn fail(message: &str) -> ! {
    eprintln!("migrate-spill: {message}");
    std::process::exit(2);
}

fn default_state_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".keelson")
}

/// One old spill and the conversations that mention it.
struct Plan {
    project: PathBuf,
    file: PathBuf,
    /// Session id to the files of that session that contain the mention.
    owners: BTreeMap<String, Vec<PathBuf>>,
}

impl Plan {
    fn name(&self) -> String {
        self.file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn target(&self, session: &str) -> PathBuf {
        self.project.join("outputs").join(session).join(self.name())
    }

    fn place(&self, session: &str, bytes: &[u8]) -> Place {
        let target = self.target(session);
        match fs::read(&target) {
            Ok(existing) if existing == bytes => Place::Identical,
            Ok(_) => Place::Taken,
            Err(_) => Place::Fresh,
        }
    }
}

fn scan(state: &Path) -> Vec<Plan> {
    let sessions = state.join("sessions");
    let mut texts: Vec<(PathBuf, String)> = Vec::new();
    collect_utf8(&sessions, &mut texts);

    let projects = state.join("projects");
    let Ok(entries) = fs::read_dir(&projects) else {
        return Vec::new();
    };
    let mut plan = Vec::new();
    for entry in entries.flatten() {
        let project = entry.path();
        let outputs = project.join("outputs");
        let Ok(files) = fs::read_dir(&outputs) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if !path.is_file() || !looks_like_spill(&path) {
                continue;
            }
            let needle = path.to_string_lossy().into_owned();
            let mut owners: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
            for (session_file, text) in &texts {
                if !text.contains(&needle) {
                    continue;
                }
                let Some(session) = session_of(session_file) else {
                    continue;
                };
                owners
                    .entry(session)
                    .or_default()
                    .push(session_file.clone());
            }
            plan.push(Plan {
                project: project.clone(),
                file: path,
                owners,
            });
        }
    }
    plan.sort_by(|a, b| a.file.cmp(&b.file));
    plan
}

/// `turn-3-call-1.txt` — the pre-split name. Anything in a session folder is
/// already in its own place and is not a candidate.
fn looks_like_spill(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(rest) = name.strip_prefix("turn-") else {
        return false;
    };
    let Some((turn, rest)) = rest.split_once("-call-") else {
        return false;
    };
    let Some(call) = rest.strip_suffix(".txt") else {
        return false;
    };
    !turn.is_empty()
        && !call.is_empty()
        && turn.chars().all(|c| c.is_ascii_digit())
        && call.chars().all(|c| c.is_ascii_digit())
}

/// The session id a file belongs to: `s-18e8.turns/00000011.json` and
/// `s-18e8.segments/00000003.json` both name `s-18e8`.
fn session_of(path: &Path) -> Option<String> {
    // A file inside `s-x.turns/` or `s-x.segments/` takes the folder's name.
    for parent in path.ancestors().skip(1) {
        let Some(name) = parent.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for suffix in [".turns", ".segments"] {
            if let Some(stem) = name.strip_suffix(suffix) {
                return Some(stem.to_owned());
            }
        }
    }
    // Otherwise the file itself is the document: `s-x.json`, `s-x.snapshot.json`.
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".json")?;
    Some(stem.trim_end_matches(".snapshot").to_owned())
}

fn collect_utf8(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_utf8(&path, out);
        } else if let Ok(text) = fs::read_to_string(&path) {
            out.push((path, text));
        }
    }
}

fn report(plan: &[Plan]) -> Summary {
    let mut summary = Summary::default();
    let mut shared = 0;
    for item in plan {
        if item.owners.is_empty() {
            summary.kept += 1;
            println!("  keep    {} (no session mentions it)", item.file.display());
            continue;
        }
        let Ok(bytes) = fs::read(&item.file) else {
            println!("  keep    {} (cannot read it)", item.file.display());
            continue;
        };
        if item.owners.len() > 1 {
            shared += 1;
        }
        summary.rewrites += item.owners.values().map(Vec::len).sum::<usize>();
        let mut kinds = Vec::new();
        let mut fresh = false;
        let mut identical = false;
        let mut taken = false;
        for session in item.owners.keys() {
            match item.place(session, &bytes) {
                Place::Fresh => {
                    fresh = true;
                    summary.fresh += 1;
                    kinds.push(format!("{session} (new)"));
                }
                Place::Identical => {
                    identical = true;
                    summary.identical += 1;
                    kinds.push(format!("{session} (already there)"));
                }
                Place::Taken => {
                    taken = true;
                    summary.taken += 1;
                    kinds.push(format!("{session} (name taken, kept)"));
                }
            }
        }
        if fresh && !taken {
            summary.moved += 1;
        }
        println!(
            "  {:<7} {} -> {}",
            if taken && !fresh {
                "keep"
            } else if fresh {
                "move"
            } else {
                "repoint"
            },
            item.file.display(),
            kinds.join(", ")
        );
        let _ = (fresh, identical);
    }
    if shared > 0 {
        println!("\n{shared} file(s) are mentioned by more than one conversation");
    }
    summary
}

fn write(plan: &[Plan]) -> Summary {
    let mut summary = Summary::default();
    let mut clashes = BTreeSet::new();
    for item in plan {
        if item.owners.is_empty() {
            summary.kept += 1;
            continue;
        }
        let bytes = match fs::read(&item.file) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("  skip    {}: {error}", item.file.display());
                continue;
            }
        };
        let mut blocked = false;
        let mut fresh = false;
        let mut repointed = false;
        for (session, files) in &item.owners {
            let target = item.target(session);
            match item.place(session, &bytes) {
                Place::Identical => {
                    summary.identical += 1;
                    repointed = true;
                }
                Place::Taken => {
                    // The conversation's own turn already wrote a file with
                    // this name. Its copy stays; the original is still the only
                    // copy of the older content, so it stays too.
                    summary.taken += 1;
                    blocked = true;
                    clashes.insert(target.clone());
                    continue;
                }
                Place::Fresh => {
                    if let Some(parent) = target.parent() {
                        if let Err(error) = fs::create_dir_all(parent) {
                            eprintln!("  skip    {}: {error}", target.display());
                            blocked = true;
                            continue;
                        }
                    }
                    if let Err(error) = fs::write(&target, &bytes) {
                        eprintln!("  skip    {}: {error}", target.display());
                        blocked = true;
                        continue;
                    }
                    summary.fresh += 1;
                    fresh = true;
                }
            }
            for file in files {
                if rewrite(file, &item.file, &target) {
                    summary.rewrites += 1;
                }
            }
        }
        if blocked {
            println!(
                "  keep    {} (a conversation already holds that name)",
                item.file.display()
            );
            continue;
        }
        if fresh || repointed {
            match fs::remove_file(&item.file) {
                Ok(()) => summary.moved += 1,
                Err(error) => eprintln!("  skip    {}: {error}", item.file.display()),
            }
        }
    }
    for target in clashes {
        println!("  clash   {}", target.display());
    }
    summary
}

/// Replaces the old path with the new one. Writes through a temporary file so
/// a crash cannot leave half a session document behind.
fn rewrite(file: &Path, from: &Path, to: &Path) -> bool {
    let Ok(text) = fs::read_to_string(file) else {
        return false;
    };
    let from = from.to_string_lossy();
    let to = to.to_string_lossy();
    if !text.contains(from.as_ref()) {
        return false;
    }
    let updated = text.replace(from.as_ref(), to.as_ref());
    let temporary = file.with_extension("migrating");
    if let Err(error) = fs::write(&temporary, updated) {
        eprintln!("  skip    {}: {error}", file.display());
        return false;
    }
    if let Err(error) = fs::rename(&temporary, file) {
        eprintln!("  skip    {}: {error}", file.display());
        let _ = fs::remove_file(&temporary);
        return false;
    }
    true
}
