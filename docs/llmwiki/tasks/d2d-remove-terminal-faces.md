# D2d — remove the terminal faces

Goal: the desktop is the only shipped face. Delete the terminal UI, the
one-shot runner and the rig plumbing that existed only to choose between
them.

## Decisions

- Delete `src/view/` (transcript, oneshot, bilingual chrome) and
  `src/render/` (raw mode, alternate screen, key polling), the TUI goldens,
  `docs/design/tui.md` and the M3 task records.
- Delete the `script` rig and the `run` subcommand. The CLI keeps `rig list`,
  `rig show`, `doctor`, `--version` and `--help`.
- Delete `engine::run`, `RunRequest` and `TurnReport`; the engine surface is
  `Engine::open`, `run_turn`, `history`, `next_turn_seq` and `turn`.
- Remove the rig's `ui` field and `UiMode`. A rig is now a profile and
  optional extra roots; the desktop connects to it by name.
- Drop `crossterm`, `ratatui`, `unicode-width`, and the root `dotenvy`
  dependency. The desktop keeps its own `.env` loading.
- Keep the engine boundary test, now guarding against any UI crate names.

## Steps

- [x] Delete the terminal modules, fixtures, script rig and TUI docs.
      Verify: no `src/view`, `src/render` or `tests/fixtures` remains.
- [x] Remove the CLI faces and the engine one-shot runner.
      Verify: `cargo check --all-targets`.
- [x] Remove `ui` from the rig schema and files; update `doctor`.
      Verify: `cargo run -- rig list`, `cargo run -- doctor`.
- [x] Update the ADRs, design, README, CLAUDE.md and changelog.
      Verify: no stale reference to a terminal face in the current docs.
- [x] Full verification.
      Verify: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
      `cargo test`, desktop `npm test`/`check`/`build`.

## Verification (2026-11-06)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test` pass.
- `cargo run -- doctor` loads both rigs.
- `grep` finds no terminal-face module or dependency outside the historical
  changelog and task records.
- Desktop tests, `svelte-check` and the Vite build still pass.

## Not in D2d

- New desktop features. The floating window remains D3.
- Keeping a hidden terminal fallback. A CLI face returns only with a concrete
  need and a new ADR.
