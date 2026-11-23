import { describe, expect, it } from "vitest";
import {
  COLLAPSE_MARGIN_PX,
  CULL_MARGIN_PX,
  DENSITY_SAMPLES,
  INITIAL_DENSITY,
  MAX_DENSITY,
  MIN_DENSITY,
  decideReveal,
  estimateTurnHeight,
  gapFromViewport,
  learnDensity,
  observeMargin,
  parseDensity,
} from "./cull";

describe("height reserved for an unrendered turn", () => {
  it("reserves a floor for a turn without an answer", () => {
    // A failed turn, or one that only called tools, still has to exist: a
    // thread of collapsed turns has no height to scroll through.
    expect(estimateTurnHeight(0, 0, false)).toBe(22);
    expect(estimateTurnHeight(-10, 0, false)).toBe(22);
    expect(estimateTurnHeight(Number.NaN, 0, false)).toBe(22);
  });

  it("grows with the answer at the learned density", () => {
    expect(estimateTurnHeight(1000, 0, false, 1)).toBe(1000);
    expect(estimateTurnHeight(5000, 0, false, 0.5)).toBe(2500);
    expect(estimateTurnHeight(50_000, 0, false, 0.5)).toBe(25_000);
  });

  it("keeps a density outside the plausible range from distorting a thread", () => {
    expect(estimateTurnHeight(1000, 0, false, 0)).toBe(1000 * MIN_DENSITY);
    expect(estimateTurnHeight(1000, 0, false, 99)).toBe(1000 * MAX_DENSITY);
    expect(estimateTurnHeight(1000, 0, false, Number.NaN)).toBe(
      1000 * INITIAL_DENSITY,
    );
  });

  it("adds a row per tool call and a block for a result", () => {
    expect(estimateTurnHeight(0, 10, false, 1)).toBe(22 + 10 * 24);
    expect(estimateTurnHeight(0, 0, true, 1)).toBe(22 + 70);
    expect(estimateTurnHeight(2000, 3, true, 1)).toBe(2000 + 3 * 24 + 70);
  });

  it("never reserves more than a screen of a very long turn", () => {
    expect(estimateTurnHeight(10_000_000, 0, false, 1)).toBe(40_000);
    expect(estimateTurnHeight(0, 10_000, false, 1)).toBe(40_000);
  });
});

describe("the figure carried between launches", () => {
  it("reads a stored figure", () => {
    expect(parseDensity("2.5")).toBe(2.5);
    expect(parseDensity(" 1 ")).toBe(1);
  });

  it("ignores a missing, empty or unreadable figure", () => {
    expect(parseDensity(null)).toBeNull();
    expect(parseDensity(undefined)).toBeNull();
    expect(parseDensity("")).toBeNull();
    expect(parseDensity("   ")).toBeNull();
    expect(parseDensity("wide")).toBeNull();
    expect(parseDensity("NaN")).toBeNull();
  });

  it("rejects a figure no turn could have produced", () => {
    // A stored value is a cache; one outside the range the learner allows is a
    // corrupted entry, not evidence about the rendering.
    expect(parseDensity(String(MIN_DENSITY / 2))).toBeNull();
    expect(parseDensity(String(MAX_DENSITY * 2))).toBeNull();
    expect(parseDensity("0")).toBeNull();
    expect(parseDensity("-3")).toBeNull();
  });
});

describe("learning how tall a turn is", () => {
  it("moves toward what the turns actually rendered", () => {
    // Measured in a browser: 2 KB of markdown rendered 2322 px, so the guess
    // of 0.8 px per character was three times short. The first measured turn
    // sets the figure outright; later ones average into it.
    const learned = learnDensity(0, 2048, 2322, INITIAL_DENSITY);
    expect(learned).toBeCloseTo(2322 / 2048, 6);
    const again = learnDensity(1, 2048, 1200, learned);
    expect(again).toBeLessThan(learned);
    expect(again).toBeGreaterThan(1200 / 2048);
  });

  it("ignores a turn too short to be representative", () => {
    expect(learnDensity(0, 120, 4000, 0.8)).toBe(0.8);
    expect(learnDensity(0, 2000, 0, 0.8)).toBe(0.8);
  });

  it("stops learning once enough turns have been measured", () => {
    expect(learnDensity(DENSITY_SAMPLES, 2048, 100, 0.8)).toBe(0.8);
  });

  it("clamps one odd turn", () => {
    expect(learnDensity(0, 2000, 40_000, 0.8)).toBeLessThanOrEqual(MAX_DENSITY);
    expect(learnDensity(0, 2000, 1, 0.8)).toBeGreaterThanOrEqual(0);
  });
});

