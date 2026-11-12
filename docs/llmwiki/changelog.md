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

## 2026-11-06 — D2e: a maintained model catalog

- `desktop/src-tauri/src/catalog.rs` fetches `https://models.dev/api.json`
  with the byte-identical `https://models.opencode.ai/api.json` mirror as a
  fallback; `KEELSON_CATALOG_URL` can point at another proxy. It keeps
  tool-capable models for a fixed provider allowlist and caches the reduced
  list at `<state>/model-catalog.json` for an hour. Deprecated models
  are dropped, each family keeps only its newest model, and a six-month window
  hides stale families. After the hour, an `If-None-Match` request revalidates
  with the stored ETag; a 304 keeps the cache without a body download. A
  manual refresh and a bundled offline snapshot are included; a failed refresh
  keeps the cached list.
- Provider kind, endpoint and key hint are fixed in code. The remote JSON
  supplies model metadata only, so a bad entry cannot redirect a key.
- The first wizard page is now model + key; the separate endpoint page is
  gone and the wizard is three pages. Manual provider, endpoint and model
  fields remain behind a disclosure.
- The wizard is a fixed 640x580 frame. The model list fills the body, and the
  `上一步` / `下一步` footer is on the same bottom line on every page; the
  language and finish content centers above that footer. Manual mode seeds its
  protocol, endpoint and model from the current catalog selection, sits in a
  full-height panel and labels the selector `接口协议`; the key line no longer
  prints an environment variable name. Headings, body text and model ids use
  separate display, text and mono stacks with a tighter size hierarchy.
- `ModelPicker` now owns its flex layout: the toolbar is fixed, the list takes
  the remaining height and is the only scroll container (`overflow-y: auto`,
  `overscroll-behavior: contain`). The wizard passes `fill`; settings keeps a
  fixed list. The page stage no longer serves as the model-list scrollbar.
- The stage grid uses `minmax(0, 1fr)` and the step has `min-height: 0`; a
  plain `1fr` kept the step's min-content height, so a five-model list made the
  stage scroll instead of the list. With the fix, the list shrinks and owns
  the scrollbar while the header and footer stay put.
- Page changes are opacity-only now. The old 6px translate made the page
  slide under the fixed footer, and the stage's scrollbar could appear or
  disappear between pages, shifting the content sideways. The stage clips;
  the list is the only scroll container.
- The default workspace is `~/Documents/Keelson` (or `~/Keelson` when
  Documents is absent), created on demand. A stored home path is migrated to
  it; the workspace permission root is no longer the whole user profile.
- The selected model row is marked with a check instead of a persistent
  background, so hovering another row highlights only that row.
- The settings panel uses the same searchable `ModelPicker` and refresh.
- The picker reads input modalities and marks vision and PDF models; the
  selected model's hard context is stored with the connection and passed as
  `ModelOverride.context_window`, while the profile keeps its working-set
  budget and output reserve.
- `SetupState` and the config record the catalog provider and which provider
  the stored key belongs to; switching providers requires a new key instead
  of reusing the old one silently.
- ADR 0005 records the source, the fixed-provider rule and the cache.

### Why

The first run asked the user to know a wire protocol, a base URL and a current
model name. The owner wanted to choose a model from a current list and type
only a key. A maintained catalog is the only way to keep model ids fresh
without shipping a release for every model launch; fixing endpoints in code
keeps that convenience from becoming a key-exfiltration path.

## 2026-11-06 — D2f: projects own conversations

- A project directory under `<state>/sessions/<workspace-hash>/` now holds one
  JSON document per conversation plus an `index.json`. The previous
  `default.json` is adopted as the conversation with id `default`.
- A conversation id is opaque and immutable (`s-<unix nanos in hex>`), not a
  hash of the turns. The workspace hash partitions directories; the id
  identifies a row.
- The index is a cache reconciled against the directory on every load:
  missing documents are dropped, unknown documents are adopted, and duplicate
  rows collapse to the newest. An explicit "new chat" writes an empty
  document, so it survives a restart; opening a project with no conversations
  only opens a blank view until the first turn.
- Titles come from the first non-empty line of the first user message (48
  characters). A `session-updated` event refreshes the row after every turn.
- New commands: `new_session`, `list_sessions`, `delete_session`;
  `open_session` takes an optional conversation id and otherwise opens the
  newest conversation in the project.
- `desktop.json` records up to eight recent projects, newest first. It is a
  shortcut list, not a permission grant.
- The chat shell has a left rail: brand, project switcher, `新对话`, and the
  current project's conversations with inline delete. The header shows the
  conversation title.
- A running turn blocks switching, creating and deleting. Deleting the current
  conversation creates a new one first, so the engine never writes to a
  deleted document.

### Why

A workspace was the permission root, the storage partition and one endless
conversation at once. Different projects were isolated, but a project had no
new chat and no history. A content hash cannot be the conversation key because
it changes with every turn; an opaque id plus a reconciled index gives stable
identity and lets a lost index be rebuilt without hiding documents.

## 2026-11-06 — D2f follow-up: duplicate rows

- The first boot after D2f exposed an index from an intermediate build with the
  same row twice; the Svelte keyed list threw and the app stayed on the loading
  screen. `reconcile` now collapses duplicate ids, keeps the newest row and
  rewrites the index. A unit test pins it.

## 2026-11-06 — D2f follow-up: no phantom conversation

- Opening a project with no conversations no longer writes an empty document.
  It opens a blank conversation with no row; the first turn creates the
  document, the row and the title. Visiting a folder does not leave an
  untitled conversation behind.
- An explicit `新对话` still writes an empty document, so a new chat made on
  purpose survives a restart.
- Switching conversations resets the composer draft; the empty transcript
  names the current project; deleting a conversation shows a short toast, and
  deleting the open one says a new conversation was started.

### Why

Auto-creating a conversation on open made every folder visit look like an
action and accumulated empty rows. The engine can open a store path that does
not exist yet, so the document can wait for the first turn without any new
state. Resetting the draft, naming the project and confirming a delete close
the three small gaps the first walkthrough exposed.

## 2026-11-06 — D2f fix: the live profile's model id

- `profiles/coding-live.toml` used `model = "deepseek-flash[1M]"`. The
  provider accepts `deepseek-flash` and `deepseek-v4-pro`; the `[1M]` suffix
  is not part of the API model id, so a live turn failed with
  `400 Bad Request: ... you passed deepseek-flash[1M]`.
- The profile now uses `deepseek-flash` and carries the capacity in
  `context_window = 1048576`, where it belongs.
- The desktop builds its model override when it opens a workspace. Editing
  `desktop.json` outside the app does not reconnect a running window; restart
  the app or save settings to apply the new value.

### Why

A model id and a context capacity are different fields. The desktop catalog
already supplied the capacity through `ModelOverride.context_window`; the
profile's suffix copied a client-side display convention into the wire model
name. `cargo run -- doctor` validates the profile again.

## 2026-11-06 — D2g: history operations

- `StoredSession` now carries `title` and `archived`. They live in the
  conversation document; `index.json` mirrors them, so rebuilding the cache
  cannot lose a rename or bring an archived conversation back.
- `rename_session` and `archive_session` commands return the updated row and
  emit `session-updated`. The open conversation is updated through the engine,
  so a later turn cannot overwrite the change with the values loaded at open.
- The rail's rows show hover actions: rename (inline input, Enter saves,
  Escape cancels) and archive/unarchive. Archived rows sit under a collapsed
  `已归档 · N` section, stay openable, and are the only rows with a permanent
  delete action. The backend also refuses to delete an active conversation.
- Opening a project without an id chooses the newest active conversation.
  Archiving the open conversation switches to the next active one, or a blank
  view when none remains.
- A running turn blocks archiving and deleting; renaming stays available.

### Why

The history list could create, switch and delete, but there was no way to name
a conversation, put an old one away without destroying it, or find it again.
Keeping the metadata in the document rather than only in the index makes the
index safe to rebuild, which was already the design rule for rows.

## 2026-11-06 — D2g follow-up: archived rows survive a restart

- The walkthrough archived both conversations, restarted the process and
  confirmed the archived flags and custom titles were still in the documents
  and the rail; unarchiving after the restart worked.
- Deleting an archived row works through the same inline confirmation.

## 2026-11-06 — D2g follow-up: archive before delete

- Active rows no longer show a delete action. Only archived conversations can
  be deleted, and `delete_session` refuses a non-archived document with
  `请先归档再删除`.
- The confirmation now says `永久删除?` and the toast says `已永久删除对话`,
  because deletion is final; archive is the recoverable step.
- There is no trash or timed recycle bin. The owner chose archive-first
  permanent delete over a soft-delete window.

### Why

Archive hides a conversation but keeps its document; delete erases it. Making
archive the required step gives one recoverable state before the irreversible
one without adding a second retention system. The backend enforces the rule,
so a stale UI or a direct call cannot skip it.

## 2026-11-06 — D2g follow-up: two tabs and a delete dialog

- The conversation label became a two-tab control, `对话` and `已归档 · N`.
  Both tabs render into the same list area, so expanding archived rows no
  longer shrinks the active list or moves the scrollbar.
- Opening an archived conversation selects the archived tab; unarchiving the
  open conversation returns to the active tab. `新对话` and switching projects
  reset to the active tab.
- Delete confirmation moved out of the row into a centered dialog with the
  conversation title. Cancel is focused, Escape and the backdrop cancel, and
  the row keeps its 32.6 px height while the dialog is open.

### Why

The stacked archived section changed the rail's height and made the archive
flow feel like a second list bolted onto the first. Tabs give the two states
one stable frame. The inline confirmation changed the row's height and shifted
every row below it; a rare destructive action belongs in a modal, not in the
list's flow.

## 2026-11-06 — D2g follow-up: archived conversations are read-only

- Opening an archived conversation still renders its transcript, but the
  composer is replaced by an `已归档的对话不能继续` notice and the header
  carries an `已归档` badge. The notice's unarchive action restores the
  composer.
- `send_message` refuses an archived conversation with
  `已归档的对话不能继续；请先取消归档`, so a stale UI cannot bypass the rule.
- Unarchiving the open conversation switches the rail back to the `对话` tab.

### Why

Archive should mean "put away", not "still live". Keeping history readable is
useful; letting the user keep typing without noticing the state makes archive
meaningless and hides which conversation is active. The backend guard keeps
the invariant even if the frontend misses it.

## 2026-11-06 — D2g follow-up: a stable history rail

- `.session-list` reserves its scrollbar with `scrollbar-gutter: stable`, so
  losing the last row (and its scrollbar) no longer changes the list width.
- The archived tab count has a fixed `2ch` width, so moving between `0`, `24`
  and `25` does not move either tab label.
- Row actions are an absolutely positioned overlay with an opacity
  transition. The title and timestamp keep their widths; hovering changes
  neither the row height (32.625 px) nor the title width (160.55 px).
- Switching tabs resets the new list to the top instead of inheriting a
  clamped scroll position.
- The archived notice has a `4rem` minimum height, close to the composer's
  footer height, so opening an archived conversation does not resize the
  transcript by 11 px.

### Why

The rail twitched when the last row disappeared, when the archived count
changed, and when hover actions replaced the timestamp. Each was a layout
change, not an animation: a scrollbar appearing, a count changing width, or
`display: none` on the actions. Reserving space or moving the toolbar out of
the flow removes the reflow instead of trying to hide it.

## 2026-11-08 — M4 start: project state isolation and `edit_file`

- `outputs/` and `trash/` moved under
  `<state>/projects/<workspace-hash>/`. Two projects can no longer list or
  restore each other's trashed files.
- A conversation document records the `rig` that created it;
  `SessionMeta`/`SessionView` mirror it. Legacy documents are filled on the
  next open. The suite picker still waits for a second suite.
- Keel gains `edit_file { path, old, new, replace_all? }`: a literal
  search-replace, one occurrence by default, zero or multiple matches fail
  without writing, UTF-8 regular files only, and the previous version goes to
  the trash. ADR 0018 and three contract tests.
- Keelson registers the tool and both coding profiles expose it.
  `cargo run -- doctor` loads both rigs.

### Why

The edit tool is the first M4 capability, but it multiplies the number of
writes into the trash. Per-project state is the isolation the editor needs;
recording the rig now avoids a migration when the second suite arrives.
The edit contract is deliberately exact: a fuzzy match would hide an
unintended change, and an ambiguous match must ask for a longer snippet
rather than guess.

## 2026-11-08 — M4: automatic verification

- Keel `TurnDriver::with_acceptance_after_changes`: the host checklist runs
  only when the turn successfully used a workspace-changing tool
  (`write_file`, `edit_file`, `move_file`, `delete_file`, `make_dir`,
  `restore_file`). A conversational turn skips the check entirely; `shell` is
  excluded so running tests cannot trigger a second run.
- Keelson supplies the project's check: `Cargo.toml` → `cargo test`; a
  Python `test_*.py` file → `python -m unittest discover -v` (or `python3`).
  Unknown ecosystems stay unverified instead of guessing.
- `tests/demo_eval.rs`: an ignored live eval that copies
  `examples/demo-workspace`, runs one turn, and requires the demo tests to
  pass. It is skipped without `ANTHROPIC_AUTH_TOKEN`.

### Why

Running the full test suite after every chat message would make a
conversational turn expensive and slow. The edit is the trigger, and the
project's own command is the evidence; the kernel already owns the Verify
phase and the bounded reprompt loop, so the host only supplies the checklist.
The unknown-ecosystem rule avoids inventing a check that may not exist.

## 2026-11-08 — M4: the live coding eval passes

- `cargo test --test demo_eval -- --ignored` passed in 14.27 s: the coding rig
  read the demo, repaired `stats.py`, and the automatic `unittest` check and
  the post-turn test run both passed.
- The eval uses the OpenAI-compatible endpoint with the same key as the
  desktop. The Anthropic-compatible DeepSeek endpoint still rejects thinking
  blocks on a tool round, so it is not the eval path.
- Two request-path bugs surfaced and were fixed:
  - the driver stamped every request with `config.model_route` (the profile's
    `mock-coding`) while the provider used the override model; the host now
    copies the override provider/model into the route;
  - the OpenAI wire omitted `reasoning_content` after a thinking turn with a
    tool call; it now includes the field when `thinking_budget > 0`.

