# 0082 — One window shows one conversation, and that is not a limit on parallelism

## Context

The request: split the chat into several windows so two projects can run side by
side.

What is already true. A turn belongs to a conversation, not to the window
(ADR 0077). Two conversations keep working while the window looks elsewhere, in
the same project or in another one; the sidebar marks each running conversation
with a dot, and switching back shows the whole in-flight turn (ADRs 0080, 0081).
What is missing is only *seeing* two of them at once.

What a second window would cost, measured against the current code:

- `DesktopState.current` is a single pointer. It is read 13 times in
  `commands.rs` (`current_open()`, `current_id()`, and the `current` mutex).
  It becomes a pointer per window, and every one of those reads has to name a
  window.
- Of the 35 `#[tauri::command]` functions, the ones that act "on the
  conversation" take the window's pointer implicitly. Each becomes a place where
  addressing the wrong window is a bug the compiler will not catch.
- `interactions` is one app-wide port; `current()` returns the single pending
  request. Permission prompts would need to be routed to the window that owns
  the conversation they belong to, not to whichever window asked last.
- 19 `emit` calls broadcast to every window. They have to become `emit_to` with
  the owning label, or every window keeps state for conversations it never
  shows.

That is a refactor of the command surface, not a shell around it.

## Decision

No second window. Parallelism across projects is reached by switching, not by
splitting.

1. **A window keeps one focused conversation.** Switching is a command
   (`open_session`), and the conversation that loses focus keeps running.
2. **Every running conversation is visible where it is listed.** The sidebar dot
   is the signal that work continues elsewhere; the archive and delete controls
   of a row are disabled while that conversation runs.
3. **A per-conversation cancel button and a global "N running" panel are not
   built.** They need a scheduler and a reconciler to stay honest. If a feature
   needs a watchdog to be correct, it is not worth shipping (the same budget rule
   as ADR 0077).
4. **If one view of two projects is genuinely needed later**, the cheap shape is
   a read-only strip in the sidebar that lists running conversations from other
   projects and switches to them on click — not a second window with its own
   pointer to every piece of state.

## Consequences

- Two projects can run at once today; the window shows one of them at a time.
- The command surface stays as it is: one `current` pointer, one interaction
  port, broadcast events that name their conversation.
- A second window returns only with a concrete need that switching does not meet,
  and it starts by rewriting this decision, the same way a second face would
  (see the architecture notes on ADR 0003).
