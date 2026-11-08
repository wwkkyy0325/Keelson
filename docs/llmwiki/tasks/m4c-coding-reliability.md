# M4c — coding rig reliability pass

Goal: make the coding suite safe on a real repository. The M4/M4b tools can
do the work; this pass removes the failure modes that lose work or waste a
turn, using the components the established coding agents converged on.

## Research notes

- Claude Code, OpenCode, Cline, Gemini CLI and Codex all expose the same
  shape: read (with line ranges), edit (exact search-replace), glob for file
  discovery, grep for content, shell with approval, and a todo list. The
  missing component in Keelson was `glob`.
- Claude Code requires a prior Read before Edit/Write. Qwen Code and the
  ironclaw agent reject a write when the file changed since it was read,
  with an actionable "re-read" message. The check is content-based, not
  mtime-based, and is best-effort against a race.
- Aider's search-replace format is the most reliable for frontier models
  (84-96% apply accuracy) when matching is exact. Fuzzy/whitespace fallbacks
  recover drift but can hide an ambiguity; keep them narrow.
- Windows CRLF is a concrete exact-match failure: a model emits `\n` in JSON
  while the file contains `\r\n`. Normalising line endings is the smallest
  deterministic fallback.
- Checkpoints and multi-file atomic patches are the next tier. The trash
  already makes individual writes recoverable, so they are not in this pass.

## Decisions

- A per-engine `ReadLedger` records the SHA-256 of every successful
  `read_file` and the bytes written by `write_file`/`edit_file`.
  `write_file` on an existing file and every `edit_file` require a recorded,
  current hash. A new file needs no read. The refusal names the recovery:
  read the file again. The ledger is host-wired; the kernel `new()`
  constructors stay usable without one.
- `edit_file` keeps exact match as the primary path. If it finds nothing, it
  converts `old` to the file's line ending and tries again. `new` is
  converted the same way, so an edit never turns a CRLF file into a mixed
  file. No whitespace or fuzzy matching: an ambiguity still fails loudly.
- `glob` is a new kernel tool: `*` within one segment, `**` across
  segments, a pattern without a separator matches the file name at any
  depth. Files only, newest first, bounded.
- The workspace's `AGENTS.md` (fallback `CLAUDE.md`) is appended to the
  system prompt at engine open, capped at 16 KiB. It is project context, not
  a rig setting.
- Out of scope: `multi_edit`/`apply_patch`, shadow-git checkpoints, regex
  search (needs a dependency decision), LSP, and a hard loop guard. The
  kernel already escalates on repeated failed calls.

## Steps

- [x] Keel `ReadLedger` plus read-before-write/edit guards.
      Verify: an unread existing file is refused; a file changed after the
      read is refused; a read-then-write succeeds; a new file is allowed.
- [x] Keel `edit_file` line-ending fallback.
      Verify: a CRLF file edits with an LF snippet and stays CRLF; an LF
      file edits with a CRLF snippet and stays LF; an ambiguous normalized
      match still fails.
- [x] Keel `glob` tool.
      Verify: `**/*.rs` matches nested files, `src/*.rs` does not cross a
      directory, `*.md` matches by name at any depth, build directories are
      skipped, the limit is enforced.
- [x] Host wiring: the ledger, `glob` in both profiles, and the persona.
      Verify: the registry exposes `glob`; both profiles validate; the
      persona tells the model to read before writing.
- [x] Project instructions from `AGENTS.md`/`CLAUDE.md`.
      Verify: the system prompt carries the file; `CLAUDE.md` is the
      fallback; the 16 KiB cap holds.
- [x] Live eval on a CRLF demo workspace.
      Verify: the turn finishes, the tests pass, and the tool lines show the
      model recovered from any read-before-write refusal.
- [x] Docs: ADRs, design, README, changelog.

## Verification

- `ReadLedger`: three unit tests (recorded, unread, changed). Tool tests:
  an unread existing write is refused and a new file is allowed; a read
  permits a write and an external change is refused; an unread edit is
  refused. The ledger is one per engine.
- `edit_file` line endings: a CRLF file with an LF `two\nthree` snippet edits
  to `one\r\nTWO\r\nTHREE\r\n`; an LF file with a CRLF snippet stays LF. The
  success message says the line endings were normalized. Zero and multiple
  matches keep their old failure behavior.
- `glob`: `**/*.rs` returns `src/main.rs` and `src/nested/lib.rs` and skips
  `target/generated.rs`; `src/*.rs` does not cross into `nested`; `*.md`
  matches `README.md`. Traversal uses the existing skip list and depth cap.
- Host: the registry exposes `glob`; both profiles load (`doctor`); the
  persona tells the model to read before writing and to prefer `edit_file`.
- Project instructions: `AGENTS.md` is appended to the system prompt;
  `CLAUDE.md` is the fallback; `AGENTS.md` wins when both exist; the 16 KiB
  cap is applied.
- Suites: Keel `keel-runtime` 101 lib tests + 22 integration tool tests,
  clippy; Keelson 27 tests + boundary + 5 kernel probes, clippy; desktop 25
  tests, clippy; frontend 16 tests, `svelte-check` 0/0, build.
- Live eval (2026-11-08, CRLF demo): passed in 11.86 s. Tools used:
  `list_dir`, `todo`, `read_file` ×3, `shell`, `todo`, `shell`, `edit_file`,
  `shell`, `read_file`, `todo`. `stats.py` still used CRLF after the turn and
  the post-turn `unittest` run passed.
- Not exercised: a live UI walkthrough of a refused write and a large-repo
  `glob` benchmark. Both have unit or host coverage.

## Not in M4c

- `multi_edit`, `apply_patch`, multi-file atomic edits.
- Shadow-git checkpoints and a one-click undo of a turn; the trash remains
  the recovery path.
- Regex search and gitignore semantics.
- A hard tool-loop guard.
