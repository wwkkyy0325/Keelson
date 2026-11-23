import { describe, expect, it } from "vitest";
import { ACCOUNT_EVERY_MS, accountNow, reportTurn } from "./report";
import { perf } from "./perf";

describe("the window's account of a turn", () => {
  it("carries the counters, the timers and what the page held", () => {
    perf.reset();
    perf.count("action", 3);
    perf.mark("markdown.parse", 7);
    const account = accountNow(12, "during");
    expect(account.seq).toBe(12);
    expect(account.phase).toBe("during");
    expect(account.counters.action).toBe(3);
    expect(account.timers["markdown.parse"].count).toBe(1);
    expect(account.dom.bodies).toBeGreaterThanOrEqual(0);
    expect(account.dom.tools).toBeGreaterThanOrEqual(0);
    // Without a document (a test, a console) the counts are zero, not a crash.
    expect(account.dom.nodes).toBeGreaterThanOrEqual(0);
  });

  it("samples a turn in flight often enough to bracket a spike", () => {
    expect(ACCOUNT_EVERY_MS).toBeGreaterThan(1000);
    expect(ACCOUNT_EVERY_MS).toBeLessThanOrEqual(10000);
  });

  it("says nothing for a workspace that is not open", async () => {
    // No conversation, no file: the account must not invent a path.
    await expect(
      reportTurn("", "s-1", 3, "start"),
    ).resolves.toBeUndefined();
  });
});
