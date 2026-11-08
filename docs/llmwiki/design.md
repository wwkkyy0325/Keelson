# Keelson — design

Keelson is a host for the Keel kernel: one library engine, several **rigs**,
and a desktop face. The desktop is the product. The CLI is a diagnostics
tool for rigs and the kernel handshake, not a second interface.

## Vocabulary

| Word | Meaning |
|---|---|
| kernel | The `keel-*` crates, pinned by path. Mechanisms and invariants only. |
| rig | One shell: a profile and optional extra scopes, in one file. |
| host | The desktop app. Loads a rig, runs turns, renders the conversation, stores sessions. |

A rig is data. Switching rigs changes behavior, tools and scopes; it does not
open a panel of toggles.

## Why this shape

The kernel resolves a profile into an immutable `RuntimeConfig` per turn, so a
second rig costs a file, not a code path. The risk of a shell host is that it
drifts into a settings application — the exact failure a thin agent is
supposed to avoid. Two rules hold the line:

1. A rig is a file (ADR 0001). The CLI offers `rig list` and `rig show`;
   editing is `$EDITOR`. There is no rig form in the app.
2. The engine contains no interface code (ADR 0004). The desktop is a host
   over the library, and `tests/engine_boundary.rs` rejects any UI crate name
   in `src/engine/`.

## Rig file

`rigs/coding.toml`:

```toml
name = "coding"
title = "Coding"
description = "Read, edit and verify a codebase."
profile = "../profiles/coding.toml"

# Extra scopes beyond the workspace. The workspace itself comes from the
# folder picked in the app, never from the rig.
[[extra_root]]
path = "~/notes"
access = "ro"
```

- `name` is the handle: lowercase letters, digits and `-`.
- `profile` resolves relative to the rig file and is loaded by `keel-policy`.
- `extra_root.access` is `ro` or `rw`; execution stays confirm-only.
- Unknown keys are an error, the same rule the profile schema uses.

Discovery: `$KEELSON_RIGS` (split as a path list) when set, otherwise `./rigs`
then `$KEELSON_HOME/rigs` (default `~/.keelson/rigs`). The first rig with a
given name wins; `doctor` reports the shadowed one.

## Frontend

The dependency direction is one line:

```
desktop/   Tauri commands, session, keychain, Svelte components
   |
src/lib.rs engine: turn loop, permissions, storage, tools   (no UI types)
```

- **Desktop**: `desktop/` is a Tauri v2 shell with a Svelte 5 frontend (ADR
  0003). `src-tauri/` owns the session, serializes a view model and paginates
  history by turn sequence; `src/` is a controller plus independent
  components and applies one reducer. The engine is a library.
- **First launch**: a three-page wizard (model + key → language → finish). The
  model list is fetched from `models.dev`, reduced to tool-capable models of
  the providers this host can drive, and cached for a day (ADR 0005). The key
  goes to the OS keychain, the selection becomes a `ModelOverride`, and the
  workspace is chosen inside the app. The wizard is a fixed 640x580 frame: the
  model list fills the body, and the `上一步` / `下一步` footer stays on the
  same bottom line on every page, with later pages centering their content
  above it. The model picker is a self-contained flex column: its list is the
  only scroll container. Page changes are a short opacity fade with no
  translate, so nothing slides under the fixed footer. Manual endpoint fields
  stay behind a disclosure for custom gateways.
- **History**: a project holds many conversations. The left rail lists the
  current project's conversations (`新对话`, inline rename, archive/unarchive)
  and switches projects from the recent list; the list has two tabs, `对话`
  and `已归档 · N`, and archived rows are the only ones that offer permanent
  delete. Both tabs fill the same area, so switching never changes the rail's
  height. An archived conversation opens read-only: the composer is replaced
  by an unarchive notice and the backend refuses a turn until it is
  unarchived. Delete confirmation is a centered dialog, not an inline row.
  The rail reserves its scrollbar gutter, a fixed count width and an overlay
  action bar, so archiving the last row or changing counts does not reflow it.
  The header shows the conversation title and an `已归档` badge. A
  conversation is one JSON document under
  `<state>/sessions/<workspace-hash>/` with an opaque, immutable id; title and
  archive flag live in the document, and `index.json` is a listing cache
  reconciled against the directory (ADR 0006). A running turn blocks
  switching, creating, archiving and deleting.
- **CLI**: `keelson rig list`, `rig show`, `doctor`, `--version`. It starts no
  session and runs no turn. There is no terminal UI.
