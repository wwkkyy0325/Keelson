# CLAUDE.md — Keelson

## Identity

Keelson is a host for the Keel agent kernel. It loads a rig (a file), builds
the kernel from it and gives it a face. It contains no turn loop, permission
gate or model adapter of its own.

The global guidelines in `~/.claude/CLAUDE.md` apply. This file adds the rules
that are specific here.

## Architecture

```
engine/   turn loop, permissions, storage, tools    (no UI types)
view/     transcript | chat | oneshot              (reads events, owns layout)
render/   ratatui, crossterm                       (the only terminal code)
```

- The engine may depend on `keel-*` and on neutral view types. It may not
  mention `ratatui` or `crossterm`; a test enforces this once `engine/` exists.
- A new face is a `view/` module selected by the rig's `ui` field, never a flag
  inside the engine and never a second binary.
- Rigs are files. Do not add an in-app settings screen, a theme picker or a
  per-rig toggle panel.
- No workspace index, vector store, embedding, graph database or cloud
  dependency. If a feature seems to need one, write the case in
  `docs/llmwiki/backlog.md` first.

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

## Language and git

- Code, comments, docs and commit messages in English; conversation in Chinese.
- Conventional commits, no AI traces, commit only when asked.
