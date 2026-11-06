import { describe, expect, it } from "vitest";
import type { Turn } from "./protocol";
import { initialSession, reduce } from "./session";

function turn(seq: number, input = `in ${seq}`, answer = `out ${seq}`): Turn {
  return {
    seq,
    input,
    answer,
    state: "Done",
    error: null,
    durationMs: 1,
    tools: [],
  };
}

describe("session reducer", () => {
  it("opens a session with turns sorted by sequence", () => {
    const state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "coding",
        workspace: "/w",
        sessionId: "s",
        turns: [turn(3), turn(1), turn(2)],
        nextBefore: 1,
      },
    });

    expect(state.turns.map((item) => item.seq)).toEqual([1, 2, 3]);
    expect(state.nextBefore).toBe(1);
    expect(state.running).toBeNull();
  });

  it("streams deltas only into the matching running turn", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 4,
      input: "hi",
    });
    state = reduce(state, { type: "text-delta", seq: 4, text: "hel" });
    state = reduce(state, { type: "text-delta", seq: 3, text: "stale" });
    state = reduce(state, { type: "text-delta", seq: 4, text: "lo" });

    expect(state.running?.text).toBe("hello");
  });

  it("replaces the running turn when it finishes and ignores duplicates", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 1,
      input: "hi",
    });
    state = reduce(state, { type: "text-delta", seq: 1, text: "partial" });
    state = reduce(state, { type: "turn-finished", turn: turn(1, "hi", "done") });
    state = reduce(state, { type: "turn-finished", turn: turn(1, "hi", "done") });

    expect(state.running).toBeNull();
    expect(state.turns).toHaveLength(1);
    expect(state.turns[0].answer).toBe("done");
  });

  it("prepends older pages without duplicates", () => {
    let state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "r",
        workspace: "/w",
        sessionId: "s",
        turns: [turn(4), turn(5)],
        nextBefore: 4,
      },
    });
    state = reduce(state, {
      type: "turns-loaded",
      page: { turns: [turn(2), turn(3), turn(4)], nextBefore: 2 },
    });
    state = reduce(state, {
      type: "turns-loaded",
      page: { turns: [turn(1)], nextBefore: null },
    });

    expect(state.turns.map((item) => item.seq)).toEqual([1, 2, 3, 4, 5]);
    expect(state.nextBefore).toBeNull();
    expect(state.loadingOlder).toBe(false);
  });

  it("clears the running turn and records a backend failure", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 7,
      input: "hi",
    });
    state = reduce(state, {
      type: "turn-failed",
      seq: 7,
      message: "provider down",
    });

    expect(state.running).toBeNull();
    expect(state.lastError).toBe("provider down");

    state = reduce(state, { type: "clear-error" });
    expect(state.lastError).toBeNull();
  });

  it("keeps one pending interaction and resolves it by id", () => {
    const interaction = {
      id: "i-1",
      kind: "permission_approval",
      prompt: "Allow?",
      options: [{ id: "allow_once", label: "Allow once" }],
    };
    let state = reduce(initialSession(), {
      type: "interaction-requested",
      interaction,
    });
    expect(state.pendingInteraction?.id).toBe("i-1");

    state = reduce(state, { type: "interaction-resolved", id: "other" });
    expect(state.pendingInteraction?.id).toBe("i-1");

    state = reduce(state, { type: "interaction-resolved", id: "i-1" });
    expect(state.pendingInteraction).toBeNull();
  });

  it("does not resurrect a persisted turn from a late start", () => {
    let state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "r",
        workspace: "/w",
        sessionId: "s",
        turns: [turn(1)],
        nextBefore: null,
      },
    });
    state = reduce(state, { type: "turn-started", seq: 1, input: "late" });

    expect(state.running).toBeNull();
    expect(state.turns).toHaveLength(1);
  });
});