- **Next**: the floating-window form of the desktop shell.
- **Not planned**: Electron, a separate browser/UI server, or a hidden
  terminal fallback. The Tauri webview is embedded, and no HTTP surface is
  exposed.

The boundary is testable: no file under `src/engine/` may contain `ratatui`,
`crossterm`, `tauri` or `svelte`. `tests/engine_boundary.rs` enforces it.

## Sessions

One run is one turn. A project directory under
`<state>/sessions/<workspace-hash>/` holds one JSON document per conversation,
with `<state>` from `$KEELSON_HOME` (default `~/.keelson`). Each document holds
the kernel snapshot, the task list and one row per turn with the turn's
events. The workspace is recorded in the document, so a conversation is
refused against a different project; the workspace hash keeps separate
projects from ever sharing a document.

A conversation id is opaque and immutable (`s-<unix nanos in hex>`); the
workspace hash partitions directories and the id identifies a row. The
pre-D2f `default.json` is adopted as the conversation with id `default`.
`index.json` is a listing cache, reconciled on every load: missing documents
are dropped, unknown documents are adopted and duplicates collapse to the
newest. Title and archive flag are stored in the conversation document and
mirrored in the index, so a rebuilt cache cannot lose a rename or unarchive a
conversation. An explicit new chat writes an empty document; opening a project
with no conversations only opens a blank view, and the first turn creates the
document, the row and the title. Opening a project without an id picks the
newest active conversation. Recent projects live in `desktop.json` as
shortcuts, newest first, capped at eight; they do not grant access.

When no workspace is remembered, the desktop opens `~/Documents/Keelson` (or
`~/Keelson`), created on demand, instead of the whole user home. A stored home
path is migrated the same way; the project menu changes it.

This is not the reference host's SQLite read model. The desktop reads turns
through `Engine::history`, a keyset page over the in-memory document; it never
parses the kernel events. A table layer can still be added behind the same
snapshot if a reader ever needs queries the page API cannot answer.

## Coding turn

The coding profile's working set is 64000 tokens and its output reserve is
8192; a catalog model with a smaller window lowers the working set rather
than sending a prompt it cannot hold. A turn that changed the workspace runs
one project check (Cargo, Go, nested Python tests, or Node only with a
`scripts.test`) after the turn. The Python check prefers the project's `.venv`
and runs `pytest` when the project declares it; `unittest discover` treats a
pytest module as zero tests and exits 0, which would be a false green. A
manifest in a subdirectory (depth at most three) is found and the check targets
its directory (`--manifest-path`, `go -C`, `npm --prefix`, `pytest <dir>`);
the nearest `.venv` and pytest config win. Every project of an ecosystem is a
candidate: a monorepo with two crates, two packages or two Python projects
gets one check per project, and a changed path selects the deepest one. A path
that belongs to none runs every check. The order is Cargo, Go, Python, Node
(M4p). A Python project with several test roots runs each root; a nested root
is folded into its parent (M4q). `multi_edit` counts as a workspace change.
`read_file` can read a 1-based line range
without altering its bytes; `code_search` can be case-sensitive, filtered by a
file-name or path glob and match a regex on request; the `glob` tool finds
files by path. The engine
keeps a SHA-256 read ledger: `write_file` on an existing file needs a full
read, `edit_file` and `multi_edit` need a current read, and the edit tools
match across a CRLF/LF difference without changing the file's line endings.
`code_search` can show up to five context lines around a match (M4s). A
ranged read is recorded as partial, so it cannot authorize a whole-file
overwrite (M4l). The persona forbids installing packages or changing the
interpreter to make a check pass; a missing dependency is reported. `multi_edit` validates
several replacements to one file in memory and writes them as one atomic
change. The workspace's `AGENTS.md`
(fallback `CLAUDE.md`) is appended to the system prompt at engine open, capped
at 16 KiB. Background shell commands run through the kernel's registry, write
under the project's `outputs/`, and are killed when the engine drops. Three
live evals exercise the loop: the demo repair, a two-module discount feature
in `examples/inventory-workspace`, and a larger shop task in
`examples/shop-workspace`. The two-module run used `glob`, `read_file`,
`write_file` and `shell` and left the visible tests unchanged (M4g). The shop
run used regex `code_search` and `multi_edit` and also passed the automatic
check (M4h); it exposed a transcript bug where parallel tool results were
paired by tool name, now fixed by pairing on the call id. A failed check keeps
only its non-passing evidence (bounded tail) and the transcript shows it under
the badge (M4i). A copied GameVault core accepted a root-level save-detection
task and, in a second turn, the schema-v3 persistence/backup path; with an
approving interaction port, a third turn found and fixed its own Windows path
test (293 tests pass, 69.47 s). A green host check was not product acceptance,
and denied calls lost their arguments, now fixed in the dispatcher (M4j). A
copied ValveGuard then accepted a `compare_demos.py` comparison harness in
138.66 s (23 passed, 7 new; 6 skipped); the first attempt exposed that a
dropped PATH made the model repair the global Python instead of the task
(M4k).

