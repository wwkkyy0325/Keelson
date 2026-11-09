# M4ac — coding rig audit

Goal: consolidate the first rig into one living map and record a full live
run, so the next session starts from the capability boundary instead of the
increment history.

## What changed

- `docs/llmwiki/coding-rig.md`: boundary, tools, verification, safety,
  persona, test matrix, live record, known limits and next candidates.
- `README.md` links the new map.

## Live record (2026-11-08)

| Eval | Result |
|---|---|
| `coding_eval` | first attempt failed (13.03 s), rerun passed (30.45 s) |
| `shop_eval` | passed (85.76 s) |
| `polyglot_eval` | passed (8.63 s) |
| `cross_file_eval` | passed (11.71 s) |
| `keelson doctor` | 2 rigs, 2 profiles |

A live eval is a model run, not a deterministic test. The failed `coding_eval`
attempt was a model-side miss; the rerun passed without a code change. Treat a
single live failure as a signal to inspect the transcript, not as a product
defect.

## Not in M4ac

- No code change; the per-increment task docs remain the evidence for each
  feature.
- The known limits list is the backlog, not a promise.
