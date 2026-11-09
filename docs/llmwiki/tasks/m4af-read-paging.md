# M4af — read paging and range coverage

Goal: a large file can be read without filling the working set, and can still
be rewritten after paging through it.

## What changed

Kernel only; the host needed no wiring.

- `read_file` without `offset` returns at most 1000 lines and appends
  `[showing lines 1-1000 of 4213; use offset=1001 to continue]`.
- A ranged read starts with `[file: 4213 lines; showing 1001-1200]`. When the
  file was cut at the 8 MiB limit the count reads `at least N`.
- Both the header and `line_numbers` are display only; they are not part of
  the file and must not appear in edit text.
- `ReadLedger` records 1-based inclusive line ranges. Overlapping and adjacent
  ranges merge; a file counts as fully read when one merged range covers
  `1..=N`. The digest still has to match, an edit drops the ranges unless the
  read was already full, and a byte-truncated read contributes nothing.

## Verification

- Keel: `new` tests in `read_ledger` (coverage, merging, edit drop, changed
  bytes) and in `tools` (the cap and its marker, the header, and a 1200-line
  file read in two calls that then accepts `write_file`).
- `cargo test --workspace`, `clippy -D warnings`, `fmt --check` pass; the tools
  suite is 42 tests.

## Follow-up: search reached less than read

`code_search` skipped every file above 1 MiB silently. A 40,000-line module is
about 1.2 MB, so it could be read and edited but not searched, and a search
that found nothing there was indistinguishable from an empty file. The cap is
now the same 8 MiB as `read_file`, and a skipped file is named in the output.
A 40,000-line file (1.2 MB) now takes 0.06 s for a ranged read plus an edit.

## Not in M4af

- A file over 8 MiB still cannot be edited or rewritten: the recorded digest
  is a prefix and never matches the file.
- The cap is per call, not per session; a model can still read a long file one
  range at a time.
