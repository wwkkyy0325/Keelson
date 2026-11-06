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

The first launch is a four-page wizard: key, model endpoint, language, finish.
Only the key is required; the endpoint is prefilled and the language defaults
to Chinese. The key is stored in the OS keychain, not in a config file. The
wizard also offers an offline mode for a first look without a key.

After that the app connects directly. The workspace is **not** chosen during
setup: the header shows the remembered directory and opens a folder picker.
Sessions are stored per workspace, so switching projects never mixes them.
The header's 中/EN switch changes the interface language; the gear opens a
single-page settings panel for the model, key, language and workspace info.

The window shows the conversation, streams the answer, renders tool lines
after the turn, prompts for permissions and loads older turns by a stable turn
cursor. It is a normal window; floating, always-on-top and the tray are next.

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
- `docs/adr/` — rigs as files, the desktop shell, the desktop-only face.

## Status

M1: rig schema, discovery and `doctor` against kernel `v0.1.0`.

M2: the engine — profile and permission wiring, tools, model selection
(`mock`, OpenAI, Anthropic) and the JSON session store.

D1: the basic desktop shell — Tauri + Svelte, streamed turns, tool details,
permission card and keyset history pagination.

D2: first-run onboarding, the OS keychain, a Chinese-default interface and a
one-page settings panel. The terminal transcript and the one-shot CLI were
removed in D2d; the desktop is the only face.

Next: D3 (a floating window) and M4 (the coding tools do the work). See the
milestones in `docs/llmwiki/design.md`.
