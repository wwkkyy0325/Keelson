# CLAUDE.md — Keelson

## Identity

Keelson is a host for the Keel agent kernel. It loads a rig (a file), builds
the kernel from it and gives it a face. It contains no turn loop, permission
gate or model adapter of its own.

The global guidelines in `~/.claude/CLAUDE.md` apply. This file adds the rules
that are specific here.

## Architecture

```
src/engine/      turn loop, permissions, storage, tools    (no UI types)
src/rig.rs       rig schema, discovery and validation
src/main.rs      CLI diagnostics: rig list/show, doctor
desktop/         Tauri shell + Svelte frontend            (the only face)
```

- The engine may depend on `keel-*`, never on a UI crate. It may not mention
  `ratatui`, `crossterm`, `tauri` or `svelte`; `tests/engine_boundary.rs`
  enforces this (ADR 0004).
- The desktop is a separate binary because Tauri owns its event loop and
  bundler; it consumes the same library engine and adds no engine branch
  (ADR 0003). The CLI starts no session and runs no turn.
- Rigs are files. Do not add a rig form, a theme picker or a per-rig toggle
  panel. The settings page covers the model connection and language only.
- Do not add a terminal UI or keep a hidden fallback face. A second face
  returns only with a concrete need and a new ADR.
- No workspace index, vector store, embedding, graph database or cloud
  dependency. If a feature seems to need one, write the case in
  `docs/llmwiki/backlog.md` first.
- No Electron and no separate UI server. The desktop webview is embedded.

## Kernel relationship

- The kernel is pinned in `Cargo.toml` and documented in `docs/kernel.md`.
- A kernel change goes into the Keel repository behind a trait, with a
  contract test and an ADR. Do not patch around it in the host.
- `tests/kernel.rs` holds the conformance probes. They run on every kernel
  upgrade and must pass before a real turn is attempted.

## Verification before claiming done

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Plus `cargo run -- doctor` when the change touches rig resolution.

For `desktop/`:

```bash
cd desktop
npm test
npm run check
npm run build
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

## Language and git

- Code, comments, docs and commit messages in English; conversation in Chinese.
- Conventional commits, no AI traces, commit only when asked.
