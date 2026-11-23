import { get } from "svelte/store";
import { beforeEach, describe, expect, it } from "vitest";
import type { SessionMeta, SessionView, Turn } from "./protocol";
import {
  KEEP,
  busy,
  closeAll,
  dispatch,
  dispatchTo,
  focus,
  held,
  openConversation,
  session,
} from "./session-store";

function view(id: string, extra: Partial<SessionView> = {}): SessionView {
  return {
    rig: "coding-live",
    workspace: "/w",
    sessionId: id,
    running: null,
    turns: [],
    nextBefore: null,
    sessions: [],
    projects: [],
    ...extra,
  };
}

function meta(id: string): SessionMeta {
  return {
    id,
    title: `title ${id}`,
    archived: false,
    rig: "coding-live",
    createdAt: 1,
    updatedAt: 1,
    turns: 0,
  };
}

function turn(seq: number): Turn {
  return {
    seq,
    input: `in ${seq}`,
    answer: `out ${seq}`,
    thinking: "",
    state: "Done",
    error: null,
    durationMs: 1,
    usage: { inputTokens: 0, outputTokens: 0, cachedInputTokens: 0 },
    modelCalls: 0,
    verificationPassed: null,
    verification: null,
    tools: [],
  };
}

beforeEach(() => {
  closeAll();
});

describe("a window that holds several conversations", () => {
  it("writes an event to the conversation it names, not the one on screen", () => {
    openConversation(view("a"));
    openConversation(view("b"));
    expect(get(session).sessionId).toBe("b");

    // A turn keeps running in `a` while the window shows `b`.
    dispatchTo("a", { type: "turn-started", seq: 1, input: "hello" });
    dispatchTo("a", { type: "text-delta", seq: 1, text: "wor" });
    dispatchTo("a", { type: "text-delta", seq: 1, text: "ld" });

    expect(get(session).sessionId).toBe("b");
    expect(get(session).turns).toHaveLength(0);
    expect(get(busy).has("a")).toBe(true);

    // Switching back finds the turn where it was left, not blank.
    focus.set("a");
    expect(get(session).sessionId).toBe("a");
    expect(get(session).running?.text).toBe("world");
    expect(get(session).running?.input).toBe("hello");
  });

  it("forgets a finished conversation's work in the rail", () => {
    openConversation(view("a"));
    dispatchTo("a", { type: "turn-started", seq: 1, input: "hello" });
    expect(get(busy).has("a")).toBe(true);

    dispatchTo("a", { type: "turn-finished", turn: turn(1) });
    expect(get(busy).has("a")).toBe(false);
    expect(get(session).turns).toHaveLength(1);
  });

  it("drops an event for a conversation the window never opened", () => {
    openConversation(view("a"));
    dispatchTo("elsewhere", { type: "turn-started", seq: 1, input: "x" });

    // No state was invented for it: opening it reads the engine and the disk.
    expect(held()).toEqual(["a"]);
  });

  it("writes a component's own answer to the conversation on screen", () => {
    openConversation(view("a"));
    dispatch({ type: "turn-started", seq: 1, input: "hello" });
    dispatch({ type: "text-delta", seq: 1, text: "hi" });
    expect(get(session).running?.text).toBe("hi");
  });

  it("keeps the project's conversation list the same in every held copy", () => {
    openConversation(view("a"));
    openConversation(view("b"));

    // A rename made while `b` is on screen must not leave `a` holding the old
    // name: the rail draws whichever conversation is focused.
    dispatch({ type: "sessions-loaded", sessions: [meta("a"), meta("b")] });
    // Newest first with the same tie-break on both sides, so the two copies are
    // equal and stay equal.
    focus.set("a");
    expect(get(session).sessions.map((item) => item.id)).toEqual(["b", "a"]);
    focus.set("b");
    expect(get(session).sessions.map((item) => item.id)).toEqual(["b", "a"]);
  });
});

describe("the state the window holds is bounded", () => {
  it("drops the oldest idle conversation once it holds more than KEEP", () => {
    for (let index = 0; index < KEEP + 2; index += 1) {
      openConversation(view(`c${index}`));
    }

    expect(held().length).toBe(KEEP);
    // The conversation on screen is never the one dropped.
    expect(held()).toContain(`c${KEEP + 1}`);
    // The first ones are the ones that went.
    expect(held()).not.toContain("c0");
  });

  it("never drops a conversation with a turn running", () => {
    openConversation(view("working"));
    dispatchTo("working", { type: "turn-started", seq: 1, input: "long" });

    for (let index = 0; index < KEEP + 4; index += 1) {
      openConversation(view(`idle${index}`));
    }

    expect(held()).toContain("working");
    // Whatever is dropped is idle: the cache gives up its own state before it
    // gives up a stream the user is watching.
    for (const id of held()) {
      expect(id === "working" || id.startsWith("idle")).toBe(true);
    }
  });
});
