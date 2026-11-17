# 0059 — An orphan sidecar is moved aside

Status: accepted

## Context

ADR 0057 and ADR 0058 split one document into three names: `s-<id>.json` is the
header, `s-<id>.snapshot.json` the kernel snapshot, `s-<id>.turns/` the rows.
Every sidecar is named after its header, so a header that goes away without
them leaves both behind.

That is not hypothetical. A build from before the split deleted a conversation
by removing the document it knew about, and after the split the same deletion
still only knew the header. The list never showed the leftovers — the id filter
rejects `s-<id>.snapshot`, and a directory is not a document — so nothing ever
removed them. They are also the largest files in the directory: on this project
one orphaned snapshot was 1.5 MB.

## Decision

- `Store::sweep_orphans(dir)` looks for `*.snapshot.json` files and `*.turns`
  directories and moves the ones whose `*.json` header is missing into
  `<dir>/.orphans/`.
- It is called when a project's index is loaded, which is the moment a project
  directory is read at all. It is best effort: a sweep that cannot run does not
  fail the listing.
- Nothing is deleted. A name that is already taken in `.orphans/` gains a
  numeric suffix. The files stay readable until a person looks at them and
  removes the directory, because a header missing for a reason nobody recorded
  is not a reason to destroy the conversation it named.
- `.orphans/` starts with a dot and holds no `*.json` at the top level, so the
  id filter and the index walk past it.

## Consequences

- The only files in a project directory that grow without a conversation are
  the ones that belong to one.
- A sweep moves files during a read of the project. It only ever touches
  names no document claims, so a concurrent turn cannot see its own sidecars
  move.
- Removing a conversation with a build that predates this one still leaves a
  sidecar behind; the next listing puts it in `.orphans/` instead of leaving it
  in the way.
