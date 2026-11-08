# M4i — show why verification failed

Goal: the transcript says `验证失败` but not which check failed or what it
printed. The first M4h attempt failed its automatic check and the user could
not see why; the model had the evidence, the person did not.

## Decisions

- Persist a `StoredVerification { passed, evidence }` on `StoredTurn`.
  Evidence keeps only non-passing items (failed, unverified, deferred), at
  most eight entries, with the **tail** of the detail capped at 4000
  characters. A passing turn stores no output, so normal sessions do not
  grow.
- Keep `verification_passed` for sessions written before this field. Older
  turns render the old badge and no detail; the frontend uses
  `verification?.passed ?? verificationPassed`.
- `TurnView` carries the new field. `Turn.svelte` shows the badge as before
  and, when there is evidence, a `查看失败输出` disclosure with the item id,
  outcome and output in a scrollable `pre`.
- The detail is the verifier's own text, not a second summary. Test failures
  live at the end of stderr, which is why the tail rather than the head is
  kept.
- No new dependency and no kernel change; the report already carries the
  evidence.

## Steps

- [x] Persist bounded non-passing evidence on the stored turn and expose it
      through `TurnView`.
- [x] Render the failure output under the verification badge.
- [x] Tests: tail truncation and failure filtering in the engine; the
      frontend reducer carries the evidence.
- [x] Docs: design, README, changelog.

## Verification

- Root: 29 tests (+1: failures only and a 4000-character tail) + 3 ignored
  live evals + boundary + 5 kernel probes, clippy.
- Frontend: 17 tests (+1: a failed turn carries its evidence), `svelte-check`
  0/0, build.
- Desktop backend: 28 tests, clippy.
- Old sessions: `verification` is `#[serde(default)]`; the badge falls back to
  `verificationPassed`.
- Not exercised: a live click-through of the disclosure. The data path is
  covered by the engine and reducer tests; the template is type-checked and
  built.

## Not in M4i

- Parsing test output into individual failed test names.
- A verification history, flaky-test detection or a retry button.
- Showing passing-check output; the badge is enough for a pass.
