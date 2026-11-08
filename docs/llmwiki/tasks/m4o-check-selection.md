# M4o — checks selected by changed paths

Goal: run the check of the project a turn touched instead of every detected
check, so a polyglot repository does not fail on an unrelated ecosystem.

## Kernel

- `TurnStats.changed_paths` records the paths of successful mutating calls:
  `path` for the file tools, `from` and `to` for `move_file`.
- `with_acceptance_selector` takes an `AcceptanceSelector`, a closure
  `Fn(&[PathBuf]) -> Vec<AcceptanceItem>`. It runs once after the model stops;
  an empty result finishes with a passing, evidence-free report.
- `multi_edit` was missing from `WORKSPACE_MUTATING_TOOLS`, so a turn that only
  used it skipped verification. Fixed.
- ADR 0024 records the contract.

## Host

- `ProjectCheck` pairs an acceptance item with the directory it covers.
- `selected_checks` picks the deepest project that contains each changed path
  and falls back to every check when no path matches.
- `python_project` maps a tests directory to the nearest ancestor with a
  manifest or a virtual environment.
- The engine passes a selector instead of a fixed list when at least one check
  exists.

## Verification

- Keel: the selector test observes `note.txt`; an empty selector ends green; a
  write reports its path, a move reports both, a shell reports none;
  `multi_edit` counts as a workspace change.
- Keelson: 44 lib tests (+3): nested Cargo and Node selection, root versus
  nested, nested Python.
- Live two-module eval passed in 36.12 s; desktop tests still pass (28).

## Not in M4o

- A shell call is not a workspace change, so a shell-only mutation still skips
  verification. That is the existing rule; the selector does not change it.
- A path outside every project runs every check.
- Several projects of one ecosystem still yield one check.
