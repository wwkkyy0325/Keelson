# The coding rig

A living summary of the first rig: what it can do, what it refuses, and where
it still stops. The task docs under `tasks/m4*.md` hold the per-increment
evidence; this file is the map.

## Boundary

The rig is a profile, a persona and a set of tools over the Keel kernel. It
reads and edits the workspace the user chose, runs the project's own check
after a turn that changed files, and reports evidence. It does not index the
workspace, speak LSP, or execute an arbitrary command without permission.

## Tools

| Tool | Contract |
|---|---|
| `read_file` | UTF-8, `offset`/`limit`, optional `line_numbers`; every read stops at 1000 lines and names the next offset; a ranged read names the file's line count; 8 MiB head cap; the ledger records line coverage, so ranges that cover every line allow a rewrite. |
| `write_file` | Creates or replaces a file; an existing file needs a full read; the previous version goes to the trash. |
| `edit_file` | One exact replacement, optional `replace_all`; CRLF/LF fallback preserves the file's endings; needs a current read. |
| `multi_edit` | Several replacements to one file; validates all in memory, one backup, one write. |
| `apply_edits` | Parallel `paths`/`old`/`new` lists across files; validates every replacement before any write; a write failure restores earlier files from backups. |
| `glob` | `*` and `**`, newest first, bounded; ignores `.gitignore` and build directories. |
| `code_search` | Literal or regex, case flag, glob filter, `context` 0-5 with merged blocks; bounded by matches, line length and output bytes; ignore-aware. A file above 8 MiB is not searched and is named in the output. |
| `git_diff` | Read-only `status` or unstaged `diff`, optional `path`; 15 s timeout and 32 KiB cap; no permission card. |
| `shell` | Fixed argv, no shell; allowlisted programs; optional background and `mutates` hint; confirm-only. |
| `list_dir`, `make_dir`, `move_file`, `delete_file` | Bounded directory and file operations; delete and overwrite keep a trash copy. |
| `list_trash`, `restore_file` | Inspect and restore recoverable versions. |
| `todo` | A turn-scoped task list; the kernel continues while it is open. |

## Verification

- Detection order: Cargo, Go, Python, Node. Each check targets its project
  directory at the workspace root, depth 1, 2 or 3.
- Cargo uses `--manifest-path`; Go uses `-C`; Node uses `npm --prefix`;
  Python runs `pytest` when the project declares it (config file or
  requirements) and `unittest discover` otherwise, with a project `.venv`
  preferred over the bare interpreter. The venv interpreter runs from its
  workspace path: a check may name a program by an absolute path inside the
  workspace when the file name is allowlisted.
- Every project of an ecosystem is a candidate; every Python test root that is
  not nested inside another is a candidate.
- After a turn that changed files, the host selects the deepest project that
  contains each changed path (from `write_file`, `edit_file`, `multi_edit`,
  `apply_edits`, `move_file`, `delete_file`, `make_dir`, `restore_file`, or a
  shell call marked `mutates: true`). No match runs every check.
- A workspace whose tooling is outside those ecosystems can declare its own
  checks in `.keelson/checks.toml`:

  ```toml
  [[check]]
  name = "jvm"
  program = "java"
  args = ["-version"]
  ```

  A declared check always runs, its id is `workspace-check:<name>`, and its
  program still has to be allowlisted. A file that does not parse becomes one
  required item (`workspace-checks-file`) whose reason quotes the error, so a
  typo is visible instead of silently leaving the workspace unchecked.
- Each item id is `workspace-tests:<ecosystem>:<project>`; a Python id uses
  the test root. A failure names the item.
- A command or test item reports its own run time as `[duration: 0.42 s]` in
  the evidence, on pass and on failure, so the cost of a check is visible
  where its result is.
- The shell tool is not a workspace change unless the model sets `mutates`.
  A shell-only turn that omits the hint skips verification.

## Safety

- Permissions: `workspace_auto` auto-allows reads and workspace writes;
  `shell` is confirm-only; extra roots are explicit and read-only by default.
- Read ledger: one per engine; `write_file` on an existing file needs a full
  read, edits need a current read; a changed file is refused with a re-read
  instruction.
