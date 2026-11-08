# M4g — validate the suite on a multi-file task

Goal: check that the assembled coding rig works on a task that spans more
than one file, not only on the single-file demo. The point is to observe the
real tool choices and failure modes before adding more tools.

## Setup

- `examples/inventory-workspace`: a small Python package (`models.py`,
  `service.py`, `report.py`) with six unittest cases. Three already describe a
  missing `discount` parameter on `total` and `render`; they fail with
  `TypeError`.
- The eval is `tests/coding_eval.rs`, ignored by default:
  `cargo test --test coding_eval -- --ignored --nocapture`.
- The prompt asks for the discount, says not to change the tests, and asks the
  model to run them and report the real result.
- Assertions: the turn is done, the automatic Python check passed, the tests
  pass independently, the test file is byte-identical, and both modules
  changed. The eval prints the tool trace and the model-call count.

## Result (2026-11-08)

- Passed in **37.20 s**, **11 model calls**.
- Tools used: `list_dir`, `glob`, `read_file` x5, `shell` x2, `todo` x7,
  `write_file` x2, `shell`, `read_file` x2, `todo`.
- Automatic verification passed; the independent `python -m unittest
  discover -v` run passed; `tests/test_service.py` was unchanged.
- Both `service.py` and `report.py` contain `discount`.
- Root suite at the time: 27 tests + 2 ignored evals + boundary + 5 kernel
  probes, clippy. After M4h the suite is 28 tests + 3 ignored evals. The
  evals are ignored by default; `cargo test` only compiles them.

## Findings

- `glob` was used for discovery. The two-file change did not need
  `code_search`; reading the files was cheaper.
- The model chose `write_file` (twice), not `edit_file` or `multi_edit`,
  after reading the files. Whole-file writes are rational for small files and
  keep the read ledger satisfied. `multi_edit` remains useful for larger
  files, but this task did not exercise it.
- The read-before-write guard did not block the successful path; the model
  read before writing.
- The todo list was updated seven times, which is overhead on a task this
  small but not a failure. The turn stayed within the budget.
- The automatic Python check and the model's own shell run both passed; the
  model did not edit the visible tests to make them pass.

## Implications

- The assembled suite handles a multi-file change with tests and no test
  tampering. This is the first real multi-file validation after the M4c/M4d
  additions.
- The next validation should use a larger repository or a larger file to
  exercise `code_search` regex and `multi_edit`; this task is too small to
  force them.
- No new feature is justified by this run. The next code step should be
  chosen from a larger-eval finding, not from tool inventory.

## Not in M4g

- A checkpoint click-through on this workspace; checkpoint undo is covered by
  the desktop unit tests.
- A benchmark or a scored comparison against another agent.
