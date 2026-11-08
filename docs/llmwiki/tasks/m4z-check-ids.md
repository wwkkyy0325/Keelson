# M4z — named check ids

Goal: with several checks, a failure should say which project and ecosystem
failed instead of repeating `workspace-tests`.

## What changed

- `AcceptanceItem.id` is now `workspace-tests:<ecosystem>:<project>`. A
  project at the workspace root uses `root`.
- Python uses the test root in the id, so two roots under one project stay
  distinct (`api/tests` vs `api/integration`).
- The verdict, `VerificationFinished` event and UI evidence carry the id; the
  model's reprompt names the failing check.

## Verification

- Tests assert the root, nested, polyglot and Python-monorepo ids, and that
  two Python roots under one project have different ids.
- Keelson: 51 lib tests, clippy. Polyglot eval 8.36 s; cross-file eval 8.87 s;
  desktop 28.

## Not in M4z

- The command description is still not stored in `Evidence`; the id is the
  human-readable label.
- Ids are internal; the format is not a session migration.