### Why

The eval is the first end-to-end proof that the coding suite edits and
verifies without a scripted prompt. It also caught two mismatches that a
single conversational turn could not: the mock profile's model leaking into a
live request, and DeepSeek's thinking-mode requirement. Both are host/wire
bugs, not model failures.

## 2026-11-08 — M4: verification in the transcript

- `StoredTurn` and `TurnView` carry `verificationPassed`, persisted from the
  turn outcome's acceptance report. Old turns read as `null`.
- `Turn.svelte` renders a small `验证通过` / `验证失败` badge when the field is
  present; the edit already appears as a tool line with its path and output.
- Frontend: 16 tests, `svelte-check` 0/0, build. A live UI walkthrough of the
  badge is still pending.

### Why

The eval proved the loop, but the transcript did not show that a check ran.
Persisting one boolean is enough for the first UI: the model's narrative and
the verification evidence stay separate, and a failed check is visible next
to the turn's error.

## 2026-11-08 — M4b: the coding rig reaches a real repository

- `profiles/coding.toml` and `coding-live.toml`: `max_output_tokens = 8192`,
  `budget_tokens = 64000`. `Engine::open` now lowers the working set to
  `model capacity - output reserve` when a catalog model has a smaller
  window; a 32k model gets a 23808-token working set. The override no longer
  silently claims capacity the request cannot use.
- Keel `read_file { path, offset?, limit? }`: 1-based line ranges, exact
  bytes and CRLF preserved, no line numbers. An offset past the end reports
  the line count. This is what the persona already told the model to do.
- Keel `code_search { ..., case_sensitive?, glob? }`: a boolean case switch
  and a `*`/`?` glob. A glob without a separator matches the file name; with
  one it matches the relative path. The skipped-directory list now covers
  `out`, `coverage`, `vendor`, virtualenvs, tool caches and common JS build
  output. ADR 0019 in Keel records the contract.
- Background shell is real: the host builds `LocalBackgroundTasks` under the
  project's `outputs/background`, registers the shell tool through
  `with_background`, and passes it to the driver. Dropping the engine kills
  the process tree, matching the persona's session-scoped promise.
- Automatic verification is an ordered table: Cargo, Go, Python (recursive
  to depth 2), Node only when `package.json` declares a non-empty
  `scripts.test` that is not npm's `no test specified` placeholder.
- `echo` leaves the visible tool list of both coding profiles; it remains
  registered for the kernel tests and the mock fixture.

### Why

M4 proved the loop but only on a small Python demo. The four limits that
matter on a real repository were an output cap that truncates edits, a
working set advertised as 1M but actually 16k, a `read_file` that cannot
reach a line range, and a `background` flag that the host never backed. The
live eval passed again in 19.61 s after the changes; the new tool contracts
have kernel tests and the verification table has host tests.

## 2026-11-08 — M4c: the coding rig writes safely

- Keel `ReadLedger`: a per-engine SHA-256 record of successful reads.
  `write_file` on an existing file and every `edit_file` require a current
  read; an unread or changed file is refused with a "read it again" message.
  New files need no read. The host wires one ledger into the read, write and
  edit tools. ADR 0020 in Keel records the contract.
- `edit_file` falls back to a line-ending-normalized literal match when the
  exact snippet is absent. A CRLF checkout edits with an LF snippet and stays
  CRLF; `new` is converted to the file's style. ADR 0018 gains the amendment.
- New Keel `glob` tool: `*` within a segment, `**` across segments, a pattern
  without a separator matches the file name at any depth; files only, newest
  first, capped. ADR 0021 records it. Both coding profiles expose it.
- The workspace's `AGENTS.md` (fallback `CLAUDE.md`) is appended to the
  system prompt at engine open, capped at 16 KiB.
- The persona tells the model to read before writing and to prefer
  `edit_file` for targeted changes.
- Live eval on a CRLF demo: 11.86 s, tests pass, `stats.py` still CRLF. The
  model followed read-before-edit; no refusal recovery was needed.

### Why

The M4b tools could edit, but a blind `write_file` could destroy a file and a
Windows CRLF checkout broke exact-match edits. Both are failures that lose
work, not failures that merely waste a turn. The read ledger and the narrow
line-ending fallback fix them without adding fuzzy matching that would hide
an ambiguity. `glob` closes the last obvious discovery gap against the tools
the major coding agents expose.

## 2026-11-08 — M4d: `multi_edit` is atomic per file

- Keel `MultiEditTool`: `multi_edit { path, old: string_list, new:
  string_list, replace_all? }`. Parallel lists, applied in order to an
  in-memory copy. A missing or ambiguous pair fails the whole call before a
  backup or a write; a successful batch takes one backup and one write.
- Matching is the `edit_file` contract, including the read ledger and the
  line-ending fallback. ADR 0022 in Keel records it.
- Both coding profiles expose the tool; the persona recommends it when one
  file needs several changes.
- Keel: 107 lib tests (six new multi-edit cases) and 22 integration tool
  tests, clippy.

### Why

A three-step edit made through three `edit_file` calls can leave two changes
on disk when the third fails. The trash recovers a file, but it does not make
the batch atomic. `multi_edit` closes that gap for the common one-file case;
a multi-file patch remains a separate decision.

## 2026-11-08 — M4e: regex content search

- `code_search` accepts `regex: true`. The literal substring path remains the
  default; `case_sensitive` applies in both modes, the pattern is compiled
  once per call, and an invalid pattern fails before the walk.
- Matching stays line-oriented with the existing file-size, depth, result and
  line-length limits, skip list, `glob` filter and read permission.
- Adds `regex` 1.13 (MIT OR Apache-2.0) to `keel-runtime`; `cargo deny check
  licenses` passes and the audit table records it. ADR 0023 in Keel records
  the contract.
- Three new search tests; Keel 107 lib + 25 integration tool tests, clippy.
  Keelson 27 tests, desktop 25, clippy; `doctor` loads both profiles.

### Why

Literal search was the last coding component the surveyed agents all have and
Keelson lacked. A regex search is smaller and more certain than a custom
multi-file patch format: the model already knows regex, the wire already
carries booleans, and the existing traversal supplies the limits. Pattern
search no longer needs a shell confirmation or a platform grep.

## 2026-11-08 — M4f: undo a turn

- `desktop/src-tauri/src/checkpoints.rs`: a shadow git repository per project
  under `<state>/checkpoints/<workspace-key>/repo`. `send_message` snapshots
  the workspace before and after each turn and records the commit pair in
  `turns.json`; a snapshot failure is ignored and never blocks the turn.
- `undo_last_turn` diffs the two commits with `--no-renames` and, for each
  path, restores it from the before commit or deletes a file the turn created.
  Only the latest turn is offered; a repeat or missing record is refused.
- UI: the header shows `撤销本轮` when the latest turn used a mutating tool;
  a confirmation explains that the workspace is restored and the transcript
  is kept. The action hides until the next turn after a successful undo.
- No new Rust dependency; git is external and the feature is inert without
  it. ADR 0008 records the design.
- Desktop: 28 tests (three checkpoint tests), clippy; frontend 16 tests,
  `svelte-check` 0/0, build.

### Why

The trash restores one file at a time. A turn that touches several files has
no single undo, and a failed or unwanted turn costs a manual recovery. The
shadow repository gives the turn a unit without touching the user's git
history or requiring a new dependency, and the diff-and-checkout approach
handles creates, deletes and renames without a patch grammar.

## 2026-11-08 — M4g: multi-file suite validation

- Added `examples/inventory-workspace`: a small Python package with three
  modules and six unittest cases; three tests describe a missing `discount`
  feature and fail with `TypeError`.
- Added `tests/coding_eval.rs` (ignored by default): the model must add
  discount support across `service.py` and `report.py` without changing the
  tests, and the automatic Python check must pass.
- Live run: passed in 37.20 s with 11 model calls. Tools: `list_dir`, `glob`,
  `read_file` x5, `shell` x2, `todo` x7, `write_file` x2, `shell`,
  `read_file` x2, `todo`. The visible tests were byte-identical; both modules
  changed; the independent unittest run passed.
- Findings: the model used `glob` for discovery and whole-file `write_file`
  after reading, not `code_search` or `multi_edit`. A two-file change is too
  small to force those tools. The read ledger did not block the successful
  path, and the model did not tamper with the tests.
- No new tool is justified by this run; the next validation needs a larger
  repository or file to exercise regex search and multi-edit.

### Why

After adding several tools it is easy to assume they are used. This eval
measures the actual tool choice on a multi-file task and confirms the
end-to-end loop: discovery, reads, edits, shell checks and automatic
verification. It also shows which tools a small task does not need, which is
useful before adding more.

## 2026-11-08 — M4h: larger workspace and the parallel-result pairing bug

- Added `examples/shop-workspace`: twelve Python modules and 14 unittest
  cases; twelve tests describe a missing `discount` parameter across
  `pricing`, `reporting`, `checkout`, `api`, `wholesale`, `inventory` and
  `shipping`.
- Added `tests/shop_eval.rs` (ignored): the prompt asks for the feature, a
  regex `code_search` of the call sites and a `multi_edit` of the several
  changes in `pricing.py`, and forbids test edits. The workspace is kept
  under `target/eval/shop` after a failure for inspection.
- Live run: the first attempt reached `Done` but failed the automatic check
  after 243 s (diagnostics not retained); the second attempt passed in
  70.10 s with 13 model calls. Adoption: `code_search=true`,
  `multi_edit=true`, `edit_file=true`, `write_file=false`. The first search
  was `query=(total|subtotal)\( regex=true`; the two multi-edits made 4 and 2
  replacements. The automatic check and the independent unittest run passed;
  the tests were byte-identical.
- Found and fixed a host bug: `tool_lines` paired a `ToolFinished` event to
  the newest unfinished row with the same tool name, so two parallel
  `code_search` calls could swap args and outputs in the stored transcript.
  It now pairs by `ToolCallId`; a unit test feeds finishes in reverse order.
- Root suite: 28 tests + 3 ignored live evals, clippy.

### Why

M4g showed that a two-file task does not force regex search or multi-edit.
The larger shop task does, and it confirms both tools are usable. Inspecting
the stored trace also exposed a real transcript bug that a pass/fail
assertion would not catch: the tool names were right, but the evidence rows
were crossed. The evals are manual and non-deterministic; the first failure
and the second pass are both recorded.

## 2026-11-08 — M4i: show why verification failed

- `StoredTurn.verification` keeps a bounded `StoredVerification`: `passed`
  plus non-passing evidence only, at most eight items, each detail's tail
  capped at 4000 characters. `verification_passed` remains for old sessions.
- `TurnView` carries the field; `Turn.svelte` shows the existing badge and,
  when there is evidence, a `查看失败输出` disclosure with the item id,
  outcome and output. A passing turn stores no output.
- Root: 29 tests (+1 tail/filter test) + 3 ignored live evals, clippy.
  Frontend: 17 tests (+1 evidence reducer test), `svelte-check` 0/0, build.
  Desktop backend: 28 tests, clippy.
- The first M4h attempt failed verification and the UI could not say why;
  this closes that gap. Test failures live at the end of stderr, so the tail
  is kept instead of the head.

### Why

A failure badge without evidence turns a real test failure into an opaque
state. The model already receives the report; the user should see the same
failing output without opening the workspace or asking the model to repeat
it.

## 2026-11-08 — M4j: a real task in a copied GameVault core

- Ran the coding rig on GameVault's documented limitation: root-level save
  files (VX Ace `.rvdata2`, Tyrano `.sav`) are not detected. The copy is
  `D:/workspace-new/wwkkyy0325/keelson-task-gamevault` (core crate only); the
  real project was read, never written.
- Turn 1: 182.65 s; the model added `SaveLayout.save_file` plus root-level
  detection and four tests. The copy had 286 passing tests (282 baseline + 4),
  existing tests unchanged, directory behavior unchanged.
- Turn 2: 261.85 s; the model added `SaveProfile.save_file_relative`, the
  schema-v3 migration, repo/service/backup wiring and end-to-end tests. The
  host check ran 293 tests: 292 passed, 1 failed.
