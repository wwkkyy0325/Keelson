# Keelson

A shell host for the Keel agent kernel. One engine, several rigs, a CLI and a
desktop face.

A **rig** is a file. It names a kernel profile, a view and optional scopes
outside the workspace. The workspace itself comes from where you launch it.

```
$ keelson rig list
coding        transcript   Coding
script        oneshot      Script
$ keelson doctor
keelson 0.1.0  kernel 0.1.0  events 1  snapshot 1
rig dirs    rigs
rigs        2
  coding      transcript   profile coding (WorkspaceAuto)
  script      oneshot      profile coding (WorkspaceAuto)
$ keelson run script --input "summarize this tree"
mock turn complete
```

Coding is the first rig and the reference for the shape. A second rig is a
file, not a new binary: the kernel already resolves a profile into an immutable
configuration per turn, so switching shells changes behavior, tools and face
without touching the engine.

## Running

`keelson run <rig>` runs a session and lets the rig's view render it.

The `script` rig is the `oneshot` face: one answer on stdout. The input comes
from `--input` or stdin:

```
$ echo "run the tests" | keelson run script
```

The `coding` rig is the `transcript` face: the terminal UI from
`docs/design/tui.md`. It needs an interactive terminal; `--input` prefills the
prompt and `--lang auto|zh|en` picks the chrome language:

```
$ keelson run coding
```

The run continues a session at `<state>/sessions/<id>.json`, where `<state>`
is `$KEELSON_HOME` (default `~/.keelson`). `--session-id` picks the session
and `--store` overrides the file. The file keeps the kernel snapshot, the task
list and one row per turn with its events.

A rig that names `chat` reports that the face arrives later; it is not
silently downgraded.

## Desktop

`desktop/` is a Tauri v2 shell with a Svelte 5 frontend (ADR 0003). It links
the same library engine and reads the same rigs and session files.

```bash
cd desktop
npm install
cargo tauri dev
```

Requirements: Node 20+, Rust, the Tauri prerequisites, and WebView2 on
Windows. In a debug build the app also searches the repository's `rigs/`; a
bundled build uses `$KEELSON_RIGS` or `~/.keelson/rigs`.

The first launch is a four-page wizard: key, model endpoint, language, finish.
Only the key is required; the endpoint is prefilled and the language defaults
to Chinese. The key is stored in the OS keychain, not in a config file. The
wizard also offers an offline mode for a first look without a key.

After that the app connects directly. The workspace is **not** chosen during
setup: the header shows the remembered directory and opens a folder picker.
Sessions are stored per workspace, so switching projects never mixes them.
The header's 中/EN switch changes the interface language; the gear opens a
single-page settings panel for the model, key, language and workspace info.

The window shows the transcript, streams the answer, renders tool lines after
the turn, prompts for permissions and loads older turns by a stable turn
cursor. It is a normal window; floating, always-on-top and the tray are next.

## Rigs are files, not settings

There is no settings screen. `keelson rig show <name>` prints a rig as the host
resolves it; `keelson rig new <name>` (later) copies a commented template and
opens it in `$EDITOR`. A rig is diffable, shareable and reproducible.

## Docs

- `docs/llmwiki/design.md` — what this is, the layers, the milestones.
- `docs/kernel.md` — the pinned kernel, the consumed surface, the upgrade path.
- `docs/adr/` — rigs as files, the frontend boundary, the desktop shell.
- `docs/design/tui.md` — the terminal view specification.

## Status

M1: rig schema, discovery and `doctor` against kernel `v0.1.0`.

M2: the engine — profile and permission wiring, tools, model selection
(`mock`, OpenAI, Anthropic), one turn, a JSON session store and the `oneshot`
view.

M3: the transcript view — the ported terminal UI, streamed answers, tool
lines, permission prompts, `/add-dir`, `/cost`, the bilingual chrome and the
reference goldens.

D1: the basic desktop shell — Tauri + Svelte, streamed turns, tool details,
permission card and keyset history pagination.

Next: D2 (a floating window) and M4 (the coding tools do the work). See the
milestones in `docs/llmwiki/design.md`.
