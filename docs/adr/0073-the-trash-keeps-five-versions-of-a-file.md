# 0073 — The trash keeps five versions of a file

## Context

Keel grew `TrashStore::prune(&Retention)` and returns a report; it picks no
number (Keel ADR 0073). This machine's trash is the input to that choice:

- 11 projects, 4 446 versions, 122.3 MB, 937 distinct originals.
- Versions per original: min 1, median 2, max 196 — one HTML file was backed up
  196 times, one Rust source 91 times, one markdown file 86 times.
- The oldest version is 10 days old; the newest is minutes old.

Retention is a policy the host owns, because the host is what knows a turn can
be undone and a workspace cannot.

| kept per path | files | bytes |
|---|---|---|
| 3 | 41% | 26% (31.4 MB) |
| **5** | **51%** | **35% (42.6 MB)** |
| 10 | 64% | 48% (59.1 MB) |

## Decision

- `TRASH_VERSIONS = 5`, `TRASH_MAX_AGE = 30 days`, in `src/engine/mod.rs` beside
  the store it applies to.
- Five is two more than the case the trash is for: an edit made twice, where
  four of the five are the states that edit passed through.
- Thirty days is a guard, not a working part of the policy: nothing is older
  than ten days today, so the age bound removes nothing. It exists for the path
  that is written once and then never touched again, where rank alone would keep
  it forever.
- The prune runs **once per process**, from the engine's tool construction, and
  the report is ignored. A full scan opens one small meta file per version; that
  is cheap once and not cheap per turn.
- It runs on the tokio runtime the engine already has, spawned rather than
  awaited, so the first turn does not wait for a scan of the trash. If no
  runtime handle exists, the prune does not run: a process that cannot spawn is
  not a process a turn is running in.

## Consequences

- The first start after this lands removes 2 179 files and about 79.7 MB on this
  machine. That is once, and it is not recoverable: the removed versions are
  older than the five newest of their path.
- A long session does not re-prune. A process left open for a week keeps growing
  its trash; the next start bounds it again. That is the trade for not scanning
  on a timer.
- The numbers are a guess, as asked for, and the place to change them is one
  constant. What the report contains — how many versions and how many bytes a
  prune removed — is not logged anywhere yet; when it becomes interesting, the
  progress record or the perf ring is where it belongs.
- A version removed here is not in any archive behind the trash. `restore_file`
  keeps working for what remains.
