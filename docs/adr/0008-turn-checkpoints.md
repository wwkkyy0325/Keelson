# ADR 0008 — Undo a turn through a shadow git repository

## Status

Accepted and implemented in `desktop/src-tauri/src/checkpoints.rs`.

## Context

The trash restores one file at a time. It cannot answer "undo what this turn
did": a turn that adds a file, edits two others and deletes one has no single
restore point. The coding rig needs a turn-level undo that is independent of
the user's repository.

A copy of the workspace per turn would be simple but wasteful. The reference
coding agents use a shadow git repository: it snapshots the workspace at turn
boundaries, deduplicates unchanged files, and never touches the real `.git`.

## Decision

1. **A shadow repository per project.** The repo lives at
   `<state>/checkpoints/<workspace-key>/repo` and uses the workspace as its
   work tree. The user's repository and history are untouched.
2. **Snapshot before and after every turn.** A snapshot is
   `git add -A`, an allow-empty commit and `rev-parse HEAD`. The pair is stored
   in `turns.json`, keyed by session id and turn sequence.
3. **Undo the latest turn only.** Undoing an older turn while later turns
   exist would make the transcript disagree with the workspace. The command
   resolves the latest sequence and the UI offers the action only there.
4. **Diff, do not replay patches.** `git diff --name-only --no-renames`
   lists every path that differs between the two commits. A path that exists
   in the before commit is restored with `git checkout <before> -- <path>`;
   a path that does not is deleted. This covers create, modify, delete and
   rename without a patch grammar.
5. **Best effort, never blocking.** A snapshot failure (git missing, an
   unreadable file) does not fail the turn; the undo command then reports
   that no checkpoint exists. A restore is a sequence of file operations, not
   a transaction; a mid-way failure leaves a partial restore and the record
   stays un-undone so a retry can finish it.
6. **Git is an external program, not a dependency.** No Rust crate is added.
   The feature is inert when git is absent.
7. **Only what git can see.** `git add -A` honours the workspace's
   `.gitignore`, so an ignored file is not snapshotted. Changes made outside
   the turn after it finished are also outside the checkpoint.

## Consequences

- "Undo turn" reverts the workspace, not the transcript. The conversation
  keeps the turn; the files return to the pre-turn state.
- The confirmation says the workspace is restored. A file the user edited
  after the turn is overwritten if the turn had touched it; that is the
  meaning of restoring the turn.
- The shadow repo stores a copy of the workspace, including any secret a
  `.gitignore` does not exclude. It stays under the user's state directory and
  is never uploaded.
- Storage grows with changed content; git deduplicates unchanged files. A
  garbage-collection policy is a later concern.
- A project with a `.gitattributes` keeps that normalization; the restore
  follows the repository's own rules. An empty directory created by
  `make_dir` is not tracked by git and is not removed by undo.
- Tests cover modify, create, delete, a repeat undo and a missing record; the
  UI offers the action only on the latest turn with a mutating tool.
- Not in this decision: redo, a checkpoint browser, undoing an older turn,
  and snapshotting ignored files.
