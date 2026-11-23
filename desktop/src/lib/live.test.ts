import { describe, expect, it } from "vitest";
import {
  LIVE_FLUSH_MAX_MS,
  LIVE_FLUSH_MIN_MS,
  bottomKey,
  flushDelay,
  type BottomParts,
} from "./live";

describe("streaming flush window", () => {
  it("keeps a short answer on the smooth floor", () => {
    expect(flushDelay(0)).toBe(LIVE_FLUSH_MIN_MS);
    expect(flushDelay(2000)).toBe(LIVE_FLUSH_MIN_MS);
    expect(flushDelay(20000)).toBe(LIVE_FLUSH_MIN_MS);
  });

  it("slows down as the answer grows", () => {
    // 40 KB costs about 15 ms a sample; 80 ms would be a fifth of the core.
    expect(flushDelay(40_000)).toBeGreaterThan(LIVE_FLUSH_MIN_MS);
    expect(flushDelay(80_000)).toBeGreaterThan(flushDelay(40_000));
  });

  it("never schedules a sample further out than the ceiling", () => {
    expect(flushDelay(400_000)).toBe(LIVE_FLUSH_MAX_MS);
    expect(flushDelay(Number.POSITIVE_INFINITY)).toBe(LIVE_FLUSH_MIN_MS);
    expect(flushDelay(Number.NaN)).toBe(LIVE_FLUSH_MIN_MS);
  });

  it("keeps the work per second bounded up to the measured range", () => {
    // The measured cost model: ~0.37 us per character per sample. Within the
    // sizes measured, a sample stays under a seventh of the time.
    for (const chars of [10_000, 50_000, 100_000]) {
      const costMs = chars * 0.00037;
      expect(costMs / flushDelay(chars)).toBeLessThan(0.15);
    }
    // Past the ceiling the duty does rise; that is the trade the ceiling
    // makes, so the test pins it rather than hiding it.
    expect(flushDelay(300_000)).toBe(LIVE_FLUSH_MAX_MS);
  });
});

describe("the transcript's bottom edge", () => {
  const idle: BottomParts = {
    answerChars: 0,
    reasoningChars: 0,
    turns: 12,
    runningSeq: null,
    runningInputChars: 0,
    tools: 0,
    background: 0,
    error: "",
  };

  it("moves when a turn starts, though nothing else has changed yet", () => {
    // The live card is appended with the user's text in it before the first
    // delta. A send whose turn starts below the fold has to be followed, so the
    // start alone must move the figure.
    const started = { ...idle, runningSeq: 13, runningInputChars: 9 };
    expect(bottomKey(started)).not.toBe(bottomKey(idle));
  });

  it("distinguishes a rebuilt card at another length", () => {
    const short = { ...idle, runningSeq: 13, runningInputChars: 9 };
    const long = { ...idle, runningSeq: 13, runningInputChars: 40 };
    expect(bottomKey(long)).not.toBe(bottomKey(short));
  });

  it("moves for each thing that grows under the reader", () => {
    const base = bottomKey(idle);
    const moved = [
      { ...idle, answerChars: 1 },
      { ...idle, reasoningChars: 1 },
      { ...idle, turns: 13 },
      { ...idle, runningSeq: 13 },
      { ...idle, tools: 1 },
      { ...idle, background: 1 },
      { ...idle, error: "boom" },
    ].map(bottomKey);
    for (const key of moved) expect(key).not.toBe(base);
    // No two of them collide, so one change cannot mask another.
    expect(new Set(moved).size).toBe(moved.length);
  });

  it("holds still while the store dispatches something else", () => {
    // A redraw that changes nothing on screen must not queue a scroll: the
    // figure is the effect's only dependency and it is unchanged.
    expect(bottomKey({ ...idle })).toBe(bottomKey({ ...idle }));
  });
});
