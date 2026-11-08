# M4p — every project of an ecosystem

Goal: a monorepo with several crates, packages or Python projects should check
the one a turn changed, not only the first.

## What changed

- `workspace_checks` iterates every manifest directory per ecosystem instead
  of taking the first.
- `find_python_tests` returns every test directory; `workspace_checks` maps
  each to its project and keeps one check per project.
- The M4o selector picks the deepest project for each changed path.
- The root project still comes first in the candidate list.

## Verification

- `a_cargo_monorepo_runs_each_crate`: `api` and `worker` both get a check; a
  changed path selects the matching `--manifest-path`.
- `a_node_monorepo_runs_each_package`: both packages get a check; a changed
  path selects the matching `--prefix`.
- `a_python_monorepo_runs_each_project`: both projects get a check; a changed
  path selects the matching tests directory.
- Keelson: 47 lib tests (+3), clippy. Live two-module eval passed in 43.33 s;
  desktop tests pass (28).

## Not in M4p

- A project with several test roots gets one Python check on the shallowest
  root; pytest discovers the rest from there, `unittest` may not. M4q later
  runs each remembered root.
- Manifests deeper than two directories. M4r extends the depth to three.
- A root project and a nested project of the same ecosystem are both
  candidates; a changed path selects the nested one, which is intended.
