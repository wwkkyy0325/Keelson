# M4m — monorepo project checks

Goal: the coding rig should verify a project whose manifest sits in a
subdirectory, not only a repository root.

## What changed

- `project_directories` finds `Cargo.toml`, `go.mod` and `package.json` at the
  workspace root, depth 1 or depth 2, root first (M4r later extends this to
  three).
- `find_python_tests` finds the first test file and the directory that holds
  it; the check targets that directory.
- Commands: `cargo test --manifest-path <dir>/Cargo.toml`,
  `go -C <dir> test ./...`, `npm --prefix <dir> test`,
  `python -m pytest -q <dir>` (or `unittest discover -s <dir> -v`).
- `python_program` and `uses_pytest` walk the tests directory and its
  ancestors, so `backend/.venv` and `backend/pyproject.toml` are found.
- One ecosystem still wins in the order Cargo, Go, Python, Node; a polyglot
  repo with a root Cargo manifest still runs only Cargo. M4n later runs every
  detected ecosystem.

## Verification

- Six new engine tests: nested Cargo, nested Go, nested Node, nested Python
  venv, nested Python pytest, and root precedence.
- The exact commands were run in a temporary tree: nested `cargo test
  --manifest-path`, `go -C`, `npm --prefix` and `pytest <dir>` all exited 0.
- Keelson: 39 lib tests (+6), clippy. Live two-module eval still passed
  (41.37 s).

## Not in M4m

- Running every ecosystem in a polyglot repository. The driver takes a list,
  but one check keeps a turn's cost bounded; choosing by changed files needs a
  kernel change.
- Manifests deeper than two directories and several test roots; M4r and M4q
  later close these.
