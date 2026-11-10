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

A load (parse) measured 21 ms in a debug build; every send does one
(`is_archived`) and every turn rewrites the document.

## 1. Store metrics instead of events

- [ ] Replace `StoredTurn.events` with the values the host reads: usage is
      already a field; add the turn's last `context_pressure` point.
- [ ] Keep reading `events` for documents that have them (usage backfill and
      context points), never write new ones.
- Verify: an old document still shows usage and context points; a new turn
  writes no events; the document shrinks ~17%.

## 2. Cap what is kept per turn

- [ ] Store at most N characters of each tool output and of the thinking
      text (for example 8k); append a `(+X characters)` note. The live view
      still shows everything while the turn runs.
- Verify: a turn whose `read_file` returns 1 MB stores the cap and the note;
  the transcript shows the note; the audit table is unchanged otherwise.

## 3. Do not parse the document on send

- [ ] Cache the archive flag in `CurrentSession` (set at open, updated by
      the archive command) and use it in `send_message` instead of
      `sessions::is_archived`.
- Verify: the send path performs no `Store::load`; an archived conversation
  still refuses a send.

## 4. Decide on the snapshot (kernel)

- [ ] The 318k kernel snapshot is owned by `keel-core`; decide whether to
      trim segment entries, or split the host document into an index and
      per-turn blobs. Either needs an ADR before code.