## Checkpoints

A shadow git repository per project lives under
`<state>/checkpoints/<workspace-key>/repo`; it uses the workspace as its work
tree and never touches the user's `.git`. A snapshot is taken before and
after each turn and the pair is recorded in `turns.json`. `撤销本轮` diffs the
two commits and, for each path, restores it from the before commit or deletes
it when the turn created it. Only the latest turn is offered. A snapshot
failure is ignored so the turn always runs; gitignored files are outside the
checkpoint (ADR 0008).

## Module layout

```
src/
  lib.rs         the library the CLI and the desktop link
  main.rs        the CLI: rig and kernel diagnostics only
  rig.rs         rig schema, discovery, validation
  doctor.rs      version handshakes and rig/profile checks
  engine/        turn execution, driver wiring, session store
desktop/
  src-tauri/     Tauri commands, config/keychain, events, interaction port
  src-tauri/src/catalog.rs
                 the models.dev fetch, provider allowlist and cache
  src-tauri/src/sessions.rs
                 per-project conversation documents, index and migration
  src-tauri/src/checkpoints.rs
                 shadow-git turn snapshots and undo
  src/           controller + components: Wizard, ModelPicker, Chat,
                 Sidebar, AppHeader, Transcript, Turn, Composer,
                 PermissionCard, SettingsPanel
  src/lib/       session reducer, i18n locale store and catalog helpers
```

A module is added when it has content; no empty placeholders.

## Milestones

M1 — Shell (done)

- repository, rig schema, discovery, validation, `rig list`, `rig show`,
  `doctor`, tests.
- Verify: `cargo test`, `cargo clippy -D warnings`, and `keelson doctor`
  loading the rigs against the pinned kernel.

M2 — Engine (done)

- The turn loop: driver, permissions, event sink, session store, mock and
  live model selection.
- Verify: the mock-model unit tests and the kernel conformance probes. A
  real-model turn needs a provider key and is a manual check.

M3 — Terminal view (removed)

- The ported terminal transcript, its goldens and the one-shot CLI existed
  here and were removed in D2d (ADR 0004). The history remains in the
  changelog; the code is gone.

M4 — Coding rig does the work (done)

- `edit_file` is an exact-match search-replace tool in Keel (ADR 0018),
  registered by the host; `code_search` remains the project search path.
- Automatic verification: a turn that changed the workspace runs the
  project's check (`cargo test`, or Python unittest discovery); a
  conversational turn skips it (ADR 0004 after-changes mode).
- Verify: the live eval repaired `examples/demo-workspace` and its `unittest`
  check passed; the transcript shows a verification badge.

M4b — Coding rig capability pass (done)

- The profile carries an 8192-token output reserve and a 64000-token working
  set; a catalog model with a smaller window reduces the working set.
- Keel `read_file` reads a 1-based line range with its exact bytes preserved
  (ADR 0019). Keel `code_search` is case-sensitive on request and accepts a
  file-name or path glob.
- `shell` background commands are wired to the kernel's
  `LocalBackgroundTasks`, with output under the project's `outputs/`.
- Automatic verification detects Cargo, Go, Python (nested) and Node (only
  with a `scripts.test`), in that order.
- Verify: the live eval still repairs the demo; the new tool contracts have
  kernel tests.

M4c — Coding rig reliability (done)

- A per-engine read ledger: `write_file` on an existing file and `edit_file`
  require a current read; a blind overwrite and a stale edit are refused with
  a re-read instruction (ADR 0020 in Keel).
- `edit_file` falls back to a line-ending-normalized literal match, so a CRLF
  checkout edits with an LF snippet and stays CRLF. Exact match remains the
  primary path.
- `glob` finds files by path (`*`, `**`); results are newest first and
  bounded. ADR 0021 records the contract.
- The workspace's `AGENTS.md` (fallback `CLAUDE.md`) is appended to the
  system prompt at engine open, capped at 16 KiB.
- Verify: the live eval edits a CRLF demo, keeps its line endings and passes
  its tests; kernel and host tests pin the guards.

