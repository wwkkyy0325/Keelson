# Changelog

Append-only record of changes to Keelson. Kernel changes are recorded in the
Keel repository.

## 2026-11-05 — M1: the shell host

- New repository, sibling to `Keel`, consuming the pinned kernel `v0.1.0` by
  path during joint development.
- Rig schema: `name`, `title`, `description`, `ui`, `profile`,
  `[[extra_root]]`. Unknown keys are errors; a relative extra root is refused;
  a name must be a handle.
- Discovery over `$KEELSON_RIGS` or `./rigs` then `~/.keelson/rigs`, first
  directory wins, one broken file does not hide the rest.
- Commands: `rig list`, `rig show <name>`, `doctor`, `--version`.
- `doctor` prints the three version handshakes and loads every rig's profile
  through `keel-policy`.
- Conformance probes (`tests/kernel.rs`) for a text turn, a tool turn, a model
  failure, cancellation and the snapshot version.
- Copied from the reference host: the TUI specification, the coding profile
  and its prompt, and the failing-test demo workspace.

### Why

The kernel is frozen. A product on top is the next step, and the first
question is what the product is: a shell host whose rigs are files. That shape
keeps the agent thin and makes a second capability a configuration, not a code
path.

## 2026-11-05 — M2: the engine

- `engine/`: profile and permission wiring, extra roots, the twelve built-in
  tools, the five context stages, model selection (mock, OpenAI, Anthropic)
  and one turn through `TurnDriver`.
- Sessions are one JSON document per session under
  `<state>/sessions/<id>.json`: kernel snapshot, task list, and one row per
  turn with its events. A session id is refused against a different workspace.
- `view/oneshot` prints the answer on stdout. `keelson run <rig>` uses the
  rig's `ui`; `transcript` and `chat` report that they arrive with M3.
- `rigs/script.toml`: the coding profile with the oneshot face, for scripts
  and pipes.
- `KEELSON_HOME` now governs both `rigs/` and the session store, as the
  discovery section already described; the M1 code only used `~/.keelson`.
- Boundary test: no file under `src/engine/` names a terminal library.

### Why

The host needs a turn loop before a face can render one. The engine returns a
neutral report and keeps the terminal out, so the transcript view in M3 is a
module over the same path, not a second engine. The store stays a single
document because no current view queries turns; a read model can be added over
the same snapshot when one does.

## 2026-11-05 — M3: the transcript view

- `engine::Engine`: a long-lived handle that builds the driver once, runs one
  turn at a time, persists after each turn and exposes events, tasks, roots,
  mode switches and interjections. The provider is wrapped when the view wants
  deltas; text reaches the view through a channel, not a kernel event.
- `view/transcript.rs`: the ported terminal UI — the block model, streaming,
  tool lines, inline permission prompts, process collapse, `/add-dir`,
  `/cost`, `/help`, `/roots`, `Ctrl-O`, `Shift+Tab` and page scrolling.
- `view/ui_text.rs`: the bilingual chrome table (zh/en, locale and OS
  detection, `--lang auto|zh|en`).
- `render/terminal.rs`: raw mode, the alternate screen, key polling and the
  restore-on-drop guard.
- `keelson run coding` opens the transcript; `keelson run script` keeps the
  oneshot face. A piped stdin is refused for the interactive view.
- Goldens copied from the reference host into `tests/fixtures/` with the
  fixture workspace renamed `keel` → `keelson`.

### Why

The engine and the store were tested but had no face. The transcript is the
face the design names first, and porting it reuses the reference host's
reducer and goldens instead of inventing a second UI. The mock provider changed
from a one-shot script to a repeating answer so one engine can serve many
turns; the kernel's own probes still cover tool dispatch.

The view owns layout and uses ratatui text types; `render/` owns the terminal
lifecycle. The enforced boundary is unchanged: no engine file names a terminal
library.

## 2026-11-06 — M3b: transcript daily use

- A tool line carries its target (`path=…`, `program=…`) and duration. A failed
  call stays visible after the turn folds, with its first error line on the
  same row; `Ctrl-O` still expands arguments and output.
- The finished-turn aggregate no longer prints token counts. `/cost` prints the
  turn total and the session total, and `/help` lists the keys and commands.
- `BackgroundStarted`, `BackgroundResult` and `SubAgentFinished` render one
  line each, in both languages.
- Permission prompts have English and Chinese goldens
  (`tui-prompt.en.txt`, `tui-prompt.zh.txt`); the key hints are unchanged.
- A 40-column render test; a winpty smoke covers Ctrl-C and CJK input.

### Why

The ported screen worked but still leaked process detail (tokens in the
aggregate), hid failed tools the user needs to act on, and had no line for
delegated or background work. These are the daily-use items the copied
specification had left open; closing them makes the transcript the face the
design describes without adding panels.

## 2026-11-06 — M3c: a bounded transcript

- `Ctrl-O` / `/details` expands only the most recent turn. The previous global
  toggle revealed every tool argument and output in the session at once.
- On each collapse, the process detail of earlier turns is released: tool
  arguments and output are cleared, while compact lines, answers, summaries
  and user messages remain. Failed tools stay visible with their error.
- The screen redraws only when something changed (a key, an interaction, a
  delta, a turn event) or while a turn runs; an idle screen no longer rebuilds
  the transcript every 80 ms.
- Tests pin the per-turn expansion, the release, and the existing goldens.

### Why

Expanding the whole history floods the screen and makes each frame cost grow
with the session. A per-turn expansion matches how the process is actually
read, and releasing old detail bounds both memory and the work a redraw does.
The block model already carried the turn boundary, so no cursor or selection
model was needed.

