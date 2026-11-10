import { describe, expect, it } from "vitest";
import type { SessionMeta, Turn } from "./protocol";
import { initialSession, reduce } from "./session";

function turn(seq: number, input = `in ${seq}`, answer = `out ${seq}`): Turn {
  return {
    seq,
    input,
    answer,
    thinking: "",
    state: "Done",
    error: null,
    durationMs: 1,
    usage: { inputTokens: 0, outputTokens: 0, cachedInputTokens: 0 },
    verificationPassed: null,
    verification: null,
    tools: [],
  };
}

function meta(id: string, updatedAt: number, title = ""): SessionMeta {
  return {
    id,
    title,
    archived: false,
    rig: "coding",
    createdAt: updatedAt,
    updatedAt,
    turns: 0,
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
        sessions: [],
        projects: [],
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

  it("streams thinking separately from the answer", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 4,
      input: "hi",
    });
    state = reduce(state, { type: "thinking-delta", seq: 4, text: "检查" });
    state = reduce(state, { type: "thinking-delta", seq: 4, text: "文件" });
    state = reduce(state, { type: "text-delta", seq: 4, text: "答案" });

    expect(state.running?.thinking).toBe("检查文件");
    expect(state.running?.text).toBe("答案");
  });

  it("parks a call's commentary when that call goes on to use a tool", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 1,
      input: "hi",
    });
    state = reduce(state, {
      type: "text-delta",
      seq: 1,
      text: "let me look",
    });
    state = reduce(state, {
      type: "tool-started",
      seq: 1,
      callId: 1,
      name: "read_file",
      args: "a",
    });

    expect(state.running?.text).toBe("");
    expect(state.running?.process).toEqual(["let me look"]);

    state = reduce(state, { type: "text-delta", seq: 1, text: "answer" });
    expect(state.running?.text).toBe("answer");
  });

  it("keeps live tool rows keyed by call id and ignores stale turns", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 4,
      input: "hi",
    });
    state = reduce(state, {
      type: "tool-started",
      seq: 4,
      callId: 7,
      name: "read_file",
      args: "path=a",
    });
    state = reduce(state, {
      type: "tool-started",
      seq: 3,
      callId: 8,
      name: "stale",
      args: null,
    });
    state = reduce(state, {
      type: "tool-finished",
      seq: 4,
      callId: 7,
      name: "read_file",
      ok: true,
      durationMs: 3,
      error: null,
      output: "one",
    });

    expect(state.running?.tools).toHaveLength(1);
    expect(state.running?.tools[0]).toMatchObject({
      callId: 7,
      name: "read_file",
      args: "path=a",
      ok: true,
      output: "one",
    });
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
        sessions: [],
        projects: [],
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

    state = reduce(state, { type: "turn-started", seq: 8, input: "again" });
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
        sessions: [],
        projects: [],
        turns: [turn(1)],
        nextBefore: null,
      },
    });
    state = reduce(state, { type: "turn-started", seq: 1, input: "late" });

    expect(state.running).toBeNull();
    expect(state.turns).toHaveLength(1);
  });

  it("opens a project's conversations newest first", () => {
    const state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "r",
        workspace: "/w",
        sessionId: "a",
        sessions: [meta("a", 1), meta("b", 2)],
        projects: [{ path: "/w", name: "w", openedAt: 1 }],
        turns: [],
        nextBefore: null,
      },
    });

    expect(state.sessions.map((item) => item.id)).toEqual(["b", "a"]);
    expect(state.projects.map((item) => item.name)).toEqual(["w"]);
  });

  it("moves an updated conversation to the front with its new title", () => {
    let state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "r",
        workspace: "/w",
        sessionId: "a",
        sessions: [meta("a", 1), meta("b", 2)],
        projects: [],
        turns: [],
        nextBefore: null,
      },
    });

    state = reduce(state, {
      type: "session-updated",
      meta: meta("a", 3, "first question"),
    });

    expect(state.sessions.map((item) => item.id)).toEqual(["a", "b"]);
    expect(state.sessions[0].title).toBe("first question");
  });

  it("keeps the verification result on a finished turn", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 1,
      input: "fix it",
    });
    state = reduce(state, {
      type: "turn-finished",
      turn: { ...turn(1, "fix it"), verificationPassed: true },
    });

    expect(state.turns[0].verificationPassed).toBe(true);
  });

  it("carries the verification evidence on a failed turn", () => {
    let state = reduce(initialSession(), {
      type: "turn-started",
      seq: 1,
      input: "fix it",
    });
    state = reduce(state, {
      type: "turn-finished",
      turn: {
        ...turn(1, "fix it"),
        verificationPassed: false,
        verification: {
          passed: false,
          evidence: [
            {
              itemId: "workspace-tests",
              verifier: "test",
              outcome: "failed",
              detail: "FAILED (failures=1)",
            },
          ],
        },
      },
    });

    expect(state.turns[0].verification?.evidence[0].detail).toContain("FAILED");
  });

  it("carries the archived flag through a row update", () => {
    let state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "r",
        workspace: "/w",
        sessionId: "a",
        sessions: [meta("a", 1)],
        projects: [],
        turns: [],
        nextBefore: null,
      },
    });

    state = reduce(state, {
      type: "session-updated",
      meta: { ...meta("a", 2, "renamed"), archived: true },
    });

    expect(state.sessions[0].archived).toBe(true);
    expect(state.sessions[0].title).toBe("renamed");
  });

  it("replaces the conversation list without touching the transcript", () => {
    let state = reduce(initialSession(), {
      type: "session-opened",
      view: {
        rig: "r",
        workspace: "/w",
        sessionId: "a",
        sessions: [meta("a", 1)],
        projects: [],
        turns: [turn(1)],
        nextBefore: null,
      },
    });

    state = reduce(state, {
      type: "sessions-loaded",
      sessions: [meta("b", 5), meta("a", 1)],
    });

    expect(state.sessions.map((item) => item.id)).toEqual(["b", "a"]);
    expect(state.turns).toHaveLength(1);
  });
});
