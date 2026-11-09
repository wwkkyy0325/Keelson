# M4ah — a large file with formatting damage

Goal: prove the rig can repair a file whose formatting was broken by an edit —
the failure mode a reformatting model produces — without spending the whole
working set.

## Fixture

`examples/messy-workspace`:

| File | Lines | Damage |
|---|---|---|
| `app/engine.py` | 1274 | rules 010-040 have their bodies at the `def` column; five later methods have one line a column too far |
| `src/Engine.java` | 570 | one method lost the brace closing its `if`; another gained a stray brace after its opening line |
| `tests/test_engine.py` | 32 | reference behavior, must not change |
| `src/Main.java` | 13 | reference behavior, must not change |

- Python is detected from `tests/` and runs `python -m unittest discover -s
  tests -v`; the broken module raises `IndentationError`, so the check fails
  before any test runs.
- Java has no detector. `.keelson/checks.toml` declares `javac -d build ...`
  with `-J-Duser.language=en -J-Duser.country=US`, so diagnostics are English
  and identical on any machine locale.
- The Python module is longer than one read: the model has to page to see all
  of it, which is the M4af path.

## Verification

- Deterministic: `the_messy_example_gets_a_python_check_and_its_declared_java_check`
  asserts the ids `workspace-tests:python:tests` and `workspace-check:java`.
- Manual verifier run on a copy: both items fail with their real output
  (`IndentationError`; 8 `javac` errors) and both carry a duration. The
  reference repair — generated from the same source and applied to a separate
  copy — passes 5 tests and `javac` with exit 0.
- Live: `messy_eval` copies the example, runs one turn, and asserts the report
  passes with both items, the test and `Main.java` are byte-identical, and a
  direct `unittest` and `javac` run also pass.

## Not in M4ah

- The eval needs a JDK and a provider key; it is `#[ignore]` like the other
  live evals.
- The damage is textual, not semantic: the repaired file must match the
  original behavior, which the reference tests pin.
