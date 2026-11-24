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
  split sit in `outputs/` itself and are never pruned: 118 files, 0.86 MB
  across the six projects (2026-11-23), which is small but is also the only set
  that no code path ever looks at again. What the model sees of a
  tool output is `TOOL_HEAD_CHARS` + `TOOL_TAIL_CHARS` (3 000 characters,
  `keel-runtime/src/summary.rs`); 4 KiB is the host's live preview
  (`UI_PREVIEW_BYTES`), and 8 000 characters is the document's cap.
- Nothing compacts old conversations yet. The other half of this line used to
  be "nothing deletes a row directory whose header has been removed by an older
  build": that is now a sweep at index load, which moves an orphan snapshot or
  row directory into `.orphans/` instead of deleting it (ADR 0059).

## The archive grows, the memory does not

Measured over the 32 conversations in this state directory (`~/.keelson/sessions`,
11 project folders, 2026-11-23), 61.5 MB in total:

- Turn rows, 57.7 MB — 94%. `s-<id>.turns/<seq>.json`, one file per turn,
  written once and never pruned. One 94-turn conversation is 15.7 MB of them
  in 94 files. A row is mostly its tool calls: in the largest single row
  (275 KB) the `tools` list is 256 KB, and that is 119 stored `output`
  previews — 1.9 KB on average, up to 4.1 KB where the host's preview cap
  stops them — of an output the kernel showed the model 3 000 characters of.
- Segment files, 3.6 MB. The largest file is 1.4 MB: one segment holding
  1.1 MB of entries and 15,578 terms, recorded before the per-turn term cap
  (ADR 0069 in the Keel repository). The same 15,578 terms appear again in a
  second segment of this project, a 140 KB file with two entries.
- Snapshots and headers, 0.43 MB: 0.21 MB of `s-<id>.snapshot.json` and
  0.22 MB of `s-<id>.json`. The largest snapshot is 52 KB, the smallest 4 KB.

Opening a conversation parses every row file it has (`Store::load_unchecked`
→ `read_turns`, `src/engine/store.rs:473`), and the page the desktop asks for
is sliced out of that in-memory list (`Engine::history`), so the per-conversation
cost lands at open: 15.7 MB of JSON for the 94-turn conversation, not 30 rows
of it. Paging bounds what crosses into the frontend, not what is read.

The memory the model reads is bounded where the archive is not:

- The memo is capped at `working_set / 10`, clamped to 256..4096 tokens, and
  rewritten every third seal (`keel-runtime/src/summarizer.rs`).
- The facts block stops at a quarter of the stage allowance; since Keel ADR
  0071 it keeps the newest facts, and any confirmed or user-explicit one, ahead
  of the rest, and reports what it left out. Re-measured 2026-11-23: the largest
  conversation carries 197 facts, 39 KB, about 9.8 k tokens against a quarter of
  what the stages ahead of it leave, 16 k at most in a 64 k working set, and
  across all 16 snapshots the largest block is 3 178 tokens against a 5 952
  budget — nothing clips yet. This is the one memory item that grows with the
  conversation.
- Turn rows never enter a prompt. The kernel rebuilds history from segments and
  clears the entries of a segment once a summary covers it; after that the row
  file is the only verbatim copy of what was said.

So the three candidate changes cost different things:

- Compacting the state buys 0.2 MB of snapshots, the largest 52 KB, and risks
  the least recoverable data. A fact is a user statement extracted once;
  dropping it loses it, because recall matches segment terms and never facts.
- Pruning turn rows buys 94% of the disk and costs the scrollback: the app
  restores a conversation from those rows, and nothing else holds the text of
  an absorbed segment. Shrinking the stored `tools[].output` preview bounds the
  same number without losing a row, at the cost of a transcript that shows less
  of an old command than it does today.
- Reordering the facts block buys nothing on disk and keeps the newest facts in
  the prompt. It is the only one of the three that is a context change.

For the turn rows the first step is a retention policy with a visible limit,
not a compaction pass: keep the last N rows verbatim and mark on the
conversation that older rows are gone. That is destructive, so it needs a
number the user can see and change, not a silent sweep.

Wanted: no for the disk items (2026-11-23). 61.5 MB across 32 conversations is
accepted: the largest snapshot is 52 KB and the memo is already bounded. Each
of the two disk changes trades something unrecoverable for a number that is not
hurting — a pruned turn row is the only verbatim copy of a segment the kernel
has absorbed. The numbers that would reopen them are a conversation past about
50 MB of rows, or an open a user notices waiting for; neither is close. The two
segments recorded before the term cap (ADR 0069 in the Keel repository) stay as
history.

