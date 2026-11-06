# M3c — a bounded transcript

Goal: a long session must not flood the screen when details are revealed, and
the view must not rebuild unbounded history on every frame.

## Decisions

- `Ctrl-O` / `/details` expands the **most recent turn only**. The block model
  carries the turn boundary; no cursor or selection model is added.
- When a new turn collapses, the previous turns release their process detail:
  tool arguments and output are cleared, while the compact line, the answers,
  the summaries and the user messages stay. A failed tool is visible, so it
  keeps its error.
- The run loop redraws on a key, an interaction, a streamed delta, a finished
  turn, a new event, or a tick while a turn runs. An idle screen does not
  rebuild or redraw.
- Viewport-only line building is deliberately not implemented yet. The next
  step, if a real session still shows pressure, is to render only the blocks
  intersecting the viewport rather than caching or pruning the conversation.

## Steps

- [x] Per-turn expansion; older turns stay folded.
      Verify: `ctrl_o_expands_only_the_latest_turn`,
      `ctrl_o_reveals_the_last_turn_process`, `ctrl_o_toggles_process_details`.
- [x] Release the process detail of older turns on the next collapse.
      Verify: `older_turn_details_are_released`.
- [x] Suppress idle redraws and redraw only on change.
      Verify: a winpty smoke (turn, Ctrl-C, `/quit`); the loop is visible in
      `run()`.
- [x] Docs, changelog and this task record.
      Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test`, `cargo run -- doctor`.

## Verification (2026-11-06)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 64 unit tests, 1 boundary test and 5 kernel probes pass.
- winpty smoke: one mock turn completed and persisted, the screen redrew while
  running, and `/quit` exited with code 0.

## Not in M3c

- A scrollback viewer or a per-turn cursor: only the latest turn expands.
- Viewport-only line building; measure first.
