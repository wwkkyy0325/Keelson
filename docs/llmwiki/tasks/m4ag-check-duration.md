# M4ag — a check reports its own cost

Goal: know what a check costs before deciding whether a cheaper one would pay
for itself.

## What changed

Kernel only.

- `AcceptanceVerifier` measures each command or test item and appends
  `[duration: 0.42 s]` to the evidence detail, on pass and on failure.
- Items that never ran (not allowlisted, spawn failure) carry no duration.
- Compute and file-hash items are unchanged. No event or schema change.

## Verification

- `command_and_test_items_run_allowlisted_programs` now asserts the marker on
  every evidence entry.
- The host's `messy-workspace` verifier run showed `[duration: 0.48 s]` for
  `javac` on a 570-line class.

## Why

`VerificationFinished` carries `passed` and the failing ids but no timing, so
the only recorded cost was the whole turn, which mixes in model latency. The
evidence text is where a user already looks at a failure and needed no schema
change.
