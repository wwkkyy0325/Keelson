# 0070 — A crossing is observed where it happens

## Context

ADR 0069 made culling symmetric and left every turn's observer watching out to
the collapse margin (`COLLAPSE_MARGIN_PX`, 2400 px), on the reasoning that a
wider box also hears about a body being left behind. A turn that carries no
body is judged against the reveal margin (`CULL_MARGIN_PX`, 600 px) inside the
same callback. There is no second callback: an IntersectionObserver reports a
target only when its intersection with the expanded root **changes**, and a
turn that is already inside that box has an unchanged intersection of 1 while
it crosses the viewport.

So each turn reported exactly once, 2400 px away, where `decideReveal` said
`hold` — and never again. Scrolling back up a conversation showed the reader's
own questions and blank space where the answers were. Measured in the Chromium
harness against the real `cull` action, 40 turns, two on screen at a time:

```
initial   crossing 2  withBody 2
at-bottom crossing 1  withBody 1
up-2      crossing 2  withBody 1
up-3      crossing 1  withBody 0
up-4      crossing 2  withBody 0
… up-12   crossing 1  withBody 0
```

Every step from the third onwards had no body at all. The harness spent its
first two reveals on the turns that were inside 600 px at mount.

## Decision

The margin an observer is built with follows the state of the body
(`observeMargin`, `lib/cull.ts`):

- An **unrendered** body is watched at the reveal margin: the entry that starts
  the watching is the entry that builds the body.
- A **rendered** body is watched at the collapse margin: the entry that reports
  it again is the one that says the reader has left.
- A reveal or a collapse rebuilds the observer, so the new margin takes effect
  at once. That is once per crossing, not once per scroll event; between the
  two margins the state does not change and neither does the observer.

The two thresholds keep their hysteresis (ADR 0069): nothing happens between
600 and 2400 px, so a turn on the boundary is not rebuilt per scroll event. The
invariant is a test — `decideReveal(observeMargin(false), false)` is `reveal`,
and `decideReveal(observeMargin(true), true)` is `hold`.

## Consequences

Same harness, same scrolling, after the change:

```
up-1 … up-12       crossing 1–2  withBody = crossing  blank 0
flick-1 … flick-8  crossing 1–2  withBody = crossing  blank 0   (2600 px per step)
down-1 … down-8    crossing 1–2  withBody = crossing  blank 0
```

- No step has a blank body on screen, including a flick that skips the whole
  reveal margin in one frame: the landing entry is measured against the
  viewport, so a body that lands on screen is revealed by the same callback.
- The number of bodies in the DOM stays bounded: 3–4 of 40 turns in the
  harness, regardless of direction, which is what ADR 0069 was for.
- Cost: one `IntersectionObserver` construction per crossing. A crossing is
  something a reader does, not something a scroll event does.
