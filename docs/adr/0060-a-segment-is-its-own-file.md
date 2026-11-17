# 0060 — A segment is its own file

Status: accepted

## Context

ADR 0058 took the snapshot out of the header, so the list stopped parsing the
conversation to paint a row. It moved the cost, it did not remove it: every
turn still rewrites the whole snapshot file, and the snapshot is mostly
segments — the entries, the terms and the record of every topic span the
conversation has had.

Measured on the conversations in this project's state directory, one of them
had a 1.5 MB snapshot and another 232 KB, and the terms alone were 53% of it.
A conversation that is not being advanced is not being written, but the one
that is pays for all of its history on every save.

The segments themselves are already shaped for this: the kernel writes them as
`SegmentSnapshot` values, a sealed segment is never reopened, and the snapshot
is documented as "a serializable view of one session, for a host store". Which
file holds which part is the host's decision, not the kernel's.

## Decision

- Each segment lives in `s-<id>.segments/<segment-id>.json`, beside the state
  file. `s-<id>.snapshot.json` keeps everything else: the counters, the memo,
  the facts, the format version — and an empty segment list.
- `s-<id>.segments/manifest.json` names the segments the snapshot has, with a
  hash of what was written for each. A save serializes every segment, compares
  its hash with the manifest, and writes the segment file only when the hash
  differs. An unchanged segment costs no file I/O at all.
- The manifest is written after the segment files, so a save that dies in the
  middle leaves a segment file the manifest does not name. Such a file is
  ignored on read and removed by `prune_segments` on the next save, which also
  removes segments the kernel absorbed into a summary.
- A segment the manifest names but the disk does not have is an error that
  names the id and the path, the same way a missing snapshot is. A conversation
  restored without its largest segment would answer as if it had never
  happened.
- A snapshot written before this change carries its segments inline. It is read
  as it is; the next save writes them out and clears the list. That is the
  whole migration.
- The orphan sweep treats `s-<id>.segments` as a sidecar named after its
  header, like `s-<id>.turns`, and `Store::delete` removes it.

## Consequences

- A save writes the state (facts, memo, counters), the open segment, and, once,
  each segment that seals. It no longer writes the conversation.
- Reading a conversation now opens one file per segment. The nine documents
  in this state directory were rewritten in place: the largest snapshot was
  1,535,386 bytes and is now a 5,636-byte state file plus ten segment files,
  and this project's own conversation went from 232,576 bytes to a 10,917-byte
  state file plus nineteen segment files.
- The hash is `DefaultHasher` over the segment's JSON. It is only ever compared
  with hashes in the same manifest, so a hash algorithm change costs one rewrite
  of every segment in a conversation, never a wrong read.
- A manifest that cannot be parsed is treated as empty: the next save rewrites
  every segment and rewrites the manifest. Losing the manifest loses no segment.
- The state file still carries the memo and the facts, which grow with the
  conversation. Compacting old conversations is the remaining item on the
  backlog, and it is a kernel decision about what a summary may replace.