- The only failure was the model's own new Windows test: it expected
  `root.join("Game/Save01.rvdata2")`, mixing the separator, while the
  production path used `\`. A one-line human fix
  (`variant_dir.join("Save01.rvdata2")`) gives **293 passed, 0 failed**.
- All `shell` calls were denied by the eval's deferring interaction port, so
  the model could not run `cargo test` or see the assertion values; it ended
  `VerificationFailed` and did not claim a green run.
- Turn 3: the harness gained an `ApproveExecution` port (allow once, trusted
  copy), and the one-line test fix was reverted. The model ran `cargo test`,
  saw the Windows path mismatch, changed the test to
  `root.join("Game").join("Save01.rvdata2")`, ran targeted and full tests,
  fmt and clippy; **293 passed in 69.47 s**. The host verification also
  passed; no human edit was needed.
- Found and fixed a Keel transcript bug: denied calls emitted
  `args_preview: None`, so a denied `shell` row showed no command. The
  dispatcher now keeps the redacted preview; a dispatch test pins it.
- Task doc: `docs/llmwiki/tasks/m4j-real-task-gamevault.md`.

### Why

This is the first foreign-codebase task run in a copy. Across two turns the
rig added detection, a schema migration, persistence, file backup/restore and
tests in a real Rust project. It also showed that a green host check is not
product acceptance, that a platform-specific test assertion can be the only
failure, and that an unattended turn cannot self-verify without execution.

## 2026-11-08 — M4k: a real Python task, and pytest verification

- `workspace_check` selects `python -m pytest -q` when the project declares
  pytest (config files or a requirements file), instead of always running
  `unittest discover`. On pytest modules the old command exited 0 with zero
  tests, a false green.
- The Python check prefers a project `.venv`/`venv` interpreter.
- The persona asks for platform-neutral expected paths in tests.
- The real-task harness accepts `KEELSON_TASK_PYTHON`, prepends its directory
  to `PATH`, prints the resolved interpreter, and denies host-changing
  commands (package managers, process killers, installers). A test pins the
  guard.
- Ran a copied ValveGuard (1.3 MB, baseline 16 passed/6 skipped) on the
  unchecked `compare_demos.py` task. The first run passed 20/6 but spent
  1477.76 s: a `D:/...` PATH entry was dropped by Git Bash, so `python`
  resolved to a broken conda environment and the model installed user-site
  packages. The clean re-run with `KEELSON_TASK_PYTHON` resolved the project
  venv, made no host changes, and passed **23 passed (7 new), 6 skipped** in
  138.66 s.
- Task doc: `docs/llmwiki/tasks/m4k-real-task-valveguard.md`.

### Why

A test command that discovers zero tests must not count as a pass, and the
interpreter a project depends on is part of the rig, not an accident of PATH.
The first run also showed that blanket execution approval is unsafe on a real
machine; the harness now refuses commands that leave the copy.

## 2026-11-08 — M4l: coding rig polish

- `write_file` over an existing file now requires a full read. The read ledger
  records a coverage flag: a ranged or truncated read is partial, a later
  partial read does not clear an earlier full read, and an edit keeps the flag
  of the read it checked. The refusal says "read the whole file before
  overwriting it; a line range is not enough".
- `edit_file` and `multi_edit` keep accepting any current read; their exact
  match protects the untouched bytes.
- The coding persona forbids installing packages, changing the interpreter or
  running a package manager to make a check pass; a missing dependency is
  reported. It also says a `write_file` over an existing file needs a full
  read.
- Keel: 111 `keel-runtime` lib tests (+4), clippy. Keelson: 40 tests, clippy;
  the desktop backend still compiles and passes 28 tests. The live two-module
  discount eval still passed in 19.10 s with whole-file reads.
- Task doc: `docs/llmwiki/tasks/m4l-rig-polish.md`. ADR 0020 amended.

### Why

A one-line read was enough to authorize a whole-file overwrite, discarding
lines the model never saw; the ledger now distinguishes partial from full
reads. The real-task run also showed a model repairing a broken interpreter
with `pip --user`; the persona now tells it to report the missing dependency
instead of changing the host.

## 2026-11-08 — M4m: monorepo project checks

- `workspace_check` now finds a manifest at the workspace root, depth one or
  depth two. `Cargo.toml` and `go.mod` target their directory
  (`--manifest-path`, `go -C`), a `package.json` with a `scripts.test` uses
  `npm --prefix`, and a Python test file selects its own directory for
  `pytest` or `unittest discover -s`.
- `python_program` and `uses_pytest` inspect the tests directory and its
  ancestors, so a nested `backend/.venv` or `backend/pyproject.toml` is used
  before the workspace root.
- Root manifests still win, and the ecosystem order is still Cargo, Go,
  Python, Node; one check per turn.
- Keelson: 39 lib tests (+6), clippy. The exact nested commands were run in a
  temporary tree (cargo, go, npm, pytest) and all exited 0. The live
  two-module eval still passed in 41.37 s.
- Task doc: `docs/llmwiki/tasks/m4m-monorepo-checks.md`.

### Why

A repository whose backend lives in `backend/` and frontend in `frontend/` had
no automatic check at all unless a manifest happened to be at the root. The
same heuristic now points at the subproject without changing the one-check
policy or the driver.

## 2026-11-08 — M4n: every ecosystem gets a check

- `workspace_check` became `workspace_checks`: one required item per detected
  ecosystem, in Cargo, Go, Python, Node order. The engine passes the whole
  list to `with_acceptance_after_changes`, so a failing frontend check is
  visible even when the turn edited Rust.
- Each item keeps the M4m project targeting: a nested manifest, a project
  venv, and pytest when declared.
- Keelson: 41 lib tests (+2), clippy. One test builds a Cargo + Node
  workspace and runs the real `AcceptanceVerifier`; it reports two passing
  evidence items in 1.48 s.
- Task doc: `docs/llmwiki/tasks/m4n-multi-ecosystem-checks.md`.

### Why

M4m found a subproject but still stopped at the first ecosystem, so a
repository with a Rust core and a Node frontend never checked the frontend.
The driver already accepts a list; the host was constructing a single item.

## 2026-11-08 — M4o: checks selected by changed paths

- Keel `TurnDriver` gained `with_acceptance_selector`: the host supplies a
  closure over the paths a turn changed and returns the acceptance items that
  apply. The driver records `path` for the file tools and `from`/`to` for
  `move_file`; an empty selection finishes with a passing report.
- `multi_edit` was missing from `WORKSPACE_MUTATING_TOOLS`, so a turn that
  only used it skipped the after-changes check. Fixed; a unit test pins it.
- Keelson pairs each check with its project directory and selects the deepest
  project for each changed path, falling back to every check when a path
  belongs to none. `python_project` maps a tests directory to the nearest
  manifest or virtual environment.
- Keel: 115 lib tests (+4), full workspace suite, clippy. Keelson: 44 lib
  tests (+3), clippy. Desktop: 28.
- ADR 0024 in Keel; task doc `docs/llmwiki/tasks/m4o-check-selection.md`.

### Why

M4n made every check required, so a Rust-only turn in a Rust + Node repository
could fail on a pre-existing frontend problem. The driver already knew a turn
changed the workspace; recording which paths changed lets the host run the
checks that belong to them without a second diff.

## 2026-11-08 — M4p: every project of an ecosystem

- `workspace_checks` now walks every manifest directory per ecosystem: two
  crates, two packages or two Python projects each get their own check. The
  M4o selector picks the deepest project for each changed path.
- `find_python_tests` returns every test directory; one check is kept per
  project, on the shallowest test root.
- Keelson: 47 lib tests (+3), clippy. Live two-module eval passed in 43.33 s;
  desktop tests pass (28).
- Task doc: `docs/llmwiki/tasks/m4p-all-projects.md`.

### Why

M4o could only choose among the first project of each ecosystem, so a
monorepo with `api/` and `worker/` still checked only one of them. The
selector already knows the changed path; the candidate list now contains every
project.

## 2026-11-08 — M4q: every Python test root

- `workspace_checks` keeps one Python check per test root that is not nested
  inside another root. `api/tests` and `api/integration` both run; `tests/unit`
  folds into `tests/`.
- Both roots map to the same project, so a change under that project selects
  every root.
- Keelson: 49 lib tests (+2), clippy. Live two-module eval passed in 26.64 s;
  desktop tests pass (28).
- Task doc: `docs/llmwiki/tasks/m4q-python-test-roots.md`.

### Why

M4p kept one Python check per project, so a project with separate unit and
integration roots only ran the shallowest. The roots are now independent
checks under the same project, and the M4o selector runs all of them for a
change in that project.

## 2026-11-08 — M4r: discovery three directories deep

- `project_directories` and `find_python_tests` now descend to depth 3, so
  `services/api/backend/Cargo.toml` and `backend/src/tests/test_stats.py` are
  found. The build-directory skip list is unchanged.
- Keelson: 51 lib tests (+2), clippy. Live two-module eval passed in 24.16 s;
  desktop tests pass (28).
- Task doc: `docs/llmwiki/tasks/m4r-deeper-discovery.md`.

### Why

M4m stopped at depth 2, which covers `backend/` and `services/api/` but not a
service nested one level deeper. The walk is bounded and already skips build
directories, so one more level is cheap.

## 2026-11-08 — M4s: search context lines

- Keel `code_search` gained `context` (0-5, default 0). With context, a match
  prints up to N neighbors: `path:line: text` for the match, `path:line- text`
  for a neighbor, and `--` between blocks. Matches are ordered by path and
  numeric line; `context = 0` keeps the old output.
- The existing result, line-length and output-byte caps still apply.
- Keel: 115 lib tests, 28 tool tests (+3), clippy. Keelson: 51 lib tests,
  clippy; live two-module eval passed in 17.04 s; desktop tests pass (28).
- ADR 0025; task doc `docs/llmwiki/tasks/m4s-search-context.md`.

### Why

A hit line alone often lacks the signature or branch that makes it useful, so
the model issued a `read_file` per hit. Context turns one search into the read
the model actually needed, without lifting the output caps.

## 2026-11-08 — M4t: search output polish

- `render_context` merges overlapping or adjacent context ranges per file, so
  a line prints once; a line that is both context and a match keeps the match
  marker. Disjoint ranges and different files stay separated by `--`.
- A byte-cap cut now appends "output truncated at the byte limit" instead of
  the result-limit message.
- Keel: 115 lib tests, 30 tool tests (+2), clippy. Keelson: 51 lib tests, live
  two-module eval passed in 12.23 s; desktop tests pass (28).
- Task doc: `docs/llmwiki/tasks/m4t-search-output.md`.

### Why

M4s printed a block per match, so nearby matches repeated their context lines.
The byte cap also reused the result-limit message, which told the model the
wrong limit had been reached.

## 2026-11-08 — M4u: polyglot check selection, end to end

- `examples/polyglot-workspace` pairs a Node package with a passing test and a
  Python project with a deliberately failing test.
- `tests/polyglot_eval.rs` (ignored, needs a provider key) runs one live turn
  that fixes only `frontend/app.js`. It asserts one passing evidence item (the
  Node check), no Python failure, unchanged test files, and a green
  `npm --prefix frontend test`.
- **10.34 s**; the selector ran only the Node check and skipped the failing
  Python one.
- Task doc: `docs/llmwiki/tasks/m4u-polyglot-eval.md`.

### Why

M4m–M4t were covered piecewise: the selector, the project walker, the search
output. This fixture exercises the whole chain with a live model in a
polyglot workspace, where a selector mistake would turn a green frontend turn
into a red one.

## 2026-11-08 — M4v: shell mutation hint

- Keel's shell schema accepts `mutates` (boolean, default false). A successful
  shell call with `mutates: true` sets `workspace_changed` and reports no
  path, so the selector falls back to every check. The execution path ignores
  the flag.
- The coding persona tells the model to set it for formatters, generators,
  `git checkout` and scripts that edit files.
- Keel: 115 lib tests, 42 turn tests (+2), clippy. Keelson: 51 lib tests,
  polyglot eval 9.57 s, desktop 28.
- ADR 0026; task doc `docs/llmwiki/tasks/m4v-shell-mutation-hint.md`.

### Why

A shell-only mutation skipped verification because the driver treats shell as
invisible. Classifying programs was rejected as unreliable; an explicit hint
closes the common case without a second test run after every test command.

## 2026-11-08 — M4w: read_file line numbers

- Keel `read_file` accepts `line_numbers` (default false). Numbered output is
  `%5d | line`, absolute to the file; a ranged read keeps the absolute number.
  The ledger still records raw bytes, and the truncation marker is not
  numbered.
- The persona says the prefix is not part of the file and must not appear in
  edit text.
- Keel: 115 lib tests, 32 tool tests (+2), clippy. Keelson: 51 lib tests,
  polyglot eval 11.35 s, desktop 28.
- ADR 0027; task doc `docs/llmwiki/tasks/m4w-read-line-numbers.md`.

### Why

A raw read gives the model the bytes but not the location; a search hit gives
a line number in a different shape. An opt-in numbered read lets the model
name a location precisely without changing the default or the edit contract.

## 2026-11-08 — M4x: cross-file atomic edits

- Keel gained `apply_edits`: parallel `paths`, `old` and `new` lists, with the
  same path allowed to repeat. Every file is read once, the ledger is checked
  once per file, and every replacement is validated in memory before any
  backup or write. A missing, ambiguous, empty or identical replacement fails
  the whole call.
- Changed files are backed up and written one by one; a write failure restores
  the previously written files from their backups, best effort. The driver
  records every path for the check selector.
- The host registers the tool with the shared ledger and exposes it in both
  coding profiles; the persona points to it for changes that span files.
- Keel: 115 lib tests, 37 tool tests (+5), 42 turn tests, clippy. Keelson: 51
  lib tests, polyglot eval 13.87 s, desktop 28.
- ADR 0028; task doc `docs/llmwiki/tasks/m4x-cross-file-edits.md`.

### Why

`multi_edit` made one file atomic, but a rename or a small refactor often
spans files. Applying edits one file at a time can leave a partial rewrite
when a later snippet does not match. The cross-file tool validates the whole
set first, so the common failure touches nothing.

## 2026-11-08 — M4y: cross-file apply_edits, live

- `examples/cross-file-workspace` pairs a two-file rename: `math.js` exports
  `add`, `report.js` uses it, `test.js` asserts the result.
- `tests/cross_file_eval.rs` (ignored, needs a provider key) runs one live
  turn asking for the rename with `apply_edits`. It asserts both sources use
  `sum`, `test.js` is unchanged, the session used `apply_edits`, and
  `npm test` passes.
- **9.59 s**; the model applied the cross-file rename in one call.
- Task doc: `docs/llmwiki/tasks/m4y-cross-file-eval.md`.

### Why

M4x is a new tool; a kernel test proves its contract, but only a live turn
shows the model reaches for it on a real cross-file change and that the
resulting workspace still passes the project check.

## 2026-11-08 — M4z: named check ids

- `AcceptanceItem.id` is now `workspace-tests:<ecosystem>:<project>`, with
  `root` for a project at the workspace root. Python uses the test root, so
  two roots under one project stay distinct.
- The verdict, `VerificationFinished` event, UI evidence and the model's
  reprompt now name the failing check instead of repeating
  `workspace-tests`.
- Keelson: 51 lib tests, clippy. Polyglot eval 8.36 s; cross-file eval 8.87 s;
  desktop 28.
- Task doc: `docs/llmwiki/tasks/m4z-check-ids.md`.

### Why

M4n made every detected ecosystem a required check, but every item shared one
id. A failure with two checks listed `workspace-tests` twice, which told the
model and the user nothing about which project or ecosystem failed.

## 2026-11-08 — M4aa: ignore-aware search and glob

- Keel `code_search` and `glob` now walk with `ignore::WalkBuilder`, honouring
  `.gitignore`, `.ignore`, `.git/info/exclude` and global git excludes.
  `require_git(false)` makes the rules apply to a workspace copy without
  `.git`; `hidden(false)` keeps hidden files searchable.
- The walk is deterministic, depth-bounded, off the async thread, and keeps
  the existing build-directory filter, cancellation and result caps.
- Added the `ignore` dependency (MIT/Unlicense); `cargo deny check licenses`
  passes.
- Keel: 115 lib tests, 39 tool tests (+2), clippy. Keelson: 51 lib tests,
  polyglot eval 11.65 s, cross-file eval 8.86 s, desktop 28.
- ADR 0029; task doc `docs/llmwiki/tasks/m4aa-ignore-aware-search.md`.

### Why

The tools skipped a fixed list of build directories but walked everything
else, including a project's ignored `data/` or `generated/` trees. On a real
repository that costs time and returns noise; the project's own ignore rules
are the right filter.

## 2026-11-08 — M4ab: read-only git review

- Added host tool `git_diff`: `mode = status|diff` (default diff), optional
  `path`. It is read-only and needs no permission card.
- Runs `git -c color.ui=false --no-optional-locks -C <root> ...`; the diff
  uses `--no-ext-diff`; a 15 s timeout and 32 KiB cap bound it. A directory
  that is not a repository returns `not a git repository`.
- Registered in the host and exposed by both coding profiles; the persona asks
  the model to review the tree before finishing.
- Keelson: 55 lib tests (+4), clippy. Polyglot eval 11.36 s; cross-file eval
  12.39 s; desktop 28.
- Task doc: `docs/llmwiki/tasks/m4ab-git-review-tool.md`.

### Why

The shell tool runs git only after the user confirms. Reviewing the working
tree is a read operation the model should do before reporting done, and a
dedicated tool removes that friction without widening write access.

## 2026-11-08 — M4ac: coding rig audit

- Added `docs/llmwiki/coding-rig.md`: the rig boundary, tool contracts,
  verification rules, safety model, persona, test matrix, live record, known
  limits and next candidates. `README.md` links it.
- Full live run: `coding_eval` first attempt failed (13.03 s) and passed on a
  rerun (30.45 s); `shop_eval` 85.76 s; `polyglot_eval` 8.63 s;
  `cross_file_eval` 11.71 s; `keelson doctor` loads 2 rigs.
- Task doc: `docs/llmwiki/tasks/m4ac-rig-audit.md`.

### Why

The first rig is spread across a long series of task docs. A single map makes
the capability boundary and the remaining gaps explicit, and the live record
separates deterministic tests from model-run variance.

## 2026-11-09 — M4ad: a venv interpreter runs

- Kernel: `AcceptanceVerifier` accepts an absolute program path inside the
  workspace when the file name alone is allowlisted and the path has no `..`.
  A project venv's interpreter now runs; the shell allowlist is unchanged.
  ADR 0030 records it.
- The host already preferred `.venv/Scripts/python.exe`; before this every
  turn in such a project failed a required check with `not allowlisted`
  before any test ran. The copied repositories in earlier live runs had their
  venv left out, which is why the gap survived.
- Contract tests: an allowlisted workspace path runs, a path outside the
  workspace is refused, a path with an unlisted name is refused.
- A temporary project with a real `python -m venv` passed the check through
  the venv interpreter; the same scratch check fails on the stashed kernel.
- Task doc: `docs/llmwiki/tasks/m4ad-venv-check-path.md`.

### Why

The check must use the interpreter the project installed its dependencies
into, and a path to that interpreter is not a widening of what a check may
run: the workspace owns the file and its name still has to be allowlisted.

## 2026-11-09 — M4ae: declared workspace checks

- `.keelson/checks.toml` declares `[[check]]` entries (`name`, `program`,
  `args`). They are appended to the detected candidates and always run; the id
  is `workspace-check:<name>` and the program still has to be allowlisted.
- A file that does not parse, or a check with a missing, duplicate or invalid
  name or an empty program, becomes one required `workspace-checks-file` item
  whose reason quotes the error, so the turn is visibly not verified instead
  of silently unchecked.
- The file is read at engine open with the rest of check detection.
- Persona: `.keelson/checks.toml` is the user's declaration; do not edit it to
  make a check pass.
- Tests: candidate shape, always-runs selection, a passing and a failing
  declared check through `AcceptanceVerifier`, a broken file, and four invalid
  declarations. 60 lib tests (+5).
- Task doc: `docs/llmwiki/tasks/m4ae-declared-checks.md`.

### Why

The four detected ecosystems are the common case, not every case. A project
with Maven, .NET or a Makefile had no verified turn at all, so the rig could
report success without anything running. A file the user owns is the smallest
way to close that gap without inventing a command from a manifest.

## 2026-11-09 — M4af–M4ah: large files

- Kernel: a default `read_file` stops at 1000 lines and names the next offset;
  a ranged read opens with a display-only `[file: N lines; showing a-b]`
  header. The ledger records line ranges and counts coverage of every line as
  a full read, so a large file can be paged and then rewritten (ADR 0031).
- Kernel: every command or test check appends `[duration: 0.42 s]` to its
  evidence, on pass and on failure (ADR 0032).
- Host: `examples/messy-workspace` — a 1274-line Python module whose rules
  010-040 sit at the `def` column and five later methods are one space too
  far, plus a 570-line Java class with a missing and a stray brace. The Java
  check is declared in `.keelson/checks.toml` with English `javac`
  diagnostics.
- Host: `messy_eval` runs one turn to repair both files and asserts both
  checks pass, the reference test and `Main.java` are unchanged, and a direct
  `unittest` and `javac` run also pass.
- Deterministic: the fixture's detected ids are `workspace-tests:python:tests`
  and `workspace-check:java`; a manual verifier run showed both failing with
  their real output before a repair.
- 61 host tests, Keel 42 tool tests and 10 verification tests; clippy clean.
- Task docs: `m4af-read-paging.md`, `m4ag-check-duration.md`,
  `m4ah-messy-large-file.md`.

### Why

An edit can re-indent a block or drop a brace, and the rig has to repair a
file that no longer parses or compiles. Before this, a 1274-line file could not
be read in one call without filling the working set, and could never be
rewritten after paging. The fixture is the regression test for both, and the
duration line is what makes the cost of a check measurable at all.

## 2026-11-09 — live evals on a local endpoint

- `tests/common/mod.rs`: every live eval takes its model from
  `KEELSON_EVAL_BASE_URL`, `KEELSON_EVAL_MODEL`, `KEELSON_EVAL_API_KEY` and
  `KEELSON_EVAL_CONTEXT`, defaulting to the DeepSeek endpoint. Only
  `DEEPSEEK_API_KEY` is accepted for that default: an `ANTHROPIC_AUTH_TOKEN`
  is never sent to another provider.
- First run against a local `q2_0` server (256k window): `coding_eval` passed
  in 17.76 s over 12 model calls, `messy_eval` passed in 132.47 s with the
  1274-line Python module and 570-line Java class both repaired.
- The endpoint answered a 256,285-token needle prompt correctly.

## 2026-11-09 — a real task on a copied DocFusion

- `real_task_eval` ran one turn on a clone of the user's DocFusion (211 tests
  green, clippy clean) and found a contract violation: `docfusion show` with no
  document and `docfusion --bogus` exited 2, which `docfusion schema` publishes
  as "some inputs succeeded and some did not".
- The turn changed `main` to `Cli::try_parse` with an `argument_error` that
  exits 1 for a bad command line and 0 for `--help`/`--version`, added
  `crates/docfusion-cli/tests/argument_errors.rs` (4 cases, runs the binary),
  and wrote the project's changelog entry.
- Verified independently: 576 tests pass, clippy clean, `show`/`--bogus` exit
  1, `--help`/`--version` exit 0, a mixed `add` still exits 2, and
  `docfusion schema` is byte-identical to the original build.
- The turn took 617.79 s and read the clap crate source from the cargo registry
  to confirm `ErrorKind` variants before editing.
- Patch: `target/docfusion-exit-code.patch`. The original repository was not
  touched.

## 2026-11-09 — a feature task on a copied ValveGuard

- `real_task_eval` ran one turn on a clone of the user's ValveGuard. The repo is
  genuinely multi-ecosystem for the rig: it detected `workspace-tests:cargo:desktop`,
  `workspace-tests:cargo:desktop-ui/src-tauri` and
  `workspace-tests:python:tests`.
- The task was the repo's own open item (`docs/llmwiki/tasks/demo-artifact.md`):
  implement `compare_demos.py`, the human / synthetic / cheater harness.
- The turn took 137.20 s over 41 tool calls and passed acceptance. It added
  `src/valveguard/scripts/compare_demos.py` (197 lines) and
  `tests/test_compare_demos.py` (6 tests), and touched no existing module or
  test.
- Selection was correct on a real tree: a changed path under `src/` or `tests/`
  selects only the Python check, while `desktop/src/lib.rs` selects
  `workspace-tests:cargo:desktop`.
- Independent check: 15 passed / 6 skipped (the 9 existing tests still pass); a
  hand-built two-label tree plus an empty label produced the expected counts,
  rate and mean/median/p95/max, and `--csv` matched the table.
- Two bookkeeping slips: the task doc still says "(15 tests)" although the suite
  now runs 21, and the new changelog entry reuses 2026-10-21 rather than today.

## 2026-11-09 — closing the first rig for daily use

- Tool count measured rather than guessed: the coding profile exposed 16 tools;
  across three real runs (DocFusion, ValveGuard, the fixtures) 10 were used and
  none was chosen wrongly. `make_dir` was the one redundant entry — `write_file`
  already creates missing parent directories, which is how the DocFusion run
  created `crates/docfusion-cli/tests/` — so it is no longer visible in either
  coding profile. The kernel keeps it.
- Added "Writing a brief" to `coding-rig.md`: goal, where, constraints,
  acceptance, evidence, with the 617.79 s versus 137.20 s contrast as the
  reason.
- Persona: when the brief leaves a choice that changes the result, keep
  existing behavior, state the assumption in one sentence, and continue.

## 2026-11-09 — the rig asks when it cannot decide

- The capability was already there: every non-final model call carries the
  control tool (`clarify`, `choice`, `plan_approval`), the provider turns it
  into an `InteractionRequest`, the driver routes it to the host, and the
  desktop renders any interaction with options in `PermissionCard`. Only the
  persona never used it.
- Persona: ask once, before editing, with two or three concrete options, when
  the repository cannot answer a choice that changes the result — which file
  holds the behavior, whether a test may change, which reading is meant.
  Otherwise keep existing behavior, state the assumption, and continue.
- Desktop: `clarify` and `choice` cards are labelled (`需要你的选择` /
  `请选择一个` and their English pair) instead of printing the raw kind.
- New live eval `ask_eval`: a brief whose currency decision is not in the
  repository. The model asked one `choice` with three options and finished the
  turn with the answer (26.24 s); a second run asked whether the two existing
  render assertions may change.

## 2026-11-09 — the card marks the recommended option

- Kernel: `InteractionRequest::default` — a field no provider parsed — becomes
  `recommended: Option<String>`; both providers parse it and advertise it in
  the control tool schema. ADR 0033.
- Persona: when the rig asks, it names the option it would pick in
  `recommended`.
- Desktop: the matching option gets a `推荐` / `Recommended` badge and an accent
  border; a typo'd id simply marks nothing.
- `ask_eval` run: the model asked three options and set
  `recommended="configurable"` (34.44 s).

## 2026-11-09 — closing the desktop for daily use

- Settings: the manual-endpoint form says that a local or self-hosted endpoint
  with no authentication accepts any placeholder key, so pointing the app at a
  LAN model server is one save, not a guess.
- README: the desktop section names the manual endpoint path and the placeholder
  key.
- Verified: `npm test` 17, `svelte-check` 0 errors, `npm run build`, and the
  Tauri crate's 28 tests plus clippy.

## 2026-11-09 — a conversation is created by its first turn

- `new_session` wrote a document and a history row the moment 新对话 was
  pressed, so selecting a workspace and starting a chat left "无题" rows that
  could not be deleted without archiving first. It now opens a blank
  conversation with a fresh id and writes nothing.
- `sessions::list` no longer lists a document with no turns. The first turn
  writes the document and `touch` adds the row with the first message as its
  title, so the history matches what the user actually said.
- `sessions::create` is now a test fixture (`#[cfg(test)]`), and five session
  tests were updated to the new rule.
