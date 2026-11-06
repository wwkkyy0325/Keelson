# M3 — the transcript view

Goal: `keelson run coding` opens the terminal transcript from `docs/design/tui.md`,
streams the answer, shows tool lines and permission prompts, and keeps the
reference host's goldens.

## Decisions

- `Engine` is the long-lived host handle: it builds the driver once, runs one
  turn at a time, persists after each turn and exposes events, tasks, roots and
  the permission mode. `engine::run` remains the one-shot convenience.
- The provider is wrapped when the view wants deltas: text reaches the view
  through a channel, not through a kernel event.
- The view owns the screen model, the reducer and the layout; `render/` owns
  raw mode, the alternate screen and key polling. The engine still may not name
  a terminal library (the boundary test stays).
- Session listing (`/sessions`) is not ported: the JSON store has no query
  layer. The documented command set is `/help /details /add-dir /clear /cost
  /roots /cancel /quit`.
- Goldens are copied from the reference host with the fixture workspace renamed
  from `keel` to `keelson`; only the footer padding follows the longer name.
- The mock provider repeats one text answer, so one engine can serve many
  turns. Tool dispatch stays covered by the kernel probes.

## Steps

- [x] `engine::Engine`: open with an injected interaction port and an optional
      delta channel; run and persist one turn; expose config, events, tasks,
      roots, mode switches and interjections.
      Verify: `engine::tests` — one engine runs two turns, and a live view
      receives text deltas.
- [x] `view/ui_text.rs` ported, with the Keelson command list.
      Verify: `ui_text::tests`.
- [x] `view/transcript.rs`: the state reducer and screen model from the
      reference host, minus the SQLite-only commands.
      Verify: the ported reducer tests.
- [x] `render/terminal.rs`: raw mode, alternate screen, key polling, restore on
      drop and panic.
      Verify: `cargo test`; the pseudo-terminal run below.
- [x] `keelson run coding` opens the transcript; `--input` prefills, `--lang`
      selects `auto|zh|en`; a piped stdin refuses the interactive view.
      Verify: manual run; `run script` still works for pipes.
- [x] Goldens in `tests/fixtures/` pass.
      Verify: the four golden tests and `cargo test`.
- [x] Docs, changelog and this task record.
      Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test`, `cargo run -- doctor`.

## Verification (2026-11-05)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 56 unit tests, 1 boundary test and 5 kernel probes pass.
- `keelson run coding` under `winpty`: typed `hello`, the mock turn streamed and
  collapsed, the session was written with one `Done` turn, and `/quit` exited
  with code 0. The chrome resolved to Chinese from the OS language.
- `keelson run script --input hello`: prints `mock turn complete`.
- `printf 'hi' | keelson run coding`: refused with "needs an interactive
  terminal" (exit 2).
- `keelson doctor`: two rigs load their profiles against kernel `0.1.0`.
- Not run here: a real-model turn needs `OPENAI_API_KEY` or
  `ANTHROPIC_API_KEY` and a profile that selects the provider.

## Not in M3

- The chat view and any second face (M5).
- Syntax highlighting, images and mouse.
- Session listing, forks and the SQLite read model.
- The reference host's remaining daily-use checklist in `docs/design/tui.md`
  (sub-agent and background lines, permission-prompt zh goldens, the manual
  80/40-column sweep).
