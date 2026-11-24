# 0083 — A restart keeps the answer the turn had already shown

## Context

ADR 0080 keeps an in-flight turn while the window looks at another conversation,
and ADR 0081 keeps the question across a restart. Neither keeps the answer. The
text that streams lives in two places only: the deltas the host relays into the
webview, and the kernel's own buffer for the call it is making. The engine writes
a turn when it ends, so a process that dies mid-answer leaves a document with no
turn in it and a note with nothing but the question.

That is not a corner case. On 2026-11-24 a restart was requested while a turn was
answering; the note on disk read `{"seq":73,"input":"重启了，继续","startedAt":…}`
and several thousand words that had already been on screen were gone with the
process. ADR 0081's card offers to ask again, which is right, but it looked as if
the model had never said anything.

Two shapes were on the table: the host keeps a bounded copy of the deltas it
already relays and hands it back through `SessionView.running`, or the kernel
exposes the buffer it holds while a call runs. The first is host-only and bounded
by construction; the second is a kernel change with a contract test and no gain
over the first for what the reader needs.

## Decision

Keep a bounded head of the answer, in the host, where the turn is already kept.

1. **`RunningTurn` gains `partial: String`.** The host's record of a running turn
   is one struct; the answer so far belongs beside the question, the sequence and
   the background registry it already holds.
2. **The bound is `PARTIAL_BYTES` = 8 000, and the head is kept, not the tail.**
   What the reader has already read is the beginning; a tail would open a card in
   the middle of a sentence whose subject was never shown. The number matches the
   document's cap on a tool output, and it is what keeps rewriting the note cheap
   while an answer streams for minutes.
3. **A bound in bytes may not cut a character.** `push_partial` walks back to a
   UTF-8 boundary before appending. The note is JSON and a lone continuation byte
   would make it unreadable, which would cost the question too.
4. **The copy is taken before the delta is emitted.** A window looking at another
   conversation drops the event (ADR 0077); the note must not follow the window.
5. **The note carries it.** `PendingTurn` gains `partial`, `#[serde(default)]` so a
   note written before the field existed still reads. `sessions::note_partial` is
   a read-modify-write on `<id>.pending.json` that takes effect only when the note
   names the same `seq` (a note for another turn is not ours to fill) and only
   when the text actually changed. The file is replaced through a temporary, as
   the question itself is.
6. **Writing is throttled, not queued.** `PartialNote` writes at most once per
   `PARTIAL_WRITE` = 500 ms and not at all while the answer has not grown. Deltas
   arrive per frame; a rewrite per frame would cost more than the turn it records.
   The write is synchronous because the relay is: a few kilobytes into a replaced
   file, twice a second.
7. **The writer holds a `Weak<OpenConversation>`.** The relay outlives the
   conversation — it is spawned and ends with the turn — and an `Arc` there would
   keep an evicted conversation's engine, and its background commands, alive for
   the length of the process.
8. **The view carries it back.** `SessionView.running.partial` feeds
   `liveTurn(seq, input, partial)`, so a live card rebuilt after switching back to
   a conversation starts where the stream stopped. ADR 0080's rebuild path had the
   question and the sequence but started the text at empty; this closes it.
9. **The retry card shows it as plain text.** Under the question, in
   `.interrupted-answer`, faded at the bottom by a mask. It is what the model said,
   not a finished answer: no markup, no tool rows, and the card still offers one
   action, 再问一次. Nothing is replayed (ADR 0081 stands).
10. **The throttle alone loses the tail, so a tick writes it.** `PartialNote` only
    runs when a delta arrives, and rule 6 makes it skip the deltas inside the
    window. A burst of text that ends in a tool call therefore leaves its last
    frames in memory until the next frame — and the model then thinks or runs a
    tool for a minute. Measured on 2026-11-24 against the shipped build: a 41
    character message was on screen and the note held 3 characters of it. A
    conversation-sized task holds a `Weak<OpenConversation>` and calls
    `note_tail` every `PARTIAL_TICK` = 250 ms while a turn is running, writing
    whatever the buffer has grown by. `TailNote::due` keeps it one write per turn
    and one per growth; `note_partial` refuses a buffer no longer than the one the
    note holds, so the immediate writer and the tick cannot undo each other with
    an older copy.
11. **Deleting a conversation deletes its note.** `sessions::remove` drops
    `<id>.pending.json` beside the document. A turn killed mid-answer leaves a
    note, and once the document is gone nothing would ever read or remove it.

## Consequences

- A restart shows how far the answer got instead of an empty card. What is lost is
  the rest of a long answer: past 8 000 bytes the kept copy is a head, so the note
  is a record of what was shown, not a transcript.
- Thinking text is not kept. Only the answer is; reasoning is not what the reader
  came back for, and it doubles the bytes for no visible gain.
- No new file type and no new lifecycle: the field rides in the note that already
  exists, and the bound bounds the file. No queue, no reconciliation, no watchdog.
- A turn that streams for a minute rewrites one small file about 120 times, each
  atomic, plus one lock-and-compare of the buffer every 250 ms while the
  conversation is open; it writes only while a turn is running and the head has
  grown. A turn that dies before the first model call writes nothing, and the note
  is exactly what ADR 0081 wrote.
- A note whose turn did land, written by a process that died between the document
  and `clear_pending`, is never read: `unanswered` drops it because the engine has
  the sequence. It is left on disk rather than removed by a view — a view that
  reads the store may not write it (ADR 0072) — and the next send replaces it.
- The kept text is not resumable. A retry is a new turn with its own `seq`, its own
  tool calls and its own cost, because the half-run turn already had side effects.