- Verified: 28 desktop tests, clippy clean; the dev app rebuilt and restarted.

## 2026-11-09 — the composer resizes from its top edge

- The text area used the browser's bottom-right handle, but the composer is
  anchored at the bottom of the window, so dragging down made it grow upward
  from an edge the user was not touching.
- The handle is now a grip on the composer's top edge: dragging up makes it
  taller, `ArrowUp`/`ArrowDown` resize it from the keyboard, the height is
  clamped to 64px–60% of the window, and it is remembered in `localStorage`.
- Verified: `svelte-check` 0 errors 0 warnings, `npm run build`, 17 tests.

## 2026-11-09 — the composer sizes itself

- The manual grip is gone. The text area grows with the draft and stops at 40%
  of the window, where it scrolls; sending clears the draft and the box shrinks
  back. No handle, no remembered height, nothing to drag.
- Removed the grip markup, its CSS, its `localStorage` key and the
  `app.resizeComposer` string.

## 2026-11-09 — switching providers keeps the key and the endpoint

- Keys now live in one keychain slot per provider (`api-key:<provider_id>`).
  Switching from the local server to a cloud provider no longer overwrites the
  other key. A key saved before the slots existed is moved into its slot on
  first read, and only the provider recorded when it was saved can claim it.
- `DesktopConfig` records `key_providers` (which providers have a key) and the
  last manual endpoint (`manual: { provider, baseUrl, model }`). The settings
  panel asks for a key only when the selected provider has none, and prefills
  the manual form from the remembered endpoint.
- `open_inner` no longer rejects a key that belongs to another provider: the
  lookup is per provider, so a mismatch cannot happen.
- Verified: 30 desktop tests (two new config tests), clippy clean, svelte-check
  0 errors, 17 frontend tests.

## 2026-11-09 — the manual form starts empty

- Opening 手动填写接口 used to copy the currently selected provider's values in
  as real text, so a first look at the form showed DeepSeek's URL and model as
  if they were the manual endpoint.
- The form now starts empty with examples as placeholders
  (`http://127.0.0.1:8080/v1`, `q2_0`), and fills only from a remembered manual
  endpoint. The same applies to the first-run wizard.

