# M2 — the engine

> Historical record. The one-shot view, `RunRequest` and the `run` subcommand
> were removed in D2d (ADR 0004); `Engine` is now the only entry point.

Goal: a rig runs one turn through the kernel, the session survives across
runs, and a `oneshot` view puts the answer on stdout. No terminal UI.

## Decisions

- The session store is one JSON document per session under
  `<state>/sessions/<id>.json`, with `<state>` from `$KEELSON_HOME` (default
  `~/.keelson`). The reference host's SQLite tables are a read model; Keelson
  has no reader that needs them yet. The kernel snapshot stays opaque and is
  serialized as-is.
- The engine returns a neutral `TurnReport`; a view owns the wording and the
  stream it writes to. No file under `src/engine/` may name a terminal library.
- `keelson run <rig>` selects the face from the rig's `ui` field. M2 implements
  `oneshot`; `transcript` and `chat` report that they arrive with M3.
- Interactions are deferred (`DeferAllInteractionPort`): a one-shot run has no
  prompt and every permission that needs a person is denied by the kernel's
  policy rather than answered by a fake yes.

## Steps

- [x] Dependencies and an async entry point.
      Verify: `cargo build` succeeds.
- [x] Session store: load, save, resume, refuse another workspace.
      Verify: `store::tests` pass.
- [x] Engine wiring: profile, permissions, extra roots, tools, stages, model
      selection (mock, OpenAI, Anthropic), driver, snapshot resume, one turn.
      Verify: `engine::tests` — a mock turn returns its answer and writes one
      turn; a second run resumes and appends turn 2.
- [x] `oneshot` view and `keelson run <rig>` with `--input`, `--session-id`,
      `--store`.
      Verify: `cargo run -- run script --input hello` prints the mock answer;
      piped stdin gives the same result.
- [x] Frontend boundary test for `src/engine/`.
      Verify: `cargo test --test engine_boundary`.
- [x] `KEELSON_HOME` applies to rig discovery and the session store.
      Verify: `rig::tests`; `cargo run -- doctor`.
- [x] Docs, changelog and the M2 task record.
      Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test`, `cargo run -- doctor`.

## Verification (2026-11-05)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 13 unit tests, 1 boundary test, 5 kernel probes pass.
- `keelson doctor`: two rigs load their profiles against kernel `0.1.0`.
- `keelson run script --input hello` and a piped run both answer from the
  mock provider; a second run with the same session id appends turn 2 and
  keeps the first turn's events.
- Not run here: a real-model turn needs `OPENAI_API_KEY` or
  `ANTHROPIC_API_KEY` and a profile that selects the provider.

## Not in M2

- The transcript and chat views, and any terminal code.
- The SQLite read model, event queries, forks and views of the reference host.
- MCP plugins, bundled jobs, acceptance checklists, the keyring and `rig new`.
