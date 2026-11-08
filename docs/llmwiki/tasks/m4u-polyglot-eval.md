# M4u — polyglot check selection, end to end

Goal: prove that a frontend-only turn runs the Node check and skips a
deliberately failing Python check.

## Fixture

- `examples/polyglot-workspace/frontend`: `package.json` with a `test` script,
  `app.js` with `add(a, b) = a - b`, and `test.js` asserting `add(2, 3) == 5`.
- `examples/polyglot-workspace/backend`: `pyproject.toml` and a test that
  always fails. It must not run for a frontend change.

## Eval

`tests/polyglot_eval.rs` is ignored and needs a provider key. It copies the
fixture, runs one turn, and asserts:

- the turn is done and verification passes;
- the report has exactly one evidence item (the Node check);
- the detail contains the Node `ok` and not the Python failure;
- neither test file was changed;
- `npm --prefix frontend test` passes after the turn.

## Result

- **10.34 s**. The model fixed `frontend/app.js`; the selector ran only
  `npm --prefix frontend test`; the failing Python check was not executed.
- The first run failed only in the eval's own final `npm` invocation because
  Rust's `Command` does not resolve the Windows `.cmd` shim; using `npm.cmd`
  fixed the harness, not the product.

## Not in M4u

- A backend-only turn; the unit selection tests cover it.
- A live task in a real polyglot repository; this fixture is controlled.
- Shell-only mutations; a shell call is not a workspace change.