## 2026-11-09 — the model/list switch is a segmented control

- 手动填写接口 was a small text link below the form, so the way back to the
  model list was easy to miss and sat far from the thing it switched.
- Both surfaces now use the same two-state control as the language switch —
  `模型列表 | 手动填写` — placed directly under the section heading and above the
  picker or the manual form.
- Removed the dead `.settings-mode` style and four now-unused strings.

## 2026-11-09 — the rig is a choice, not a side effect

- The desktop picked the rig from the provider (`mock` → `coding`, everything
  else → `coding-live`), so a second rig file could never be run and the user
  could not see which one was running.
- `DesktopConfig.rig` records the chosen rig file; `setup_state` returns the
  discovered rigs and the one that will run; the settings panel gains a Rig
  section that lists them and saves + reconnects on change. An empty value
  keeps the old provider-derived rule, so existing installs are unchanged, and
  a rig file that disappeared falls back the same way.
- CLAUDE.md and the README are updated: the app chooses which rig file runs,
  never its behavior.
- Verified: 30 desktop tests, clippy and fmt clean, svelte-check 0 errors,
  17 frontend tests.

## 2026-11-09 — the flow starts from what the user sees

- An empty conversation now offers four starters above the composer (fix the
  tests, get it running, explain a file, add a feature). A click fills the
  draft and focuses it; nothing is sent automatically. A user who does not know
  what to ask only has to recognise something close.
- 撤销本轮 moved from the window header to the turn it would undo, next to the
  verification badge. The header keeps only the title, language and settings.
- Removed the dead `.undo` style.

## 2026-11-09 — copy and branch from a turn

- A turn now shows its actions on hover: 复制 on the question and on the
  answer, and 从这里迁出 on the answer. The action row keeps its space so the
  layout does not jump when it appears.
- 迁出 is a host command (`branch_session`): it copies the turns before the
  chosen one into a new conversation, leaves the source untouched, opens the
  branch, and the composer starts with that turn's input so it can be edited.
  The new row is named `<title>（分支）`.
- Deletion of turns is deliberately not offered yet: it has to rewrite the
  document, renumber `seq`, and reopen the engine, and it would not roll back
  the files the turn changed. The confirm flow for that is a separate change.
- Verified: 30 desktop tests, clippy, svelte-check 0 errors, 17 frontend tests.

## 2026-11-09 — turn actions sit under the message, as icons

- The action row moved from beside the text to under it, left-aligned under the
  answer and right-aligned under the question, which is where chat interfaces
  put them.
- 复制 and 从这里迁出 are 20px icon buttons with a hover tooltip (`data-tip`,
  drawn in CSS above the button) and an `aria-label`; a successful copy turns
  the icon green and its tooltip says 已复制 for a moment.

## 2026-11-09 — an empty branch is visible, and starters stay quiet

- Branching from the first turn creates a conversation with no turns yet. It
  was hidden by the zero-turn rule, so the app switched to a conversation that
  the sidebar did not show, and the starter chips appeared on top of the
  prefilled draft.
- A document with no turns but a name is now listed: the branch is visible from
  the moment it is created, while an abandoned untitled conversation stays
  hidden.
- The starter chips hide as soon as the composer has text, so a branch opens
  with just the turn's input and no suggestions.
- One new session test.

## 2026-11-09 — a turn's tool lines belong to that turn

- The kernel event log lived as long as the engine and was never cleared, so
  `persist` paired every tool event since the engine opened: the second turn
  in a session repeated the first turn's tool rows, and its stored event list
  kept every earlier event too. `run_turn` now clears the log at the start;
  sequence numbers stay monotonic across a clear.
- `each_turn_records_only_its_own_tool_calls` fails (2 rows instead of 1)
  without the clear.
- Verified: `cargo fmt --check`, clippy with `-D warnings`, 62 library tests.

## 2026-11-09 — the save button only lights up for a change

- Save was enabled on an untouched form, so switching a value back still
  offered 保存并重连 and the user could not tell whether anything was pending.
  The panel snapshots the persisted connection and rig when it opens; the
  button stays disabled until the kind, provider, address, model, rig or the
  key draft differs from that snapshot.
- The button now names the step it is on — 正在保存…, then 正在重连… — instead of
  looking inert while the workspace reopens.
- The snapshot is only updated after the reconnect succeeds. If the settings
  were written but the reconnect was refused (a turn is still running), Save
  stays enabled so the retry is one click instead of closing and reopening.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — the rig choice waits for Save

- The rig select saved and reconnected on `change`, so picking an option closed
  the settings panel before the user pressed Save. It is a pending field now,
  like the model fields: the footer Save writes the rig and the model, then
  reconnects once.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — rig names follow the interface language

- The rig selector showed `Coding · coding` and `Coding (live) · coding-live`
  in both languages, so the two shipped rigs looked identical and the handle
  was noise. Options now show only the title, and the Chinese interface uses
  the app's own names: 编码 and 编码（在线）, with their descriptions.
- The mapping is keyed by rig name and falls back to the rig file's own title
  and description, so a rig the user adds still shows what the file says, and
  the English interface is unchanged.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — history rows share the rail's right edge

- The conversation list reserved an 8px stable scrollbar gutter, so every row
  ended 8px short of the project button, 新对话 and the tabs above it. The
  gutter is reclaimed with a matching negative margin: rows and the components
  above now share the same right edge, and the scrollbar sits in the rail's
  right padding.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — the header keeps only the gear

- The 中/EN button is gone from the header. Language is a setting, not a
  per-session action; the settings panel and the wizard already carry it.
- The header is now the title, the archived badge and the gear. The
  `app.switchLanguage` string went with the button.
- README's header description updated to match.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — nothing informational sits above the composer

- The toast strip (archived, unarchived, deleted and undo confirmations) and
  the error banner above the composer are gone. The rail already shows those
  outcomes: the row moves between tabs, disappears, or the undo button goes.
- A failed action now renders at the end of the transcript as a quiet red
  line, so it belongs to the conversation instead of hovering over the input.
  It is cleared when the next turn starts.
- The reducer's `clear-error` action went with the dismiss button. The only
  element that can still sit above the composer is a pending permission or
  choice card, which the turn blocks on.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — the starter suggestions are gone

- The four chips above an empty composer were noise: they never matched the
  workspace, and after a branch they appeared on top of a prefilled draft. In
  use they added a decision instead of removing one, so they are removed
  together with their strings, styles and the `showStarters` prop.
- The composer is a single row again; an empty conversation is just an empty
  composer.

## 2026-11-09 — answers render markdown, tool calls read like a log

- Answers were shown as pre-wrapped text, so headings, tables, lists and inline
  code from the model arrived as literal markdown. `Markdown.svelte` parses
  with `marked` (GFM) and sanitizes with DOMPurify before `{@html}`; a link
  cannot navigate the webview away from the conversation.
- Styles cover headings, lists, tables, code blocks, quotes and rules. Long
  tokens wrap, so a table or a path cannot widen the column past the window.
- Tool calls now come before the answer, in the order the turn ran, and render
  as one monospace line each: status mark, tool name, argument preview and
  right-aligned duration. Clicking a line expands its arguments and output;
  durations switch to seconds above one second.
- 撤销本轮 moved from a text link in the turn foot into the answer's icon row,
  next to copy and branch, with the same hover tooltip. The foot now holds
  only the verification badge.
- A tool that finishes within the clock's millisecond reports `0`, which used
  to leave the duration column blank and look like missing data. It now reads
  `<1 ms`; an unfinished call still shows no time because there is none.
- The tool log has three visible levels: the turn rail, one row per call, and
  an indented payload rail for arguments and output, so an expanded call reads
  as belonging to its row rather than the turn.
- Two dependencies: `marked` and `dompurify`.
- Verified: svelte-check 0 errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — the turn shows what it changed, and undo follows the diff

- Undo used to be offered whenever the last turn called a mutating tool.
  `shell` was on that list, so a `cargo test` run lit the button up, and the
  user could not see what a click would restore.
- The latest turn now reads its checkpoint pair through a new `turn_changes`
  command (`git diff --numstat` plus `--name-status` between the turn's two
  commits) and shows `N 个文件变更 +A −R`, expandable into one row per file
  with its status, path and line counts.
- The undo icon sits on the latest turn and is disabled with a
  「本轮没有文件变更」 tooltip when the diff is empty or the turn was already
  undone; the tool-name heuristic is gone. Undo restores workspace files only.
- Tests: a checkpoint pair with an edit, a create and a delete reports M/A/D
  with the right line counts, and an undone turn reports nothing (32 desktop
  tests). Verified: clippy, fmt, svelte-check 0 errors/0 warnings, 17 frontend
  tests, build.

## 2026-11-09 — each answer carries its own cost line

- `TurnView` now carries the turn's summed token usage (input, output and
  cached input), persisted with the turn. Turns written before the field
  exist are backfilled from their `model_finished` events, so an existing
  conversation shows the same line without a migration or a new conversation.
- The action row under an answer keeps its icons hover-only and shows a stats
  line on the right: elapsed time, `↑` input tokens, `↓` output tokens and the
  cache share when some input came from cache. These are the numbers a user
  cares about; event counts and other developer metrics stay out.
- Verified: 62 library tests, desktop tests, clippy, fmt, svelte-check 0
  errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — the composer carries its own status line

- A status line under the composer shows what the user needs before sending:
  the effective permission level, one combined rig · provider · model chip
  that opens settings, and context usage (`45.4k / 64k · 71%`).
- The permission chip opens a two-option menu (`工作区自动` / `全自动`) that
  explains each level. Choosing one saves a desktop-config override and
  reopens the session so the engine picks it up; a running turn disables the
  switch. ADR 0034 records the one deliberate exception to "rigs are files".
- Context usage comes from the last turn's `context_pressure` event, with the
  model window in its tooltip; clicking it lists the pressure at the end of
  the recent turns.
- `SessionStatus` is a new command; the session reducer gained an `epoch`
  bumped on every open, so the status and change list reload after a
  reconnect that keeps the same conversation id.
- Verified: 64 library tests, 32 desktop tests, clippy, fmt, svelte-check 0
  errors/0 warnings, 17 frontend tests, build.

## 2026-11-09 — the working set can grow within a session

- Both coding profiles now set `max_budget_tokens = 262144` beside the 64k
  base, so the kernel's new context ladder can raise a session to 128k or
  256k when content genuinely does not fit. The live rig is bounded by the
  model window and the 8k output reserve.
- The composer status line reports the working set in force, not just the
  profile base; the popover lists the budget and the model window.
- Keel ADR 0034 records the design: escalation on Red pressure or an
  over-budget call, monotonic per engine, no user switch.

## 2026-11-09 — a recorded transcript replays through the real loop

- `tests/replay_eval.rs` takes a Claude Code JSONL transcript, keeps only the
  human prompts, clones the project at HEAD into a temp workspace, rewrites
  the transcript's absolute paths into that clone, and runs one engine turn
  per prompt against a live model.
- After every turn it reads the kernel's own `context_pressure`,
  `history_assembled` and `context_escalated` events, plus usage, tool calls,
  event volume and session document size, and appends a row to
  `turns.jsonl`. A per-turn timeout cancels a stuck turn; a wall-clock budget
  stops the loop; three consecutive model errors end the run.
- `profiles/replay.toml` is the eval profile: the coding tool set without
  `shell` and `delete_file`, so the model cannot reach outside the copy. No
  rig lists it.
- Verified: one-turn smoke run on q2_0 in 56 s (16 model calls, 27 tool
  calls, context 21,005 / 64,000, 91% cached input).
- The long run over prompts 105..185 is in progress.

## 2026-11-09 — kernel 0.1.1: tool errors say how to continue

- Keel 0.1.1 is consumed through the existing path dependencies;
  `Cargo.lock` moved from 0.1.0 to 0.1.1. `keelson doctor` reports
  `kernel 0.1.1  events 1  snapshot 1`.
- `todo close` is idempotent when the list already holds the requested
  outcome. The list closes itself once every item is terminal, so the
  explicit close that follows no longer reports a false failure. A
  conflicting close names the state, a rejected completed close names the
  open items, and an unknown tool argument names the accepted ones.
- The three fixes came from a live Sojourn turn: 47 tool calls, 4 failures —
  three model mistakes that fixed themselves, one false failure from the
  kernel. Keel ADR 0035 records the policy.
- Verified: Keel workspace clippy and tests; Keelson 5 conformance probes,
  64 library tests, clippy, fmt; `keelson doctor`.

## 2026-11-09 — the running turn streams tools and thinking

- The running view no longer waits for the turn row. The desktop polls the
  engine's per-turn event log every 120 ms and emits `tool-started` /
  `tool-finished` with the kernel's bounded, redacted previews, so each call
  appears as it happens.
- The engine's delta channel now carries `ModelDelta::Text` and
  `ModelDelta::Thinking`. Thinking is forwarded live (muted block, open while
  running) and kept in `StoredTurn.thinking`, bounded at 64 KiB, so the
  stored turn shows a collapsed `思考过程` section. Older sessions load it as
  empty.
- The post-turn checkpoint snapshot used to run before `turn-finished`: after
  generation the screen stayed on the raw concatenated stream (no tool rows,
  no formatting) until the git commit finished. The persisted turn is now
  emitted first; the snapshot and record follow, then `turn-changes` reloads
  the diff and undo state.
- The running view shows only what arrived: the `思考中…` placeholder and the
  blinking cursor are gone. Thinking and tool calls share one indented group,
  both collapsed by default.
- Text from a model call that goes on to use a tool was commentary; the
  reducer now moves it into a collapsed `过程` entry in that group, so the
  answer area only holds the call that finished the turn.
- ADR 0035 records the design. Verified: 65 library tests, 32 desktop
  backend tests, 20 frontend tests, clippy, fmt, svelte-check 0/0, build.

## 2026-11-09 — a model writes the rolling summary

- `Engine::open` now builds a second provider from the same settings and
  attaches the kernel's `ProviderSummarizer`. On every segment seal the model
  rewrites one rolling summary (budget = working set / 10, 256–4096 tokens,
  30 s timeout). Covered records fold out of the context; the transcripts stay
  in storage and lexical recall can still reach them.
