# 0062 — A turn keeps a step journal

Status: accepted

Superseded in part by 0063: the step record is now a database table, and the
per-turn file it describes here no longer exists.

Amends 0057.

## Context

ADR 0057 made a turn its own file, and that file records what a turn answered:
the input, the answer, the thinking, the verification result, and one line per
tool call with a bounded preview of its arguments and output.

That is a *summary* of a turn, not a record of one. It drops the model calls
(the row only carries a count), the context and segment events, the budget
events, the interactions, and the order in which any of it happened. Once the
kernel seals a segment and folds it into the summary, the transcript stops
being evidence: a reader asking "what did it actually run, and in what order"
has nothing left to read.

The host also had no durable answer for `TurnBudgetExhausted`: the kernel emits
it, nothing subscribed to it, so a turn that hit the ceiling looks exactly like
a turn that finished by itself.

## Decision

- The host writes one JSON-lines file per turn: `<base>/<session>/<turn:08>.jsonl`.
  One line per kernel event, in the order the host saw them, each carrying the
  kernel's own event sequence number so a line can be lined up with the event
  log it came from.
- `ToolStarted` and `ToolFinished` lines carry the bounded, redacted argument
  and output previews. The kernel skips those two fields when it serializes an
  event for the store; the journal is a file for a human, so they are written
  explicitly.
- Two host notes bracket the events: `turn_meta` (session, turn, model,
  provider, rig, workspace, input, and the file's own path) and `turn_end`
  (ok, error, state, model calls, tool count, duration).
- The file is opened when the turn starts and appended to as the turn runs, so
  a turn that is killed still leaves the steps it took. The poller that feeds
  the live tool view writes the journal before it emits, and whatever it did
  not reach is drained from the kernel's log after the turn.
- `<base>` follows a setting, not a constant (ADR 0062 is about the file, the
  setting is about where):
  - `project` (default) — `<workspace>/.keelson/steps`. The record travels with
    the checkout. `ignore_in_project` appends `steps/` to
    `<workspace>/.keelson/.gitignore`, so `git status` stays clean and a
    project's own `checks.toml` is neither hidden nor rewritten.
  - `state` — `<state>/journal/<workspace-hash>/`. Nothing is written into the
    tree.
  - `custom` — a directory the user types, still separated by workspace hash so
    two projects pointed at one directory cannot overwrite each other's turn
    files.
- A journal that cannot be opened is not an error the user sees: the turn runs,
  the file is simply missing. The setting is shown in Settings with the
  directory it currently resolves to.

## Consequences

- A turn costs a few hundred extra lines of disk on a tool-heavy run, and the
  file grows with what the agent did rather than with the answer.
- Nothing rotates. The record is the point, so retention is the user's
  business; `custom` is the escape hatch for a directory they manage.
- The journal is not the source of truth for anything the app renders. Losing
  it loses history, not state.
- `TurnBudgetExhausted` and the segment/context events become readable, which
  is what makes "it ground along until the end" measurable after the fact.
