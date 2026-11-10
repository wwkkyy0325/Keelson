# ADR 0035 — A running turn shows its tools and thinking

## Status

Accepted and implemented in the engine and the desktop.

## Context

The running turn rendered one block of raw streamed text: `ModelDelta` text
from every model call was concatenated, tool calls stayed invisible until the
turn row arrived, and thinking was dropped. Worse, the row was emitted only
after the post-turn checkpoint snapshot — a git commit over the project — so
after generation finished the screen stayed on the raw, unformatted stream
until the snapshot completed, then jumped to the stored turn.

## Decision

1. The host keeps the running view in three parts: the thinking stream, the
   tool rows, and the answer text. The desktop polls the engine's per-turn
   event log every 120 ms while a turn runs and emits `tool-started` and
   `tool-finished` with the kernel's bounded, redacted previews. Sequence
   numbers stay monotonic across the per-turn clear, so the poller skips the
   previous turn by sequence.
2. The engine's delta channel carries `ModelDelta::Text` and
   `ModelDelta::Thinking`. The desktop forwards thinking on its own event.
   Thinking and tool calls share one indented group, collapsed by default; the
   running view no longer shows a placeholder or a blinking cursor.
3. The engine keeps the turn's thinking in `StoredTurn.thinking`, bounded at
   64 KiB with a truncation marker, and `TurnView` carries it. The stored turn
   shows it collapsed in the same group as the tool calls. Sessions written
   before the field load it as empty.
4. A model call's text is commentary when that call goes on to use a tool.
   The reducer moves such text into a collapsed `process` entry of the same
   group, so the answer area only ever holds the call that finished the turn.
5. `turn-finished` is emitted as soon as the turn row is persisted, before the
   checkpoint snapshot. The snapshot and record then run, and a `turn-changes`
   event tells the view to reload the diff and undo state.

Polling is deliberate: the event log is already the per-turn channel, and a
kernel subscription would add an API for a local 120 ms refresh. If a second
consumer appears, the poll moves behind an `EventLog` sink without changing
the events the desktop listens to.

## Consequences

- The raw stream view ends immediately when the model stops, and the stored
  answer, tools, stats and verification appear before the checkpoint finishes.
- The change summary and undo state can appear a moment after the answer; the
  `turn-changes` event brings them in without polling the backend.
- Thinking adds at most 64 KiB per turn to the session document and is never
  part of the model history or the answer.
- The poll clones the turn's event list ~8 times a second; a heavy turn is a
  few hundred events, well below the cost of a model call.
