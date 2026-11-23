# 0063 — A turn leaves a log and a document

Status: accepted

Supersedes the storage form of 0062. Amends 0057. Its second artifact — the
progress document — is superseded in form by 0064: one file per task plus a
derived index, instead of one file with a capped list of blocks. The step log
stands as decided here.

## Context

0062 gave every turn a JSON-lines step file: one line per kernel event, in the
order the host saw them. That answered "what did the agent actually run", and it
answered it one turn at a time — `<session>/<turn:08>.jsonl`. A question that
spans turns ("every shell command that failed this week", "how many turns the
verification gate turned back") needs a file walk and a parse, and a tool-heavy
conversation leaves hundreds of files where one table belongs.

It also answered the wrong question for the reader it was written for. A person
picking a project back up does not want nine hundred event rows; they want to
know what the last task was, whether it finished, which files it touched and
where git stood. That is a document, not a log.

The two artifacts have opposite shapes. The log is high-volume, append-only,
machine-read, and worthless to a diff. The document is small, newest-first,
human-read, and possibly committed.

## Decision

One base directory per workspace (the resolution is unchanged: `project`,
`state`, `custom`), two artifacts.

- `<base>/journal.db` — SQLite, two tables. `turns` holds one row per turn: the
  header written before the kernel runs (session, turn, model, provider, rig,
  workspace, input) and the ending filled in after it returns (ok, error, state,
  model calls, tool count, duration). `steps` holds one row per kernel event
  with the kernel's sequence number, a UTC millisecond stamp, the event name and
  the event body as JSON — including the two previews the persisted event
  drops. Indexed on `(session, turn, step)` and on time. WAL,
  `synchronous=NORMAL`, a five-second busy timeout: an open reader never blocks
  a running turn, and every insert commits on its own so a killed turn keeps
  the steps it took.
- `<base>/progress.md` — markdown, newest block first, one block per finished
  task: when and which turn, the rig and the model, how it ended, where git
  stood, the ask quoted in part, and a table of the files it changed with line
  counts. A write prepends a block and never rewrites the ones below it, so a
  block the user hand-corrects survives. Past twenty blocks the oldest drops;
  the log keeps everything.
- A block carries an HTML-comment marker (`<!-- keelson turn <session>:<turn> -->`),
  so writing the same turn twice replaces its block instead of stacking a copy.
- The change list comes from the checkpoint records the app already keeps for
  undo — one owner of "which files did this turn change" — and the git line is
  one `git --no-optional-locks status` at the moment the turn ends, so the
  answer is where the tree is now rather than where it was.
- In `project` mode only `records/journal.db` and its WAL sidecars are added to
  `.keelson/.gitignore`; `progress.md` stays visible because a project may want
  to commit its own history. `checks.toml` and any rules the user already wrote
  are untouched, and an existing rules file is appended to, never replaced.

## Consequences

- A workspace with a long history has one small database instead of one file per
  turn. Queries are ordinary SQL, which is what makes a later telemetry export
  a read instead of a project.
- The log is a disposable artifact: nothing the app renders depends on it, and
  deleting the database costs history, not state.
- SQLite here is an embedded single-file store, not the workspace index or
  vector store the architecture forbids. It holds an append-only log and never
  becomes the source of truth for a conversation.
- Two artifacts mean a turn does two writes where one would do. That is the
  price of the two readers wanting different things.
- `progress.md` grows a block per turn up to twenty; a project that wants the
  whole history reads the database instead.
- The document is written in Chinese: it is a user-facing artifact like the
  settings panel, not a repository document.
- A `.keelson/.gitignore` written by 0062 may still name `steps/`. The line is
  harmless, and it is left alone rather than rewritten under the user.
