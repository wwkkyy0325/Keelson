# 0061 — A spill belongs to one conversation

Status: accepted

## Context

A tool result longer than the kernel's cap is written to a file and the model is
told to read it back: `turn-<n>-call-<m>.txt`, where `n` is the conversation's
turn number and `m` the call index inside that turn. The host chose the
directory: `projects/<workspace-hash>/outputs/`, one per workspace.

Two conversations in one workspace therefore write the same names. Measured on
this project's state directory — one project, three conversations — eleven spill
names were referenced by more than one conversation, and eight spill files that
a turn row points at were gone from disk. An old turn's "read the full output"
marker either opens another conversation's text or fails.

The kernel cannot fix this: it is handed a directory and does what the host
says. `MAX_SPILL_FILES` (200) prunes that one directory, so the two
conversations also evict each other's files.

## Decision

- A conversation spills into `projects/<workspace-hash>/outputs/<session-id>/`,
  named after its document's file stem — the same id `screens/` already uses.
- The permission root stays the project's `outputs/`, unchanged. The kernel
  allows a read whose path starts with that root, so the spills written before
  this split — and the rows that point at them — keep resolving.
- Background commands move under the same folder (`<session-id>/background`).
  They are session-scoped: dropping the engine kills them, so nothing outside
  the conversation reads their logs.
- Files already in `outputs/` are left where they are. Moving or deleting them
  would break the rows that name them, and they are bounded by what was already
  written.

## Consequences

- Two conversations no longer overwrite each other's spills, and neither can
  evict the other's.
- The 200-file cap is now per conversation, so the total grows with the number
  of conversations in a workspace instead of staying at 200. A conversation's
  own oldest spills are still pruned, so a turn row can point at a file that is
  gone; that remains on the backlog.
- Nothing in the app resolves a spill path against the outputs directory: a
  turn row carries the absolute path, and the desktop previews what the row
  names. The move is invisible to the UI.
