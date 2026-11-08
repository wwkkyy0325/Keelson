# M4q — every Python test root

Goal: a project with separate `tests/` and `integration/` roots should run
both, not only the shallowest.

## What changed

- `workspace_checks` prunes a test root nested inside another root, then keeps
  one check per remaining root. Two sibling roots under one project both stay;
  `tests/unit` is folded into `tests/`.
- Both checks map to the same project, so a change under that project selects
  both.

## Verification

- `a_python_project_runs_each_test_root`: `api/tests` and `api/integration`
  both get a check; `api/app.py` selects both.
- `a_nested_test_root_folds_into_its_parent`: `tests/` and `tests/unit/` yield
  one check on `tests/`.
- Keelson: 49 lib tests (+2), clippy. Live two-module eval passed in 26.64 s;
  desktop tests pass (28).

## Not in M4q

- `unittest discover` on a parent root may not recurse into a child package;
  the fold keeps the parent command, which is the best single command.
- Test files deeper than two directories are still not found. M4r extends the
  depth to three.