describe("a body that has been read and left behind", () => {
  it("reveals a turn that comes near the viewport", () => {
    expect(decideReveal(0, false)).toBe("reveal");
    expect(decideReveal(CULL_MARGIN_PX, false)).toBe("reveal");
    expect(decideReveal(CULL_MARGIN_PX + 1, false)).toBe("hold");
  });

  it("keeps a revealed body while it is still near", () => {
    expect(decideReveal(0, true)).toBe("hold");
    expect(decideReveal(COLLAPSE_MARGIN_PX, true)).toBe("hold");
  });

  it("gives a revealed body back once it is far behind", () => {
    // Revealing used to be one-way, so a window that was read for an hour kept
    // every body it had shown — the DOM and the heap grew with the reading, not
    // with what was on screen (one real window reached 1.4 GB).
    expect(decideReveal(COLLAPSE_MARGIN_PX + 1, true)).toBe("collapse");
    expect(decideReveal(50_000, true)).toBe("collapse");
  });

  it("does not build and tear down a body on the boundary", () => {
    // Between the two margins nothing happens: a turn on the edge of the
    // viewport is neither revealed nor collapsed twice per scroll event.
    const between = (CULL_MARGIN_PX + COLLAPSE_MARGIN_PX) / 2;
    expect(decideReveal(between, false)).toBe("hold");
    expect(decideReveal(between, true)).toBe("hold");
  });

  it("holds when the distance cannot be read", () => {
    expect(decideReveal(Number.NaN, true)).toBe("hold");
    expect(decideReveal(Number.POSITIVE_INFINITY, false)).toBe("hold");
  });

  it("measures the gap above and below the viewport", () => {
    expect(gapFromViewport({ top: 100, bottom: 500 }, { top: 150, bottom: 300 })).toBe(0);
    expect(gapFromViewport({ top: 100, bottom: 500 }, { top: 500, bottom: 700 })).toBe(0);
    expect(gapFromViewport({ top: 100, bottom: 500 }, { top: 700, bottom: 900 })).toBe(200);
    expect(gapFromViewport({ top: 100, bottom: 500 }, { top: -300, bottom: -100 })).toBe(200);
  });
});

describe("where a turn is observed", () => {
  // An IntersectionObserver reports a target only when its intersection with
  // the expanded root changes. Watching every turn at the wide collapse margin
  // meant one entry per turn, 2400 px out, where the reveal test still said
  // hold — and no second entry while the body crossed the viewport. Every turn
  // the reader scrolled back to stayed a blank stand-in.
  it("watches an unrendered body where the reader arrives", () => {
    expect(observeMargin(false)).toBe(CULL_MARGIN_PX);
  });

  it("watches a rendered body where the reader leaves", () => {
    expect(observeMargin(true)).toBe(COLLAPSE_MARGIN_PX);
  });

  it("keeps the two apart, so a crossing is always reported", () => {
    expect(observeMargin(true)).toBeGreaterThan(observeMargin(false));
  });

  it("follows the margins it is given", () => {
    expect(observeMargin(false, 120, 900)).toBe(120);
    expect(observeMargin(true, 120, 900)).toBe(900);
  });

  it("reveals the moment an unrendered body is first reported", () => {
    // The two numbers have to agree: the entry that starts the watching is the
    // entry that has to build the body.
    expect(decideReveal(observeMargin(false), false)).toBe("reveal");
  });

  it("does not give a body back at the distance it came from", () => {
    // And a rendered body must survive the rebuild, or the transition would
    // undo itself and the thread would flicker.
    expect(decideReveal(observeMargin(true), true)).toBe("hold");
  });
});
