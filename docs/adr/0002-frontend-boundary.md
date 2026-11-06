# 0002 — The engine does not know the terminal

Status: superseded by ADR 0004. The `view/` and `render/` layers were removed
with the terminal faces; the engine-boundary rule remains in force.

## Context

Keelson will carry several rigs, and different rigs want different faces: a
coding rig needs a transcript with tool lines and expandable output, a writing
or secretary rig needs compact answers, a script needs one line on stdout.
Adding a face is cheap only if the turn loop does not know about any of them.

The tempting shortcut is to let the loop push straight into the terminal
buffer: fewer types, less plumbing. It works until the second face appears,
and then the loop grows a mode flag — the beginning of a renderer with
`if` branches for every rig.

## Decision

Three layers with one dependency direction:

```
engine/   turn loop, permissions, storage, tool wiring
view/     transcript | chat | oneshot — reads events, owns layout and wording
render/   ratatui, crossterm — the only code that touches the terminal
```

- The rig's `ui` field selects a view. A view is a module in this repository,
  not a new binary and not a flag inside the engine.
- `engine/` may depend on `keel-*` and on `view`'s neutral types, never on
  `ratatui`, `crossterm` or a terminal escape.
- A test scans `src/engine/` for those two names and fails the build on a
  match. This is the same shape as the kernel's own boundary test.
- Tokens reach a view through a wrapped `ModelProvider`; the kernel's event
  stream carries boundaries, not tokens.

## Consequences

- A browser view later is `view/web` plus `serve`, reusing the engine and the
  event stream. No engine change is expected.
- The neutral surface between engine and view has to be defined once: the
  view sees session state, events, streaming text and a small command enum
  (send, cancel, interject, approve). That surface is the only place to touch
  when a view is added.
- The terminal stays the only shipped face until a concrete limitation
  appears. `oneshot` covers scripts and pipes in the meantime.
