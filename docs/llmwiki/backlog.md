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
What is left:

- A save still rewrites the whole snapshot every turn — on this project's
  495 KB conversation the snapshot alone is 435 KB. It no longer fills the list
  path, but it still grows with the conversation. The first Keel change is in
  (ADR 0069 there: a turn buys at most 128 terms, and a term is at most 40
  characters); storing segments outside the snapshot is the rest of it.
- `tools[].output` pointing at the spill file under
  `projects/<hash>/outputs/` is still not an archive: those names carry no
  session id and `MAX_SPILL_FILES` (200) prunes by directory, so two
  conversations in one workspace overwrite each other. What the model sees of a
  tool output is `TOOL_HEAD_CHARS` + `TOOL_TAIL_CHARS` (3 000 characters,
  `keel-runtime/src/summary.rs`); 4 KiB is the host's live preview
  (`UI_PREVIEW_BYTES`), and 8 000 characters is the document's cap.
- Nothing compacts old conversations yet. The other half of this line used to
  be "nothing deletes a row directory whose header has been removed by an older
  build": that is now a sweep at index load, which moves an orphan snapshot or
  row directory into `.orphans/` instead of deleting it (ADR 0059).

