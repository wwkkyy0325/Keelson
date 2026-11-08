# M4d — atomic edits within one file

Goal: a multi-step change to one file either lands completely or not at all.
`edit_file` is one replacement per call; a failure on the third call leaves
the first two on disk and costs a recovery turn.

## Decisions

- `multi_edit { path, old: string_list, new: string_list, replace_all? }`.
  The lists are parallel and must have the same non-zero length.
- Every pair is applied in order to an in-memory copy. A missing or
  ambiguous snippet fails the whole call; no backup and no write happen.
- Matching is the `edit_file` contract: literal, exact first, line-ending
  fallback, read ledger checked once. One backup and one write for the batch.
- `replace_all` applies to every pair. A mixed call can be split.
- No multi-file transaction in this pass. `apply_patch` and shadow-git
  checkpoints remain separate decisions.

## Steps

- [x] Keel `MultiEditTool` with the parallel-list shape, atomic validation,
      one backup/write and the read ledger.
      Verify: a missing pair leaves the file unchanged and the trash empty;
      a successful batch makes one backup; unequal lists are refused.
- [x] Host registration and profile exposure.
      Verify: the registry exposes `multi_edit`; both profiles validate; the
      persona tells the model to use it for several changes to one file.
- [x] Docs: ADR 0022, design, README, changelog.

## Verification

- Keel `keel-runtime`: 107 lib tests (six new `multi_edit` tests) and 22
  integration tool tests, clippy.
- Tests pin: a two-pair success with one backup; a missing second pair with
  no write and no backup; unequal list lengths; the read guard; the CRLF
  fallback; `replace_all`.
- Host: `multi_edit` is registered and both profiles load. The persona
  recommends it for several changes to one file.
- Keelson suites are re-run after the wiring; the live eval is unaffected
  because the demo needs one replacement.

## Not in M4d

- A multi-file patch grammar (`apply_patch`), cross-file rollback and
  shadow-git checkpoints.
- Per-pair `replace_all` flags.
- A per-pair result in the transcript; the call is one tool line.
