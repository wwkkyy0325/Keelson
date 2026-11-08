# M4t — search output polish

Goal: context blocks that overlap should print each line once, and a byte-cap
cut should not be reported as a result-limit stop.

## What changed

- `render_context` merges overlapping or adjacent ranges per file and marks a
  line that is both context and a match with the match marker.
- The byte-cap truncation appends "output truncated at the byte limit" instead
  of "stopped after N matches"; the result-limit message is unchanged.
- Blocks from different files or disjoint ranges remain separated by `--`.

## Verification

- `code_search_context_merges_overlapping_blocks`: two adjacent matches yield
  one block with each line once.
- `code_search_reports_output_truncation`: a large context search reports the
  byte limit, not the result limit.
- Keel: 115 lib tests, 30 tool tests (+2), clippy. Keelson: 51 lib tests, live
  two-module eval passed in 12.23 s; desktop tests pass (28).

## Not in M4t

- The byte cut can land mid-line; `char_boundary` keeps it valid UTF-8.
- A single very long match line is still capped at the line limit.
