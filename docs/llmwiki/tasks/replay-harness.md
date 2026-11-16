# Transcript replay harness

> Retired: the harness (`tests/replay_eval.rs`) and its `profiles/replay.toml`
> were removed from the tree; see `docs/llmwiki/changelog.md`. This file stays
> as the record of what it did and how each step was verified.

Goal: pour a real, long Claude Code history into the engine and observe the
kernel's own mechanisms (context assembly, digests, the working-set ladder,
tool loops, usage) under a live model.

1. [x] Extract human prompts from a Claude Code JSONL transcript.
   Verify: 185 prompts from the 209 MB AIPhone session.
2. [x] Prepare a disposable workspace from the project at HEAD.
   Verify: `git clone --shared` into the temp dir; no source file changes.
3. [x] Rewrite absolute paths that point at the original project.
   Verify: prompt 105 reaches the copy; the model reads real files.
4. [x] Run one engine turn per prompt and record kernel events.
   Verify: one-turn smoke run on q2_0, 56 s, 27 tool calls, 16 model calls,
   context 21,005 / 64,000, 91% cached input.
5. [x] Bound the run: per-turn timeout, wall-clock budget, error stop.
   Verify: the smoke run cancels nothing and stops after the budget.
6. [x] Write `turns.jsonl` and the summaries the report is built from.
   Verify: the files exist in `REPLAY_OUT` after the run.
7. [x] Long run over prompts 105..185 on q2_0.
   Verify: report in `REPLAY_OUT`; summary carries delivered/digest/dropped
   and any `context_escalated` steps. Two budget-bounded runs completed:
   0.2.0 clean (105–135, 31 turns, 60 min) and 0.2.1 cap (105–129, 25 turns,
   45 min). The full range was not reached; a 81-prompt run needs a longer
   budget.
