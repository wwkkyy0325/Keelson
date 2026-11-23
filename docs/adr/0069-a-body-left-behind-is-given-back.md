# 0069 — A body left behind is given back

## Context

A turn's body was revealed once and stayed rendered for the life of the
window: `cull()` disconnected its observer on the first intersection, because
"a turn that has been read stays rendered, so scrolling back up does not
rebuild it". Reading a long conversation therefore grew the DOM and the
JavaScript heap with the reading, not with what is on screen. The window this
was reported from had 45 turns, 3492 tool rows and `Physical footprint 528.6M`
with `RSS 1418 MB` after an hour, and the `sample` above it showed
`Element::~Element`, `Node::removedLastRef` and repeated
`Document::setMarkupUnsafe` — elements being built and thrown away while
nobody was doing anything.

The same binary on the same conversation, freshly opened, sat at 3.2% CPU and
126 MB with one screen of bodies rendered. The difference was how much of the
thread had been read.

## Decision

Culling is symmetric (`lib/cull.ts`, `Turn.svelte`):

- Two thresholds: a turn is revealed within `CULL_MARGIN_PX` (600) of the
  viewport and only given back beyond `COLLAPSE_MARGIN_PX` (2400). Between the
  two, nothing happens, so a turn on the boundary does not build and tear down
  its body on every scroll event. `decideReveal` is a pure function with tests.
- The observer stays connected and watches out to the collapse margin, so a
  body that is being left behind is heard about too. The reveal test is measured
  against the real viewport box, not against that expanded margin. (The margin
  itself follows the state of the body as of ADR 0070: watching a body that has
  not rendered at the collapse margin never reports it again.)
- A body that is given back takes its **measured** height as its stand-in
  (`measuredPx`, taken while it was rendered) **minus the header** — the user's
  message and its action row, which stay in the article when the body goes.
  That part is measured once, at mount, as the article minus the stand-in it
  holds then (`headerPx`). Taking the whole article as the stand-in added the
  header back on every collapse: in the Chromium harness one turn went
  4356 px rendered → 4454 px collapsed, and the 98 px was the header. With the
  subtraction the same turn goes 4356 → 4356.
- The thread therefore does not change size when a body collapses, and the
  reading position cannot move because of it — the anchor keeper keeps its own
  job for everything else (ADR 0067).
- The newest turn is never given back: it is where the reader is looking.

## Consequences

- Measured in the Chromium harness on the real 45-turn conversation: scrolling
  from the middle of the thread to the top drops the rendered tool rows from
  161 to 46 and the node count from 1809 to 985. A collapse that does not
  resize the thread is a collapse the reader cannot see.
- Read end to end and back in the same harness (45 turns, 129,121 answer
  characters, 1600 px steps): never more than **4 bodies of 45** in the
  document, 1226 nodes at the bottom against 2341 at the top, and no turn on
  screen without a body at any of the 241 steps. A body that was read and left
  behind is given back, so reading the whole thread does not make the thread
  permanently heavier.
- A thread read for an hour now holds a screen of bodies, not a transcript's
  worth, which is what the 1.4 GB was.
- Scrolling back rebuilds a body from its text; that costs one parse, which is
  now memoized (ADR 0068), so going back and forth over the same turns is a map
  lookup after the first pass.
- Density learning is unaffected: it stops after `DENSITY_SAMPLES` turns, and a
  re-revealed turn only re-measures.
