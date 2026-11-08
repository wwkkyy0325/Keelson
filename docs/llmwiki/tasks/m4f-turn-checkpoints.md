# M4f — undo a turn

Goal: a turn that changed the workspace can be reverted as one unit. The
trash restores one file at a time and cannot tell which changes belong to
which turn; a turn that adds a file, edits two others and deletes one has no
single undo.

## Decisions

- The desktop keeps a shadow git repository per project under
  `<state>/checkpoints/<workspace-key>/repo`. It is not the user's repository
  and never touches its history. `git add -A` respects the workspace's
  `.gitignore`.
- A snapshot is taken before and after every turn. The pair is recorded in
  `<state>/checkpoints/<workspace-key>/turns.json` keyed by session id and
  turn sequence.
- Undo is offered for the latest turn only. A restore of an older turn while
  later turns exist would make the transcript lie about the workspace.
- Undo uses `git diff --name-only --no-renames <before> <after>`. For each
  path: restore it from `<before>` when it existed there, otherwise remove
  it. This handles create, modify, delete and rename without parsing a patch.
- The snapshot is best effort. If git is missing or a snapshot fails, the
  turn still runs; the undo command reports that no checkpoint exists.
  Ignored files and changes made by a shell command are included only when
  git can see them; a gitignored file is not in the snapshot.
- The undo is not atomic across files. It is a sequence of checkout/remove
  operations; a mid-way failure leaves a partial restore and the record
  stays un-undone so the user can retry. The confirmation says this reverts
  the workspace.
- No new Rust dependency: git is an external program, and the desktop already
  requires a developer toolchain. The feature is inert when git is absent.

## Steps

- [x] `checkpoints.rs`: shadow repo init, snapshot, record, undo and a git
      availability test.
- [x] `send_message`: snapshot before and after the turn and record the pair;
      never block the turn on a checkpoint failure.
- [x] `undo_last_turn` command and the desktop action with a confirmation.
- [x] Docs: design, README, changelog, task verification.

## Verification

- `checkpoints.rs` tests: modify, create and delete are all restored; a
  second undo, a missing record and a state directory inside the workspace
  are refused. The git-backed tests skip when git is unavailable.
- Desktop backend: 28 tests (three new checkpoint tests), clippy.
- `send_message` takes the before snapshot inside the spawned turn task and
  the after snapshot before the turn-finished event; a snapshot failure is
  ignored and the turn still runs.
- `undo_last_turn` refuses while a turn is running, resolves the latest turn
  sequence, and runs the blocking restore on the blocking pool.
- UI: the header shows `撤销本轮` when the latest turn used a mutating tool;
  the confirmation explains that the workspace is restored and the transcript
  is kept. After a successful undo the action hides until the next turn.
- Frontend: 16 tests, `svelte-check` 0/0, build.
- Not exercised: a live click-through of the undo action. The backend path is
  covered by the checkpoint tests; the UI is covered by type checking and the
  build.

## Not in M4f

- A checkpoint history browser, redo, or undoing an older turn.
- Snapshotting gitignored files or large binary assets.
- Replacing the per-file trash; it remains the restore path for a single file.
