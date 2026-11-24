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

use serde_json::{json, Value};

/// Accounts kept. One report is a few kilobytes, and what matters is the turn
/// that just misbehaved, not a history of the session.
pub const FILES_KEPT: usize = 40;

/// Where a workspace's state directory keeps the accounts.
pub fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("perf")
}

/// Writes one account and drops the oldest beyond [`FILES_KEPT`].
///
/// The name carries the conversation, the turn and the phase: two conversations
/// number their turns from one, so without the conversation the account of turn
/// 4 in one project was written over by the turn 4 of whichever conversation ran
/// last, and the account that survived could not be tied back to the turn that
/// produced it. The phase stays in the name so the account of a turn in flight
/// is written over itself while the account of its end stays.
pub fn write(
    dir: &Path,
    session: &str,
    seq: u32,
    phase: &str,
    report: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!(
        "turn-{}-{seq:04}-{}.json",
        safe_session(session),
        safe_phase(phase)
    ));
    std::fs::write(&path, stamped(report))?;
    prune(dir);
    Ok(path)
}

/// Names the build in the account on its way to disk.
///
/// The window cannot say which binary it is: it ships inside the app, its
/// version string is the same in every build, and the account outlives the
/// process that wrote it. The host knows its own stamp, so it adds one, and an
/// account written by an older build stays distinguishable from the one written
/// now (ADR 0084).
///
/// A report that is not a JSON object is written as it arrived. The account is
/// evidence, and losing it to a stamp would be worse than an unstamped one.
fn stamped(report: &str) -> String {
    let Ok(Value::Object(mut fields)) = serde_json::from_str::<Value>(report) else {
        return report.to_owned();
    };
    fields.insert("build".to_owned(), json!(crate::build::ID));
    Value::Object(fields).to_string()
}

/// A conversation id is chosen by the host, and it still becomes part of a file
/// name: letters, digits and dashes survive, everything else is dropped.
fn safe_session(session: &str) -> String {
    let cleaned: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(24)
        .collect();
    if cleaned.is_empty() {
        return "session".to_owned();
    }
    cleaned
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
    fn a_conversation_becomes_part_of_the_file_name() {
        assert_eq!(safe_session("s-18e8b20ffa85b980"), "s-18e8b20ffa85b980");
        assert_eq!(safe_session("../../etc/passwd"), "etcpasswd");
        assert_eq!(safe_session(""), "session");
    }

    #[test]
    fn two_conversations_do_not_write_over_each_other() {
        // Both are on turn 4; the account of each has to survive.
        let dir = scratch("sessions");
        let first = write(&dir, "s-aaa", 4, "end", "{}").unwrap();
        let second = write(&dir, "s-bbb", 4, "end", "{}").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_mid_flight_account_is_written_over() {
        let dir = scratch("overwrite");
        let first = write(&dir, "s-1", 7, "during", r#"{"a":1}"#).unwrap();
        let second = write(&dir, "s-1", 7, "during", r#"{"a":2}"#).unwrap();
        assert_eq!(first, second);
        let text = std::fs::read_to_string(&first).unwrap();
        assert!(text.contains(r#""a":2"#), "{text}");
        assert!(!text.contains(r#""a":1"#), "{text}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_account_names_the_build_that_wrote_it() {
        let dir = scratch("stamp");
        let path = write(
            &dir,
            "s-1",
            3,
            "end",
            r#"{"seq":3,"phase":"end","counters":{}}"#,
        )
        .unwrap();
        let account: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // The window's own account survives and gains the host's stamp.
        assert_eq!(account["seq"], json!(3));
        assert_eq!(account["build"], json!(crate::build::ID));
        assert!(
            crate::build::ID.contains("keelson "),
            "{}",
            crate::build::ID
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_report_that_is_not_an_account_is_kept_as_it_arrived() {
        let dir = scratch("raw");
        let path = write(&dir, "s-1", 4, "end", "not json").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_newest_accounts_survive() {
        let dir = scratch("prune");
        for seq in 0..(FILES_KEPT as u32 + 5) {
            write(&dir, "s-1", seq, "end", "{}").unwrap();
            // The file system's timestamp is coarse; a distinct mtime per file
            // is what the prune orders by.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), FILES_KEPT);
        assert!(!dir.join("turn-s-1-0000-end.json").exists());
        assert!(dir
            .join(format!("turn-s-1-{:04}-end.json", FILES_KEPT as u32 + 4))
            .exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
