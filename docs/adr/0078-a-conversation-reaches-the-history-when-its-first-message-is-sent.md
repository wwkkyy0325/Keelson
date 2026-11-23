# 0078 — A conversation reaches the history when its first message is sent

## Context

A blank conversation writes nothing. That was deliberate: pressing 新对话 and
walking away used to leave a row in the history with no name, no turn and no way
to tell it from a real conversation, so the rule became "a document with no turns
and no name is not a conversation" (`sessions::is_listed`).

The rule was right about blank conversations and wrong about the first message.
Nothing wrote the document until the turn *ended*: `sessions::touch` runs after
the turn's result, and the engine writes the document when it persists the turn.
So a new conversation was absent from the project's history for the whole first
turn. Measured in this project on 2026-11-23: the message was sent at
`18:57:59.674`, the turn's document and row appeared at `18:58:49.879` — a gap of
fifty seconds in which the thing the user had just typed did not exist anywhere
in the history. Worse was the failing first turn: the turn never wrote a
document, so the question disappeared with it and the conversation itself was
never created.

The half-made record was being dropped by the very rule that keeps the list
honest, and the honest reading of "the message was sent" is that a conversation
exists — with what was asked, and with no answer yet.

## Decision

A conversation is listed from the moment its first message is sent.

1. **`sessions::begin` runs at turn start.** `send_message` calls it right after
   `turn-started`, before the engine does any work, and emits `session-updated`
   so the rail adds the row without a reload.
2. **The document it writes is named, not half-made.** It holds the first
   message as the title, the rig the turn runs on, and no turns. `is_listed`
   already keeps a named document with no turns visible — that is how a branch
   behaves — so no new rule was needed and the rule that drops an unnamed, empty
   document still does its job.
3. **A conversation that already has a document is only refreshed.** `begin`
   finds a header and calls `sync`, so a second message never appends a second
   row and a conversation with turns keeps its count.
4. **The turn still owns the transcript.** `begin` writes no turn, no answer and
   no step log: what the turn produces is written by the turn, as before. The
   row says "asked, not answered" — `turns: 0` — which is the truth while the
   first turn runs and stays the truth if it fails.

## Consequences

- A new conversation appears in the history under what was typed, immediately.
  A first turn that takes a minute no longer hides its own conversation for that
  minute.
- A first turn that fails leaves the question in the history. Its conversation
  has no turns, which is exactly what happened; the failure is still reported by
  the turn's own error path.
- The list is written twice per first turn (at start, at end). Both writes are
  small, atomic, and under `session_io`, which already serializes index writes.
- A document written this way has no `title` from a rename and no turns; the
  engine's save at turn end keeps the transcript authoritative and the title
  falls back to the first message when it does not carry one, so the row's name
  cannot drift from the question asked.
- Deleting such a conversation needs no special case: it is an ordinary
  document with a row.