- Trash: overwrite, delete and restore keep the previous version; the desktop
  offers `list_trash`/`restore_file`.
- Checkpoints: the desktop snapshots the workspace in a shadow repository
  before and after a turn; `撤销本轮` restores the latest turn's paths.
- Secrets: the API key lives in the OS keychain; `.env` is ignored; no key is
  logged or committed.

## Persona

The coding persona forbids inventing output, claiming an unrun command,
weakening tests, writing secrets, installing packages or changing the
interpreter to make a check pass, and mixed-separator test paths. It asks for
a full read before `write_file`, `line_numbers` when lines matter, `mutates`
for a writing shell command, `apply_edits` for a cross-file change, a
`git_diff` review before finishing, and a completed `todo` list in one turn.

## Test matrix

| Layer | Coverage |
|---|---|
| Keel kernel | Tool contracts: read ledger, CRLF, multi-edit, apply-edits, glob, regex/context search, ignore walk, shell, trash, verification, turn driver, acceptance selector. |
| Keelson host | Engines, stores, rig schema, check detection/selection, named ids, git tool, registry, persona/instructions. |
| Deterministic evals | `examples/demo-workspace`, `inventory-workspace`, `shop-workspace`, `polyglot-workspace`, `cross-file-workspace`, `messy-workspace`. |
| Live evals | `coding_eval` (Python two-module), `shop_eval` (Rust multi-file), `polyglot_eval` (Node vs failing Python), `cross_file_eval` (two-file rename), `messy_eval` (1274-line Python plus 570-line Java repair), `real_task_eval` (external copy). |

Live eval record on 2026-11-09 against a local OpenAI-compatible endpoint
(`q2_0`, 256k window):

| Eval | Result |
|---|---|
| `coding_eval` | passed (17.76 s, 12 model calls) |
| `messy_eval` | passed (132.47 s) — both indentation regions and both braces repaired; python check 0.09 s, `javac` 0.58 s |
| `real_task_eval` on a copied DocFusion | passed (617.79 s) — found and fixed `docfusion` exiting 2 on an argument error, a code its own schema reserves for a partial run; 576 tests and clippy pass, `schema` output unchanged |

The same endpoint held a 256,285-token prompt with a leading needle
(`4739281`) and answered it correctly; prefill 28.7 s for 187k fresh tokens,
184 tokens/s decode.

Live eval record on 2026-11-08:

| Eval | Result |
|---|---|
| `coding_eval` | first attempt failed (13.03 s), rerun passed (30.45 s) |
| `shop_eval` | passed (85.76 s) |
| `polyglot_eval` | passed (8.63 s) |
| `cross_file_eval` | passed (11.71 s) |
| `keelson doctor` | 2 rigs, 2 profiles |

A live eval is a model run, not a deterministic test. The first `coding_eval`
attempt was a model-side miss; the rerun passed without a code change. Treat a
single live failure as a signal to inspect the transcript, not as a product
defect.

## Known limits

- No LSP, no symbol graph, no semantic rename. `apply_edits` is textual.
- Discovery stops at depth 3; a manifest or test root deeper is missed.
- A file larger than 8 MiB cannot be read whole: the recorded digest is of the
  prefix, so neither an edit nor a rewrite matches it. Only inspection works.
- One check per project; a project with no recognized manifest is unverified
  unless the workspace declares a check. A declared check runs at the
  workspace root and cannot name a working directory; it also runs on every
  verified turn, not only when its files changed.
- The declared-checks file is read when the engine opens, so an edit takes
  effect in the next session. It is the user's configuration; the persona
  tells the model not to edit it to make a check pass.
- The Node check needs a `scripts.test`; a Python check needs a
  `test_*.py`/`*_test.py` file; Go runs `go test ./...` even with no test
  files; Cargo runs the workspace.
- Shell-only mutations depend on the model setting `mutates`.
- The desktop checkpoint undo covers the latest turn only.
- A `git_diff` in a non-repository workspace returns a plain message.

## Next candidates

1. LSP diagnostics or a read-only `diagnostics` tool behind a safe policy.
2. A per-workspace check command override for projects outside the four
   ecosystems.
3. Checkpoint history with per-turn undo.
4. A real-repository eval with several ecosystems and tests.
