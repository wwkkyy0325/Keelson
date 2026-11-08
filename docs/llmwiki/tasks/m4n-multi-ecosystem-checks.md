# M4n — every ecosystem gets a check

Goal: a polyglot workspace should verify each detected ecosystem, not only the
first one in a fixed order.

## What changed

- `workspace_check` became `workspace_checks`: it returns one required
  `AcceptanceItem` per detected ecosystem, in Cargo, Go, Python, Node order.
- The engine passes the whole list to
  `with_acceptance_after_changes(checks, 2)`, so one failing check blocks the
  turn and the evidence names the item.
- Each ecosystem still targets its own project directory, including a nested
  manifest (M4m). A workspace with no known ecosystem stays unverified.
- The test-only `workspace_check` helper returns the first item so the
  existing single-check tests stay focused.

## Verification

- `a_polyglot_workspace_gets_every_check`: a Cargo + Python + Node fixture
  yields `cargo`, `python`, `npm` in that order, all required.
- `a_polyglot_workspace_runs_every_check`: the real
  `AcceptanceVerifier` runs both `cargo test` and `npm test` in a temporary
  workspace and the report passes with two passing evidence items (1.48 s).
- Keelson: 41 lib tests (+2), clippy. Live two-module eval still passed in
  20.79 s.

## Not in M4n

- Selecting checks by the files a turn changed. The driver tracks only
  "workspace changed"; per-path selection needs a kernel change.
- Running several projects of the same ecosystem (for example two Python
  packages). The first project directory wins.
- Making secondary checks optional. Every detected check is required, which
  matches a CI view of the workspace.
