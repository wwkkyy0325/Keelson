# Session document growth

Goal: a conversation document stays small enough that an open, send and save
do not grow with history, without losing what the transcript and the status
line show.

Baseline (2026-11-10, a live 8-turn conversation, 1.5 MB on disk / 819k
characters):

| Part | Characters |
|---|---|
| kernel snapshot | 318k |
| tool outputs | 192k |
| per-turn events | 142k |
| thinking | 134k |
| answers | 10k |
| tool arguments | 9k |

A load (parse) measured 21 ms in a debug build; every send did one
(`is_archived`) and every turn rewrites the document.

## 1. Store metrics instead of events (done)

- [x] Replace `StoredTurn.events` with the values the host reads: usage is
      already a field; add the turn's last `context_pressure` point.
- [x] Keep reading `events` for documents that have them (usage backfill and
      context points), never write new ones.
- Verify: `a_stored_context_point_wins_over_the_events`,
  `an_old_turn_recovers_its_context_point_from_the_events`,
  `a_new_turn_omits_the_event_stream`; the engine tests assert a persisted
  turn writes no events.

## 2. Cap what is kept per turn (done)

- [x] Store at most 8 000 characters of each tool output and of the thinking
      text, with a `(+X characters)` note. The live view still shows
      everything while the turn runs.
- Verify: `a_megabyte_tool_output_is_stored_capped`; the cap is applied in
  `persist`, after the live events have already carried the full preview.

## 3. Do not parse the document on send (done)

- [x] Cache the archive flag in `CurrentSession` (set at open, updated by the
      archive command) and use it in `send_message` instead of
      `sessions::is_archived`.
- Verify: `the_open_conversation_reports_its_archived_flag`; `send_message`
  reads `current.archived` and calls no `Store::load`; the delete path still
  reads the document, because it runs after the conversation closes.

## 4. Decide on the snapshot (kernel)

- [ ] The 318k kernel snapshot is owned by `keel-core`; decide whether to
      trim segment entries, or split the host document into an index and
      per-turn blobs. Either needs an ADR before code.
