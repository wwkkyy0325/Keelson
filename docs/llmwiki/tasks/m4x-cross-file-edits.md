# M4x — cross-file atomic edits

Goal: a refactor that spans files should fail before touching the workspace
when one snippet is wrong, instead of leaving a partial rewrite.

## Kernel

- New `apply_edits` tool: parallel `paths`, `old` and `new` lists; the same
  path may repeat.
- Every file is read once, the ledger is checked once per file, and every
  replacement is applied to an in-memory copy before any backup or write.
- A missing, ambiguous, empty or identical replacement fails the whole call.
- Changed files are backed up and written one by one; a write failure restores
  the previous files from their backups, best effort.
- The driver records every path, so the check selector sees the whole
  refactor.
- ADR 0028.

## Host

- The tool is registered with the shared read ledger and added to both coding
  profiles.
- The persona tells the model to use it when one change spans files.

## Verification

- `apply_edits_updates_two_files`: both files change.
- `apply_edits_writes_nothing_when_one_edit_is_missing`: neither file changes.
- `apply_edits_applies_two_edits_to_one_file`.
- `apply_edits_requires_a_read`.
- `apply_edits_rejects_mismatched_lists`.
- Driver: `apply_edits` is a workspace change and reports every path.
- Keel: 115 lib tests, 37 tool tests (+5), 42 turn tests, clippy. Keelson: 51
  lib tests, polyglot eval 13.87 s, desktop 28.

## Not in M4x

- Filesystem writes are not transactional across a crash; the trash holds the
  previous versions.
- `replace_all` applies to every edit in the call, not per edit.
