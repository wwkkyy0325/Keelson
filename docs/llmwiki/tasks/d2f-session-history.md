# D2f — projects, conversations and history

Goal: a workspace holds many conversations. The app lists them, creates a new
one, switches between them and deletes one, and remembers recent projects.

## Decisions

- A workspace stays the permission root and the storage partition. Its
  directory under `<state>/sessions/<workspace-hash>/` now holds one JSON
  document per conversation plus an `index.json` for listing.
- A conversation id is opaque and immutable: `s-<unix nanos in hex>`, generated
  once when the conversation is created. It is not a hash of the turns; a
  content hash changes with every turn and cannot be a primary key.
- The workspace hash still partitions directories, so two projects can never
  share a document. The existing `default.json` is adopted as the session with
  id `default`; no conversation is lost in the migration.
- `index.json` is a listing cache, not the source of truth. It is reconciled
  against the directory on every load: a missing document is dropped, an
  unknown document is adopted, and duplicate rows collapse to the newest. A
  lost, stale or duplicated index therefore cannot hide a conversation.
- An explicit "new chat" writes an empty document immediately, so it survives
  a restart. Opening a project with no conversations only opens a blank view;
  the document and row appear with the first turn, so visiting a folder does
  not leave an untitled conversation behind.
- The title is the first non-empty line of the first user message, cut to 48
  characters. The index stores it; the UI shows `未命名对话` until it exists.
- Recent projects live in `desktop.json` (`projects`, newest first, at most
  eight). Opening a folder upserts it. This is a list of shortcuts, not a
  permission grant: the workspace picker still decides what the engine may
  reach.
- The chat shell gains a left rail: brand, project switcher, `新对话`, and the
  conversation list for the current project. The header shows the current
  conversation title instead of the workspace.
- A running turn blocks switching, creating and deleting. Deleting the current
  conversation creates a new one first, so the engine never keeps writing to a
  deleted document.

## Steps

- [x] Session layout, index, migration and titles in the desktop backend.
      Verify: unit tests for migration, create, touch, delete and reconcile;
      `cargo test` in `desktop/src-tauri`.
- [x] Commands: open by id, create, list, delete; project recents; a
      `session-updated` event after each turn.
      Verify: `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- [x] Frontend protocol, API and reducer for the conversation list.
      Verify: `npm test`, `npm run check`.
- [x] Left rail: project menu, new chat, conversation rows and inline delete.
      Verify: `npm run build`, then create two conversations, switch, delete and
      reopen the app.
- [x] Design, changelog and README updates.

## Verification

- Desktop backend: 21 unit tests, `cargo clippy --all-targets -D warnings`.
  Root: `cargo test` (16 unit, 1 boundary, 5 kernel probes) and clippy.
  Frontend: 14 tests, `svelte-check` 0/0, Vite build.
- An isolated instance with a temporary `KEELSON_HOME` and `provider = mock`
  (no key, no real state):
  - two conversations were created, each titled from its first message and
    listed newest first;
  - switching restored the transcript and the header title;
  - stopping and relaunching the process kept both conversations and selected
    the newest;
  - deleting the current conversation created a new empty one first, removed
    the old document and kept the other;
  - a second folder in the project menu showed an empty history, and switching
    back restored the first project's rows.
- The first boot after the change hit an index written by an intermediate
  build that contained the same row twice; the keyed list threw
  `each_key_duplicate` and the boot stayed on the loading screen. WebView2 CDP
  identified it; `reconcile` now collapses duplicate rows to the newest and
  rewrites the index. The real state now lists one row.
- The real state was used only for new-chat, switch and delete of an empty
  conversation; it was restored to its one empty session. No live model turn
  was sent and the keychain was not written.
- The app window and the isolated window were driven through UIA for
  screenshots and through the WebView2 debugging protocol for clicks and DOM
  assertions.
- A second isolated run with a fresh mock home checked the follow-up:
  - opening the workspace listed no conversations and wrote no document; the
    header said `未命名对话` and the empty transcript said
    `在「workspace」中开始新对话。`;
  - the first mock turn created the document, the row and the title;
  - `新对话` wrote an explicit empty row, and switching to it reset the
    composer draft;
  - deleting the open conversation showed `已删除，已新建对话`, removed the
    old row and opened a new empty one;
  - a process restart kept both conversations.
- The duplicate-row regression test still passes; a new test covers the first
  turn creating the row for a conversation the index did not know about.

## Not in D2f

- Renaming a conversation by hand.
- Searching conversations across projects.
- Moving or exporting a conversation.
- A conversation title model call; the first message is the title.
