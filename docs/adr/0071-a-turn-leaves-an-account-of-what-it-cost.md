# 0071 — A turn leaves an account of what it cost

## Context

Two reports of a frozen conversation at full CPU arrived with nothing attached.
The window could not say which path burned the core, so the only way forward was
to guess from outside the process: reproduce the turn, sample the WebContent
process, hope the spike happens while the sampler runs. The first attempt at
that produced 28% peak for a turn that had just been reported at 115%, because
the spike was not in the turn but in an idle loop beside it (ADR 0072) — and
there was no record either way.

A profiler answers this for one run. What has to survive is an always-on count
of the few paths that can run away, plus the browser's own evidence that the
main thread was blocked.

## Decision

The window keeps an account of itself (`lib/perf.ts`) and writes it while a turn
runs (`lib/report.ts`).

The account is counts and a bounded tail of durations, never a trace:

- `perf.count(name)` — streamed batches, dispatches, poll ticks, markdown cache
  hits.
- `perf.time(name, fn)` / `perf.timeAsync(name, fn)` — markdown parse and
  sanitize, background poll. Each timer keeps `SAMPLE_BOUND` (64) recent samples
  plus count, total, max, last, so a session of hours cannot grow it.
- Frames the browser could not deliver (`STALL_MS`, 50 ms) and, where the engine
  reports them, long tasks. WebKit does not report long tasks, so a missed
  animation frame is the portable evidence that the main thread was blocked —
  and a frame watcher that runs when nothing moves is itself a repaint, so it
  arms on activity and stops itself after two idle seconds rather than keeping a
  `requestAnimationFrame` loop alive for the life of the window.

The account of a turn is written at its start, every `ACCOUNT_EVERY_MS`
(5000 ms) during it, and at its end, to `<state>/perf/turn-NNNN-<phase>.json`
by `perf_report` (`src-tauri/src/perf.rs`), which keeps the newest forty and
drops the oldest. It carries the counts above plus three DOM figures — every
element, the answer bodies actually rendered, and the tool rows in the page —
so "the transcript grew" can be told from "the transcript was re-rendered".

The write is one `invoke` of a few kilobytes, off the main thread
(`spawn_blocking`), and never awaited by the turn. A diagnostic that makes the
turn slower is the thing it was meant to explain; a failed write is swallowed,
because the account is evidence, not a step.

## Consequences

The next report of "it froze" arrives with the counters that say which path ran
away, instead of a reproduction request.

- Whatever is written says what the page believed at that moment. It cannot
  attribute work to a thread the page does not see: a burn in a WebKit process
  thread that no script runs in still needs `sample`. The account narrows the
  window; it does not replace the sampler.
- Forty accounts of a long turn are a few hundred kilobytes, and the forty
  oldest are deleted on the way in. Nothing prunes them by time, so the newest
  account is always the newest turn.
- The frame watcher's idle stop means a stall is missed if it happens in the two
  seconds after the last activity — the case worth catching (a loop that freezes
  the window) is activity, so it is armed.