- The summary call carries no tools and no thinking, and its usage is added to
  the turn that triggered the seal. Failure or timeout falls back to the
  kernel's deterministic memo and emits `summary_failed`.
- The mock provider keeps no summarizer, so the offline rig stays
  deterministic.
- A replay comparison on q2_0 (records only vs model summaries, same
  transcript) is running. ADR 0036 records the decision.
- Verified: 65 library tests, clippy, fmt.

## 2026-11-09 — kernel 0.2.0, summary cadence and `search_history`

- Keel 0.2.0 is consumed through the path dependencies (`Cargo.lock` 0.2.0;
  `keelson doctor` reports kernel 0.2.0).
- The rolling summary is rewritten at most every third seal, or immediately
  at Amber pressure. A deferred seal keeps its structured record in context,
  so a seal streak no longer pays a cache miss every turn.
- Covered segments render as a bounded index beside the summary (goal,
  result, files), so a topic the rewrite dropped stays discoverable and its
  paths stay exact.
- The summary call asks for a leading `SEGMENT_MEMO:` line; the kernel stores
  it on the segment record and recall returns it instead of a raw excerpt.
- New host tool `search_history` (ReadOnly, no permission request): a
  deterministic keyword search over the stored turns — ASCII words and CJK
  bigrams — returning up to eight hits with bounded excerpts and tool names.
  It is visible in every coding profile and listed in the replay profile.
  ADR 0037 records the decision.
- The diagnostic replay showed why: 8 seals in 16 turns, seal-turn cache
  share down to 13%, and the summary shrinking from 11.1k to 3.6k characters
  as older topics were dropped.
- Verified: Keel workspace tests, clippy, fmt; Keelson 68 library tests (three
  new for `search_history`), 5 conformance probes, clippy, fmt.

## 2026-11-09 — replay biases fixed

- `OpenRequest.acceptance` lets a host that measures the conversation skip
  the workspace's own post-turn checks. The desktop passes `true`; the replay
  passes `false`. In a fresh clone the checks' `cargo test` exceeded the 60 s
  tool timeout and turned most turns into Error, which buried the context and
  summary measurements under the clone's build cache.
- The replay answers interactions with `AutoAnswerPort` instead of deferring
  them. Deferred choices left `interaction N was deferred` open items, and the
  kernel never absorbs an open item into the summary; coverage was therefore
  underestimated. Auto-answer picks the recommended option, then the first
  option, then approves a permission/plan and answers freeform with
  "proceed".
- The harness now records per turn: summary rewrites, chars and covered
  segments; summary failures; record segments/tokens; summary tokens; recall
  segments/tokens. `summary.json`/`summary.md` carry the totals. The error
  count now includes `TurnState::Error`, not only a failed `run_turn` (the
  first long run reported 0 errors while 13 turns ended in Error).
- Verified: 68 library tests, 32 desktop backend tests, clippy, fmt; the
  clean replay is running separately on q2_0.

## 2026-11-09 — kernel 0.2.1: an active segment seals on text size

- Keel 0.2.1 adds `SEGMENT_TOKEN_CAP` (12,000 estimated text tokens): a
  segment seals on size even while the topic continues, beside the
  twelve-turn cap. The clean replay's active segment had stayed open across
  ten same-topic turns; that text now becomes a record and a summary
  candidate earlier.
- This bounds the history contribution of the active segment. A single turn
  with many tool calls can still raise the per-call context; tool-result
  elision is unchanged.
- `Cargo.lock` and `docs/kernel.md` move to 0.2.1; `keelson doctor` reports
  kernel 0.2.1. A 45-minute validation replay is running on q2_0.
- Verified: Keel workspace tests, clippy, fmt; Keelson 68 library tests,
  clippy, fmt; doctor.

## 2026-11-10 — 0.2.1 size-cap validation replay

- Ran prompts 105–129 (25 turns) for 45 minutes on q2_0: 24 Done, 0 Error,
  1 turn cancelled by the budget deadline. Summary rewrites 7, failures 0;
  recall 2 segments / 798 tokens; peak context 122.5k under a 229k ceiling;
  two escalations, the second at turn 119 (the clean 0.2.0 run reached its
  final step at turn 114).
- The cap fires on this workload: 19 of 21 sealed segments already held
  12,000+ estimated tokens at seal time (median 20,250, max 57,647), and 18
  of the 21 held a single turn. Every sealed segment cleared the compression
  gate (21 records, 6 memos), so per-turn sealing costs a record, not content.
- Against the 0.2.0 clean run (31 turns, 60 minutes), input per model call
  fell from 85.1k to 67.5k. The comparison is not a controlled A/B: the model
  diverged on identical prompts (turn 106 made 53 calls in the clean run and
  1 here), so only the mechanism evidence and the order-of-magnitude cost
  change are trustworthy.
- Peak context is still driven by the current turn's tool output, not by
  history. Tool-result elision remains the only lever for that peak, as
  ADR 0038 says.
- Verified: Keel workspace tests, clippy, fmt; Keelson tests, clippy, fmt;
  desktop `npm test`, `npm run check`, `npm run build`, backend tests and
  clippy; `keelson doctor` reports kernel 0.2.1 and both rigs.

## 2026-11-10 — a turn's tool budget was tried and rejected

- Keel 0.2.2 added a per-turn tool-result budget (tested at 12,000 and 24,000
  estimated tokens) that elided the oldest results before each model call.
  It lowered the peak context (122.5k to 83k) but the model re-scanned the
  workspace it had already read.
- Measured on prompts 105–118, q2_0, one hour, one server, against a
  no-elision control: control 14/14 turns in 816 s with 309 calls; 24k 11/14
  in 1,488 s; 12k 10/14 in 1,927 s. The 0.2.1 evening replay of the same
  window matches the control (14/14, 971 s).
- Reverted: the kernel stays at 0.2.1 and the experiment is recorded as a
  rejected ADR (Keel ADR 0039). The replay harness keeps an `el` column that
  counts the overflow elision path.
- Verified: Keel tests, clippy, fmt; Keelson tests; `doctor` reports 0.2.1.

## 2026-11-10 — kernel 0.2.2: task list lifecycle and workspace execution

- Keel 0.2.2 resets a closed task list at the start of the next user turn
  (ADR 0040). A session's second task had lost every `todo add` because the
  previous list had completed; every call failed with `todo list is closed`
  (`invalid_arguments`) in under a millisecond.
- Keel 0.2.2 lets `workspace_auto` execute a command inside a write root
  without a prompt (ADR 0041). Execution previously confirmed on every root,
  so the first `shell` call of each session interrupted the user and the
  approval escalated to `full_auto` for eight hours.
- The permission chip now applies immediately: `save_permission` switches the
  open engine's base level and drops its grants instead of reopening the
  workspace; the status line reads the effective level.
- `docs/kernel.md` pins 0.2.2; `design.md` states what each permission level
  covers.
- Verified: Keel tests, clippy, fmt; Keelson tests; `doctor` reports 0.2.2.
- The running turn now shows `验证中… N 项检查` once the kernel starts the
  acceptance plan (`verification_started`, Keel ADR 0042), so a written answer
  no longer looks stuck until the turn row and badge arrive.
- Background shell commands (`background: "true"`) now show as their own rows
  in the running turn with a live elapsed time, and the output path when they
  finish; the host forwards `background_started/result` to the view. The
  shell tool also accepts a per-call `timeout_ms` (Keel ADR 0043).
- Background rows now live at the end of the transcript and survive the turn
  that started them. The host polls a non-consuming `background_tasks` view
  (Keel ADR 0044) while a row is shown; a row disappears when the engine hands
  the result to the model at the next turn.

## 2026-11-10 — web search and fetch on the provider key

- Added `web_search` and `web_fetch` host tools (ADR 0038). `web_search` uses
  the provider's Search endpoint and is registered only when the provider has
  one; `web_fetch` always exists and falls back to a local download with SSRF
  guards. Both are read-only and gated by the rig's `tools.visible`.
- DeepSeek serves both at `{base}/v1/tools/{search,fetch}` on the chat key, so
  the coding rigs get them with no extra configuration. Verified against the
  live endpoints with the stored key: search returned 3 results, fetch
  returned the page Markdown.
- The claim that the models.dev catalog is the only non-model request is
  amended by ADR 0038.
- File-change summaries now belong to the turn that made them. The checkpoint
  record stores the changed files with the pair (records written before this
  are diffed on read), `load_turns` and the session view return them per
  turn, and the view no longer moves the latest diff onto a newer turn or
  drops it when a later turn changes nothing.
- `web_search` can use a self-hosted SearXNG instance when the model provider
  has no search endpoint: Settings carries an optional search URL (no key),
  the provider endpoint still wins, and the tool stays absent when neither is
  configured (ADR 0038 amended).

## 2026-11-10 — kernel 0.2.3: write_file keeps line endings

- Keel 0.2.3 makes `write_file` write in the existing file's line-ending
  style when the file contains a CRLF pair (ADR 0045), so a small rewrite of
  a CRLF file no longer shows as a whole-file change in the checkpoint card.
- `docs/kernel.md` pins 0.2.3.
- Verified: Keel tests, clippy and fmt; host tests; `doctor` reports 0.2.3.

## 2026-11-10 — a wide message and the latest-message follow

- A pasted URL has no spaces to break at; the user bubble now wraps anywhere,
  and the transcript hides horizontal overflow so a wide line cannot pan the
  whole conversation.
- The transcript released its follow only when more than 80 px from the
  bottom, which a wheel notch does not always reach, so the next event pulled
  the view back. An upward scroll now releases the follow whatever its size,
  and a wheel up releases it before the browser scrolls.

## 2026-11-10 — kernel 0.2.4: a stream chunk keeps a split character

- Keel 0.2.4 holds an incomplete UTF-8 sequence across network chunks in both
  model adapters; a live answer had stored three U+FFFD where one Chinese
  character was cut by a chunk boundary.
- `docs/kernel.md` pins 0.2.4.

## 2026-11-10 — kernel 0.2.5: an inferred todo close reopens

- Keel 0.2.5 lets an add or status change reopen a task list that closed
  itself when its last item became terminal; an explicit close stays
  terminal (ADR 0046). The live turn's `todo add` no longer fails after the
  model finishes its plan.
- `docs/kernel.md` pins 0.2.5.

## 2026-11-10 — web fetch timeout and error text

- Provider fetch allows 60 s; a local download keeps 30 s. A live fetch of a
  slow government page was aborted at 30.0 s and succeeded on the retry at
  28.4 s.
- Request errors carry the reqwest source chain, so the tool output names
  `operation timed out` instead of only `error sending request for url`.
- ADR 0038 amended.

## 2026-11-10 — the permission setting and the grant

- The status chip read the effective level, so an eight-hour full-auto grant
  made it say "full auto" while the persisted setting stayed workspace auto;
  after a restart the grant was gone and it looked like a lost setting. The
  chip now shows `engine.base_permission()`, and reads "full auto
  (temporary)" while a grant is in force (Keel ADR 0047).
- Choosing a level always persists it; choosing the same level now drops a
  live grant instead of being a no-op.
- `docs/kernel.md` pins 0.2.6.

## 2026-11-10 — stopping a turn and guarding a second send

- `web_search` and `web_fetch` drop their request when the turn is
  cancelled instead of waiting for the 30 s / 60 s timeout.
- The composer blocks a second Enter while the first send is in flight (the
  turn-started event arrives a moment later) and the cancel button reports
  "cancelling…" and disables until the turn ends.
- Keel 0.2.7 makes acceptance checks honour the cancel token, so cancel
  during "verifying" stops instead of waiting for the whole plan; a
  cancelled turn is still not written to the kernel history.
- `docs/kernel.md` pins 0.2.7.

## 2026-11-10 — an interrupted turn is part of the history

- Keel 0.2.8 records a cancelled turn's input and streamed partial answer in
  the model history, so "继续" after an interruption continues the
  interrupted question instead of the previous completed turn. The
  transcript shows the partial answer the turn had streamed.
- `docs/kernel.md` pins 0.2.8.

## 2026-11-10 — an output-limit cut and the send feedback

- Keel 0.2.9 reports a response cut by the output limit as such instead of a
  JSON parse error from a truncated tool call, and keeps the text streamed
  before the failure (ADR 0050).
- The live profile raises `max_output_tokens` from 8192 to 32768; the model
  allows far more, and a large edit no longer hits the cap inside its JSON.
- The "waiting for the model" line is removed; the composer already clears
  on the click. `docs/kernel.md` pins 0.2.9.
- Queued the session document growth work: `backlog.md` and
  `tasks/session-document-growth.md`.

## 2026-11-10 — the model list opens from cache

- `model_catalog(refresh: false)` no longer touches the network: it returns
  the cached list (marked stale when older than a day) or the bundled
  snapshot, so settings and the wizard draw the list immediately instead of
  waiting on models.dev.
- A stale list is revalidated behind the visible list, and the explicit
  refresh control still forces a fetch.

## 2026-11-10 — the offline mock is gone

- The `coding` rig (mock provider) is removed; the app's rig list now has one
  rig, `coding-live`. Settings and the wizard no longer offer an offline
  provider, and a saved offline config goes back through setup.
- The eval suite keeps the mock only as a test fixture
  (`tests/fixtures/coding-rig.toml` plus `profiles/coding.toml`), which no
  rig directory exposes.

## 2026-11-10 — streamed deltas, scrolling and the settings load

- Chat's change and status effects read the session store directly; a
  writable store re-runs such an effect on every value, so each streamed
  token invoked `turn_changes` and `session_status`. They now depend on
  derived primitives and run the loads untracked, so the backend is called
  only when the conversation, the checkpoint or the status really changes.
- The transcript follows the stream at most once per animation frame.
- Settings starts the catalog load independently of `setup_state`, and both
  calls time out after eight seconds with an error, so a restarting backend
  cannot leave the model and rig lists spinning forever.

## 2026-11-10 — a second rig, and a rig fixed per conversation

- `general` joins `coding-live`: a file-only rig (`rigs/general.toml` →
  `profiles/general-live.toml` → `prompts/general.md`) for writing, research
  and file work, without the code-only tools. No engine change.
