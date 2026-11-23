/// Parse and sanitize an answer once per distinct text.
///
/// The transcript re-renders on every streamed delta and on every poll tick
/// (the elapsed clock, the background rows, the context figure). Each of those
/// renders used to parse and sanitize the markdown of every revealed answer
/// again, because the HTML was a `$derived` of a prop: a parent update that
/// leaves the text alone still costs a full pass. Measured on the real answers
/// of one conversation: 1.1 ms to parse 5.7 KB, 1.4 ms to sanitize it, and
/// 34 ms for the 100 KB answer that streams. A thread of a few revealed turns
/// times a few hundred events a second is one core.
///
/// Rendering is a pure function of the text, so the result is taken once and
/// kept. The cache is bounded by entries rather than bytes: a conversation
/// holds a handful of answers, and an unbounded map would keep every
/// intermediate version of a streaming answer alive.
import DOMPurify from "dompurify";
import { marked } from "marked";
import { perf } from "./perf";

/// Answers kept parsed. More than a screen's worth of turns, so scrolling back
/// and forth does not re-parse; small enough that the strings stay a few MB.
export const MARKDOWN_CACHE_MAX = 8;

const cache = new Map<string, string>();

/// The sanitized HTML of `text`, parsed now if it has not been seen.
///
/// Model output is untrusted: it is parsed as markdown, then sanitized, so the
/// transcript cannot inject markup or scripts. Caching the sanitized result
/// keeps that guarantee — the sanitized string is what is kept, never the raw
/// HTML.
export function renderMarkdown(text: string): string {
  const cached = cache.get(text);
  if (cached !== undefined) {
    perf.count("markdown.cached");
    return cached;
  }
  const html = perf.time("markdown.parse", () =>
    DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true }) as string),
  );
  // Re-inserting an existing key keeps insertion order meaningful: the map is
  // read as "oldest first" when it has to shed an entry.
  cache.delete(text);
  cache.set(text, html);
  while (cache.size > MARKDOWN_CACHE_MAX) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
  return html;
}

/// How many answers are held, for a test or a diagnostic.
export function markdownCacheSize(): number {
  return cache.size;
}

/// Drops everything. A test uses it to start from a known state; nothing in the
/// app needs it, because the bound keeps the map small on its own.
export function clearMarkdownCache(): void {
  cache.clear();
}
