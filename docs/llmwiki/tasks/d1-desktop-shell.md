# D1 — the basic desktop shell

Goal: a normal Tauri window that opens a rig, streams a turn, shows the
transcript, pages older turns by a stable cursor and answers kernel
interactions. No floating window yet.

## Decisions

- The engine is a library; the CLI and the desktop are two hosts. ADR 0003
  records the exception to the "one binary" rule.
- The backend owns the session. The frontend has one `SessionState` and one
  reducer; there are no `isLoading`/`isStreaming`/`hasError` flags. `running`
  and `pendingInteraction` are the state, and the status is derived.
- History is keyset paginated: `load_turns(before, limit)` returns turns with
  `seq < before`, oldest first, plus `nextBefore`. Appends cannot shift a page.
- Live events carry `turn_seq`; the reducer upserts and deduplicates by it.
  A page and a live event that overlap apply in either order.
- The backend persists the compact tool rows (name, ok, duration, bounded
  args/output) on each turn, so history can render tool lines without the
  kernel event stream.

## Steps

- [x] Extract `src/lib.rs`; the CLI binary uses the library.
      Verify: `cargo test`.
- [x] `TurnView`, `TurnPage`, `ToolLine`, `Engine::history`, `next_turn_seq`
      and `Engine::turn`.
      Verify: `history_pages_by_turn_sequence`; the tool row assertions.
- [x] Tauri backend: `bootstrap`, `open_session`, `send_message`,
      `cancel_turn`, `load_turns`, `respond_interaction`; events for turn
      start, text delta, turn finish/failure and interactions.
      Verify: `cargo check` and `cargo tauri build --debug --no-bundle`.
- [x] Svelte 5 + TypeScript frontend: setup screen, transcript, streaming,
      tool details, permission card, cancel, older-page button with scroll
      anchoring.
      Verify: `npm test`, `npm run check`, `npm run build`.
- [x] Launch the built binary and confirm it stays up.
      Verify: process alive after six seconds.
- [x] Docs, ADR 0003, changelog and this task record.
      Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test`, `npm test`.

## Verification (2026-11-06)

- `cargo test`: 65 unit tests, 1 boundary test and 5 kernel probes pass.
- `cargo clippy --all-targets -- -D warnings`: clean in both crates.
- `npm test`: 7 reducer tests pass.
- `npm run check`: 0 errors, 0 warnings across 174 files.
- `npm run build`: frontend bundles to ~56 KB JS / 4 KB CSS.
- `cargo tauri build --debug --no-bundle`: builds `keelson-desktop.exe`.
- The binary launched and stayed alive (~36 MB), then was killed.
- UI automation against the real window: opened the `coding` session,
  typed `hello`, sent it with the Send button and with Enter, and confirmed the
  streamed `mock turn complete` and the `✓ echo` tool row rendered.
- Not yet exercised through the window: connecting to `coding-live` and a real
  streaming response. The backend path was already verified from the CLI.

## Not in D1

- Floating, frameless, always-on-top window; tray; global shortcut.
- Markdown/code highlighting in the transcript (plain `pre-wrap` now).
- Live tool lines while a turn runs; the finished turn carries them.
- A folder picker; the setup screen accepts a workspace path.
