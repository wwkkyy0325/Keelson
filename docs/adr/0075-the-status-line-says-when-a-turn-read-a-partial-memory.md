# 0075 — The status line says when a turn read a partial memory

## Context

Keel builds one `[memory]` block per turn from the session's facts, bounded at a
quarter of the context allowance, and now reports what it left out
(Keel ADR 0071): the block ends with `(N older entries were left out of this
block)` and the kernel emits `Event::FactsClipped { turn_id, kept, skipped }`.

The event had no reader here. The host matches the kernel's events to build the
turn's stored fields, and everything it does not name falls through to `_ => {}`,
so a turn answered from a memory that had dropped four entries looked exactly
like a turn answered from all of them. The context figure in the status line is
what a user reads to decide whether the model is still working from what they
said; a partial memory under a complete-looking number is the wrong answer.

## Decision

- `StoredContext` gains `facts_clipped: u32`, `#[serde(default)]` so documents
  written before the field still load. It is written when the turn ends, from
  the last `FactsClipped` event in the turn's stream — the same read the
  existing `context_pressure` fold does, and both live in one place.
- `ContextPoint` — the per-turn row the status line reads — carries it too, and
  an older document recovers it from its event stream like the pressure figure
  (`facts_clipped_from_events`), so the field is not a new-format-only fact.
- The context popover shows one muted line under the turn it belongs to:
  `略过 {count} 条记忆` / `{count} memories left out`. Only when the count is
  non-zero; a turn that fits its block shows nothing extra.

## Consequences

- A user can tell the difference between "the model forgot" and "the model was
  handed a shortened memory", which are different repairs: the first is a fact
  the user should restate, the second is a session that has grown past its
  budget.
- The kernel's block already tells the model (`(N older entries…)`), and a
  well-behaved model mentions it. The status line is for the user who did not
  read the model's sentence.
- The field is one number per turn, not a list: which entries went is the
  kernel's business, and a host that showed them would be re-deriving an
  ordering the kernel owns.
- Nothing is shown live during a turn. The figure appears with the turn's row,
  because the block is built once, before the first model call.
