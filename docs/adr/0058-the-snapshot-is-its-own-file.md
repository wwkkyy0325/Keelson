# 0058 — The snapshot is its own file

Status: accepted

## Context

ADR 0057 took the turns out of the session document: `s-<id>.json` became a
header and each turn its own row under `s-<id>.turns/`. What stayed in the
header is the kernel's snapshot, and the snapshot is the part that grows with
the conversation. On the conversations in this project's state directory the
nine headers add up to 19 MB, and listing a project parses all of them to paint
nine rows.

The list only needs the workspace, the rig, the title, the archive flag and the
row count. It was reading half a megabyte of kernel state per conversation to
get five short fields.

## Decision

- The snapshot lives in `s-<id>.snapshot.json`, beside the header. The header
  keeps what the list needs and nothing that grows with the conversation.
- `Store::read_header` reads that header and never opens the snapshot. It is
  what the list, and the row refreshed after a turn, now use.
- `Store::read_document` reads the header and then the snapshot, and is what
  resuming a conversation uses. `Store::load` adds the turn rows to it.
- The snapshot file is written before the header. A crash between the two
  leaves the old header with the new snapshot — the state resume reads — rather
  than a header pointing at a file that is not there.
- A document written before this change carries its snapshot inline and is read
  from there; the next save writes the separate file. That is the whole
  migration.
- A header without either is an error that names the missing path. A session
  silently restored with an empty snapshot would lose the conversation while
  still opening.

## Consequences

- Listing parses a few hundred bytes per conversation instead of the whole
  state.
- A save still rewrites the whole snapshot every turn. This change moves the
  cost out of the list path; it does not remove it. Bounding the snapshot is a
  Keel change — ADR 0069 there is the first one, a term budget that took the
  same 495 KB conversation's snapshot from 435 KB of segments down to a size
  that no longer tracks what was pasted into it.
- `s-<id>.snapshot.json` sorts next to the header, and the list's id filter
  rejects it because `s-<id>.snapshot` is not a valid id.
