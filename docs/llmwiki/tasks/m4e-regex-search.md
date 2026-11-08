# M4e — regex content search

Goal: the coding suite can find a pattern, not only a literal substring.
Every surveyed coding agent exposes a regex content search; `code_search`
was the last tool still limited to literals.

## Decisions

- `code_search` gains `regex: boolean`, default false. The existing substring
  behavior is unchanged when it is off.
- `case_sensitive` applies to both modes. In regex mode it sets the regex
  builder's `case_insensitive(!case_sensitive)`; the default is still
  case-insensitive.
- The regex is compiled once per call. An invalid pattern is
  `InvalidArguments` with the compiler message; nothing is scanned.
- Matching is line-oriented. There is no multiline mode, lookaround or
  backreference; Rust's `regex` crate does not support them, and a per-line
  result is what the existing output format shows.
- The traversal, file-size limit, line truncation, result cap, skip list and
  `glob` filter are unchanged. Regex does not unlock new paths or bypass the
  read permission.
- `regex` is a new workspace dependency (MIT OR Apache-2.0). It is compiled
  once per call and reused for every line. The license audit table gains a
  row.
- No ripgrep and no `.gitignore` parsing. Those are separate decisions.

## Steps

- [x] Add the `regex` dependency to the workspace and `keel-runtime`; update
      the license audit table.
- [x] Extend `code_search` with `regex` and add contract tests.
      Verify: a pattern finds a match; invalid regex is refused; the case
      flag applies in both modes; the existing literal and glob tests still
      pass.
- [x] Update the persona, design, README and changelog.

## Verification

- Keel `keel-runtime`: 107 lib tests and 25 integration tool tests, clippy.
  Three new search tests: `fn\s+\w+` finds a function signature; an
  invalid `(` is `InvalidArguments`; `hello` matches `HELLO` by default and
  stops matching with `case_sensitive`.
- `cargo deny check licenses`: `licenses ok`; the audit table records
  `regex 1.13.1`.
- Keelson: 27 tests, clippy; desktop 25 tests, clippy; `doctor` loads both
  profiles.
- The literal search, case flag, glob filter and skip-list tests are
  unchanged and still pass.
- Not exercised: a live model choosing regex search. The tool schema is a
  single boolean addition; the wire already carries booleans for
  `case_sensitive` and `replace_all`.

## Not in M4e

- Ripgrep, `.gitignore` semantics, multiline regex.
- A separate `grep` tool name; `code_search` stays the single search path.
