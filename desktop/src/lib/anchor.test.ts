import { describe, expect, it } from "vitest";
import {
  ANCHOR_SETTLE_FRAMES,
  AnchorKeeper,
  anchorCorrection,
  type AnchorTarget,
} from "./anchor";

/// A scroller whose anchor is measured in thread coordinates, with the frames
/// the keeper asks for queued so the test decides when they run.
function harness(start: number) {
  const state = {
    /// The anchor's top in the thread's own coordinates.
    content: start,
    scrollTop: 0,
    /// The largest move the browser will make (the end of the thread).
    limit: Number.POSITIVE_INFINITY,
    shifts: [] as number[],
    frames: [] as (() => void)[],
  };
  const target: AnchorTarget = {
    read: () => state.content,
    shift: (delta) => {
      const applied = Math.max(-state.limit, Math.min(state.limit, delta));
      state.shifts.push(applied);
      state.scrollTop += applied;
    },
  };
  const keeper = new AnchorKeeper(target, (run) => state.frames.push(run));
  const pump = (frames = 1): void => {
    for (let i = 0; i < frames; i++) {
      const queued = state.frames.splice(0);
      for (const run of queued) run();
    }
  };
  return { state, keeper, pump };
}

describe("where the reading position goes", () => {
  it("turns a move of the anchor into a scroll of the same size", () => {
    // Content above grew by 40, so the anchor sits 40 px further down the
    // thread and the position has to move 40 px to leave the text in place.
    expect(anchorCorrection({ top: 100 }, { top: 140 }, false)).toBe(40);
    // Content above shrank: the position has to move back up.
    expect(anchorCorrection({ top: 100 }, { top: 60 }, false)).toBe(-40);
  });

  it("leaves the position alone while the newest row is followed", () => {
    expect(anchorCorrection({ top: 100 }, { top: 9000 }, true)).toBe(0);
  });

  it("treats a sub-pixel move as rounding", () => {
    expect(anchorCorrection({ top: 100 }, { top: 100.3 }, false)).toBe(0);
    expect(anchorCorrection({ top: 100 }, { top: 99.7 }, false)).toBe(0);
  });
});

describe("the keeper", () => {
  it("puts the anchor back after the thread changes size", () => {
    const { state, keeper, pump } = harness(100);
    keeper.record();
    state.content += 40; // a stand-in above the reader became its body
    keeper.nudge();
    pump();
    expect(state.shifts).toEqual([40]);
    expect(state.scrollTop).toBe(40);
  });

  it("follows the anchor through several causes in a row", () => {
    const { state, keeper, pump } = harness(100);
    keeper.record();
    state.content += 30;
    keeper.nudge();
    pump();
    state.content += 12;
    pump();
    expect(state.shifts).toEqual([30, 12]);
    expect(state.scrollTop).toBe(42);
  });

  it("stops asking for frames once the change has settled", () => {
    const { keeper, pump, state } = harness(100);
    keeper.record();
    keeper.nudge();
    pump(ANCHOR_SETTLE_FRAMES);
    expect(state.frames.length).toBe(0);
    pump(5);
    expect(state.frames.length).toBe(0);
  });

  it("does nothing while someone else owns the position", () => {
    const { state, keeper, pump } = harness(100);
    keeper.record();
    keeper.own(true);
    state.content += 40;
    keeper.nudge();
    pump(ANCHOR_SETTLE_FRAMES);
    expect(state.shifts).toEqual([]);
    // ... and takes up the new position when ownership comes back.
    keeper.own(false);
    state.content += 10;
    keeper.nudge();
    pump();
    expect(state.shifts).toEqual([10]);
  });

  it("does not chase a move the browser clamped", () => {
    const { state, keeper, pump } = harness(100);
    state.limit = 25; // the thread cannot scroll further
    keeper.record();
    state.content += 40;
    keeper.nudge();
    pump(ANCHOR_SETTLE_FRAMES);
    expect(state.shifts).toEqual([25]);
    expect(state.scrollTop).toBe(25);
  });

  it("reads the reader's own scrolling as no change at all", () => {
    // The anchor is measured in thread coordinates, so scrolling cannot move
    // it: only the layout can.
    const { state, keeper, pump } = harness(100);
    keeper.record();
    state.scrollTop += 3000;
    keeper.nudge();
    pump();
    expect(state.shifts).toEqual([]);
  });
});