- A conversation now runs the rig recorded in its document. Before this the
  setting decided the rig on every open, so changing it silently changed
  conversations already in progress; the document's field was written but
  never read (ADR 0039).
- The setting is the default rig for new conversations. The empty
  conversation's status line is a rig select; picking one saves the default
  and reopens the blank conversation under the same id, keeping the draft.
  After the first turn the segment is plain text and the rig never changes.
- The rig · provider · model segment no longer opens settings; the header
  gear remains the only entry. The permission chip stays interactive.
- The rig chooser is a chip with the same popover as the permission menu, not
  a native select: the native dropdown drew over its neighbours in the status
  line.
- Opening a conversation scrolls to the newest turn again. Replacing the
  transcript fires a scroll event that the follow tracker read as "the user
  scrolled back", so the view stayed at the top.
- Settings no longer has a Rig section. The default rig is now set by picking
  it in an empty conversation, the only place a rig is chosen.

## 2026-11-10 — research and office rigs, split by output contract

- The `general` rig is renamed `office`; its tools gain `code_search` and
  `multi_edit`. Its prompt now states the scenario rules that were missing:
  edit only the requested range, write deliverables to files, cite outside
  claims, ask once on ambiguity, use a todo list, and the kernel behaviour
  (context folding, result clipping, wrap-up).
- `research` is a new rig for cited answers. Every factual sentence carries a
  source id and the reply ends with a source ledger: title, URL, how it was
  read (page text, search snippet only, local file), the quotation, and the
  claim it supports. Unsourceable claims are listed as unverified. The prompt
  states the access boundary: no bypassing captchas, login walls, paywalls or
  rate limits.
- The split follows the output contract rather than the topic; a deck or
  video rig arrives only when the host can verify the artifact (ADR 0040).

## 2026-11-10 — scenario evals for office and research

- `tests/office_eval.rs` checks the document contract: a targeted edit leaves
  every other byte of the file alone, a summary is written to the file the
  user named, and an ambiguous "organise this" asks before acting.
- `tests/research_eval.rs` checks the evidence contract with local sources so
  no search backend is needed: a conflict is reported with both files and a
  quotation, and a licence the sources never mention is not invented.
- Both are `#[ignore]` live tests that assert on workspace artifacts. They
  skip when no endpoint is configured; each rig's pass rate is tracked on its
  own suite.

## 2026-11-10 — the tools URL follows the API origin

- `web_search` and `web_fetch` built their URL by appending `/v1/tools/...` to
  the chat base. The desktop's Anthropic-compatible provider uses
  `https://api.deepseek.com/anthropic`, so the request went to
  `/anthropic/v1/tools/...` instead of the documented `/v1/tools/...`. The
  base is now reduced to its origin before the tools path is added.

## 2026-11-10 — rig wiring checked without a provider

- `tests/rig_tools.rs` opens each rig with a model override that is never
  called and asserts the effective tool set: office has the document tools
  (`multi_edit`, `code_search`, file operations, web) and not the coding-only
  `git_diff`/`apply_edits`; research has search, content search and file
  writes, and neither file reorganisation nor the batch edit tools. Both
  personas are asserted to have loaded.
- The research profile no longer lists `list_trash`/`restore_file`, which
  only make sense beside `delete_file` that the rig does not expose.

## 2026-11-10 — the composer takes files

- A file dropped on the window or picked with the paperclip is copied into
  `<state>/attachments/<workspace-key>/<session-id>/` and the conversation
  folder is added as a read-only root. The message carries an
  `Attachments (read-only):` block of paths, so the transcript and the model
  agree after a restart and no session schema changed.
- Copies live outside the workspace: a dropped file never appears in the
  turn's checkpoint diff, and undo cannot delete it. Duplicate names number
  themselves; 50 MB per file and 10 files per message; a batch is validated
  before the first copy. Deleting a conversation removes the folder (ADR
  0041).
- Images are accepted as files but are not sent to the model: the kernel's
  message content is a string and both adapters send text blocks, so vision
  needs a kernel change and a vision model.

## 2026-11-10 — attachments copy on send, once

- Choosing a file no longer copies it. The composer records the source path
  and shows a chip; the copy happens when the message is sent, so browsing a
  drop or removing a chip has no side effect.
- The same source path is one chip. Sending an unchanged file again reuses
  the existing copy instead of numbering a duplicate; only a changed file
  with the same name becomes `notes (2).txt` (ADR 0041, amended).

## 2026-11-10 — folders as bounded attachment snapshots

- A dropped or picked folder is copied as a tree (symlinks and `.git`
  skipped) up to 200 files, 200 MB, 8 levels and 400 MB per batch. Past those
  bounds the copy is refused with the suggestion to open the folder as the
  workspace: attachments are read-only, so a directory meant to be reorganised
  belongs there.
- An unchanged folder reuses its snapshot; a changed one is numbered, the same
  rule as a file. The composer gains a folder picker beside the file picker.

## 2026-11-10 — the session document stops growing with every turn

- A new turn no longer stores the kernel event stream. It keeps a `context`
  point (the last `context_pressure`), and documents written before the field
  existed still read their usage and context points from `events`.
- Each stored tool output and the thinking text are capped at 8 000
  characters with a `(+X characters)` note. The live view keeps showing the
  full kernel preview while the turn runs; only the document is bounded.
- The send path reads a cached archive flag instead of parsing the
  conversation document on every message. The flag is set at open and kept in
  step by the archive command; the delete path still reads the document
  itself.
- The remaining growth is the kernel snapshot (318k of 819k in the measured
  conversation), which needs a kernel-side decision (task step 4).

## 2026-11-10 — the model can look at the screen

- Kernel 0.2.10 (Keel ADR 0051): messages and tool results carry images in
  memory; both adapters encode them; an image costs 1024 estimated tokens.
- Two host tools (ADR 0042): `list_windows` names visible windows, and
  `screen_capture` captures a window (id or title), a region or a monitor,
  scales it to 1568 px, saves the PNG under the project's `screens/<session>/`
  and hands the image to the model in the same turn. No OCR: pixels go to the
  model, and a model without vision leaves the user to describe the screen.
- The captures folder is the permission scope: `workspace_auto` confirms with
  the target named, the session grant silences later captures, and
  `full_auto` does not ask. The coding, office and research rigs list both
  tools.

## 2026-11-10 — look again, launch, and read controls

- `view_image` reopens a saved screenshot or an attached image by path and
  attaches it to the result; the read is a normal path permission.
- `launch_app` starts an application detached and returns its pid;
  `wait_for_window` polls window titles until the application's window
  exists. Starting an executable is an `Execute` permission on its resolved
  path.
- `list_controls` walks the UI Automation control view of a window (type,
  name, class, bounds, up to 200 elements and six levels) for labelled
  controls that pixels describe poorly.
- All four are listed by every rig. Click-and-type automation is explicitly
  out of scope (ADR 0043).

## 2026-11-10 — the desktop tools proven on a real window

- An end-to-end probe launches the on-screen Paint, finds its new window,
  captures it (image + saved PNG), lists its UI Automation controls and
  reopens the PNG with `view_image`. It found a real bug: a bare `notepad`
  resolved to Git's extensionless shell script on PATH, which Windows cannot
  start; a bare name now searches `.exe`, `.cmd`, `.bat`.
- `tests/desktop_eval.rs` is the live vision check: a generated red circle,
  `view_image`, and answers must name the colour and the shape and the turn
  must contain a real `view_image` call.

## 2026-11-10 — kernel 0.2.11: tool images follow the tool group

- A live turn failed with `400 Bad Request: An assistant message with
  'tool_calls' must be followed by tool messages responding to each
  'tool_call_id'` when a screenshot was captured in the same round as another
  call. The OpenAI adapter inserted the image message between tool results;
  it now emits every tool message first and one user image message after the
  group.

## 2026-11-11 — cleanup after a conversation, and a clearer capture prompt

- Deleting a conversation removes its screenshots with its attachments; the
  engine owns the screens folder layout and exposes it to the host.
- A capture confirmation names the window without calling it a command
  (kernel 0.2.12); the screen capture requests a write scoped to its folder.

## 2026-11-11 — the snapshot stops growing with every turn

- Measured a real 1.1 MB conversation: 423k of its 434k snapshot was the
  verbatim assistant transcript of one segment that never sealed (eleven
  entries of about 38k, mostly tool summaries). Splitting the host document
  would not shrink that field.
- Kernel 0.2.13 (ADR 0052): one entry is capped at 12 000 characters with a
  note; a segment a summary covers drops its transcript while keeping its
  record, memo and a bounded recall excerpt. Restore applies both rules, so
  an old document shrinks on its next save. Host tests still pass against the
  new kernel.

## 2026-11-11 — restore is idempotent

- Restoring every real conversation document twice exposed a defect in the
  kernel cap: an entry that already carried the note was capped again, and its
  count became the note's own length (about 60).
- Kernel 0.2.14 keeps a capped entry unchanged on the next restore (ADR 0052
  amendment). Real documents: 5,556,594 -> 4,159,190 bytes across seven
  conversations, records, turns and recall unchanged.

## 2026-11-11 — old documents lose their event streams

- A document written before the usage/context fields existed now folds each
  turn's numbers out of its events and drops the stream on the next save. A
  turn, a title or an archive all go through the same save path.
- Real documents: 28 streams across seven conversations absorbed;
  5,556,594 -> 4,268,489 bytes with usage, context, answers and snapshot
  unchanged. After one more turn (which also applies kernel 0.2.14's bounded
  history) the same seven documents total 2,883,218 bytes, 48.1% smaller.

## 2026-11-11 — the explainer rig

- A fourth rig, `explainer`, whose deliverable is a narrated film. The script,
  storyboard, style anchor, contact sheet and a 30-second preview are stop
  points; the full render waits for approval (ADR 0044).
- Duration is measured from the generated narration; subtitles are shifted
  onto the same timeline. Numbers have one source (`data/spec.json`), are
  derived by `data/build.py`, drawn from `data/model.json`, and recomputed by
  `data/check.py`, which QA runs.
- Two host tools: `render_scene` renders a project scene to a PNG sequence
  through a headless browser and a loopback frame sink; `probe_media` reports
  a file's duration, geometry and streams. The starter kit lives in
  `kits/explainer/`.
- A real pilot produced a 55-second CNN explainer; reviewing its frames
  caught a hand-typed result that disagreed with the feature map, a
  single-row sweep under "扫完整张图", and two subtitle/panel collisions.

## 2026-11-11 — evidence and material gates

- `claims.json` splits every claim into fact, interpretation and value.
  `data/check_claims.py` (run by QA) requires two independent publishers or
  one primary source with a quotation and access date for a fact, a named
  framework for an interpretation, an on-screen label for a value, and both
  sides for a conflict. A scene names the claims it uses.
- Pictures are programmatic scenes plus user-supplied `assets/`. The rig does
  not download images or footage, and every asset records its license.

## 2026-11-11 — the explainer proves itself on a real topic

- A live run on the argumentative topic "为什么很多人觉得整个社会都在'去责任化'" produced 14 claims (10 facts, 3 interpretations, 1 value), two conflicts carried in the open, five unevidenced sub-claims deleted, and a six-scene script and storyboard. The claim gate passed.
- The source check found one dead citation; a repair turn replaced it, and all 15 sources now resolve. The run also showed that a null `output` and a `format.rate` were not handled; both are fixed in the kit.
- The gate now also rejects a stitched quote, a revealed fact without a `source_card`, a duration typed into the storyboard, and a ledger/scene mismatch.

## 2026-11-11 — thinking-mode tool follow-ups work

- Kernel 0.2.15 keeps a signed Anthropic thinking block with the tool-call
  message and sends it first on the follow-up, including an empty signed
  block. `deepseek-flash` no longer rejects the second model call with 400.
- The live probe `thinking_probe` uses the rig's own model and passes; the
  explainer eval no longer needs the `deepseek-chat` workaround.

## 2026-11-11 — a malformed tool payload is retried

- Kernel 0.2.16 retries a model call once when the error is a malformed
  tool-argument payload (`InvalidResponse`), discarding the failed attempt's
  draft text. Provider errors and stream cuts keep their previous path.
- The live flash run no longer needs a special model for this class of
  failure; the dossier it wrote before the error is already gated.

## 2026-11-11 — a 3D scene over the same frame sink

- `kit/scene3d.html` renders Three.js (vendored, MIT) into an offscreen WebGL
  canvas and composites it under the 2D labels; `render_scene` gained a `page`
  argument and starts the headless browser with SwiftShader. A 12-second AC
  airflow pilot (particle field, camera drift, labels) passed QA.
- A 3D picture is labelled 示意 unless a real simulation produced it; the
  scene's flow and camera are analytic and seeded, so frames are repeatable.

## 2026-11-11 — 3D scene boundaries

- The explainer's 3D rules now require a fitted camera (title and subtitle
  bands kept clear), a closed model instead of wall clamping, and labels
  anchored to projected points with arrows taken from the local field.
- The AC airflow pilot was rebuilt to follow them: the stream function is
  tangent to every wall, return air leaves through the intake and respawns at
  the vent, temperature relaxes to a spatial field, and parcels draw as short
  streaks. The room now fits the safe band and the labels follow the flow.

## 2026-11-11 — the 3D airflow shot, finished

- The AC pilot was taken to its final look: curved three-segment trails faded
  with vertex alpha, a seeded temperature noise that breaks the cold/warm
  boundary into wisps, a decaying supply impulse so the cold jet leaves the
  vent, a squared and blurred bloom added before the labels, a five percent
  push-in, a detailed unit with a display and contact shadow, and a lifted
  floor. The bloom lives in `kit/scene3d.html`, so every 3D scene gets it.
- The alpha fade is a rule now: dimming a trail's colour painted black lines
  over the dark room. The prompt and the kit README say so.
- Verify: 355 frames in 38 s; the frame-to-frame difference has no spike
  (mean 3.45, max 3.73 grey levels); the kit's live 3D test passes; QA at
  12.00 s, 1080p, −18.04 LUFS, 16 MB.

## 2026-11-11 — mixed films render from the storyboard

