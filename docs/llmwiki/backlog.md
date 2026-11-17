# Backlog

Cases that are not scheduled yet, with the evidence and the first step.

## Two different timeouts share one word

`tool_timeout` in a rig is a wall-clock deadline: the dispatcher wraps the whole
`tool.call` in `tokio::time::timeout` (`keel-runtime/src/dispatch.rs`). A
verification command's window is an inactivity window: any byte on stdout or
stderr resets it, and the total duration is unbounded
(`keel-runtime/src/command.rs` documents this, but the field is still called
`timeout`). The same `cargo test` therefore dies at `tool_timeout` under
`shell` and never dies under `[verification]`, and a user reading "timed out"
cannot tell which clock struck.

Settled: the verification window is its own number and follows the rig —
`[verification] command_timeout_ms`, default 60 s, read through
`Policy::verification_timeout` (ADR 0065 in the Keel repository). It is not
`min(60 s, tool_timeout)` and it is not a wall-clock deadline.

Still open: name the two after what they measure (`deadline`, `idle_window`)
and make `doctor` print the effective value of each.

## A workspace walk has a result cap but no visit budget

`MAX_GLOB_COLLECTED` bounds hits, not entries visited. A pattern with no match
and a root outside the workspace walks to the end: `glob` over
`/Users/deepspace` for `.zshrc` took 64.8 s. There is a depth cap (32) and a
skip list, and the walk honours `.gitignore`, but nothing counted work.

A budget now exists (`WalkBudget`, 200 000 entries or 5 s, ADR 0064 in the Keel
repository) and a stopped walk says "... stopped after N entries visited".

What is still open: a root outside the workspace is walked without asking, at
the "full access" permission level. Counting the work made that survivable, not
correct — the confirmation should come from the permission request, not from
the walk running out.

## One session document, rewritten every turn

Done: a turn row is its own file (ADR 0057). `s-<id>.json` is a header and
`s-<id>.turns/<seq>.json` holds the rows, each written once, so a save costs
one turn instead of the whole conversation. Done since: the list reads the
header alone (`read_header`, `turn_count`, `first_turn`), and the kernel
snapshot has its own file (`s-<id>.snapshot.json`, ADR 0058), so opening a
project no longer parses history or state to count and name conversations.
Done too: a segment is its own file, written once and rewritten only when it
changes (ADR 0060), and a turn buys at most 128 terms (ADR 0069 in the Keel
repository). A save now costs the state file — the memo, the facts and the
counters — plus the segment that grew, instead of the whole conversation.
What is left:

- The state file still holds the memo and every fact. What that costs, and
  why compacting it is not the win it looks like, is measured below.
- `tools[].output` pointing at the spill file under
  `projects/<hash>/outputs/<session-id>/` is still not an archive: the folder is
  per conversation since ADR 0061, so two conversations no longer overwrite each
  other, but the kernel keeps only the newest 200 `turn-*-call-*.txt` in it and
  an older marker then points at a file that is gone. Spills written before the
  split sit in `outputs/` itself and are never pruned. What the model sees of a
  tool output is `TOOL_HEAD_CHARS` + `TOOL_TAIL_CHARS` (3 000 characters,
  `keel-runtime/src/summary.rs`); 4 KiB is the host's live preview
  (`UI_PREVIEW_BYTES`), and 8 000 characters is the document's cap.
- Nothing compacts old conversations yet. The other half of this line used to
  be "nothing deletes a row directory whose header has been removed by an older
  build": that is now a sweep at index load, which moves an orphan snapshot or
  row directory into `.orphans/` instead of deleting it (ADR 0059).

## The archive grows, the memory does not

Measured over the nine conversations in this project's state directory
(`~/.keelson/sessions`, 2026-11-17), 21.8 MB in total:

- Turn rows, 19.2 MB — 88%. `s-<id>.turns/<seq>.json`, one file per turn,
  written once and never pruned. One 52-turn conversation is 10.3 MB of them.
- Segment files, 2.3 MB. Terms dominate: 0.45 MB across all nine, and 97% of
  this project's own 221 KB — one segment recorded before the term cap holds
  15,578 of them.
- State files, 0.30 MB: the memo, the facts and the counters. The largest is
  32.7 KB, the smallest 191 bytes.

The memory the model reads is bounded where the archive is not:

- The memo is capped at `working_set / 10`, clamped to 256..4096 tokens, and
  rewritten every third seal (`keel-runtime/src/summarizer.rs`).
- The facts are not capped. The largest conversation carries 130 of them,
  21 902 bytes, and the memory stage puts all of them into every prompt. This
  is the one memory item that grows with the conversation.
- Turn rows never enter a prompt. The kernel rebuilds history from segments and
  clears the entries of a segment once a summary covers it; after that the row
  file is the only verbatim copy of what was said.

So the three candidate changes cost different things:

- Compacting the state buys under 33 KB and risks the least recoverable data. A
  fact is a user statement extracted once; dropping it loses it, because recall
  matches segment terms and never facts.
- Pruning turn rows buys 88% of the disk and costs the scrollback: the app
  restores a conversation from those rows, and nothing else holds the text of
  an absorbed segment.
- Capping the facts buys prompt tokens on every turn. Of the three, it is the
  only one that is a context win rather than a disk win.

The first step is a retention policy with a visible limit, not a compaction
pass: keep the last N turn rows verbatim and mark on the conversation that
older rows are gone, and cap the facts at N with confirmed and user-explicit
entries kept first. Both are destructive, so both need a number the user can
see and change, not a silent sweep.

Wanted: no (2026-11-17). All three are left as they are. 21.8 MB across nine
conversations is accepted: the largest state file is 32.7 KB, the memo is
already bounded, and the one unbounded memory item — the facts list — costs
roughly 5.5 k tokens of a 64 k working set in the largest conversation. Each
candidate trades something unrecoverable for a number that is not hurting: a
fact is extracted once and cannot be re-derived, and a pruned turn row is the
only verbatim copy of a segment the kernel has absorbed. The numbers that would
reopen this are a state file past a few hundred KB, or facts past about a tenth
of the working set; neither is close. The 15 578-term segment recorded before
the term cap (ADR 0069 in the Keel repository) stays as history.

## The trash keeps every version forever

Every write, edit, move and delete backs the old file up into
`projects/<hash>/trash/` before touching it, and nothing ever removes one.
Measured over the six projects in this state directory (2026-11-17, version files
plus their `meta/` sidecars): 31 MB in the trash, 1 012 backed-up versions,
against 21.8 MB of conversations — one project holds 19 MB of it and 727
versions, this one 8.4 MB and 204. Screenshots in the neighbouring `screens/`
folder are pruned twice, by count and by age (`KEEP_SCREENS`,
`prune_old_screens`, seven days); the trash has neither.

The undo file is the whole reason `write_file` may overwrite a file the agent
has not read, so this is not a candidate for removal. What is missing is a
bound: the newest N versions of a path (the entry id already carries the path
hash and the timestamp, so versions of one file are recognisable) and an age
limit for the rest, printed the way the screenshot limit is. A version and its
`meta/` sidecar have to go together, or the listing starts naming backups that
are already gone. It is a deletion
of a deletion's safety net, so the number belongs where the user can see it.

