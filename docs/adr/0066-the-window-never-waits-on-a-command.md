# 0066 — The window never waits on a command

## Context

Two failures shared one symptom: the app showed "无法进入工作区" and then sat
unresponsive, with no control that could take it back.

The first was a regression. `ensure_idle` asked the current conversation for
its cancel token and answered "还没有打开会话" when there was none. The boot
path calls it before a conversation exists, so every cold start failed.

The second is structural. Tauri executes a command written as a plain `fn` on
the thread that paints the window, and `await invoke()` on the JS side does not
change that: the Rust call still owns the thread for its whole duration. The
commands on the boot and paging path were all plain `fn`, and the work behind
them is real: the rig discovery walks directories, the session index reads
every conversation header, a history page reads one file per turn, and the
change list ran two `git diff` processes per turn. This project's state
directory is 58 MB with turn rows above 1 MB, so opening a conversation could
hold the paint thread for seconds — a beachball with no button on it.

## Decision

1. Idle means "the conversation on screen is not running a turn". No current
   conversation is idle, not an error, and `open_inner` may proceed.
2. Commands that touch the filesystem, the process table or a repository are
   `async fn` and do their work on `tauri::async_runtime::spawn_blocking`.
   Plain `fn` remains for reads of state already in memory.
3. Opening a conversation reads only the change lists the checkpoint index
   already recorded. A missing list is filled by `turn_changes` for the turn
   it belongs to, not by diffing thirty turns at once.
4. There is a way out that does not require killing the process:
   `force_recover` cancels the tokens of every open conversation, drops the
   engines and clears the current session; the next open builds a fresh one.
   The boot screen offers it after three seconds of waiting — on the loading
   screen and on the failure screen — and "cancel and retry" falls back to it
   when a turn ignores its cancel.

## Consequences

- A slow boot reads as slow: the loading screen counts seconds and shows two
  ways forward instead of a spinner.
- An abandoned turn is not lost work. It still writes its result into the
  conversation document; only the window's handle on the engine is dropped.
- Turning a `fn` command async is invisible to the frontend, so the change
  carries no protocol change.
- The window's own menu has no recovery entry yet: a webview whose JS thread
  is wedged cannot be recovered from the frontend. That belongs in the native
  menu (⌘⇧R) and needs the same `forget_open`; it is left for the next change
  so this one stays reviewable.
