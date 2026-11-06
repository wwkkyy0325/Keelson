# 0003 — A desktop shell

Status: accepted

## Context

M1–M3 built the rig host, the engine and the terminal transcript. The
transcript works, but the product owner does not use a TUI and the intended
personal use case is a desktop assistant that can later become a floating
window on Windows and macOS.

This conflicts with ADR 0002, which said the terminal stays the only shipped
face until a concrete limitation appears. The limitation is the user's own
workflow, not a rendering defect: a terminal face is the wrong shape for a
personal desktop assistant. The decision is recorded here rather than
silently contradicting the earlier ADR.

## Decision

- The engine becomes a library (`src/lib.rs`). The CLI binary and the desktop
  binary share it; both are thin hosts.
- `desktop/` is a Tauri v2 shell with a Svelte 5 + TypeScript frontend. Tauri
  owns the window, the event loop and the bundler, so it has its own binary.
  It is a face over the same engine, not a fork: `engine/` still contains no
  UI type and does not know the desktop exists.
- The rig's `ui` field keeps selecting the CLI face (`transcript`, `chat`,
  `oneshot`). The desktop connects to a rig by name; it does not add a branch
  inside the engine or a per-rig flag.
- The desktop owns no session state:
  - the backend serializes a session view and a keyset history page;
  - the frontend applies one reducer to a single `SessionState`;
  - live events carry a stable `turn_seq`, and the reducer upserts by it, so
    duplicate delivery and page/live overlap are harmless.
- History is paginated by `seq < before` with a stable cursor, never by
  offset. Appends cannot shift an open page.
- The terminal transcript and the one-shot CLI stay. The floating window is a
  later change to the same desktop shell.

## Consequences

- The "never a second binary" rule in ADR 0002 is amended: a shell with its
  own event loop may have its own entry point, provided the engine stays
  shared and UI-agnostic. The enforced engine boundary is unchanged.
- The repository gains a second toolchain (Node/Vite) for the desktop.
- Tauri's embedded webview is not Electron and is not a browser server; no
  HTTP surface is exposed.
- If a second desktop face is ever needed, it should be a frontend route or a
  second rig connection, not another engine path.
