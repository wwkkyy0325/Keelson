# M4k — a real Python task in a copied ValveGuard

Goal: run the coding rig on a real, documented limitation in a second
language (Python/pytest), in a copy, without touching the original. This is a
validation, not a feature of Keelson.

## Task source

`ValveGuard/docs/llmwiki/tasks/demo-artifact.md`, the only unchecked step:

> `compare_demos.py` human / synthetic / cheater harness.
> Verify: labeled demos produce grouped distributions and a summary table.

The copy is `D:/workspace-new/wwkkyy0325/keelson-task-valveguard`: `src`,
`tests`, `docs`, `configs`, `scripts`, the manifests and requirement files
(1.3 MB). `data`, `artifacts`, `.venv` and `reports` were left out; tests that
need the packaged CNN artifact skip. Baseline: **16 passed, 6 skipped**.

## Host gap: pytest was invisible

`workspace_check` always ran `python -m unittest discover -v`. On pytest-style
modules that exits 0 with **zero tests**, which the verifier counts as a pass.
A false green is worse than no check.

- `uses_pytest` reads `pytest.ini`, `conftest.py`, `pyproject.toml`,
  `setup.cfg`, `tox.ini` and root `requirements*.txt` (comment lines ignored)
  and selects `python -m pytest -q`.
- `python_program` prefers `<workspace>/.venv/Scripts/python.exe` (or
  `bin/python`), then `venv`, then the bare interpreter.
- Four engine tests pin the pytest selection, the commented mention, the
  dependency declaration and the venv path.

## Turn 1 — the task was right, the environment was wrong

- **1477.76 s**; the host check passed with **20 passed, 6 skipped** (16
  baseline + 4 new).
- The generated `compare_demos.py` and `tests/test_compare_demos.py` matched
  the contract; the task doc step was marked done.
- Most of the run was outside the task. The harness exported
  `PATH="D:/.../.venv/Scripts:$PATH"`, but Git Bash does not translate a
  `D:/...` entry when it spawns a Windows process, so `python` resolved to
  `D:\dev\anaconda3\python.exe`. That interpreter had a broken numpy, and the
  model tried to repair it with `pip --user`: numpy 2.5.3, pandas 2.2.3,
  bottleneck 1.6.0, numexpr 2.14.1, torch 2.6.0, scikit-learn 1.7.2 (about
  1.3 GB in the user site).
- This was possible because `ApproveExecution` approved every permission
  prompt, including `pip`, `conda` and `taskkill`.

## Turn 2 — clean re-run

The copy was reset (new files removed, doc step back to `[ ]`) and the harness
changed:

- `KEELSON_TASK_PYTHON` is prepended to `PATH` from Rust, so the project
  interpreter reaches both the model's shell and the host check. The harness
  prints the resolved `python`.
- `ApproveExecution` rejects host-changing commands: package managers,
  `taskkill`/`wmic`, installers. A non-ignored test pins the guard.

Result: **138.66 s**, 35 model calls; `python` resolved to
`D:\workspace-new\wwkkyy0325\ValveGuard\.venv\Scripts\python.exe`, and there
were no package-manager calls. Host check: `workspace-tests (test): exit code
0`, **23 passed, 6 skipped**. The model added
`src/valveguard/scripts/compare_demos.py` and seven tests (grouping,
median/p95, a missing `result.json`, an empty root, deterministic output, the
CLI table and CSV), and marked the task doc step with the run command.

## Findings

1. **A check that runs no tests must not pass.** Pytest modules exit
   `unittest discover` with 0 and exit code 0. The host now runs pytest when
   the project declares it.
2. **The interpreter is part of the rig.** A bare `python` and a dropped PATH
   turned a three-minute task into a 24-minute environment repair and changed
   the user site. The harness now pins the interpreter, and the guard denies
   host changes.
3. **Language portability holds.** With a stable environment, the model read a
   foreign Python project, matched an existing report schema, and produced a
   deterministic script with edge-case tests in 138 s.
4. **The copy has no `.git`.** The model tried `git diff`, which cannot show
   the task's changes there. If a task needs history, the harness should copy
   the repository or set `GIT_CEILING_DIRECTORIES` to keep discovery inside
   the copy.

## Not in M4k

- Cleaning the user-site packages installed by turn 1; that is a separate
  decision.
- Merging the copy back into ValveGuard.
- A sandbox for unattended shell; the desktop still asks the user.
