import { describe, expect, it } from "vitest";
import {
  ARM_MS,
  Perf,
  SAMPLE_BOUND,
  STALL_MS,
  isStall,
  stillArmed,
} from "./perf";

describe("front-end telemetry", () => {
  it("counts named events", () => {
    const perf = new Perf();
    perf.count("keelson:text-delta");
    perf.count("keelson:text-delta");
    perf.count("action", 5);
    expect(perf.report().counters).toEqual({
      "keelson:text-delta": 2,
      action: 5,
    });
  });

  it("keeps count, total, worst and last for a timed path", () => {
    const perf = new Perf();
    perf.mark("live.flush", 4);
    perf.mark("live.flush", 12);
    perf.mark("live.flush", 8);
    const stat = perf.report().timers["live.flush"];
    expect(stat.count).toBe(3);
    expect(stat.totalMs).toBe(24);
    expect(stat.maxMs).toBe(12);
    expect(stat.lastMs).toBe(8);
    expect(stat.samples).toEqual([4, 12, 8]);
  });

  it("keeps only the recent samples, so a long session stays bounded", () => {
    const perf = new Perf();
    for (let index = 0; index < SAMPLE_BOUND + 20; index += 1) {
      perf.mark("live.flush", index);
    }
    const stat = perf.report().timers["live.flush"];
    expect(stat.count).toBe(SAMPLE_BOUND + 20);
    expect(stat.samples).toHaveLength(SAMPLE_BOUND);
    expect(stat.samples.at(-1)).toBe(SAMPLE_BOUND + 19);
  });

  it("times a synchronous run even when it throws", () => {
    const perf = new Perf();
    expect(() =>
      perf.time("markdown.parse", () => {
        throw new Error("boom");
      }),
    ).toThrow("boom");
    expect(perf.report().timers["markdown.parse"].count).toBe(1);
  });

  it("treats a missed frame as a stall only past the threshold", () => {
    expect(isStall(STALL_MS)).toBe(false);
    expect(isStall(STALL_MS + 1)).toBe(true);
    expect(isStall(Number.NaN)).toBe(false);
  });

  it("hands out a copy, so a reader cannot change what was measured", () => {
    const perf = new Perf();
    perf.mark("live.flush", 5);
    const report = perf.report();
    report.timers["live.flush"].samples.push(999);
    report.counters.injected = 1;
    const again = perf.report();
    expect(again.timers["live.flush"].samples).toEqual([5]);
    expect(again.counters.injected).toBeUndefined();
  });

  it("clears everything on reset", () => {
    const perf = new Perf();
    perf.count("action");
    perf.mark("live.flush", 3);
    perf.reset();
    const report = perf.report();
    expect(report.counters).toEqual({});
    expect(report.timers).toEqual({});
    expect(report.stalls.count).toBe(0);
    expect(report.uptimeMs).toBeGreaterThanOrEqual(0);
  });

  it("times a promise as well as a call, and records it either way", async () => {
    const perf = new Perf();
    await expect(
      perf.timeAsync("background.sync", async () => {
        throw new Error("no host");
      }),
    ).rejects.toThrow("no host");
    expect(perf.report().timers["background.sync"].count).toBe(1);
  });

  it("lets the frame watcher stop once nothing has happened", () => {
    // A perpetual frame loop is itself a cost, so the watcher is only wanted
    // for a while after the last counted event.
    expect(stillArmed(1000, 3000)).toBe(true);
    expect(stillArmed(3000, 3000)).toBe(false);
    expect(stillArmed(9000, 3000)).toBe(false);
    expect(stillArmed(0, Number.NaN)).toBe(false);
    expect(ARM_MS).toBeGreaterThan(STALL_MS);
  });
});
