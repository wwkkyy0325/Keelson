import { describe, expect, it } from "vitest";
import { atBottom, followAfterScroll } from "./follow";

describe("following the newest row", () => {
  it("calls the end the bottom only inside the band", () => {
    expect(atBottom(930, 1500, 500)).toBe(true);
    expect(atBottom(920, 1500, 500)).toBe(false);
    expect(atBottom(1000, 1500, 500)).toBe(true);
    expect(atBottom(700, 1500, 500)).toBe(false);
    // Scrolled past the end can only be the result of a clamp.
    expect(atBottom(1200, 1500, 500)).toBe(true);
  });

  it("releases the follow when the user moves up", () => {
    const previous = { top: 1000, height: 1500 };
    expect(followAfterScroll(previous, 900, 1500, 500)).toBe(false);
  });

  it("follows again once a downward move reaches the end", () => {
    const previous = { top: 900, height: 1500 };
    expect(followAfterScroll(previous, 1000, 1500, 500)).toBe(true);
    // Moving down without reaching the end is not a request to follow.
    expect(followAfterScroll(previous, 920, 1500, 500)).toBe(false);
  });

  it("keeps following when the content shrank under the view", () => {
    // A background row finished and disappeared: the browser clamped the
    // position up by the height it lost, which looked exactly like a user
    // scrolling back. The newest rows then stayed below the fold.
    const previous = { top: 1000, height: 1500 };
    expect(followAfterScroll(previous, 972, 1472, 500)).toBe(true);
  });

  it("does not grab the view when a shrink happens while reading above", () => {
    const previous = { top: 300, height: 1500 };
    expect(followAfterScroll(previous, 272, 1472, 500)).toBe(false);
  });
});
