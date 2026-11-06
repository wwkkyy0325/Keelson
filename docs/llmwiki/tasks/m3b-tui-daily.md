# M3b — the transcript, daily use

Goal: finish the remaining `docs/design/tui.md` checklist so the transcript is
comfortable for a real session, not just a ported demo.

## Steps

- [x] Tool line compaction: one line per call with a target and duration; a
      failed call keeps its error after the turn collapses; `Ctrl-O` expands
      the arguments and output. The finished-turn aggregate drops token
      counts.
      Verify: `a_tool_line_names_its_target`,
      `a_failed_tool_stays_visible_after_the_turn`, and the updated
      `tui-result.txt` golden.
- [x] Background and sub-agent lines from the event log, in both languages.
      Verify: `background_and_sub_agent_events_render_one_line` asserts
      `background_started`, `background_result` and `sub_agent_finished` in
      English and Chinese.
- [x] Permission prompt goldens in English and Chinese; the keys are unchanged.
      Verify: `the_permission_prompt_is_inline` (`tui-prompt.en.txt`) and
      `the_permission_prompt_is_localized` (`tui-prompt.zh.txt`).
- [x] `/cost` prints the turn total and the session total; `/help` lists the
      keys and commands.
      Verify: `cost_reports_the_turn_and_the_session`,
      `chrome_text_has_both_arms`.
- [x] Narrow terminal: the screen degrades at 40 columns without panicking.
      Verify: `the_screen_degrades_at_forty_columns`; a winpty smoke with
      `Ctrl-C` and CJK input.
- [x] Docs, changelog and this task record.
      Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test`, `cargo run -- doctor`.

## Verification (2026-11-06)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 62 unit tests, 1 boundary test and 5 kernel probes pass.
- winpty smoke: typed `中文`, got the mock answer, the stored turn input is the
  UTF-8 bytes `e4 b8 ad e6 96 87`, and `Ctrl-C` on the idle screen exited with
  code 0.
- `keelson run script --input hello` still prints `mock turn complete`.
- Not run in a real terminal: resize and raw-mode restore after a panic. The
  40-column layout is covered by the TestBackend test.

## Decisions

- The aggregate line keeps tools, files, blocked calls and duration. Token
  totals moved to `/cost`, where they are asked for.
- A failed tool stays visible after its turn collapses; successful calls fold
  into the aggregate. `Ctrl-O` still reveals everything.
- Background and sub-agent lines are process notices: visible while the turn
  runs, folded with the rest when it ends.

## Not in M3b

- Sub-agent detail panels, background output viewers, syntax highlighting.
- The real-model turn (needs a provider key).
