# Keelson — design

Keelson is a host for the Keel kernel: one binary, several **rigs**, each rig a
file. Coding is the first rig and the reference for the shape. The app's job is
to load a rig, build the kernel from it, and give it a face. It is not a
settings application.

## Vocabulary

| Word | Meaning |
|---|---|
| kernel | The `keel-*` crates, pinned by tag. Mechanisms and invariants only. |
| rig | One shell: a profile, a view mode and optional extra scopes, in one file. |
| view | How a rig talks to the user: `transcript`, `chat`, `oneshot`. |
| host | This program. Loads rigs, runs turns, renders a view, stores sessions. |

A rig is data. Switching rigs changes behavior, tools and face; it does not
open a panel of toggles.

## Why this shape

The kernel already resolves a profile into an immutable `RuntimeConfig` per
turn, so a second rig costs a file, not a code path. The risk of a
shell-switching app is that it drifts into a settings screen — the exact
failure a thin agent is supposed to avoid. Two rules hold the line:

1. A rig is a file (ADR 0001). The app offers `rig list`, `rig show`,
   `rig new`; editing is `$EDITOR`. There is no form.
2. The engine never imports a terminal library (ADR 0002). The rig's `ui`
   field selects a view, and a view is a module, not a fork.

## Rig file

`rigs/coding.toml`:

```toml
name = "coding"
title = "Coding"
description = "Read, edit and verify a codebase."
ui = "transcript"
profile = "profiles/coding.toml"

# Extra scopes beyond the workspace. The workspace itself comes from the
# launch directory or an argument, never from the rig.
[[extra_root]]
path = "~/notes"
access = "ro"
```

- `name` is the handle on the command line: lowercase letters, digits and `-`.
- `profile` resolves relative to the rig file and is loaded by `keel-policy`.
- `ui` is `transcript` (default), `chat` or `oneshot`.
- `extra_root.access` is `ro` or `rw`; execution stays confirm-only.
- Unknown keys are an error, the same rule the profile schema uses.

Discovery: `$KEELSON_RIGS` (split as a path list) when set, otherwise `./rigs`
then `$KEELSON_HOME/rigs` (default `~/.keelson/rigs`). The first rig with a
given name wins; `doctor` reports the shadowed one.

## Frontend

Three layers, one direction of dependency:

```
engine/    turn loop, permissions, storage, tools   (no UI types)
   |
view/      transcript | chat | oneshot             (reads events, owns layout)
   |
render/    ratatui, crossterm                       (the only terminal code)
```

The kernel emits a typed event stream and streams tokens through a wrapped
provider; a view consumes both. Concretely:

- **Now**: terminal only. `transcript` is the ported Keel TUI, already covered
  by `TestBackend` goldens. `oneshot` is plain stdout for scripts and pipes.
- **Next**: `chat` for a second rig — answers only, tool lines behind one key.
- **Later, only on a real limit**: `keelson serve` plus a browser view, reusing
  the same engine and event stream. The trigger is a concrete failure of the
  terminal (long tables, images, copy-paste, a second device), not ambition.
- **Not planned**: native GUI, Electron, a second process for the UI.

The rule that keeps this honest is testable: no file under `src/engine/` may
contain `ratatui` or `crossterm`. That test lands with the first engine
module.

## Module layout

```
src/
  main.rs        argument parsing and command dispatch
  rig.rs         rig schema, discovery, validation
  doctor.rs      version handshakes and rig/profile checks
  engine/        M2: session loop, driver wiring, store
  view/          M3: transcript, chat, oneshot
  render/        M3: ratatui and crossterm
```

A module is added when it has content; no empty placeholders.

## Milestones

M1 — Shell (this iteration)

- repository, rig schema, discovery, validation, `rig list`, `rig show`,
  `doctor`, tests.
- Verify: `cargo test`, `cargo clippy -D warnings`, and `keelson doctor`
  loading `rigs/coding.toml` against the pinned kernel.

M2 — Engine

- Port the turn loop from the reference host: driver, permissions, event sink,
  session store, one `oneshot` run.
- Verify: a mock-model unit test plus one real-model turn in
  `examples/demo-workspace` that fixes the failing test.

M3 — Transcript view

- Port the terminal UI and the text table; keep the goldens.
- Verify: golden tests pass unchanged, then a manual run.

M4 — Coding rig does the work

- A search-replace edit tool, project search over `git`/ripgrep, automatic
  verification after edits.
- Verify: an eval case that edits and repairs `examples/demo-workspace` without
  a scripted prompt.

M5 — A second rig

- The proof that this is a shell host: one non-coding rig (`chat` view) built
  from a file, with no engine change.
- Starts only after M4, so the coding rig exposes what the engine actually
  needs.

## Non-goals

- An in-app settings screen, theme picker, keybinding editor.
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
