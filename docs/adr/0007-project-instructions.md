# ADR 0007 — Project instructions are workspace context

## Status

Accepted and implemented in `src/engine/mod.rs::project_instructions`.

## Context

A coding agent makes fewer avoidable mistakes when it knows the project's
build and test commands, layout and conventions. The established agents read
a repository file for this: Claude Code reads `CLAUDE.md`, Codex and most
cross-tool agents read `AGENTS.md`. Keelson had only the rig's persona, which
is shared by every project.

This is not a rig setting: two projects under the same coding rig need
different instructions. It is also not a settings panel: the instructions
live in the repository, are diffable and are reviewed with the code.

## Decision

1. **Read at engine open.** The host reads `AGENTS.md` from the workspace
   root. If it is absent or empty, it reads `CLAUDE.md`. If both exist,
   `AGENTS.md` wins.
2. **Append to the system prompt.** The text is appended after the rig's
   persona under a `# Project instructions (<file>)` heading. The kernel's
   context stages treat it as part of the system prompt.
3. **Bound the size.** At most 16 KiB is appended; a truncated file carries a
   marker. The cap keeps a large document from crowding out the turn.
4. **Root only.** Subdirectory instruction files are not loaded in this
   version. A monorepo can keep one root file; per-directory scoping is a
   later decision with its own token budget.
5. **Not a permission grant.** Instructions cannot widen the workspace, add
   an extra root or approve execution. The permission engine remains the
   boundary; an instruction is context, not authority.

## Consequences

- A cloned repository can put text into the system prompt. That is the same
  trust model as opening the repository in an editor, and the permission gate
  still governs what the agent may read, write or execute. The user should
  review `AGENTS.md` like any other project file.
- The instructions cost tokens on every turn. The 16 KiB cap is roughly 4k
  tokens, a small fraction of the 64k working set.
- A change to `AGENTS.md` takes effect when the engine is reopened, not in
  the middle of an existing conversation. That is consistent with the
  engine's immutable per-turn configuration.
- There is no in-app editor for the file. The repository is the source of
  truth; the user edits it with their normal tools.
