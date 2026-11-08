# M4h — validate regex search and multi-edit on a larger workspace

Goal: a bigger task than M4g, where a literal scan and a whole-file rewrite
are not the obvious choices. The run must show whether `code_search` with a
regex and `multi_edit` are adopted, and whether the transcript pairs their
results correctly.

## Setup

- `examples/shop-workspace`: a dozen Python modules and 14 unittest cases.
  Twelve tests describe a missing `discount` parameter across `pricing`,
  `reporting`, `checkout`, `api`, `wholesale`, `inventory` and `shipping`.
- The eval is `tests/shop_eval.rs`, ignored by default:
  `cargo test --test shop_eval -- --ignored --nocapture`.
- The prompt asks for the parameter, explicitly asks for a regex
  `code_search` of the call sites and a `multi_edit` of the several changes
  in `pricing.py`, forbids test edits, and asks for the real result.
- The eval keeps the workspace under `target/eval/shop` after a failure so
  the transcript and tool trace can be inspected.

## Result (2026-11-08)

- First attempt: the turn reached `Done`, but the automatic Python check did
  not pass; the run took 243 s. The diagnostics were not retained (the
  workspace was still a `TempDir` then).
- Second attempt with the persistent workspace: passed in **70.10 s** with
  **13 model calls**.
- Tool trace: `list_dir`, `glob`, 11 `read_file`, 2 `code_search`, 2
  `multi_edit`, 6 `edit_file`, 3 `shell`, then more reads.
- Adoption: `code_search=true`, `multi_edit=true`, `edit_file=true`,
  `write_file=false`.
- The first `code_search` args were
  `path=shop query=(total|subtotal)\( regex=true`; the second was a literal
  `discount` search. The two `multi_edit` calls succeeded with 4 and 2
  replacements.
- Automatic verification passed; the independent unittest run passed; the
  test file was byte-identical; all seven target modules changed.

## Finding: parallel results were paired by tool name

The stored session showed the two `code_search` rows with **swapped**
arguments and outputs. Both read-only calls ran in the same round; the host's
`tool_lines` matched a `ToolFinished` event to the newest unfinished row with
the same tool name, so a finish order opposite to the start order crossed the
two rows.

Fix: `tool_lines` now pairs `ToolStarted` and `ToolFinished` by `call_id` in a
map. A unit test feeds two started events and two finished events in reverse
order and asserts each row keeps its own args and output. Root suite: 28
tests + 3 ignored live evals, clippy.

## Findings

- With a larger task and an explicit prompt, the model used regex search and
  multi-edit; both worked. This is the adoption evidence M4g could not give.
- `edit_file` was used six times for the single-call-site modules;
  `write_file` was not used at all on this workspace.
- The first attempt failed the automatic check after retries. The task is
  solvable (the second attempt passed), but model variance and the cost of a
  failed large turn are real. Verification and the re-prompt are what caught
  it.
- The parallel-call pairing bug was in the host's transcript builder, not in
  tool execution. It affected the audit/UI evidence, and is now fixed.

## Implications

- Regex search and multi-edit are useful on a task of this size and should
  stay.
- A larger task can still fail; the next reliability step should be a
  visible stop or retry policy for a turn that keeps failing verification,
  or a smaller task decomposition, not another tool.
- The evals are manual and non-deterministic; they are not CI gates.

## Not in M4h

- A scored benchmark or repeated trials to measure pass variance.
- A checkpoint click-through on the shop workspace; checkpoint undo is
  covered by the desktop unit tests.
