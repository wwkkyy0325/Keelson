# 0072 — A view that reads the store may not write it

## Context

A conversation with one background command running held the renderer at 117%
until the command exited, and fell back the moment its runtime froze. Nothing in
the turn was doing it.

Two effects in the transcript touch the store and dispatch into it. Both were
correct read on their own, and both were loops:

```ts
// 1. reconcile the rows once per conversation
$effect(() => {
  void backgroundEpoch;                    // reads the store
  untrack(() => void syncBackground());    // …which dispatches into it
});

// 2. poll while a row is running
$effect(() => {
  if (backgroundVisible && $visible) pollBackground();   // reads the store
  else stopPoll();
  return () => stopPoll();                 // clears what the run armed
});
```

Svelte re-runs an effect when anything it read changes, and the store replaces
its value on every dispatch. Effect 2's body called `pollBackground`, which calls
`syncBackground`, which reads `$session.epoch` **synchronously**, before its first
`await` — so the effect depended on the whole store, not on the two booleans it
names. The cleanup then cleared the interval the same run had armed, so the next
run armed a fresh one and polled again immediately.

On an idle strip none of this fires: effect 1's reconcile reports what is already
on screen, `sameBackground` returns the previous state, and no second dispatch
happens. A **running** row breaks that: `durationMs` is measured, so the list
differs on every poll and the guard that made an idle poll harmless never matches.

Measured in the real WebKit webview, the real `Chat` component, one real running
row on screen, four seconds:

| | no running row | one running row |
|---|---|---|
| `background_tasks` calls | 1 | **2001** |
| intervals armed by the view | 0 | **1999** |
| dispatches | 314 (a full streamed turn) | **2004** |
| WebContent CPU peak | ~6% | **117.1%** |
| dropped frames | 0 | 2, longest 134 ms |

`sample` of the renderer during the loop put every sample in
`DOMTimer::fired → ScheduledAction::execute → MicrotaskQueue::performMicrotaskCheckpoint`:
the thread was inside the poll, not inside rendering.

## Decision

Two rules for an effect that dispatches into the store.

1. **It does not read the store in its tracked body.** The work goes inside
   `untrack`, and what the body reads is a value the work cannot change — here a
   boolean, and for effect 1 the conversation's `epoch`.
2. **Its cleanup does not undo work the run just did.** Effect 2 no longer
   returns a cleanup that clears the interval it armed; the interval is stopped
   by `stopPoll` when the boolean flips, and by a second effect with no tracked
   dependency that exists only to stop it when the component is destroyed.

```ts
$effect(() => {
  const wanted = backgroundVisible && $visible;
  untrack(() => (wanted ? pollBackground() : stopPoll()));
});
$effect(() => () => stopPoll());
```

`epoch` is bumped only by `session-opened` — opening, switching or reconnecting a
conversation — which is exactly the condition the reconcile exists for. That part
is a test: `background-synced` with a moved clock yields a *new* state (so views
re-run) with the *same* epoch (so the guard holds), while `session-opened` yields
a changed epoch. If a poll ever starts bumping the epoch, that test fails and
names the loop it would bring back.

## Consequences

Same harness, same four seconds, after both fixes:

```
row: False  background_tasks 2   intervals armed 0  dispatches 0   cpu peak 5.8%
row: True   background_tasks 6   intervals armed 1  dispatches 5   cpu peak 4.6%
```

Six calls for a running row is the reconcile, the first poll, and one poll per
second — the shape the poll was written for. The 117% is gone.

- A running row costs one host call per second, as intended, and the interval is
  armed once rather than once per dispatch.
- A row that starts *during* a turn still appears: the kernel's start event is
  dispatched, and that dispatch carries the row. The reconcile is not the path
  for it, and does not need to be.
- Both rules are about a shape this file invites again. `sessionEpoch`,
  `turnCount`, `changesEpoch`, `changedSeq` and `runningSeq` are the guards the
  other effects use; a new effect that writes back has to name one, and a cleanup
  has to be safe to run while the effect is still armed.
