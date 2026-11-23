//! What a turn cost the window, kept on disk.
//!
//! Two reports of a window at full CPU arrived with nothing attached: the app
//! could render a plausible explanation of any of them — a long answer being
//! parsed again, a thread of a hundred tool rows being laid out, a build the
//! turn itself started — and no fact that chose between them. A profiler
//! answers that for one run; what is needed afterwards is a record of the turn
//! as the window lived it.
//!
//! So the window writes its own account: the number of streamed batches it
//! received, the milliseconds it spent inside a markdown sample, the frames the
//! browser could not deliver, how much of the thread was in the page. The host
//! keeps the files in `<state>/perf/`, bounded, one per turn — a mid-flight
//! account that overwrites itself and one written when the turn ends.

use std::path::{Path, PathBuf};

/// Accounts kept. One report is a few kilobytes, and what matters is the turn
/// that just misbehaved, not a history of the session.
pub const FILES_KEPT: usize = 40;

/// Where a workspace's state directory keeps the accounts.
pub fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("perf")
}

/// Writes one account and drops the oldest beyond [`FILES_KEPT`].
///
/// The name carries the phase, so the account of a turn in flight is written
/// over as it progresses while the account of its end stays.
pub fn write(dir: &Path, seq: u32, phase: &str, report: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("turn-{seq:04}-{}.json", safe_phase(phase)));
    std::fs::write(&path, report)?;
    prune(dir);
    Ok(path)
}

/// The phase becomes part of a file name, so it is reduced to letters and
/// digits: the name is chosen by the window, not by the model.
fn safe_phase(phase: &str) -> String {
    let cleaned: String = phase
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    if cleaned.is_empty() {
        "phase".to_owned()
    } else {
        cleaned
    }
}

/// Keeps the newest [`FILES_KEPT`] accounts.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, path))
        })
        .collect();
    if files.len() <= FILES_KEPT {
        return;
    }
    files.sort_by_key(|(when, _)| *when);
    let excess = files.len() - FILES_KEPT;
    for (_, path) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keelson-perf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_phase_becomes_a_plain_file_name() {
        assert_eq!(safe_phase("during"), "during");
        assert_eq!(safe_phase("../escape"), "escape");
        assert_eq!(safe_phase(""), "phase");
        assert_eq!(safe_phase("a/b:c*d"), "abcd");
    }

    #[test]
    fn the_mid_flight_account_is_written_over() {
        let dir = scratch("overwrite");
        let first = write(&dir, 7, "during", r#"{"a":1}"#).unwrap();
        let second = write(&dir, 7, "during", r#"{"a":2}"#).unwrap();
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), r#"{"a":2}"#);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_newest_accounts_survive() {
        let dir = scratch("prune");
        for seq in 0..(FILES_KEPT as u32 + 5) {
            write(&dir, seq, "end", "{}").unwrap();
            // The file system's timestamp is coarse; a distinct mtime per file
            // is what the prune orders by.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), FILES_KEPT);
        assert!(!dir.join("turn-0000-end.json").exists());
        assert!(dir
            .join(format!("turn-{:04}-end.json", FILES_KEPT as u32 + 4))
            .exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
