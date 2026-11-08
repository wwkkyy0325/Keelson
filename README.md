# Keelson

A host for the Keel agent kernel. One engine, several rigs and a desktop
face. The CLI is a diagnostics tool for rigs and the kernel handshake.

A **rig** is a file. It names a kernel profile and optional scopes outside the
workspace. The workspace itself is picked in the app.

```
$ keelson rig list
coding        Coding
coding-live   Coding (live)
$ keelson doctor
keelson 0.1.0  kernel 0.1.0  events 1  snapshot 1
rig dirs    rigs
rigs        2
  coding      profile coding (WorkspaceAuto)
  coding-live profile coding-live (WorkspaceAuto)
```

Coding is the first rig and the reference for the shape. A second rig is a
file, not a new binary: the kernel already resolves a profile into an
immutable configuration per turn, so switching shells changes behavior, tools
and scopes without touching the engine.

## Desktop

`desktop/` is a Tauri v2 shell with a Svelte 5 frontend (ADR 0003). It links
the library engine and reads the same rigs and session files.

```bash
cd desktop
npm install
cargo tauri dev
```

Requirements: Node 20+, Rust, the Tauri prerequisites, and WebView2 on
Windows. In a debug build the app also searches the repository's `rigs/`; a
bundled build uses `$KEELSON_RIGS` or `~/.keelson/rigs`.

