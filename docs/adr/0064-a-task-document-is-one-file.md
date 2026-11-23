# 0064 — A task document is one file, and the index is a cache

Status: accepted

Supersedes the form of the progress document in 0063. Amends 0063's second
artifact only; the step log is untouched.

## Context

0063 gave a workspace one `progress.md` with a block per finished task, newest
first, capped at twenty. Two things went wrong with that shape.

It was read by the wrong reader. The document's purpose is to let work be
picked back up — by the user, and increasingly by the agent itself, which starts
a follow-up request with no memory of the request before it. For that reader a
single growing file is the wrong unit: every question costs the whole file, and
the older it gets the more it costs. The reader has one question ("what did the
last attempt at *this* leave behind"), and one file per task answers it with one
row plus one file.

The cap was also a silent deletion. Twenty blocks meant a project older than
twenty turns lost its own history from the only artifact a human would open.
That was described as "the database has it", which is true and unhelpful: the
database is not a document, and a project that commits its records should not
commit a document that erases itself.

The content was written for a person. It is now read mainly by an agent, and an
agent pays for what it cannot parse: prose with a stable shape is cheap, prose
whose keys have to be guessed is not.

## Decision

`<base>/progress/` holds one markdown file per finished task plus an index.

- **File name**: `<YYYYMMDD>T<HHMM>Z-t<turn>-<slug>.md`, in UTC. Time first, so
  a plain directory listing is chronological; then the turn, so a document ties
  back to the rows in the step log; then the goal, sanitised to letters, digits
  and `-` in any script (`fix: the /stop/ button` → `fix-the-stop-button`, a
  Chinese ask keeps its words). A blank ask is `task`.
- **Header**: a `---` block of flat `key: value` lines — `task` (`session:turn`,
  the identity), `turn`, `session`, `at` (RFC 3339 UTC), `at_ms`, `status`
  (`done` / `failed`), `rig`, `model`, `provider`, `model_calls`, `tools`,
  `duration_ms`, `files_changed`, `workspace_git`, `git_branch`, `git_head`,
  `git_subject`, `git_dirty`. Every string is quoted, an unknown number is
  `null`. The keys are fixed so a reader parses instead of guessing.
- **Sections**, in this order and always present: `# <headline>` (the ask in
  one line), `## Task` (the ask, bounded), `## Result` (outcome, calls, tools,
  duration), `## Changes` (the table the checkpoint already recorded), `## Git`
  (branch, head, subject, dirty count, as of the end of the turn), `## Next`.
- **`## Next` is the handoff slot.** The host writes a placeholder and never
  overwrites what it finds: rewriting a task — a retried turn, a reopened
  window — carries the section over verbatim and replaces the file in place.
- **`INDEX.md` is derived, not authoritative.** One row per task, newest first:
  UTC, turn, status, goal, changed-file count, link. Every row is read back from
  the task files, so deleting the index regenerates it and a hand edit to it is
  not a source of truth. It lists the newest 60 and states how many it left out,
  because the index is read on every continuation and the directory is not.
- **Nothing is deleted for age.** The step log holds the events; these hold the
  summaries. Retention is the user's call — a `custom` base directory on another
  volume, or committing `progress/` with the project.
- `records/journal.db` and its WAL sidecars stay ignored; `progress/` is not,
  because it is project history the user may want in the repository.

## Consequences

- The atomic unit is a turn, because a turn is what the host observes. A task
  that spans several requests appears as several documents; the index's session
  and turn columns are what let a reader see the chain. A real task unit would
  need the agent to declare its own boundaries, which is a kernel tool and a
  different ADR.
- A turn costs one directory scan (the header of every task file) to rebuild the
  index. That is linear in turns and bounded by the files' first lines; it buys
  an index that cannot disagree with the documents.
- Two documents can share a minute and a turn number across sessions. The header
  `task` field, not the name, is the identity — a rewrite finds its file by
  reading that field.
- File names are UTF-8. A checkout that shares history with Windows still works
  (`<`, `>`, `:`, `"`, `/`, `\`, `|`, `?`, `*` are stripped by the sanitiser),
  and a reader that cannot render the script still has the turn number.
- The document's prose stays in the language the user writes. The structure —
  header keys and section headings — is fixed English, so a reader anchors on
  it without parsing.
