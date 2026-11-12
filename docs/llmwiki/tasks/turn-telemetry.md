# Task — turn telemetry and the live cost line

## Goal

The usage line showed a cumulative input count with no explanation, and a
running turn showed no cost until it ended. Audit the tool-call event path at
the same time.

## Steps

- [x] Label the finished turn's tokens as the sum over every model call and
  clamp the cache share at 100%. Verify: `turn.inputTokensHint` in the i18n
  catalogue; `svelte-check`.
- [x] Record the model call count on a finished turn and show it as `52x`.
  Old documents recover it from their `model_finished` events. Verify:
  `old_turns_recover_usage_from_their_events` in `src/engine/mod.rs`.
- [x] Show a running turn's elapsed time, tokens, calls and cache share,
  updated as each model call finishes. Verify:
  `accumulates model usage on the running turn` in
  `desktop/src/lib/session.test.ts`; `svelte-check`.
- [x] Read only the new tail of the event log in the live poller
  (`EventLog::since`) instead of cloning the whole log every 120 ms. Verify:
  `since_reads_only_the_new_tail` in `crates/keel-runtime/src/event_log.rs`.
- [x] Fix `render_scene`, `render_film` and `screen_capture` declaring
  read-only while requesting writes. Verify:
  `the_render_tools_declare_the_writes_they_request` and
  `a_capture_asks_for_its_own_folder`.

## Notes

- A per-tool "ran concurrently" marker was considered and left out: the
  persisted tool rows carry only durations, so a truthful marker needs a start
  timestamp and the risk level in the event, and it does not change a
  decision the user makes.
- The large input totals are the conversation resent for each tool round.
  They are bounded by the context governor and mostly served from cache.
