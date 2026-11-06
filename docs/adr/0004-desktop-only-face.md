# 0004 — The desktop is the only face

Status: accepted

## Context

ADR 0003 moved the product to a desktop shell but kept the terminal
transcript and the one-shot CLI. The owner does not use the TUI, and two faces
mean two sets of modules, dependencies, goldens and docs, none of which earn
their place in a personal desktop assistant.

The rig's `ui` field was a selector for those faces. With one face left, the
field no longer selects anything.

## Decision

- Delete the terminal faces: `src/view/`, `src/render/`, the TUI goldens,
  `docs/design/tui.md`, the `script` rig and the `run` subcommand.
- The CLI keeps diagnostics only: `rig list`, `rig show`, `doctor`,
  `--version` and `--help`. It starts no session and runs no turn.
- Delete the one-shot engine runner (`engine::run`, `RunRequest`,
  `TurnReport`). A host opens an `Engine`, supplies its interaction port and
  optional delta channel, and calls `run_turn`.
- Remove `ui` and `UiMode` from the rig schema. A rig is a profile plus
  optional extra roots, and the desktop connects to it by name.
- Keep `tests/engine_boundary.rs`: the engine may not mention a UI crate
  (`ratatui`, `crossterm`, `tauri`, `svelte`).

## Consequences

- ADR 0002's three-layer `engine | view | render` layout is superseded. Its
  durable rule survives: the engine is UI-agnostic, and a test enforces it.
- The "terminal transcript and the one-shot CLI stay" bullet in ADR 0003 is
  superseded. The desktop decision and the history design in ADR 0003 are
  unchanged.
- The root crate no longer depends on `crossterm`, `ratatui` or
  `unicode-width`.
- A second face may return only with a concrete need and a new ADR; it is not
  kept dormant "just in case".
