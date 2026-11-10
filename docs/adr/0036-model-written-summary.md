# ADR 0036 — A model writes the session summary

## Status

Accepted and implemented in the engine.

## Context

A segment seals when the next input changes topic. The sealed segment leaves a
structured record — goal, result, changed paths, open items, verification — and
that record is the only compressed view of older history when no summarizer is
attached. The decisions, their reasons, the exact error text and the commands
are not in it, so a long session loses exactly the material a reader needs. The
kernel ships `ProviderSummarizer`; no host had wired one.

## Decision

1. `Engine::open` builds a second provider instance from the same settings and
   attaches `ProviderSummarizer`. The turn's stream is not reused: the summary
   call carries no tools, no control tool, `thinking_budget = 0` and
   `ReasoningLevel::None`.
2. The memo policy stays the kernel's: one rolling summary rewritten at each
   seal, budget `working_set / 10` clamped to 256–4096 tokens, a 30 s timeout,
   and the deterministic fallback plus `summary_failed` when the call does not
   land. Covered records fold out of the context; the transcripts stay in
   storage and lexical recall can still reach them.
3. The mock provider (the offline rig) never gets a summarizer: there is
   nothing to summarize, and the offline demo stays deterministic.
4. Summary usage is added to the turn that triggered the seal — `model_calls`
   and tokens — so the cost line tells the truth.

## Consequences

- Older history stays readable (decisions, error text, open items) instead of
  shrinking to a record row, at the cost of one model call per topic change.
- The call runs at the start of the next turn, before context assembly, so its
  latency shows up as a slower first token; the 30 s timeout bounds it.
- A provider that thinks by default may spend reasoning tokens on the summary
  call even though `thinking_budget` is 0. That is a provider behavior, not a
  reason to disable summaries.
- Rollback is host-only: remove the `with_summarizer` call. No session
  migration and no kernel version change.
