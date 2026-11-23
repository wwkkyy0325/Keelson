# 0077 — A window keeps one state per conversation, keyed by the id on the event

## Context

`listenToEvents` dropped every event whose `sessionId` was not the one the window
showed:

```ts
const owned = (payload: EventTag): boolean =>
  payload.sessionId === undefined || payload.sessionId === get(session).sessionId;
```

That was written when a second conversation could not exist on screen, and as a
guard it was right: a turn in conversation A must never paint into B. It became
a hole once switching away was allowed while A's turn kept running. The turn
was still there — the engine still had it, the backend still emitted its events —
but the window had nowhere to put them, so they were thrown away. Coming back
re-read the transcript from disk and started the live card over, which looked
like a conversation that lost the middle of its answer.

The filter was the symptom. The cause was the shape: one `SessionState`, one
`epoch` guard, one `sessionId`, for a window that can hold several engines.

## Decision

State is keyed by the conversation id the backend already puts on every event.

1. **A registry, not a singleton.** `lib/session-store.ts` holds
   `Map<sessionId, Writable<SessionState>>`. Each conversation reduces its own
   events through the same pure `reduce`; nothing about the reducer changed.
2. **The seam is the store, so no component changed.** `session` is now a
   readable view that subscribes to the state of the focused conversation and
   re-points when the focus moves. Components keep writing `$session.turns` and
   `$session.running`; the transcript, the status line and the composer did not
   learn that there is more than one conversation.
3. **Events route by name.** `dispatchTo(id, action)` writes to the named
   conversation whether or not it is on screen; `dispatch(action)` is for a
   component's own answer (a poll, an undo), which belongs to whatever it is
   drawing. The `owned` filter is gone: it was the thing that lost the turn.
4. **An event for a conversation the window holds no state for is dropped, not
   parked.** Opening a conversation reads the engine and the disk, and that open
   answers everything the dropped deltas would have said. Inventing state for
   every id that spoke would let a project with fifty conversations cost fifty
   transcripts.
5. **The state is a cache with a plain bound.** `KEEP = 8`, evicted least
   recently used. The conversation on screen and the ones with a turn or a
   background command in flight are never candidates: dropping one of those
   throws away a stream the user is watching. Nothing watches for a conversation
   that was forgotten and nothing puts it back — forgetting costs one open, and
   only when it is opened again.
6. **Project-wide answers go to every copy.** `sessions-loaded` and
   `session-updated` rewrite the conversation list, and each entry carries its
   own copy. Two copies that disagreed would show a rename in one conversation
   and the old title in another, so these broadcast instead.
7. **The rail marks work, and that is all it does.** A conversation with a turn
   running gets a dot in the sidebar, and its own archive/delete controls wait
   for it; every other row stays usable. The mark is derived from each state on
   write, so no timer reconciles it with what is drawn.

## What was deliberately not built

No state machine, no watchdog, no timeout that notices a conversation stopped
reporting. Each of those would be a second source of truth about whether a turn
is running, and the first one — the state of the conversation that owns the
turn — is already exact. The rule for this area: if keeping several
conversations honest needed reconciliation timers, per-conversation UI state
machines or a global scheduler, it would not be worth the code, and one
conversation at a time would stay the answer.

So this stops short of parallel UI: there is no per-conversation cancel button
in the rail and no "N conversations running" entry point. Cancelling still
happens in the conversation being shown. That is a small enough thing to add
when someone asks for it, and a large enough thing to leave out now.

## Consequences

- Switching to a conversation the window already holds is instant and complete:
  it draws the state that has been receiving events all along.
- The window can now be looking at B while A finishes a turn, which is the point.
- Memory is bounded by `KEEP` states, not by the project's conversation count,
  and the bound cannot evict something that would be missed.
- A delta that arrives before its conversation is opened is not shown. The open
  that follows reads `running`, the turns and the pending interaction from the
  engine, so the card is rebuilt rather than lost.
- `closeAll()` exists for recovery: the window forgets every conversation
  without asking the backend to drop an engine.
