import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

/// The backend half: every `listen` the api installs is kept here, so a test can
/// deliver an event the way Tauri would.
const { handlers } = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handle: (event: { payload: unknown }) => void) => {
    handlers.set(name, handle);
    return () => {
      handlers.delete(name);
    };
  },
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: async () => null,
}));

import { listenToEvents } from "./api";
import type { SessionView } from "./protocol";
import {
  closeAll,
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

function fire(name: string, payload: unknown): void {
  const handle = handlers.get(name);
  if (handle === undefined) throw new Error(`no listener for ${name}`);
  handle({ payload });
}

describe("a backend event lands in the conversation it names", () => {
  beforeEach(async () => {
    handlers.clear();
    closeAll();
    await listenToEvents();
  });

  it("keeps streaming into a conversation the window is not showing", () => {
    openConversation(view("a"));
    openConversation(view("b"));
    focus.set("a");

    fire("keelson:turn-started", { sessionId: "b", seq: 1, input: "hello" });
    fire("keelson:text-delta", { sessionId: "b", seq: 1, text: "wor" });
    fire("keelson:text-delta", { sessionId: "b", seq: 1, text: "ld" });

    // Nothing leaked into the conversation on screen.
    expect(get(session).sessionId).toBe("a");
    expect(get(session).running).toBeNull();

    // Switching back finds the answer whole, without waiting for a reload.
    focus.set("b");
    expect(get(session).running?.text).toBe("world");
  });

  it("drops an event for a conversation the window holds no state for", () => {
    openConversation(view("a"));

    fire("keelson:turn-started", { sessionId: "elsewhere", seq: 1, input: "x" });
    fire("keelson:text-delta", { sessionId: "elsewhere", seq: 1, text: "x" });

    // No state was invented for it: opening it reads the engine and the disk.
    expect(held()).toEqual(["a"]);
    expect(get(session).running).toBeNull();
  });

  it("lets a project-wide event reach every conversation of the project", () => {
    openConversation(view("a"));
    openConversation(view("b"));
    focus.set("a");

    fire("keelson:session-updated", {
      meta: {
        id: "b",
        title: "renamed elsewhere",
        archived: false,
        rig: "coding-live",
        createdAt: 1,
        updatedAt: 2,
        turns: 0,
      },
    });

    expect(get(session).sessions.map((item) => item.title)).toEqual([
      "renamed elsewhere",
    ]);
    focus.set("b");
    expect(get(session).sessions.map((item) => item.title)).toEqual([
      "renamed elsewhere",
    ]);
  });
});
