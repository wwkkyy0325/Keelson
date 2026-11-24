# 0081 — A question is noted until its turn is written, and resuming means asking again

## Context

The engine writes a turn when it ends (`Engine::persist`). A turn that is still
running exists nowhere on disk: not in the document, not in the turn files, not
in the index. So an app that quits — or is killed — in the middle of a turn
leaves the question surviving only as the conversation's title, truncated to 48
characters, in a conversation with no turns.

The proposal was to make that half-finished record resumable: keep the partial
turn and continue it after a restart.

A turn is not resumable as it stands. It is a run of model calls and tool calls
inside the kernel's loop, and the loop holds no checkpoint of where it got to.
The calls it already made had side effects — files written, commands run — so
neither replaying from a checkpoint nor re-running from the start produces the
same turn. Making one resumable means kernel-side step checkpointing plus a
tool contract that says which calls may repeat. That is a large change in Keel
for the case where the app died mid-turn.

## Decision

Keep the question, offer to ask it again, replay nothing.

1. **The question is noted at send time.** `sessions::mark_pending` writes
   `<id>.pending.json` beside the conversation document, holding the `seq` the
   turn will take, the text that was sent and when. It runs in the same
   blocking step as `sessions::begin`, before the first model call (ADR 0078).
2. **The note is host-owned and cannot be mistaken for a conversation.** The
   name fails `valid_id`, so `documents()` skips it, and it is neither a
   `*.snapshot.json` nor a sidecar directory, so `Store::sweep_orphans` leaves
   it alone. It is written to a temporary file and renamed, so a reader never
   sees half a question.
3. **It is dropped when the turn is written.** The send path clears it after the
   engine returns, whether the turn succeeded or failed — a failed turn is a
   stored turn (ADR 0079).
4. **`open_view` reports it only when nothing answers it.** No running turn in
   the engine, and `engine.turn(seq)` is `None`. A turn that did land hides the
   note, so a stale file can never draw a second card beside the real turn.
5. **The card offers one action: 再问一次.** It shows the question and calls the
   ordinary send path with the same text. The new turn is a new turn — new `seq`,
   own tool calls, own cost — and the note is rewritten for it.

## Consequences

- Quitting mid-turn no longer loses what was asked. The card says the turn did
  not finish and how it can be asked again.
- Nothing is replayed on its own. The user decides, because the turn's side
  effects are already in the workspace.
- The old turn's cost is not recovered: the tokens it spent are gone with the
  process, and the retry pays again. The step log (`records::StepLog`) is where
  the partial work is still visible.
- The note is at most one file of a few kilobytes per conversation, replaced on
  every send. No queue, no reconciliation, no watchdog.
