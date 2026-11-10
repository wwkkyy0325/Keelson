# ADR 0034 — The desktop may override the rig's permission level

## Status

Accepted.

## Context

The desktop needs to show the permission level next to the composer and let the
user switch between `workspace_auto` and `full_auto` without editing a rig
file. Rigs are files and the app deliberately does not edit them or grow
per-rig behavior toggles (see `CLAUDE.md`). The profile's `[permission] level`
was the only source of truth, so a switch needed a host-level decision.

## Decision

1. The permission level is the one behavior the host may override. The
   override lives in `DesktopConfig.permission` (`workspace_auto` or
   `full_auto`), never in a rig file and never in the session document.
2. It is applied when the engine opens: the rig profile supplies the default,
   and a host override replaces it before the permission engine is built. No
   other profile value is touched.
3. The desktop shows the effective level in the composer status line. Choosing
   a level saves the config and reopens the session, so the switch takes effect
   immediately and is visible where it happened.
4. Rigs still own everything else: tools, prompt, verification and context. No
   other behavior toggles are added.

## Consequences

- The app can change what runs without editing a rig. This is a deliberate,
  narrow exception to the "rigs are files" rule, and it stays visible in the
  status line rather than hiding in a file.
- `full_auto` removes confirmations. The menu states that consequence, and the
  switch requires an explicit choice rather than a cycle-through control.
- A running turn blocks the switch because the engine must reopen first.

## Alternatives rejected

- Editing the rig file from the app: silent writes to a user file, and a second
  source of truth.
- A per-rig toggle: expands into a rig editor, which is explicitly out of scope.
- Read-only display: the product needs the switch in the composer.
