# Cross-file workspace

A small Node project for the cross-file edit eval. `src/math.js` exports
`add`, `src/report.js` imports it, and `test.js` exercises `report`. The task
is to rename `add` to `sum` in both source files without touching the test.
