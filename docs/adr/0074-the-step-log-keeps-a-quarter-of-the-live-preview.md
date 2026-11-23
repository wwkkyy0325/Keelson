# 0074 — The step log keeps a quarter of the live preview

Status: accepted

## Context

A conversation's turn rows are the record of what a turn did: one file per turn
under `<state>/sessions/<workspace-hash>/<session>.turns/`, each holding its
tools with the arguments they were called with and the output they returned.
They are written for reading after something went wrong.

Measured on this machine's store, 368 turn files:

| | |
|---|---|
| tool calls | 31 699 |
| output characters | 39 534 936 |
| argument characters | 4 553 930 |
| turn files on disk | **58.2 MB** |

The output is already not raw. The dispatcher cuts every preview at
`UI_PREVIEW_BYTES` = 4 096 before it leaves the kernel, which is why the largest
recorded output is 4 108 characters and not a build log. The step log then stored
that preview as it came: the same 4 KB, written to disk, for a reader who is
looking for *which* call failed rather than for its full text.

The distribution says the tail is where the bytes are:

| cap | share of output characters kept |
|---|---|
| 512 | 28.2% |
| **1 024** | **47.8%** |
| 2 048 | 75.1% |

Median output is 665 characters; 41.9% of calls exceed 1 024.

## Decision

- `ROW_PREVIEW_CHARS = 1 024` in `desktop/src-tauri/src/records.rs`. The step log
  clips `args` and `output` to it and appends
  `\n… truncated in the step log`.
- The map is by **characters**, not bytes, and cuts on a character boundary, so a
  CJK output is not split mid-character.
- A preview at or under the cap is stored byte for byte. The log is still a
  record of what the model saw for the majority of calls.
- The live event stream is untouched: `UI_PREVIEW_BYTES` still governs what the
  conversation renders, and a running turn shows the full 4 KB. The two numbers
  answer different readers at different moments.

## Consequences

- The recorded row is about a quarter of what it was for the tail — the median
  call is unchanged — and the truncation is visible in the file, so a reader is
  never left wondering whether the tool printed that little.
- A row is no longer a complete transcript of a call. The full output of a large
  call still exists where it always did: in the kernel's spill file, which the
  row's own output names.
- The number is a guess at a good default, not a measurement of a need: 1 024
  keeps a failing assertion and the shape of a diff, and it is the point on the
  curve where the next doubling buys 27 points of characters for a doubled log.
  It is read from a constant in one place and will move on telemetry.
- Existing rows keep their old size; nothing rewrites history.
