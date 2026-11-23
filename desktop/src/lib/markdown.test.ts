import { beforeEach, describe, expect, it, vi } from "vitest";

const { parse, sanitize } = vi.hoisted(() => ({
  parse: vi.fn((text: string) => `<p>${text}</p>`),
  sanitize: vi.fn((html: string) => html.replace("<script", "&lt;script")),
}));

vi.mock("marked", () => ({ marked: { parse } }));
vi.mock("dompurify", () => ({ default: { sanitize } }));

import {
  MARKDOWN_CACHE_MAX,
  clearMarkdownCache,
  markdownCacheSize,
  renderMarkdown,
} from "./markdown";

beforeEach(() => {
  clearMarkdownCache();
  parse.mockClear();
  sanitize.mockClear();
});

describe("markdown rendered once per text", () => {
  it("parses a text the first time and looks it up after that", () => {
    // The transcript re-renders on every streamed delta and every poll tick.
    // The answer under it has not changed, so neither should the parse.
    const first = renderMarkdown("a table\n\n| a | b |");
    const again = renderMarkdown("a table\n\n| a | b |");
    expect(again).toBe(first);
    expect(parse).toHaveBeenCalledTimes(1);
    expect(sanitize).toHaveBeenCalledTimes(1);
  });

  it("parses a text that really changed", () => {
    renderMarkdown("one");
    renderMarkdown("one and more");
    renderMarkdown("one");
    expect(parse).toHaveBeenCalledTimes(2);
    expect(sanitize).toHaveBeenCalledTimes(2);
  });

  it("keeps the sanitized HTML, never the raw parse", () => {
    const html = renderMarkdown("<script>alert(1)</script>");
    expect(html).not.toContain("<script");
    // The sanitized string is what a later lookup hands back.
    expect(renderMarkdown("<script>alert(1)</script>")).toBe(html);
    expect(sanitize).toHaveBeenCalledTimes(1);
  });

  it("keeps the cache bounded, so a streaming answer cannot fill it", () => {
    for (let index = 0; index < MARKDOWN_CACHE_MAX + 4; index += 1) {
      renderMarkdown(`answer ${index}`);
    }
    expect(markdownCacheSize()).toBe(MARKDOWN_CACHE_MAX);
    parse.mockClear();
    // The oldest entry was shed, so it is parsed again; the newest is still
    // there, so asking for it costs nothing.
    renderMarkdown("answer 0");
    renderMarkdown(`answer ${MARKDOWN_CACHE_MAX + 3}`);
    expect(parse).toHaveBeenCalledTimes(1);
    expect(parse).toHaveBeenLastCalledWith("answer 0", expect.anything());
  });

  it("renders the empty answer without parsing anything else", () => {
    expect(renderMarkdown("")).toBe("<p></p>");
    expect(markdownCacheSize()).toBe(1);
  });
});
