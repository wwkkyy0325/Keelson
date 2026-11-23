# 0079 — A failed turn stays in the transcript with what it gathered

## Context

`turn-failed` cleared the live card:

```ts
case "turn-failed":
  return { ...state, running: null, pendingInteraction: null, lastError: action.message };
```

`running.input` is where the message the user sent lives until the turn is
persisted. Clearing it therefore removed the question from the transcript as
well as the answer, and everything the turn had done — the tool rows, the text
that streamed, the thinking — went with it. What was left was an error line
under a turn that no longer existed.

The engine sees it differently. A failed turn is a stored turn: `StoredTurn` has
`state` (`Error`) and `error`, and a turn that fails keeps its input. So the
document and the view disagreed about whether the turn happened. Reloading the
conversation showed the turn; staying in it showed nothing. A first turn that
failed was the worst case: an empty conversation, titled after a question it did
not display.

## Decision

A turn that fails is over, and the transcript keeps it.

1. **The live card becomes a finished turn.** On `turn-failed` the reducer maps
   the card into a `Turn` with the input that was sent, the text and thinking
   that streamed, the tool rows it gathered, the usage it spent, and the error
   it stopped on.
2. **It is marked the way the kernel marks it.** `state: "Error"` — the string
   `TurnState::Error` serialises to — so a live failure and a reloaded one draw
   the same turn.
3. **A failure for another turn is ignored.** Only the card with that `seq`
   becomes a turn; a stray event leaves the running card alone.
4. **`lastError` still reports the same failure** for the status line; the turn
   and the banner are two views of one fact, not two facts.
5. **Nothing is written.** The transcript here is the state of a view; the
   document is written by the engine, which already stores the failed turn. When
   the conversation is reopened, `session-opened` replaces the state with what
   the document says and the two agree.

## Consequences

- The message that was sent stays visible after the turn stops, which is what
  "发出去的话先落到对话历史里" asks for.
- The work a failed turn did stays visible: a turn that ran twenty tool calls
  and then lost the provider no longer erases the evidence of them.
- The synthetic turn may differ from the stored one in detail — it has no
  `changes` (the checkpoint is recorded after a successful turn) and its
  `durationMs` is measured on the client. Reopening the conversation replaces it
  with the stored turn, which is the authority.
- `durationMs` reads the clock inside the reducer. The card already measures its
  own elapsed time, so this makes the reducer's dependence on wall time explicit
  rather than introducing it.
