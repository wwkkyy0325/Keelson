# 0080 — A re-read of a conversation keeps what already streamed

## Context

Opening a conversation builds the whole view from the backend:

```ts
case "session-opened":
  return {
    ...initialSession(),
    ...
    running: action.view.running
      ? liveTurn(action.view.running.seq, action.view.running.input)
      : null,
  };
```

`SessionView.running` is `{ seq, input }`. It does not carry the answer, the
thinking, the tool rows or the usage, because those are streamed events and the
engine keeps no live copy of them.

Since the window keeps one state per conversation (ADR 0077), the store already
holds everything that streamed while the reader was elsewhere. Rebuilding the
card from the view threw that away and kept only the question: switching to
another conversation and back showed an answer that started at the switch, with
the text before it gone and no way to get it back until the turn ended. That is
the same observation as "发出去的话没有先落到对话历史里" from the other side — the
transcript had the prefix, the window threw it out.

## Decision

`session-opened` keeps the live card when the view names the same turn.

1. **Same `seq` means the same turn.** The `seq` is assigned by the engine from
   the document, so a card and a view agreeing on it are the same turn. The card
   survives; only the input is taken from the view, since that is the one field
   the view knows better (it is what the engine was given).
2. **A different `seq`, or none, is the engine's word.** A view that reports no
   running turn ends the card; the turn itself is in `turns` if it was written.
   A view that names another `seq` builds a new card.
3. **Nothing else is preserved.** `turns`, `sessions`, `nextBefore`, `projects`
   and the interaction slot still come from the view — the store's copy of those
   is a cache of the document, and the document is the authority.

## Consequences

- Switching away from a running turn and back shows the whole turn so far.
- The store cannot drift: it only ever keeps a card for the `seq` the engine
  still reports, so an ended turn cannot linger as a live one.
- The reducer no longer resets to `initialSession()` on a same-conversation
  re-read. Everything it does reset is still reset, because it is listed
  explicitly.
- A window restart still loses the streamed text: that state lived only in
  memory. The question survives it (ADR 0081); the answer did not exist yet.
