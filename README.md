# Keelson

A shell host for the Keel agent kernel. One binary, several rigs.

A **rig** is a file. It names a kernel profile, a view and optional scopes
outside the workspace. The workspace itself comes from where you launch it.

```
$ keelson rig list
coding        transcript   Coding
$ keelson doctor
keelson 0.1.0  kernel 0.1.0  events 1  snapshot 1
rig dirs    rigs
rigs        1
  coding      transcript   profile coding (WorkspaceAuto)
```

Coding is the first rig and the reference for the shape. A second rig is a
file, not a new binary: the kernel already resolves a profile into an immutable
configuration per turn, so switching shells changes behavior, tools and face
without touching the engine.

## Rigs are files, not settings

There is no settings screen. `keelson rig show <name>` prints a rig as the host
resolves it; `keelson rig new <name>` (M2) copies a commented template and
opens it in `$EDITOR`. A rig is diffable, shareable and reproducible.

## Docs

- `docs/llmwiki/design.md` — what this is, the layers, the milestones.
- `docs/kernel.md` — the pinned kernel, the consumed surface, the upgrade path.
- `docs/adr/` — rigs as files, the frontend boundary.
- `docs/design/tui.md` — the terminal view specification.

## Status

M1: rig schema, discovery and `doctor` against kernel `v0.1.0`. The turn loop,
the terminal view and the coding tools follow; see the milestones in
`docs/llmwiki/design.md`.
