# 0068 — An unchanged answer is not rendered again

## Context

A window that had been open for an hour sat at 101–111% CPU with nothing on
screen moving: `top` reported `59334 WebContent 101.2 / 108.3 / 111.1%` over
ten seconds while the app was idle. A `sample` of that process put 2022 of
2096 main-thread samples inside WebKit's shared timer → event loop → microtask
checkpoint, with the leaves in `JSC::Yarr::Interpreter` (regex),
`stringProtoFuncToLowerCase`, `HTMLTokenizer::processToken`,
`MarkupAccumulator::appendCharactersReplacingEntities` and
`Element::~Element`. That is the markdown path: `marked` lexes with regular
expressions, `DOMPurify` lowercases tag names and re-serializes the sanitized
tree, and the result is written back with `{@html}`.

The cost per answer was measured earlier in this project: 5.7 KB of answer
costs 1.1 ms to parse and 1.4 ms to sanitize; the 100 KB answer that streams
costs 34 ms. What made those numbers a full core was the scheduling: the HTML
was a `$derived` of a prop in `Markdown.svelte`, and the transcript re-renders
on every streamed delta, every one-second clock tick and every poll tick. A
derived that is re-evaluated for a text that did not change still parses and
sanitizes the whole answer again.

## Decision

Rendering moves to `lib/markdown.ts`, a memoized function:

- `renderMarkdown(text)` returns the sanitized HTML, parsing only when the
  text has not been seen. The map holds the sanitized string, never the raw
  parse, so the guarantee that model output cannot inject markup is unchanged.
- The cache is bounded by entries (`MARKDOWN_CACHE_MAX = 8`), not by bytes: an
  unbounded map would keep every intermediate version of a streaming answer
  alive. Eight covers more than a screen of turns, so scrolling back and forth
  does not re-parse.
- `Markdown.svelte` keeps its `$derived`, so a render happens at the same point
  in the frame as before; the difference is that an unchanged text is a map
  lookup.
- Two counters make the path visible in the settings diagnostics:
  `markdown.parse` (a timer, with the parse+sanitize cost) and
  `markdown.cached` (how many renders were answered from the map).

## Consequences

- Ten store writes against a freshly seeded 45-turn thread now cost
  `+0 parses, +1 cache hit`, measured in the Chromium harness on the real
  conversation (`/tmp/kscroll/memo.mjs`). Before, every write re-parsed the
  answers the window was showing.
- A streaming answer still parses once per flush — it really is new text — so
  the existing `live.flush` budget (`lib/live.ts`) remains the only cost that
  scales with answer length.
- The memo cannot hide a stale render: the key is the whole source text, not a
  hash of a length or a version number.
