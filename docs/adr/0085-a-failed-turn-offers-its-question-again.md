# 0085 — A failed turn offers its question again

## Context

ADR 0079 kept a failed turn in the transcript: its question, the text that
streamed, the tool rows it gathered and the error it stopped on. What it did not
give the user was a way back. `Turn.svelte` ended the article with

```svelte
{#if turn.error}
  <div class="error">{turn.error}</div>
{/if}
```

and the only way to try again was to read the question back off the card and
retype it. In the conversation this came from, the user typed 继续 — "continue" —
because that is faster than selecting and copying, and the kernel answered it as
a new turn with no memory of what had been lost.

The transcript already had the other half of this. A question whose turn never
came back at all (ADR 0081, ADR 0083) is shown as a note with a 再问一次 button
that calls `onResume(note.input)`, wired in `Chat.svelte` to the same `send` the
composer uses. A turn that failed with an error is a different fact and had a
different affordance: one offered the question again, the other offered nothing.

Keel ADR 0075 makes the gap worth closing deliberately. The kernel now resends a
cut stream only while the round has shown the user nothing; a connection that
drops mid-answer still ends the turn, because the kernel will not spend a second
answer on text the user has already read. That leaves a class of failures whose
only sensible recovery is a human pressing a button.

## Decision

1. **The error line gains one control.** `重试` / `Retry`, in the same row as the
   error text (`.error-row`, `.error-retry`), styled like `.interrupted-resume`
   so the two recovery affordances read as one family.
2. **It sends the question, it does not resume the turn.** The click calls
   `onRetry(turn.input)`, which `Transcript.svelte` routes to the existing
   `onResume`, which `Chat.svelte` routes to `send`. No new command, no new
   engine call, no new state: a retry is an ordinary new turn, and the failed
   turn keeps its record — its usage, its duration, its error.
3. **Offered on the newest turn only, and only while nothing runs.**
   `retryable={index === $session.turns.length - 1 && !$session.running}`. A
   question from five turns back, sent again now, lands at the end of a
   conversation that has moved on; the card stays as history and says what it
   says. The second half of the condition is the guard the composer already
   keeps: `send` itself does not check whether a turn is in flight.
4. **The button sits in the part of the card that never collapses.** The error
   row is outside the body that ADR 0069 gives back, so it is inside the header
   `Turn.svelte` measures at mount; adding a control there does not change what
   a stand-in reserves.
5. **Two keys, both locales.** `turn.retry` and `turn.retryHint` ("把这条消息重新
   发一次，作为新的一轮"), the hint as the button's `title`, because "重试" alone
   does not say that the retry is a new turn and pays for itself.

## Consequences

- A model failure costs one click instead of a retype, and the question that
  failed is the one that gets sent — not 继续, which tells the model nothing
  about what to continue.
- A retry re-pays for its own tokens. Nothing is deduplicated and nothing is
  resumed mid-stream, because the kernel has no notion of resuming a call: the
  second request is a fresh one at the same context.
- The button disappears when a later turn is appended. A failure that the user
  walked away from does not sit there offering itself forever.
- The error text now wraps in a narrower column, since the row holds a control
  beside it.
- No test: the desktop suite covers `src/lib` (reducers, cull, markdown,
  follow), and this is markup plus one derived prop, checked by `npm run check`.
