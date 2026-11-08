# M4y — cross-file apply_edits, live

Goal: prove a live model uses `apply_edits` for a two-file rename and leaves
the test alone.

## Fixture

`examples/cross-file-workspace` is a Node package: `src/math.js` exports
`add`, `src/report.js` imports and uses it, `test.js` asserts
`report(2, 3) == 5`, and `package.json` runs `node test.js`.

## Eval

`tests/cross_file_eval.rs` (ignored, needs a provider key) copies the fixture
and runs one turn asking to rename `add` to `sum` in both source files with
`apply_edits`. It asserts:

- the turn is done and verification passes;
- both source files use `sum` and no longer use `add`;
- `test.js` is unchanged;
- the session's tool list contains `apply_edits`;
- `npm test` passes after the turn.

## Result

- **9.59 s**. The model read both files, applied the rename with
  `apply_edits`, left `test.js` untouched, and the Node check passed.

## Not in M4y

- A failing edit case; the kernel tests cover missing, ambiguous and
  mismatched lists.
- A real repository; this fixture is controlled.
