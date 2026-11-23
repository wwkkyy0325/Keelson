# 0076 — A command names the conversation it acts on

## Context

The window showed one conversation and the turn ran in another. The user sent
the same message twice because the first one never appeared, and a reload
during that turn landed on the failure page instead of the transcript.

Both halves came from one pointer. `DesktopState::current` is "the conversation
the window shows", and `send_message`, `cancel_turn`, `session_status` and the
records all read it, because the commands carried no argument that said which
conversation they meant.

`open_inner` kept the pointer in step only when it **built** an engine. A
conversation that was already open with a document took an early return:

```rust
if let Some(open) = state.open_of(&session_id) {
    return Ok(open_view(&state, &open, …));   // current untouched
}
```

So selecting a conversation the app had opened before showed its transcript
while `current` still pointed at the one the user had just left. The next send
went there. Its events were tagged with that id, the view dropped every event
that is not its own conversation, and nothing appeared until a reload — which
then read the turns back from disk, in the other conversation.

The same pointer was the boot block. `open_inner` began with `ensure_idle`,
which checks `current`, not the conversation being opened:

| the window asks to open | `current` | result before |
|---|---|---|
| B | A, with a turn running | **refused**: "当前回合还在运行" |
| A | A, with a turn running | refused (correct — A's engine is kept anyway) |

A reload while any conversation was working therefore could not open anything,
and the boot screen offered "cancel the turn and retry" as the way in. That is
the failure page the user had to work around.

Evidence, from the session store of the project this happened in
(`s-18e864ab45bf49a0`, the conversation the sends landed in):

```
00000003.json seq 3 input '继续调整，第一点，新对话已经在进行中了…'
00000004.json seq 4 input '继续调整，第一点，新对话已经在进行中了…'   same input, sent again
```

and the conversation the window was actually showing
(`s-18e8b20ffa85b980`) has no turn for either message until the next send, which
was `Cancelled` — the cancel the failure page asked for.

## Decision

**A command that acts on a conversation names it.** `send_message`,
`cancel_turn` and `session_status` take a `session_id`; `null` means "whatever
the window has", which only the boot path uses. The window's own session id is
the authority on what it is showing, so it is what the frontend sends.

**The pointer follows the window, not the engine.** `open_inner` moves `current`
on both paths, through one `show_conversation` helper. A kept engine is still a
conversation the window shows.

**Opening a conversation is not refused because another one is busy.** A
conversation with a document, or with a turn or a background command running,
keeps its engine and cannot be rebuilt under its own work; a blank and idle one
is rebuilt with the current rig (ADR 0039). There is nothing left for a
window-wide "is anything running" check to protect, and keying it on `current`
blocked the wrong conversation.

## Consequences

- A send lands where the user is looking, and its events carry that id, so the
  view paints them: the message appears at once, not only after a reload.
- A reload during a turn reopens the conversation and shows the running turn
  (`SessionView::running`) instead of the failure page; the composer's cancel
  button is the same one as before.
- Switching conversations while a turn runs is allowed, which is what the view's
  event filter was written for. The conversation left behind keeps streaming
  into its own engine, and its turns are on disk when the window comes back.
- The boot screen's cancel passes `null`: what blocks the open is whatever the
  host still has, not necessarily what the store last named.
- `OpenConversation` now carries its `workspace` and `state_dir`. A command that
  names a conversation can do its bookkeeping without consulting the pointer,
  which may have moved on while the turn ran.
- The remaining commands (`turn_changes`, `load_turns`, `background_tasks`,
  `undo_last_turn`, `branch_session`) still read the pointer. That is now sound
  because the pointer follows the window, and they are the next candidates if a
  later bug shows the same shape.
