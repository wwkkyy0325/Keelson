# Task — a stuck retry, a frozen clock, a Stop button

## Goal

Three defects from one live session: a long turn died on malformed tool
arguments; the background row's clock froze; a running background command
could not be stopped.

## Steps

- [x] Retry a malformed tool call with a correction instead of a verbatim
  resend. Verify: `a_malformed_tool_call_is_retried_with_a_correction` in
  `crates/keel-runtime/tests/turn.rs`; the request on the retry carries the
  note.
- [x] Accept cumulative tool-argument resends and name a bounded sample in
  the parse error. Verify: the three accumulator tests in
  `crates/keel-model-openai/src/wire.rs`.
- [x] Show the engine's measured runtime in the background row and drop the
  local ticker that competed with the poll. Verify:
  `takes a background row's runtime from the engine poll` in
  `desktop/src/lib/session.test.ts`; no `startedAt` remains in the view.
- [x] Reconcile the rows with the engine on every session open, keep polling
  through a transient error, and guard a mid-request session switch.
  Verify: the open and poll effects in `desktop/src/components/Chat.svelte`;
  `svelte-check` is clean.
- [x] Continue `bg-N` handles after the logs on disk. Verify:
  `a_new_registry_continues_after_existing_handles` in
  `crates/keel-runtime/src/background.rs`.
- [x] Stop one background command by handle. Verify:
  `a_running_command_can_be_stopped` in `crates/keel-runtime/src/background.rs`;
  the row's Stop button calls `stop_background` and the next poll shows the
  task finished.
- [x] Recover a boot that fails because a turn survived a webview reload:
  show a retry screen for an onboarded app and offer "cancel the running turn
  and retry", instead of falling back to the setup wizard. Verify:
  `desktop/src/App.svelte` (`cancelAndRetry`), `svelte-check` clean.

## Verification

```bash
cd ../Keel && cargo fmt --check && cargo clippy -p keel-runtime -p keel-model-openai --all-targets -- -D warnings && cargo test -p keel-runtime
cargo fmt --check && cargo test --lib
cd desktop && npm test && npm run check && npm run build && cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```

## Notes

- A stopped command still reaches the model at the next safe point, with a
  non-zero exit; stopping is not silent.
- The retry note lives in the turn's message list only; the persisted
  transcript is unchanged.
