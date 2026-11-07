# D2g — history operations

Goal: a conversation row can be renamed, archived, unarchived and deleted;
archived conversations are still reachable from the rail.

## Decisions

- The conversation document owns the user metadata: `title` and `archived`.
  The index mirrors them as a listing cache. A rebuilt index therefore cannot
  lose a rename or bring an archived conversation back into the active list.
- The title is capped at 48 characters. An empty rename clears the custom
  title; the next listing falls back to the first user message.
- Archiving is a flag, not a move: the JSON document stays in the project and
  remains openable. The rail splits active and archived rows into two tabs.
- Opening a project without an explicit id chooses the newest **active**
  conversation; if every conversation is archived, it opens a blank view
  instead of resurrecting one.
- Archiving the open conversation switches to the newest active one, or a
  blank view when none remains. The backend refuses archiving while a turn is
  running.
- Renaming is inline in the row. The row actions appear on hover: rename,
  archive/unarchive, and delete only on archived rows. Archived rows are dimmed
  and live behind a `已归档 · N` tab, not a stacked section: the two tabs
  replace the same list area, so switching never pushes the active list around.
- Opening an archived conversation renders its transcript **read-only**. The
  composer is replaced by an `已归档的对话不能继续` notice with an unarchive
  action, and `send_message` refuses the turn until the conversation is
  unarchived. The header shows an `已归档` badge.
- Deleting is permanent and requires archiving first. Active rows have no
  delete action, and the backend refuses to delete a non-archived document.
  Confirmation is a centered modal with the conversation title; cancel is
  focused, Escape cancels. The row height does not change while it is open.
- The engine exposes metadata setters for the open conversation, so a turn
  that persists afterwards cannot overwrite a rename or an archive with the
  values loaded at open time.

## Steps

- [x] `StoredSession.title` / `.archived`, the sessions API and tests.
      Verify: rename, archive, rebuild-the-index and unarchive unit tests.
- [x] Engine setters that survive a following turn.
      Verify: a turn after `set_title` / `set_archived` keeps both values.
- [x] Commands `rename_session` / `archive_session`, active-list selection and
      the `session-updated` event.
      Verify: `cargo test`, `cargo clippy --all-targets -D warnings`.
- [x] `SessionRow` component, inline rename, archived section and Chat
      handlers.
      Verify: `npm test`, `npm run check`, `npm run build`, then a UI
      walkthrough with the mock provider.
- [x] ADR/design/changelog/README updates and verification.

## Verification

- Desktop backend: 25 unit tests including rename, clear-title, archive,
  rebuild-without-index and the archive flag; clippy passes. Root: 17 unit
  tests, boundary and kernel probes; clippy passes.
- Frontend: 15 tests, `svelte-check` 0/0, Vite build.
- Isolated mock instance (`KEELSON_HOME` in a temp directory, no key):
  - two conversations, then a rename of the open one: row and header changed;
  - active rows exposed two actions (rename, archive); archived rows exposed
    three (rename, unarchive, permanent delete);
  - `delete_session` called directly on an active id returned
    `请先归档再删除`; the UI has no delete button for active rows;
  - archiving the other row moved it to the `已归档 · 1` tab and left one row
    on the `对话` tab;
  - the archived tab showed the row with three actions; opening it kept the
    archived tab selected and rendered the transcript read-only: no composer,
    an `已归档` header badge and the unarchive notice. A direct
    `send_message` returned `已归档的对话不能继续；请先取消归档`;
  - the notice's unarchive action restored the composer, switched back to the
    `对话` tab and a following mock turn succeeded;
  - the delete action opened a centered `永久删除对话?` dialog naming the
    conversation. The row measured 32.625 px before and after opening it, so
    the list did not move; cancel was focused and Escape closed it;
  - confirming the dialog removed the archived row and showed
    `已永久删除对话`; the archived tab then showed `还没有归档的对话`;
  - archiving the open row stayed on the `对话` tab, showed a blank view and
    incremented the archived count;
  - archiving the last active row left both rows under `已归档 · 2` with no
    active rows;
  - a process restart kept both archived flags and titles and still opened a
    blank view;
  - after restart, unarchiving a row made it active and opening it showed its
    transcript;
  - deleting the **open** archived row created a new active conversation first,
    removed the archived document and showed `已永久删除，已新建对话`.
- Layout measurements with 25 conversations in the isolated app:
  - `.session-list` client width stayed 207 px with overflow, on the empty
    archived tab, and after archiving the last active row; the stable
    scrollbar gutter removed the width change;
  - the archived tab label stayed at x=145.9 px and its count at 11.04 px
    while the count moved from 24 to 25;
  - hovering a row kept its 32.625 px height and 160.55 px title width; the
    actions were a 48.6 px overlay rather than in-flow buttons;
  - the footer measured 130.2 px with the composer and 131.8 px with the
    archived notice (1.6 px) while the same toast was visible.
- The real keychain and real project were not touched by the walkthrough; the
  app was relaunched against the real state afterwards.

## Not in D2g

- A trash/recycle window. The owner chose archive-first permanent deletion
  instead: archive is the recoverable step, delete is final.
- Search across conversations, pinning, multi-select or bulk archive.
- Moving a conversation to another project.
- Editing the first user message or regenerating a title with a model call.
