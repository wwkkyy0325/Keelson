# 0065 — The transcript says when a call was answered from the memo

Status: accepted

## Context

The kernel answers an identical repeatable call from its per-turn memo
(`Keel` ADR 0070) and reports `reused: true` on `ToolFinished`. The answer it
hands back carries the *first* call's `duration_ms`, because that is what the
answer cost. Shown next to a duration, a reused call reads as "this call took
4 s", which it did not: it ran nothing.

## Decision

- The host passes `reused` through every layer a tool row travels: the live
  event, the running turn's tool list, and the stored turn row. The stored row
  takes the field with `#[serde(default)]`, so a turn written before the field
  still loads and reads as not reused.
- The transcript shows a `reused` badge in place of the duration, never beside
  it. The first run's duration stays reachable in the badge's tooltip.
- No other surface changes. The step log records the event as the kernel emitted
  it, so `reused` is already in the log for a later reader.

## Consequences

- A reader can tell "the agent ran this twice" from "the agent asked twice and
  the kernel answered once", which is the difference between a slow turn and a
  cheap one.
- The badge is a host presentation choice, not a kernel fact; the kernel keeps
  reporting a duration because the answer really did cost one.
