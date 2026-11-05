# 0001 — Rigs are files, not a settings screen

Status: accepted

## Context

An agent that carries many capabilities accumulates configuration: models,
tool lists, prompts, permissions, memory, per-feature switches. The usual
answer is a settings panel, and it fails the same way each time — the surfaces
grow, defaults stop being defensible, and the user has to configure the tool
before it does anything.

Keelson is deliberately built the other way. Its job is to switch *shells*, and
a shell must be something a person can read, diff and share.

## Decision

A rig is a TOML file on disk. The app loads it, resolves it against the kernel
profile, and runs it.

- The app offers `rig list`, `rig show <name>` and `rig new <name>`.
- `rig new` copies a commented template and opens it in `$EDITOR`.
- There is no in-app form, no theme picker and no per-rig toggle UI.
- Unknown keys are rejected so a typo fails loudly instead of being ignored.

## Consequences

- A rig is reviewable in a diff, shareable in a repository, and reproducible.
- Validation has to be excellent, because a mistake surfaces as an error
  message rather than a disabled control. Every schema error names the file,
  the key and the expected value.
- Discoverability is lower than a settings panel. Mitigation: the template
  lists every key with a default, and `doctor` reports a rig that loads but
  cannot run.
- Some values are naturally dynamic at runtime (permission escalation,
  interjections). These stay runtime actions on the command line, not fields
  in the file.