- `render_film` renders a whole film in one call: it reads the storyboard and
  the measured timeline, chooses `kit/scene.html` or `kit/scene3d.html` from
  each scene's `asset_type`, and validates ids and durations. `render_scene`
  stays for one scene while iterating.
- The prompt and kit README now state the editorial split between the two
  registers: 2D carries the model, 3D carries space; alternate at topic
  boundaries; one 3D space per section; about a third of the runtime unless
  the topic is spatial.
- The kit ships `GLOSSARY.md`, an English-to-Chinese mapping of its terms.
- ADR 0056 records the decision. The live mixed test renders a 2D and a 3D
  scene through one call.

## 2026-11-11 — the first live mixed film passes acceptance

- Keelson ran the responsibility brief end to end in one live turn: 86.29 s,
  7 scenes (06 is `scene3d` at 13.88 s), 13 claims (9 fact, 2 interpretation,
  2 value), 8 reachable sources, 13 cues, -16.18 LUFS. The live film test
  passed; every source resolved.
- The first cut had tofu subtitles: the shell ran under WSL, and libass there
  does not index the mounted Windows fonts. `kit/assemble.py` now adds
  `fontsdir=/mnt/c/Windows/Fonts` under WSL and stops if no CJK font exists;
  the film was re-burned and the subtitles render.

## 2026-11-11 — a stuck retry, a frozen clock, a Stop button

- The kernel retry for a malformed tool call now carries a correction: the
  driver tells the model that its arguments were not valid JSON and asks for
  a complete call, instead of resending the same context (Keel ADR 0055).
  The parse error also names a bounded sample of what arrived, and the
  OpenAI-wire accumulator accepts cumulative argument resends.
- The background row no longer runs a local clock at all: it shows the
  engine's measured `duration_ms`, refreshed by the one-second poll. A local
  ticker competed with the poll (and was restarted on every sync, so it could
  starve), and the poll itself could stop after one transient error. The view
  now reconciles with the engine on every session open, so a task that
  outlived a switch or a reconnect reappears instead of waiting for an event
  that will never replay; a failed poll retries on the next tick.
- A new registry continues after the `bg-N.log` handles already on disk, so a
  restarted app no longer appends a new task to an older handle's log.
- A running background command has a Stop button. The engine exposes
  `stop_background`, the kernel kills the process tree and keeps the task
  registered so the model still receives its exit at the next safe point
  (Keel ADR 0056).
- A webview reload during a running turn stranded that turn: the engine
  refuses to open a session while it runs, and a boot failure sent an
  already-onboarded app to the setup wizard, where the running turn has no
  cancel control. An onboarded boot failure now shows a retry screen with
  "cancel the running turn and retry"; the wizard is reserved for a first
  run.
- A background-reconcile effect read `$session.epoch` directly. The store
  replaces its value on every dispatch, so the effect re-ran on every state
  change and its own `background-synced` dispatch fed it again: an async IPC
  loop that held the webview renderer at 100% CPU. The effect now depends on
  a `$derived` value, and the poll effect depends on a derived boolean, so
  neither re-runs unless the value it names changes. Measured on the live
  app: renderer CPU fell from ~100% to ~1.6%.
- A collapsed tool row no longer mounts its arguments and output. A long turn
  carries hundreds of rows; rendering every hidden payload put megabytes of
  text in the DOM and made the transcript sluggish to load and scroll. The
  row now renders its payload when it is opened.
- `render_scene` and `render_film` declared themselves read-only while asking
  for write access to the frame folder; `screen_capture` had the same pairing.
  The dispatcher rejects that combination before the permission gate, so even
  full auto saw a denial with no prompt. All three now declare `Mutating`,
  which matches the write they request.
- The live tool poller cloned the whole turn's event log every 120 ms. It now
  reads only records after the last sequence it emitted (`EventLog::since`),
  so a tool-heavy turn no longer pays for the events it has already shown.
- The turn usage line's tooltips now say the number is the sum over every
  model call (cache hits included); the cache share is clamped at 100%. The
  large input totals are the conversation resent per tool round, not a leak.
- The running turn now shows its own cost line: elapsed time, input/output
  tokens, model calls and cache share, updated as each model call finishes
  (`keelson:model-finished`). A finished turn's meta shows the model call
  count (`52x`) as well.
- `StoredTurn` records `model_calls`; documents written before the field
  existed recover the count from their `model_finished` events, the same way
  old usage totals are recovered.
- `render_scene`'s relative `out` is now resolved against the project root.
  It was used as a literal relative path, so frames landed next to the app
  binary (`desktop/src-tauri/out/...`) while the caller looked under the
  project; `view_image` then reported the frame missing.
- The scene render budget now comes from the frame count (one second per
  frame plus a minute, capped at 30 minutes) instead of ten times the scene's
  wall-clock length. A 14-second 30 fps scene is 413 frames and can take
  minutes in the software headless browser; the old 198-second budget cut it
  off at 225 frames. Timeout errors name the output directory.
- The render tools declare a 40-minute call timeout. The session's 300-second
  tool timeout killed a 413-frame render at 300 seconds even though the
  renderer's own budget allowed longer. The kernel's `Tool::timeout()` lets a
  tool raise its ceiling (Keel ADR 0057); the dispatcher takes the larger of
  the session limit and the declared one. `render_scene` and `render_film`
  use it, and the renderer's internal budget still fires first.
- The scene page now reports its own failure: the frame loop is wrapped in a
  try/catch that posts `/error?message=...`, and a frame POST is retried
  three times before giving up. A rejected fetch or a throwing `draw` used to
  stop the loop silently while the browser stayed alive, so the renderer
  waited out its whole budget.
- Frames travel as PNG blobs again (`canvas.toBlob`) instead of base64 data
  URLs; the sink accepts both, so an older kit copy still renders. That skips
  the base64 detour and its garbage on every frame.
- The render loop fails after 90 seconds without a new frame, naming the last
  frame it received, instead of waiting for the multi-minute budget.
- A tool call whose arguments are not valid JSON is now a failed tool row the
  model can correct, not a turn failure (Keel ADR 0058). A 303-second turn
  with 30 model calls died on an `edit_file` call that wrote raw code as the
  `new` value; the retry repeated it. The model now sees the parse error and
  the received sample as the tool result, and the turn continues.

## 2026-11-11 — a local model can use DeepSeek for the web tools

- A saved DeepSeek key now serves `web_search` and `web_fetch` even when the
  chat model is a local OpenAI-compatible server. The backend used to be
  derived from the chat base URL, so a local model got only the plain local
  `web_fetch`, and `web_search` was dropped from the visible set.
- `OpenRequest` carries `search_key`; the desktop loads it from the DeepSeek
  keychain slot when a session opens. The key stays in memory and never
  reaches a prompt, a log or a stored turn.
- The settings hint says the saved DeepSeek key takes precedence over the
  SearXNG URL.

### Why

The local Strata server has no search endpoint. A user who runs `q2_0`
locally still needs provider-quality search, and the DeepSeek key is already
in the keychain; the search backend should not depend on which model answers
the turn.

## 2026-11-11 — the explainer kit seeds itself

- The desktop seeds `kits/explainer` into an explainer project when a
  conversation opens. It writes only missing files, so an existing script,
  claims, scenes and data are untouched.
- The kit is embedded in the binary at build time (`build.rs` plus
  `include_bytes!`), so neither a dev run nor a packaged app needs a separate
  resource directory or a manual copy.
- The explainer prompt no longer tells the model to ask the user to copy the
  kit; a missing `kit/scene.html` is reported as an incomplete installation.

### Why

An empty project could not render: the model stopped before the script and
asked the user to copy the kit by hand. The host knows where the kit is and
can write it without touching anything the project already owns.

## 2026-11-12 — quiet checkpoints, quiet consoles, release rigs

- The shadow checkpoint repository excludes generated directories
  (`.keel/`, `out/`, `node_modules/`, `target/`, `dist/`, `build/`,
  virtualenvs and Python/Next caches). A render writes thousands of frames
  under `out/`; every snapshot and diff walked them, one turn reported 15,785
  changed files and the change list took a long time to load.
- Existing shadow indexes drop the newly excluded paths once; stored turn
  records are filtered on read, so old turns stop listing frames.
- A turn's changed-file list renders at most 200 rows and keeps the true
  count in the header.
- Windows child processes — shell, background, git, `ffprobe`, the headless
  browser and `taskkill` — are created with `CREATE_NO_WINDOW`. A release GUI
  build no longer flashes a black console for every tool call (kernel ADR
  0059).
- Release builds from the source tree now find the repository's `rigs/`; the
  search path was debug-only, so a release build reported
  `找不到 rig 'explainer'`.

### Why

The nine seconds of a render's output are cheap to regenerate and expensive
to checkpoint. The host should show the script and the scenes a turn changed,
not every frame it drew, and a GUI app should not open a console for the
commands it runs.

## 2026-11-12 — interaction cards can be dismissed

- A clarify, choice or plan card now has a `跳过这个问题` action. The host
  answers `Deferred`; the driver tells the model the item was deferred, keeps
  the turn running and asks for it in the final report.
- A pending interaction is removed when its turn stops waiting — cancel, the
  broker's deadline, or a dropped engine — and the window receives a resolved
  event. Before this the card outlived the turn and could not be closed.
- A turn start, finish or failure clears a stale card, and a reloaded window
  asks the backend for the pending interaction once and restores it.
- Answers and dismissals update the card immediately instead of waiting for
  the event round trip.

### Why

The turn is blocked on the answer, so a card without a dismiss path trapped
the user between answering and cancelling the whole turn. A reload lost the
only event that described the waiting question.

## 2026-11-12 — screen captures are bounded

- `screen_capture` keeps the newest twenty PNGs in one conversation and
  prunes older ones after each capture. The tool refuses a capture above
  6 MiB and scales the long side to 1568 px.
- Opening a session removes captures older than seven days from every
  conversation of the project; deleting a conversation still removes its
  folder.
- The folder is fixed by the host under the project state directory
  (`projects/<workspace-hash>/screens/<session-id>/`); the model cannot choose
  a path. Screenshots never enter the workspace or the checkpoint diff.

### Why

Captures are evidence for one decision, not a library. Keeping every image
from every long session fills the state directory, and screen content is
sensitive by nature.

## 2026-11-12 — a series project has one folder per episode

- `new_episode(slug, title)` creates `episodes/NNN-slug/`, seeds the starter
  kit and appends the episode to `series.json`. The explainer prompt and rig
  expose it, so an episode no longer starts by copying files.
- `archive_episode(episode, status)` copies the cut, subtitles, cover and
  contact sheet to `releases/<episode>/`, writes `meta.json`, deletes
  `out/frames` and marks the episode in `series.json`.
- The host skips seeding the kit at the root when `series.md` or
  `series.json` exists; a series keeps its kit inside each episode.
- The live project was migrated to the new layout: episode 1 is
  `episodes/001-school-unit/`, its release is `releases/001-school-unit/`,
  and 2.4 GB of frames, 186 MB of review images and four debug dumps were
  removed.

### Why

One episode's `script.md`, `storyboard.json`, `scenes/` and `out/` at the
project root cannot be reused for a second episode. A self-contained episode
folder is movable, archivable and safe to prune; the index and release folder
make a series readable without opening every episode.

## 2026-11-12 — the explainer kit has five style presets

- `kit/design.js` is now a style engine: it reads `window.STYLE` and exposes
  tokens for colour, fonts, background motif, kicker, panel radius and mascot.
  `D.alpha(color, opacity)` replaces literal `rgba` values in scenes.
- Five presets live in `kit/styles/`: `paper-editorial` (the original look),
  `warm-documentary`, `dark-technical`, `bright-explainer` and
  `swiss-minimal`. `kit/style.js` is the active one.
- `new_episode` reads `series.json`'s `style`, copies that preset into the
  episode's `kit/style.js` and records it on the episode. A conflicting style
  argument is refused; change `series.json` first to switch a series.
- The prompt requires token-only scene code (no colour or font literals), a
  style anchor before the storyboard is locked, and one style per film.
- `the_shipped_kit_renders_each_style` renders a representative frame in every
  preset; the mixed 2D/3D test still passes.

### Why

One fixed design system made every film look the same across unrelated
series. A token pack per series keeps a film coherent while letting a series
choose its own visual language; literal colours in scenes would silently pin
them to the old palette.

## 2026-11-12 — acceptance stays inside the project

- A profile's `[verification] mode = "none"` now disables workspace checks even
  though the desktop asks for acceptance. `coding-live` is `none`, so a
  cleanup or analysis turn no longer runs the workspace's test suites.
- An empty change set runs no checks. A root-level change selects only the
  root project, not every nested suite; on a broad `D:\dev` the old fallback
  ran `tau2-bench`, `anaconda3` and `llama` tests for seventeen minutes.
- Discovery skips `site-packages`, `.pytest_cache`, `.mypy_cache`,
  `.ruff_cache`, `.cache`, `__pypackages__`, `.next`, `.svelte-kit` and
  `coverage`.
- The kernel verifier canonicalizes the root and the program before the
  containment check, so a `.venv` interpreter with a `\?\` prefix is allowed.

### Why

The failed acceptance check triggered two reprompts, so the same final answer
was streamed three times. A conversational turn had no tests of its own to
run; the workspace scan turned a cleanup request into a seventeen-minute
unrelated test run.

## 2026-11-12 — a local model can declare its window; huge workspaces skip checkpoints

- The manual endpoint form has a context-window field, and `SetupState`
  carries it. A local server that reports 131072 tokens can be configured
  without the profile's larger default.
- Checkpoints are skipped when a workspace exceeds 50,000 files or its shadow
  index exceeds 8 MB. Snapshot, history and undo return without running
  `git add -A`; a broad `D:\dev` no longer blocks a turn before the model
  call.
- The coding prompt prefers `pwsh` (PowerShell 7) over `powershell` (5.1) on
  Windows; the shell tool executes the program it is given and does not
  choose a version.

### Why

A 128k local model was being sent the profile's one-million-token window, and
a turn on a dev root spent minutes hashing unrelated directories before the
first model call. Both are configuration and scope problems, not model
problems.
- The settings panel opens a custom endpoint on the manual tab instead of
  falling back to the catalog's default provider; the saved local endpoint is
  now visible when the panel reopens.
