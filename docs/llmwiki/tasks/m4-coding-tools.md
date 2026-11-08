# M4 — the coding rig does the work

Goal: the coding suite can edit a repository, search it, and verify the result
without a scripted prompt.

## Decisions

- The suite is the rig. One user-facing rig per capability; the model and key
  are a connection, not a suite. A conversation binds to one rig at creation
  and does not switch rigs in place; another suite means another conversation
  in the same project.
- A sub-agent is nested under a rig, never a peer rig. The rig declares the
  sub-agent templates it may call; the child is read-only, bounded, depth one,
  and invisible in the suite picker. Composition is downward inside one rig,
  not sideways between suites.
- A project stays the isolation boundary: the workspace is the permission
  root, and a rig may add declared extra roots. Conversation history is
  partitioned by project and records the rig that created it.
- Outputs and trash become project-scoped. They are currently under the
  shared state root, which lets one project list or restore another project's
  trashed files; that is a real isolation gap and is fixed before the edit
  tool starts producing more trash.
- `edit_file` is a kernel tool, not a host workaround: a search-replace edit
  with an exact-match contract, a clear failure when the match is missing or
  ambiguous, and a diff-friendly result. It goes into Keel behind the same
  tool trait as the existing twelve tools, with contract tests and an ADR.
- Project search stays on `code_search` where it already works; if it cannot
  express the coding loop, the gap is recorded before adding another search
  path.
- Verification uses the kernel's existing Verify phase and command specs.
  The host supplies the project's checks; it does not invent a second
  verification loop.
- The first screen UI change is only what the new output needs: an edit result
  (paths and diff) and a verification summary. No broad visual pass here.

## Steps

- [x] Project-scoped `outputs/` and `trash/`; record `rig` on a conversation.
      Verify: two projects cannot list or restore each other's trash; a
      reopened conversation keeps its rig.
- [x] Keel `edit_file` search-replace tool with contract tests and ADR.
      Verify: exact match replaces once; zero or multiple matches fail
      without writing; a write is audited and recoverable through trash.
- [x] Host registers the tool; the coding profile exposes it.
      Verify: the registry contains `edit_file` and both profiles validate;
      the model-driven edit is covered by the eval step below.
- [x] Project search and automatic verification in the coding loop.
      Verify: `code_search` remains the search path; a Cargo or Python
      workspace gets its own check, and a turn with an edit runs it while a
      conversational turn skips it.
- [x] Eval on `examples/demo-workspace`: repair a failing test with no
      scripted prompt.
      Verify: `cargo test` plus the eval run. The ignored test is in
      `tests/demo_eval.rs`; the live run needs a provider key.
- [x] Minimal UI for edit results and verification.
      Verify: `npm test`, `npm run check`, `npm run build`; a turn carries
      `verificationPassed` and the transcript renders a verified/failed badge.
      The existing tool line already shows the edit path and output.

## Verification

- Project state directories derive from the canonical workspace; two
  workspaces cannot share `projects/<hash>/{outputs,trash}`. Desktop tests
  pin the project directory layout and the conversation `rig` field.
- Keel: `EditFileTool` (`edit_file { path, old, new, replace_all? }`): literal
  exact match, one occurrence by default, zero/multiple fail without writing,
  UTF-8 regular files only, trash backup before write. 92 `keel-runtime` lib
  tests pass including three edit cases; clippy passes.
- Keelson registers `edit_file` and both coding profiles expose it;
  `cargo run -- doctor` loads both rigs. Root: 19 tests + boundary + kernel
  probes, clippy. Desktop: 25 tests, clippy. Frontend: 15 tests,
  `svelte-check` 0/0, build.
- The mock-model turn test for `edit_file` is still pending; the current mock
  script calls `echo`. The tool contract and registration are pinned, but a
  real model turn still has to exercise the edit.
- Automatic verification: the kernel's `with_acceptance_after_changes` runs
  the host checklist only when a workspace-changing tool succeeded. Two new
  turn tests pin the skip and the edit path. The host detects `Cargo.toml`
  (`cargo test`) and Python `test_*.py` (`python -m unittest discover -v`);
  `code_search` remains the project search. Root: 20 tests, clippy.
- `tests/demo_eval.rs` adds the live eval: copy the demo, run one turn,
  require the demo tests to pass. It uses the OpenAI-compatible endpoint with
  the same key as the desktop. The live run passed on 2026-11-08 in 14.27 s:
  the model repaired `stats.py`, the automatic `unittest` check passed, and
  the post-turn test run succeeded.
- The first live attempts exposed two request-path bugs, both fixed:
  - the driver stamped requests with the profile's model (`mock-coding`)
    instead of the override; the host now copies the override provider/model
    into `config.model_route`;
  - the OpenAI wire omitted `reasoning_content` on a thinking turn after a
    tool call; it now includes the field when `thinking_budget > 0`.
- The Anthropic-compatible DeepSeek endpoint still rejected thinking blocks
  (`content[].thinking`); the eval uses the OpenAI-compatible endpoint, which
  is the desktop's live path.
- Minimal UI: `TurnView` carries `verificationPassed` (persisted on
  `StoredTurn`), and `Turn.svelte` renders `验证通过` / `验证失败`. The edit
  already appears as a tool line with its path and output. 16 frontend
  tests, `svelte-check` 0/0, build; a live UI walkthrough of the badge is
  still pending.

## Not in M4

- The suite picker and a second user-facing rig. The selection UI arrives with
  M5, when there is something to select; this task only records the rig.
- Wiring `spawn_agent`. The kernel already has the primitive
  (`keel-runtime/src/subagent.rs`), but the host has no `SubAgentRunner` and
  the spec has no suite selector. A read-only specialist sub-agent is a
  separate, focused task after M4; it is not a general supervisor.
- A broad visual redesign of the chat.
- The floating window (D3).
- Cross-suite orchestration. Suites do not call each other; the project
  filesystem is the only shared state.