M4d — Atomic multi-edit (done)

- `multi_edit` applies parallel `old`/`new` lists to one file in memory and
  writes once; a missing or ambiguous pair fails the whole call before any
  backup or write (ADR 0022 in Keel). The read ledger is checked once.
- The persona tells the model to use it for several changes to one file.
- Verify: kernel tests pin the atomic failure, one backup, unequal lists, the
  read guard and the line-ending fallback; both profiles expose the tool.

M4e — Regex content search (done)

- `code_search` accepts `regex: true`; the literal substring path remains the
  default. `case_sensitive` applies in both modes, the pattern is compiled
  once per call, and an invalid pattern is an argument error before the walk.
  Matching stays line-oriented with the existing limits (ADR 0023 in Keel).
- Adds the `regex` dependency (MIT OR Apache-2.0); the license audit passes.
- Verify: a signature pattern, the case flag in regex mode and an invalid
  pattern; the literal and glob tests are unchanged.

M4f — Undo a turn (done)

- The desktop keeps a shadow git repository per project and snapshots the
  workspace before and after each turn. `撤销本轮` restores every path that
  differs between the two commits, including creates and deletes; the
  transcript is kept (ADR 0008).
- The snapshot is best effort and never blocks a turn. Only the latest turn
  can be undone; a missing git disables the feature.
- Verify: desktop checkpoint tests restore modify/create/delete and refuse a
  repeat or missing record; the UI is type-checked and built.

M5 — A second rig

- The proof that this is a shell host: one non-coding rig built from a file,
  with no engine change.
- Starts only after M4, so the coding rig exposes what the engine actually
  needs.

D1 — Basic desktop shell (done)

- Tauri v2 + Svelte 5: setup screen, conversation, streamed answer, tool
  details, permission card, cancel and keyset history pages. The engine is a
  library; ADR 0003 records the desktop decision.

D2 — First-run onboarding (done)

- Four-page wizard, key in the OS keychain, model override, Chinese-default
  i18n with an English switch, and a workspace picker inside the app.

D2b — One-page settings (done)

D2c — Component split and a stable page stage (done)

D2d — Terminal faces removed (done)

- `src/view/`, `src/render/`, the TUI goldens, the `script` rig, the `run`
  subcommand and the rig's `ui` field are gone. The CLI keeps diagnostics.

D2e — Live model catalog (done)

- The desktop fetches `models.dev`, with the OpenCode mirror and a
  `KEELSON_CATALOG_URL` override for blocked networks, caches a reduced list
  (tool-capable, non-deprecated, newest per family) and shows it in a
  searchable picker with context, price, vision and PDF badges. First run is
  model + key; settings uses the same picker. The selected model's hard
  context becomes `ModelOverride.context_window`; the profile still owns the
  working set and output reserve. Manual endpoints remain behind a
  disclosure. ADR 0005 records the source and the fixed-provider rule.

D2f — Project history (done)

- One JSON document per conversation under the project's hash directory, an
  opaque immutable id, and a reconciled `index.json`. `default.json` migrates
  as id `default`. The left rail adds new chat, the project switcher and the
  conversation list; the header shows the title. Recent projects live in
  `desktop.json`. ADR 0006 records the identity and storage decision.

D2g — History operations (done)

- Inline rename, archive/unarchive, delete and an archived section in the
  rail. Title and archive flag live in the conversation document; the index
  mirrors them. Opening a project picks the newest active conversation.

D3 — Floating window

- Frameless, always-on-top, tray and a global shortcut on Windows and macOS.
- Starts after D2 proves the connect/send/history loop.

## Non-goals

- A terminal UI, or any second face kept without a concrete need.
- A paged settings wizard after the first run; a theme picker; a keybinding
  editor.
- Arbitrary provider endpoints taken from a remote list. A new provider is a
  code change with a tested wire protocol.
- Electron, a separate UI server, or a browser-process frontend.
- A plugin marketplace; MCP is the extension surface and the kernel already
  speaks it.
- Embeddings, a vector store, a graph database, an index of the workspace.
- Cloud sessions, accounts, telemetry.
- Multi-agent orchestration beyond the kernel's sub-agent tool.

## Kernel relationship

Keel `v0.1.0` is the pinned release; `docs/kernel.md` records the consumed
surface and the upgrade checklist. If a rig needs a kernel change, that change
goes into Keel behind a trait, with a contract test and an ADR — a host-only
workaround is the signal that the kernel is not thin enough.
