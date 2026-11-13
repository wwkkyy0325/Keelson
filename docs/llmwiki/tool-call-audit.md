# Tool-call audit (2026-11-13)

## Method

Read every session document under `~/.keelson/sessions/*/s-*.json`. Each turn
records `tools[]` with `name`, `args`, `durationMs`, `ok` and a 4 KB preview of
`output`. Scope: the six sessions touched since 2026-11-11 (53 turns, 2,605
tool calls, 103 failures, 3.9%). Older sessions were excluded where the code
has already changed.

## What fails

| Tool | Failures | Share | Main message |
| --- | ---: | ---: | --- |
| multi_edit | 37 | 36% | `old`/`new` is a string, not a string_list; missing `old`; old == new |
| shell | 15 | 15% | 5 timeouts at 300 s; 5 missing required arguments |
| apply_edits | 12 | 12% | same argument-shape errors as multi_edit; stale read |
| edit_file | 7 | 7% | snippet not found; read-before-write |
| read_file | 7 | 7% | path does not exist (os error 3/2); access denied |
| web_fetch | 5 | 5% | fetch failures (provider side) |
| write_file | 4 | 4% | — |
| code_search | 3 | 3% | invalid regex: unclosed counted repetition |
| todo | 3 | 3% | list closed; unknown argument |

By category: argument shape 39 (string vs list 13, missing required 10,
identical old/new 11, length mismatch 5), timeouts 5, bad paths 7, invalid
regex 3, stale read 3, tool-result invariant 2 (historical — see below).

## Findings

1. **Edit-tool argument ergonomics are the biggest cost.** 49 of 103 failures
   come from `multi_edit`/`apply_edits`, and 39 of those are a valid single
   edit expressed as a scalar instead of a one-element list. Every failure
   costs a model round trip. `keel-core/src/tool.rs:81` rejects
   `StringList` values that arrive as strings; coercing a scalar to a
   one-element list there fixes every list parameter at once (`old`, `new`,
   `paths`, `args`) with no schema change.
2. **Long shell calls dominate latency.** 815 shell calls in the window: 91
   took more than 30 s, 23 more than 120 s, 7 hit the 300 s cap. The tool
   already supports `background: "true"` and `timeout_ms`; the failures are
   commands that should have been backgrounded (servers, builds, watchers) or
   given a longer explicit timeout. This is prompt guidance, not code.
3. **Repeated identical calls.** 173 redundant calls (same name and args
   inside one turn): shell 83 (polling loops such as re-reading a log),
   view_image 37 (the same screenshot reopened), render_scene 12,
   apply_edits 8, read_file 8, multi_edit 7. Polling is sometimes the only
   way to wait; then a `wait for condition` helper would be cheaper than the
   model re-running the command.
4. **Path mistakes in read_file.** 7 failures are missing paths or
   directories opened as files. A "did you mean" hint (parent listing, closest
   sibling, or `is a directory` note) turns a round trip into a corrected call.
5. **The 4 KB preview is a UI cap, not a model cap.** 544 outputs end in
   `… truncated`; the model receives the full result, and the session
   document stores the preview. No action.
6. **Already fixed.** The two `declared read-only but requests write access`
   failures predate `f09452b` (2026-11-12), which makes `screen_capture` and
   `render_scene` mutating. Not reproducible on the current build.
7. **Health.** Cache hit rate is 92% average / 95% median; median 22 calls per
   turn (p90 103). The cost is in long turns, not in the cache.

## Density and behaviour

- Tool time is 220 min of 433 min of turn wall clock (51%). Model streaming,
  thinking and orchestration own the other half.
- **Shell owns 85% of tool wall time** (188 min, 815 calls, 13.9 s average);
  `render_film` is 11.6% (7 calls, 218 s each); every other tool together is
  under 3%. Optimising tool latency means optimising shell or not running it.
- Read behaviour is the next lever: in turns with two or more `read_file`
  calls, 16.8 reads cover 9.7 distinct paths — **42% are re-reads of a file
  already read in the same turn**. Cached input makes them cheap in tokens
  but they still cost a call and a context fragment each.
- Chains are linear, not parallel: `shell -> shell` 484, `read_file ->
  read_file` 297, `list_dir -> list_dir` 86, `view_image -> view_image` 72.
  Most are legitimate multi-file work; the shell chains are iteration loops
  (render, check, fix) and polling.
- Retry-after-failure is concentrated: `multi_edit` 20 immediate retries,
  `shell` 11, `apply_edits` 10. The edit-tool retries are the argument-shape
  problem above; each one is a wasted model round trip.
- Failure-triggered retries are not the only waste: 173 calls repeat an
  identical name+args inside one turn (see finding 3).
- Two `shell` calls failed on invalid JSON escapes (`\w` in a Windows path):
  the model wrote `D:\workspace\...` unescaped. The driver re-prompts; the
  correction message could show one escaped example.

## Paths

| Shape | Calls | Failures |
| --- | ---: | ---: |
| bare relative (`src/main.js`) | 1,178 | 22 |
| dot-relative (`./src`) | 16 | 0 |
| posix absolute (`/src/main.js`) | 4 | 4 |
| UNC (`\\server\share`) | 1 | 0 |

- 22 bare-relative failures are missing files, directories opened as files,
  or a stale name — ordinary model mistakes.
- **Every posix-absolute path failed.** On Windows `/src/main.js` resolves
  outside the workspace and does not exist. Treating a leading `/` as the
  workspace root is strictly better than the current error and is a few lines
  in the runtime's path resolver.
- No tool call used a `\\?\` verbatim path (the host strips it in its own
  display), so the verbatim form is not a user-facing problem.
- `read_file` on a directory returns a bare OS error. Naming the directory and
  suggesting `list_dir` would save the follow-up call the model otherwise
  spends rediscovering it.

## Recommended order

1. DONE — Coerce scalars into one-element `StringList`s in `keel-core`
   (`ToolSchema::widen_lists`, called by the dispatcher before validation).
   Removes roughly half of all failures, and the 20 immediate `multi_edit`
   retries with them. Tests in `keel-core/src/tool.rs`.
2. DONE — Resolve a leading `/` or `\` as workspace-relative on Windows
   (`keel_core::permission::resolve_path`). Fixes 4/4 posix failures.
3. DONE — Prompt guidance for `background`/`timeout_ms` (`profiles/prompts/coding.md`).
4. DONE — `read_file` names a directory and points at `list_dir`.
5. DONE — The malformed-arguments correction shows one escaped Windows path.
6. OPEN — Returning "unchanged since read N" for a re-read whose hash and
   mtime match the same turn: the 42% re-read share is the biggest context
   lever left, but it changes read semantics and needs its own decision.
7. Ignored — `todo` and the 4 KB preview cap are fine as they are.
