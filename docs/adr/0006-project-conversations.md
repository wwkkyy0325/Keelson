# 0006 — Projects own conversations

Status: accepted

## Context

A workspace was the permission root, the session partition and the only
conversation at once: `<state>/sessions/<workspace-hash>/default.json`. A
project therefore had one endless timeline, and the app had no new chat, no
history and no way to switch between two conversations in the same directory.

The owner asked for project isolation and conversation history, and floated a
hash of the orchestration data as the conversation identity. That hash changes
with every turn, so it cannot be a primary key.

## Decision

- A workspace stays the permission root and the storage partition. Its
  directory under `<state>/sessions/<workspace-hash>/` holds one JSON document
  per conversation plus an `index.json` for listing.
- A conversation id is opaque and immutable: `s-<unix nanos in hex>`,
  generated once at creation. It is not derived from the turns, the title or
  the workspace. The workspace hash partitions directories; the conversation
  id identifies a row.
- `index.json` is a listing cache, not the source of truth. Every load
  reconciles it against the directory: a missing document is dropped, an
  unknown document is adopted, and duplicate rows collapse to the newest.
  Losing the index cannot hide a conversation.
- The previous `default.json` is adopted as the conversation with id
  `default`, so an upgrade keeps the existing history.
- An explicit "new chat" writes an empty document immediately, so it survives
  a restart. Opening a project that has no conversations only opens a blank
  view: the document and the row appear with the first turn, so merely
  visiting a folder does not leave an untitled conversation behind.
- The conversation document owns its display title and archive flag; the index
  mirrors both. The title is the first non-empty line of the first user
  message, cut to 48 characters, until a rename replaces it. An empty rename
  clears the custom title and falls back to that first message.
- The rail supports inline rename and archive/unarchive. Delete is offered
  only on archived rows, and the backend refuses to delete an active
  conversation. Deleting is permanent (there is no trash): archiving is the
  recoverable step before it. Confirmation is a centered dialog, not an inline
  row, so the list never reflows. Archived conversations stay in the project
  and stay openable **read-only** behind a `已归档 · N` tab: the composer is
  replaced by an unarchive notice, and `send_message` refuses an archived
  conversation. The active and archived tabs share one list area. Archiving
  the open conversation opens the newest active one, or a blank view when none
  remains. Opening a project without an explicit id chooses the newest
  **active** conversation.
- **Amended 2026-11-09.** The read-only archived view is gone: the composer
  stays, and `send_message`'s refusal is met by unarchiving first, so the first
  message brings the conversation back instead of explaining itself above the
  composer. Everything else in this ADR stands.
- Recent projects live in `desktop.json` (`projects`, newest first, at most
  eight). Opening a folder upserts it. The list is a shortcut, not a grant:
  the engine's reach still comes from the workspace that was opened.
- A running turn blocks switching, creating, archiving and deleting. Deleting
  the current conversation first creates a new one, so the engine never keeps
  writing to a deleted document.
- The chat shell gains a left rail: brand, project switcher, `新对话` and the
  current project's conversations. The header shows the conversation title.

## Consequences

- Switching projects swaps both the permission root and the visible history.
  Two projects can use the same conversation id without colliding because the
  directory differs, though generated ids are already unique.
- Listing a project reads one index file and stats the directory; it does not
  parse every conversation. A corrupted index is rebuilt from the documents.
- Searching across projects, pinning and exporting remain open; none of them
  needs a different identity or storage layout.