Landed (2026-11-23), the half that loses no row: the step log now clips a call's
`args` and `output` to `ROW_PREVIEW_CHARS` = 1 024 characters and says so in the
file (`desktop/src-tauri/src/records.rs`, ADR 0074). Measured over the 368 turn
files here — 31 699 calls, 39.5 M output characters, 58.2 MB — that keeps 47.8%
of the output characters against 75.1% at 2 048 and 28.2% at 512; the median call
is 665 characters and is stored unchanged. The live stream still shows the full
`UI_PREVIEW_BYTES` = 4 096. Rows keep their own retention question, still
wanted: no.

Reopened (2026-11-23): the facts block is bounded already at a quarter of the
stage allowance, so the fix is not a cap but the order and the report — keep
the newest facts and any confirmed or user-explicit one ahead of the rest, and
say on the transcript how many were left out. Dropping them from the snapshot
would trade something unrecoverable for a number that is not hurting;
truncating the block needs no such trade.

Done (2026-11-23): the order and the report are in Keel ADR 0071, and the count
reaches the conversation in ADR 0075 here.

## The trash keeps every version forever

Every write, edit, move and delete backs the old file up into
`projects/<hash>/trash/` before touching it, and nothing ever removes one.
Measured over the six projects in this state directory (2026-11-23, version
files plus their `meta/` sidecars): 121.6 MB in the trash, 8 798 files, against
61.5 MB of conversations — this project alone holds 17 MB of it and 502
versions. Screenshots in the neighbouring `screens/` folder are pruned twice,
by count and by age (`KEEP_SCREENS`, `prune_old_screens`, seven days); the
trash has neither.

The undo file is the whole reason `write_file` may overwrite a file the agent
has not read, so this is not a candidate for removal. What is missing is a
bound: the newest N versions of a path (the entry id already carries the path
hash and the timestamp, so versions of one file are recognisable) and an age
limit for the rest, printed the way the screenshot limit is. A version and its
`meta/` sidecar have to go together, or the listing starts naming backups that
are already gone. It is a deletion
of a deletion's safety net, so the number belongs where the user can see it.

Done (2026-11-23): Keel grew `TrashStore::prune(&Retention)` and this host picks
`TRASH_VERSIONS` = 5 and `TRASH_MAX_AGE` = 30 days beside the store it applies
to (Keel ADR 0073, ADR 0073 here). Measured over the 11 projects in this state
directory: 4 446 versions, 122.3 MB, 937 distinct originals, median 2 versions
per path and one path with 196. Five keeps 51% of the files and 42.6 MB, and the
first start after it lands removes 2 179 files and about 79.7 MB — once, and not
recoverably. Thirty days removes nothing today (the oldest version is ten days
old); it is there for the path written once and never touched again. The prune
runs once per process, spawned, and its report is not logged yet — that is the
next thing to watch with telemetry, together with the two numbers above.

## A restart still loses the answer that was streaming

ADR 0080 keeps the in-flight turn across a switch, and ADR 0081 keeps the
question across a restart. The text of an answer that was streaming when the
process died was gone with it: it lived in the window's memory only, because the
engine writes a turn when it ends and keeps no live copy any host could read.

Two shapes, both small: the host could keep a bounded tail of the deltas it
already relays in its own `RunningTurn` and hand it back through
`SessionView.running`, or the engine could expose the buffer it holds while the
turn runs. The first is host-only and bounded; the second is a kernel change and
needs a contract test. Not scheduled: the retry card (ADR 0081) already covers
the case where it matters, and the case where it does not has not come up yet.

Done (2026-11-24): the host keeps the answer and the note carries it (ADR 0083).
`RunningTurn.partial` holds the head of the text, bounded at 8 000 bytes and cut
on a character boundary; `PendingTurn.partial` rides in `<id>.pending.json` with
`#[serde(default)]`; `PartialNote` rewrites that file at most twice a second and
only when the text grew; `SessionView.running.partial` feeds `liveTurn`, so a card
rebuilt after a switch starts where the stream stopped, and the retry card shows
the kept text above 再问一次. The tail was not what to keep: the beginning is
what the reader already read. The kernel was not touched — it still writes a turn
only when the turn ends, and a retry is still a new turn (ADR 0081).

## The desktop's settings are one file for every project

`DesktopConfig` holds provider, model, permission and rig for the whole app, so
two projects cannot run different models at once. Conversations already keep the
connection they were built with (`Connection`, ADR 0076 here), so the read path
is ready; what is missing is a per-project section in the config and a decision
about which one a reopened window starts from.

