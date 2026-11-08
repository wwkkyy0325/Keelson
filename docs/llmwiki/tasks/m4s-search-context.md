# M4s — search context lines

Goal: a search hit should be readable without a second `read_file`.

## Keel

- `code_search` takes `context` (0-5, default 0).
- With context, each match prints up to N lines before and after. The match
  uses `path:line: text`, neighbors use `path:line- text`, and blocks are
  separated by `--`.
- Matches are ordered by path and numeric line; `context = 0` keeps the old
  format.
- ADR 0025 records the contract.

## Verification

- `code_search_context_includes_neighbors`: one context line on each side.
- `code_search_without_context_keeps_only_the_match`: the default is
  unchanged.
- `code_search_rejects_a_non_integer_context`.
- Keel: 115 lib tests, 28 tool tests (+3), clippy. Keelson: 51 lib tests,
  clippy; live two-module eval passed in 17.04 s; desktop tests pass (28).

## Not in M4s

- Overlapping blocks are not merged; a line can appear as context for one
  match and as a match for another.
- Context is per match, not a window around an arbitrary file region.
