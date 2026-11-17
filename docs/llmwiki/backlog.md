# Backlog

Cases that are not scheduled yet, with the evidence and the first step.

## Sampling parameters in a profile

`temperature` (and the rest of the sampling knobs) cannot be set from a rig
profile: `keel-policy`'s `ModelSection` is `deny_unknown_fields` and has no
such field, so a profile that writes `temperature = 0.7` fails to load and the
whole rig disappears from the picker. `figure-live.toml` carried the field
and was broken this way until it was removed.

First step: add `temperature: Option<f32>` to `ModelSection` in the Keel
repository, forward it to the model route, and cover it with a contract test.
The rig files then get their comments' promises back.

## Profile inheritance

`ProfileSection` already parses `extends`, but `from_profile_str` refuses it
with `profile inheritance is not supported in M1`. Every live profile repeats
the same 18-line `[model]` and `[context]` block, so changing an endpoint or a
model id is five edits.

First step: decide whether `extends` merges sections field by field or only
`[model]`; then implement it in Keel with a contract test. Until then the
duplication is deliberate — one file you can read beats a merge you cannot.

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

`SessionStore::save` serialises the whole document with `to_string_pretty`,
writes a temporary file and renames it (`src/engine/store.rs:173-185`). A
conversation that grew to 10.5 MB therefore rewrites 10.5 MB at every turn, and
opening it parses all of it inside a Tauri command. Total bytes written grow
with the square of the turn count.

`tools[].output` pointing at the spill file under
`projects/<hash>/outputs/` is not a way out: those names carry no session id and
`MAX_SPILL_FILES` (200) prunes by directory, so two conversations in one
workspace overwrite each other. They are a scratch file for the model to
re-read, not an archive. What the model sees of a tool output is
`TOOL_HEAD_CHARS` + `TOOL_TAIL_CHARS` (3 000 characters, `keel-runtime/src/
summary.rs`); 4 KiB is the host's live preview (`UI_PREVIEW_BYTES`), and 8 000
characters is the document's cap.

First step: keep the newest K turns inline and give older ones their own file
under a per-session directory, so the per-turn write is bounded. One file per
turn with `index.json` holding metadata is the endpoint, but it moves the
storage layout ADR 0006 fixes, so it needs its own ADR.

