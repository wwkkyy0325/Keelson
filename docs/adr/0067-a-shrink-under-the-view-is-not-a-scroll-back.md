# 0067 — A shrink under the view is not the user scrolling back

## Context

A turn appends rows to the transcript: a tool starts, a tool finishes, a
background command reports its end. The window follows the newest row, and it
stops following when the user scrolls back to read something above. Both are
wanted, and they were decided by one rule in `Transcript.onScroll`: a scroll
event whose position moved up released the follow.

That rule cannot tell a user moving up from the browser clamping a position it
can no longer honour. Content above the view can vanish mid-turn — a background
row that finished and was cleared, a fold that closed, a culled placeholder
that collapsed — and the container clamps the scroll position down by the
height it lost. Measured in this conversation's window: the transcript showed
`bg-17 bash … 27.3 s` with two further calls already made below it; the next
capture, seconds later, still ended at the row above them. The newest tool rows
stayed below the fold for the rest of the turn, which for a long turn means the
rows somebody is waiting to read never appear.

## Decision

The scroll handler compares height as well as position (`lib/follow.ts`):

- The content shrank (`height < previous.height`) and the position moved up:
  that is a clamp, not an intent. Keep following, and follow when the clamped
  position is inside the band.
- The content did not shrink and the position moved up: the user is looking
  back. Release the follow until a downward move reaches the end.
- Either way, a position inside the 80 px band follows.

A wheel still releases the follow before the browser scrolls, so an
auto-scroll scheduled in the same tick cannot pull the view back.

## Consequences

- The rule is a pure function with its own tests, so it can be settled without
  a browser; the component only samples the container and stores the result.
- The follow now depends on how the transcript's height changed, not only on
  where it sits. A future feature that changes height above the view while a
  turn runs — collapsing an old turn, loading a summary — inherits the right
  answer instead of silently stopping the follow.
- Reading back during a turn still works: a wheel, a drag, a keyboard page or a
  trackpad flick that moves up with the height unchanged releases the follow,
  and the transcript shows the "load older" button rather than pulling itself
  away.

## Amendment: what moves the bottom is named, not guessed

The follow had a blind spot precisely where a message is sent. The figure that
decided whether to scroll counted rendered blocks, tool rows, background rows,
persisted turns and an error — every one of them empty at the instant a turn
begins. The live card appears immediately, carrying the user's own text, and
none of those numbers moved with it: the turn count only rises when the turn is
persisted. So a send from the end of a thread landed below the fold, and the
thread looked as though the send had gone nowhere until the finished turn
raised the count minutes later.

The set of things that move the bottom is now a named, tested value
(`BottomParts` in `lib/live.ts`), and it includes the running turn: its
sequence, which a fresh turn always changes, and the length of its input, so a
card rebuilt from the engine's view at a different length is not mistaken for
the same one.

The parts are lengths and ids, not elements, for the reason 0068 gives: reading
`scrollHeight` per delta would lay out every rendered turn once per chunk. The
caller passes what it already has.
