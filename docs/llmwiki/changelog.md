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
