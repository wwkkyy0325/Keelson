# M4v — shell mutation hint

Goal: a shell command that writes files should trigger the after-changes check
without treating every test-only command as a mutation.

## Kernel

- The shell schema accepts `mutates` (boolean, default false).
- A successful shell call with `mutates: true` sets `workspace_changed` and
  adds no paths, so the selector falls back to every check.
- ADR 0026 records the contract.

## Host

- The persona tells the model to set the flag for formatters, generators,
  `git checkout` and scripts that edit files, and to leave it false for a
  test-only command.

## Verification

- Driver unit test: a marked shell is a workspace change; a plain one is not.
- Two turn tests: the hint runs the after-changes check; no hint skips it.
- Keel: 115 lib tests, 42 turn tests (+2), clippy. Keelson: 51 lib tests,
  polyglot eval 9.57 s, desktop 28.

## Not in M4v

- The flag is a hint; a model that omits it keeps the old behavior.
- No command parsing and no sandbox.
