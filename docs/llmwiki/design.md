# Keelson — design

Keelson is a host for the Keel kernel: one library engine, several **rigs**,
and a desktop face. The desktop is the product. The CLI is a diagnostics
tool for rigs and the kernel handshake, not a second interface.

## Vocabulary

| Word | Meaning |
|---|---|
| kernel | The `keel-*` crates, pinned by path. Mechanisms and invariants only. |
| rig | One shell: a profile and optional extra scopes, in one file. |
| host | The desktop app. Loads a rig, runs turns, renders the conversation, stores sessions. |

A rig is data. Switching rigs changes behavior, tools and scopes; it does not
open a panel of toggles.

## Why this shape

The kernel resolves a profile into an immutable `RuntimeConfig` per turn, so a
second rig costs a file, not a code path. The risk of a shell host is that it
drifts into a settings application — the exact failure a thin agent is
supposed to avoid. Two rules hold the line:

1. A rig is a file (ADR 0001). The CLI offers `rig list` and `rig show`;
   editing is `$EDITOR`. There is no rig form in the app.
2. The engine contains no interface code (ADR 0004). The desktop is a host
   over the library, and `tests/engine_boundary.rs` rejects any UI crate name
   in `src/engine/`.

## Rig file

`rigs/coding.toml`:

```toml
name = "coding"
title = "Coding"
description = "Read, edit and verify a codebase."
profile = "../profiles/coding.toml"

# Extra scopes beyond the workspace. The workspace itself comes from the
# folder picked in the app, never from the rig.
[[extra_root]]
path = "~/notes"
access = "ro"
```

- `name` is the handle: lowercase letters, digits and `-`.
- `profile` resolves relative to the rig file and is loaded by `keel-policy`.
- `extra_root.access` is `ro` or `rw`; execution stays confirm-only.
- Unknown keys are an error, the same rule the profile schema uses.

Discovery: `$KEELSON_RIGS` (split as a path list) when set, otherwise `./rigs`
then `$KEELSON_HOME/rigs` (default `~/.keelson/rigs`). The first rig with a
given name wins; `doctor` reports the shadowed one.

## Frontend

The dependency direction is one line:

```
desktop/   Tauri commands, session, keychain, Svelte components
   |
src/lib.rs engine: turn loop, permissions, storage, tools   (no UI types)
```

- **Desktop**: `desktop/` is a Tauri v2 shell with a Svelte 5 frontend (ADR
  0003). `src-tauri/` owns the session, serializes a view model and paginates
  history by turn sequence; `src/` is a controller plus independent
  components and applies one reducer. The engine is a library.
- **First launch**: a four-page wizard (key → endpoint → language → finish).
  The key goes to the OS keychain, the endpoint becomes a `ModelOverride`,
  and the workspace is chosen inside the app, not during setup. All wizard
  steps share one grid cell, so the card height never jumps.
- **Later settings**: a **single page**, not the wizard — a slide-over with
  model/key, language and workspace information. Chinese is the default; one
  dictionary and one `locale` store leave the English switch available.
- **CLI**: `keelson rig list`, `rig show`, `doctor`, `--version`. It starts no
  session and runs no turn. There is no terminal UI.
- **Next**: the floating-window form of the desktop shell.
- **Not planned**: Electron, a separate browser/UI server, or a hidden
  terminal fallback. The Tauri webview is embedded, and no HTTP surface is
  exposed.

The boundary is testable: no file under `src/engine/` may contain `ratatui`,
`crossterm`, `tauri` or `svelte`. `tests/engine_boundary.rs` enforces it.

## Sessions

One run is one turn. The desktop stores a session as one JSON document at
`<state>/sessions/<workspace-hash>/default.json`, with `<state>` from
`$KEELSON_HOME` (default `~/.keelson`). It holds the kernel snapshot, the task
list and one row per turn with the turn's events. The workspace is recorded in
the document, so a session is refused against a different project; the
workspace hash keeps separate projects from ever sharing a document.

This is not the reference host's SQLite read model. The desktop reads turns
through `Engine::history`, a keyset page over the in-memory document; it never
parses the kernel events. A table layer can still be added behind the same
snapshot if a reader ever needs queries the page API cannot answer.

## Module layout

```
src/
  lib.rs         the library the CLI and the desktop link
  main.rs        the CLI: rig and kernel diagnostics only
  rig.rs         rig schema, discovery, validation
  doctor.rs      version handshakes and rig/profile checks
  engine/        turn execution, driver wiring, session store
desktop/
  src-tauri/     Tauri commands, config/keychain, events, interaction port
  src/           controller + components: Wizard, Chat, AppHeader,
                 Transcript, Turn, Composer, PermissionCard, SettingsPanel
  src/lib/       one session reducer store and one i18n locale store
```

A module is added when it has content; no empty placeholders.

## Milestones

M1 — Shell (done)

- repository, rig schema, discovery, validation, `rig list`, `rig show`,
  `doctor`, tests.
- Verify: `cargo test`, `cargo clippy -D warnings`, and `keelson doctor`
  loading the rigs against the pinned kernel.

M2 — Engine (done)

- The turn loop: driver, permissions, event sink, session store, mock and
  live model selection.
- Verify: the mock-model unit tests and the kernel conformance probes. A
  real-model turn needs a provider key and is a manual check.

M3 — Terminal view (removed)

- The ported terminal transcript, its goldens and the one-shot CLI existed
  here and were removed in D2d (ADR 0004). The history remains in the
  changelog; the code is gone.

M4 — Coding rig does the work

- A search-replace edit tool, project search over `git`/ripgrep, automatic
  verification after edits.
- Verify: an eval case that edits and repairs `examples/demo-workspace` without
  a scripted prompt.

M5 — A second rig

- The proof that this is a shell host: one non-coding rig built from a file,
  with no engine change.
- Starts only after M4, so the coding rig exposes what the engine actually
  needs.

D1 — Basic desktop shell (done)

- Tauri v2 + Svelte 5: setup screen, conversation, streamed answer, tool
  details, permission card, cancel and keyset history pages. The engine is a
  library; ADR 0003 records the desktop decision.

D2 — First-run onboarding (done)

- Four-page wizard, key in the OS keychain, model override, Chinese-default
  i18n with an English switch, and a workspace picker inside the app.

D2b — One-page settings (done)

D2c — Component split and a stable page stage (done)

D2d — Terminal faces removed (done)

- `src/view/`, `src/render/`, the TUI goldens, the `script` rig, the `run`
  subcommand and the rig's `ui` field are gone. The CLI keeps diagnostics.

D3 — Floating window

- Frameless, always-on-top, tray and a global shortcut on Windows and macOS.
- Starts after D2 proves the connect/send/history loop.

## Non-goals

- A terminal UI, or any second face kept without a concrete need.
- A paged settings wizard after the first run; a theme picker; a keybinding
  editor.
- Electron, a separate UI server, or a browser-process frontend.
- A plugin marketplace; MCP is the extension surface and the kernel already
  speaks it.
- Embeddings, a vector store, a graph database, an index of the workspace.
- Cloud sessions, accounts, telemetry.
- Multi-agent orchestration beyond the kernel's sub-agent tool.

## Kernel relationship

Keel `v0.1.0` is the pinned release; `docs/kernel.md` records the consumed
surface and the upgrade checklist. If a rig needs a kernel change, that change
goes into Keel behind a trait, with a contract test and an ADR — a host-only
workaround is the signal that the kernel is not thin enough.
