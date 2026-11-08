# M4l — coding rig polish

Goal: close the two reliability gaps the real-task runs exposed in the coding
rig itself, without adding a new tool or a new check.

## Steps

- [x] `write_file` requires a full read. A ranged `read_file` records the
  bytes it returned with `full = false`; `write_file` on an existing file
  checks `check_full` and refuses a partially seen file. `edit_file` and
  `multi_edit` keep accepting any current read because their exact-match
  requirement protects the bytes they do not replace.
  - Verify: `cargo test -p keel-runtime` (3 ledger tests, 1 tool test);
    `cargo test` in Keelson; the live two-module discount eval still passed
    in 19.10 s with whole-file reads.
- [x] The persona says not to install packages or change the interpreter, and
  that `write_file` over an existing file needs a full read.
  - Verify: both profiles parse and `keelson doctor` reports them.
- [x] ADR 0020 records the coverage amendment.
  - Verify: the ADR matches the ledger fields and error text.

## Why

The real-task runs showed a model repairing a broken interpreter with `pip
--user` instead of reporting the missing dependency. Separately, the ledger
recorded the whole file after a ranged read, so a one-line read was enough to
overwrite a file whose other lines the model never saw. The first is a
behavior rule; the second is an enforcement rule. Both belong to the rig, not
to the task.

## Not in M4l

- No package-manager sandbox; the desktop still asks the user.
- No line-range coverage tracking for edits; exact matching is the guard.
- No monorepo check selection.