## 2026-11-06 — D1: the basic desktop shell

- The engine moved into `src/lib.rs`; the CLI binary is now a thin host over
  the library.
- `Engine` exposes a read model for the desktop: `TurnView`, `TurnPage`,
  `ToolLine`, `Engine::history` (keyset by `turn_seq`), `next_turn_seq` and
  `turn`. Live tool rows are persisted with bounded args and output.
- `desktop/` is a Tauri v2 shell with a Svelte 5 + TypeScript frontend:
  setup screen, transcript, streamed answer, tool details, permission card,
  cancel and "load older" with scroll anchoring.
- The backend owns the session and emits typed events; the frontend has one
  `SessionState` and one reducer, and deduplicates by turn sequence.
- ADR 0003 records the desktop decision and amends the "never a second binary"
  rule for a host with its own event loop.
- `.env` is loaded by both hosts; the Anthropic provider falls back across the
  profile's key variable, `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN`.

### Why

The terminal face is not the shape the owner uses. Tauri gives a cross-platform
window and, later, floating-window behaviour without Electron or an HTTP
server. Keyset pagination and a single reducer are deliberate: they remove the
two failure modes that made earlier agent frontends painful — offset paging
that shifts under appends, and scattered loading/streaming/error flags.

## 2026-11-06 — D2: first-run onboarding and the Chinese interface

- Four-page setup wizard: key → model endpoint → language → finish, with a
  per-page fly/fade transition and a progress bar. Only the key is required.
- The key is stored in the OS keychain (Windows Credential Manager / macOS
  Keychain); `desktop.json` keeps only provider, endpoint, model, locale and
  the last workspace.
- `ModelOverride` on `OpenRequest` lets the desktop pass provider, endpoint,
  model and key without rewriting a profile or setting process environment
  variables.
- Chinese is the default interface; one dictionary and one `locale` store
  leave the English switch (`中/EN`) available, and every string goes through
  `t()`.
- The workspace moved out of setup. The header chip opens a folder picker and
  remembers the directory; desktop sessions live under
  `<state>/sessions/<workspace-hash>/default.json`.
- An offline mode on the key page saves the mock provider and enters the app
  without a key.

### Why

The owner uses a Chinese interface and does not want a startup form with a
workspace. Asking for the key first, then prefilled defaults, keeps the setup
short. The keychain and the model override keep the secret out of the config
and the repo. The workspace hash removes the workspace/session-id collision
that the old global session file would have created.

## 2026-11-06 — D2b: one-page settings

- The gear no longer reopens the paged wizard. It opens a one-page slide-over:
  model and key, language segments, workspace information, and a save that
  reconnects the current workspace.
- The paged wizard is first-run only. Saving the model or key reapplies the
  engine through `open_session`, so the new override takes effect without a
  restart.
- Offline mode keeps the prefilled endpoint and model, so switching back to a
  live provider later does not start from empty fields.
- The workspace label strips the Windows `\\?\` canonical prefix before it is
  shown or stored.

### Why

Paging belongs to the first run, when each decision needs its own screen.
Later changes are infrequent and benefit from seeing everything together. The
settings page also gives the model/key change a single explicit action
("save and reconnect") instead of walking the user through setup again.

## 2026-11-06 — D2c: split components and a stable page stage

- The wizard renders all four steps into one CSS grid cell. The card height is
  fixed by the tallest step, and step changes use `opacity`/`transform` only,
  so the card no longer reflows or jumps between pages.
- `App.svelte` is now a controller: boot, setup, connection and the settings
  overlay. The UI is split into `Wizard`, `Chat`, `AppHeader`, `Transcript`,
  `Turn`, `Composer`, `PermissionCard` and `SettingsPanel`.
- Shared state is only the session reducer store and the locale store.
  Components that persist settings call the API themselves; none reaches into
  another component's local state.
- Settings remain a single page; the paged flow is first-run only.

### Why

The previous wizard used Svelte `in/out` transitions on a keyed block: both
steps were briefly in normal flow, so the card resized during every page
change. Overlapping the steps in one grid cell removes the reflow by
construction. The single `App.svelte` had become a 700-line controller with
wizard, chat, pagination, permissions and settings mixed together; the split
gives each concern one file and a clear prop/API boundary.

## 2026-11-06 — D2d: the desktop is the only face

- Deleted the terminal face: `src/view/` (transcript, oneshot, bilingual
  chrome), `src/render/`, the TUI goldens, `docs/design/tui.md` and the M3
  task records.
- Deleted the `script` rig and the `run` subcommand. The CLI keeps `rig list`,
  `rig show`, `doctor`, `--version` and `--help`.
- Deleted the one-shot engine runner (`engine::run`, `RunRequest`,
  `TurnReport`). A host opens an `Engine`, supplies its interaction port and
  optional delta channel, and calls `run_turn`.
- Removed `ui` and `UiMode` from the rig schema; updated `rigs/coding.toml`,
  `rigs/coding-live.toml` and the `doctor` output.
- Dropped `crossterm`, `ratatui`, `unicode-width` and the root `dotenvy`
  dependency.
- The engine boundary test now rejects any UI crate name.
- ADR 0004 records the decision and supersedes the terminal parts of ADR 0002
  and ADR 0003.

### Why

Two faces meant two sets of modules, dependencies, goldens and docs, and the
owner does not use the TUI. A single face makes the desktop the product and
leaves the CLI with the diagnostics that are still useful. Removing the rig's
`ui` field follows: with one face left, it selects nothing.

The engine surface is now the same one the desktop already used, so this is a
deletion rather than a redesign. The engine-boundary test stays because the
library must remain UI-agnostic even with one known host.