The first launch is a three-page wizard: model + key, language, finish. The
model list comes from [`models.dev`](https://models.dev), filtered to
tool-capable, current models of the providers this host can drive. Each row
shows context, price and vision/PDF capability. The selected model's hard
capacity bounds the profile's 64k working set and 8k output reserve; a model
with a smaller window lowers the working set. It is cached for an hour and
revalidated with its ETag; only the key is typed.

The catalog is fetched from `models.dev`, with the byte-identical
`models.opencode.ai` mirror as a fallback. If both are unreachable, a bundled
snapshot keeps the picker usable; set `KEELSON_CATALOG_URL` in `.env` to point
at your own proxy or mirror. A manual endpoint disclosure stays available for custom
gateways. The key is stored in the OS keychain, not in a config file. The
wizard also offers an offline mode for a first look without a key.

After that the app connects directly. The workspace is **not** chosen during
setup: the left rail's project menu shows the recently opened folders and
opens a folder picker, and `新对话` starts another conversation in the current
project. If nothing is remembered, the app opens `~/Documents/Keelson` (or
`~/Keelson`), created on demand — never the whole home directory. A stored
home path is migrated to that landing folder. Each project keeps its own
conversation list, so switching projects never mixes them; a conversation is
one JSON document with an immutable id, and the rail lists it by the first
user message. Rows can be renamed and archived behind a `已归档 · N` tab;
an archived conversation opens read-only until it is unarchived, and only
archived conversations can be permanently deleted, after a `永久删除对话?`
dialog.
The header's 中/EN switch changes the interface language; the gear opens a
single-page settings panel with the same model picker, key, language and
workspace info.

The window shows the conversation, streams the answer, renders tool lines
after the turn, prompts for permissions and loads older turns by a stable turn
cursor. A failed automatic check keeps the failing output and shows it under
the turn. It is a normal window; floating, always-on-top and the tray are
next.

## CLI diagnostics

The desktop is the interface. The CLI does not run turns.

```bash
cargo run -- rig list
cargo run -- rig show coding
cargo run -- doctor
```

A rig that fails to parse is reported without hiding the other rigs. A rig
whose name is claimed by an earlier directory is reported as shadowed.

## Rigs are files, not settings

There is no rig form in the app. `keelson rig show <name>` prints a rig as the
host resolves it; a rig is diffable, shareable and reproducible. The in-app
settings page covers the model connection and language, not rig behavior.

## Docs

- `docs/llmwiki/design.md` — what this is, the layers, the milestones.
- `docs/kernel.md` — the pinned kernel, the consumed surface, the upgrade path.
- `docs/adr/` — rigs as files, the desktop shell, the desktop-only face, the
  model catalog, project conversations, project instructions and turn
  checkpoints.

## Status

M1: rig schema, discovery and `doctor` against kernel `v0.1.0`.

M2: the engine — profile and permission wiring, tools, model selection
(`mock`, OpenAI, Anthropic) and the JSON session store.

D1: the basic desktop shell — Tauri + Svelte, streamed turns, tool details,
permission card and keyset history pagination.

D2: first-run onboarding, the OS keychain, a Chinese-default interface and a
one-page settings panel. The terminal transcript and the one-shot CLI were
removed in D2d; the desktop is the only face.

D2e: the model catalog. The picker is fed by `models.dev`, cached and
refreshable; first run is a model choice plus a key.

D2f: projects own conversations. The left rail lists history, starts a new
chat and switches recent projects; each conversation is its own document with
a stable id.

D2g: history operations. Rows can be renamed, archived and permanently deleted
after archiving; archived conversations are read-only. The rail uses stable
`对话` / `已归档` tabs.

M4: the coding rig edits and verifies. `edit_file` is an exact-match
search-replace tool in Keel; a turn that changes the workspace runs the
project's own check (Cargo, Go, Python or Node). The live demo eval repairs
`examples/demo-workspace`, a second eval adds a discount across two modules
in `examples/inventory-workspace`, and a larger eval crosses the modules in
`examples/shop-workspace`. A copied GameVault core also accepted a root-level
save-detection and backup task across two turns; with execution approved, the
model then fixed its own one-line Windows path test.

M4b: the coding rig is usable on a real repository. The output reserve is 8k
and the working set 64k, lowered when the model window is smaller. `read_file`
reads line ranges; `code_search` can be case-sensitive and glob-filtered;
background shell commands are wired to the kernel registry.

M4c: the coding rig is safe to write with. A per-engine read ledger makes
`write_file` require a full read and `edit_file` a current read, so a blind
overwrite, a partial-read overwrite or a stale edit is refused instead of
applied. `edit_file` matches across CRLF/LF
without changing the file's line endings; `glob` finds files by path; the
workspace's `AGENTS.md` (fallback `CLAUDE.md`) is added to the system prompt.

M4d: `multi_edit` applies several replacements to one file atomically. A
missing or ambiguous pair fails the whole call before anything is written;
the batch takes one backup and one write.

M4e: `code_search` matches a regex on request; a literal substring remains
the default, with the same file, depth, result and permission limits.

M4f: a turn can be undone. The desktop snapshots the workspace in a shadow
git repository before and after each turn; `撤销本轮` restores the files the
turn changed, including creates and deletes, and keeps the transcript.

M4i: a failed automatic check keeps only the non-passing evidence, with the
tail of the output, and shows it under the turn's verification badge.

M4k: the coding rig works on Python. The check prefers the project's `.venv`
and runs `python -m pytest -q` when the project declares pytest;
`unittest discover` would report zero tests and pass. A copied ValveGuard
accepted a `compare_demos.py` comparison harness; the clean run passed 23
tests (7 new) and skipped 6 in 138.66 s.

M4l: `write_file` over an existing file needs a full read; a line-range read
records partial coverage and cannot overwrite. The persona refuses package
installs and interpreter changes, and reports a missing dependency instead.

M4m: a monorepo's subproject is checked. A manifest at depth one or two is
found and the check targets its directory (`--manifest-path`, `go -C`,
`npm --prefix`, `pytest <dir>`); the nearest `.venv` and pytest config are
used.

M4n: a polyglot workspace runs one check per detected ecosystem, in Cargo,
Go, Python, Node order. Every check is required, so a failing frontend test is
visible even when the turn edited Rust.

M4o: only the projects a turn touched are checked. The kernel records the
paths a mutating call changed; the host picks the deepest project for each and
falls back to every check when a path belongs to none. `multi_edit` now counts
as a workspace change.

M4p: every project of an ecosystem is a candidate. A monorepo with two crates,
two packages or two Python projects gets one check per project, and the changed
path selects the one it belongs to.

M4q: a Python project with several test roots runs each of them. Nested roots
are folded into their parent, so `tests/` also covers `tests/unit/`.

M4r: manifests and test files are found up to three directories deep, so a
service nested under `services/api/backend/` gets its own check.

M4s: `code_search` can show up to five lines around a match. The match keeps
`path:line:`, neighbors use `path:line-`, and blocks are separated by `--`;
the result and output caps are unchanged.

M4t: overlapping search context merges into one block, so a line prints once,
and the output cap reports itself instead of claiming the result limit.

M4u: the polyglot fixture proves the selector end to end. A live turn that
fixed only `frontend/` ran the Node check and skipped the fixture's failing
Python test (10.34 s).

M4v: a shell call that writes files can mark `mutates: true`; the turn is then
verified like a file-tool edit, with every check as the fallback.

M4w: `read_file` can prefix lines with absolute numbers (`line_numbers`) for
navigation; the prefix is display-only and never part of the file.

M4x: `apply_edits` applies exact replacements across files. Every replacement
is validated before any file is written; a failure leaves the workspace
untouched.

Next: M5 (a second rig) and D3 (a floating window). See the milestones in
`docs/llmwiki/design.md`.
