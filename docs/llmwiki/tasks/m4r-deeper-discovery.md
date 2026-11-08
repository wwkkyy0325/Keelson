# M4r — discovery three directories deep

Goal: find a manifest or test file nested under `services/api/backend/` or
`backend/src/tests/`, not only one or two directories down.

## What changed

- `project_directories` and `find_python_tests` descend to depth 3 instead of
  2. Build directories are still skipped by the same list.
- A manifest at depth 3 gets the same targeted command as a shallow one; a
  test file at depth 3 gets a check on its directory.

## Verification

- `a_deep_cargo_project_gets_a_manifest_check`:
  `services/api/backend/Cargo.toml` gets a `--manifest-path` check.
- `a_deep_python_test_gets_a_check`: `backend/src/tests/test_stats.py` gets a
  pytest check targeting that directory.
- Keelson: 51 lib tests (+2), clippy. Live two-module eval passed in 24.16 s;
  desktop tests pass (28).

## Not in M4r

- Depth 4 and beyond.
- A test file deeper than three directories.
- A manifest in a directory skipped by the build-directory list.
