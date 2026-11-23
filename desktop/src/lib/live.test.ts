import { describe, expect, it } from "vitest";
import {
  LIVE_FLUSH_MAX_MS,
  LIVE_FLUSH_MIN_MS,
  flushDelay,
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
