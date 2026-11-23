// One owner for the reading position.
//
// The transcript reserves a height for every turn whose body has not rendered
// (lib/cull.ts). A reserve cannot be exact, and everything that changes the
// height of a turn above the reader moves the text they are reading: a body
// rendering in place of its stand-in, a page of older turns spliced in at the
// top, a narrower window reflowing the markdown. The browser's own scroll
// anchoring is off (app.css) because a body growing at the newest turn fights
// the follow scroll; this module does the compensation instead, from one place,
// for every cause.
//
// The rule: while the newest row is not being followed, the text under the
// reader stays where it is, whatever happened elsewhere in the thread.
//
// The anchor is measured in the thread's own coordinates — the element's top
// plus the scroll position — so the reader's own scrolling never looks like a
// change: only the layout moving under them does.

export interface AnchorSample {
  /// The anchor's top in the thread's coordinates.
  top: number;
}

/// A sub-pixel move is layout rounding, not a reader losing their place.
export const ANCHOR_SLOP_PX = 0.5;

/// Frames spent checking after a change: one for the render, one for the layout
/// it causes, one for anything the render itself scheduled.
export const ANCHOR_SETTLE_FRAMES = 3;

/// Pixels to add to the scroll position so the anchor returns to `recorded`.
export function anchorCorrection(
  recorded: AnchorSample,
  current: AnchorSample,
  following: boolean,
): number {
  if (following) return 0;
  const drift = current.top - recorded.top;
  return Math.abs(drift) < ANCHOR_SLOP_PX ? 0 : drift;
}

export interface AnchorTarget {
  /// The anchor's top in the thread's coordinates, or null when it cannot be
  /// read (no turn at the top, or the conversation was replaced).
  read(): number | null;
  /// Move the reading position by a delta.
  shift(delta: number): void;
}

/// Watches the anchor for the frames that follow a change to the thread.
export class AnchorKeeper {
  readonly #target: AnchorTarget;
  readonly #frame: (run: () => void) => void;
  #recorded: number | null = null;
  #frames = 0;
  #queued = false;
  #owned = false;

  constructor(target: AnchorTarget, frame?: (run: () => void) => void) {
    this.#target = target;
    this.#frame = frame ?? ((run) => void requestAnimationFrame(run));
  }

  /// Someone else owns the position: the follow scroll, or a page of older
  /// turns being spliced in. While it is true the keeper does not correct, and
  /// it learns the position again when ownership comes back.
  own(owned: boolean): void {
    if (this.#owned === owned) return;
    this.#owned = owned;
    this.#frames = 0;
    if (!owned) this.record();
  }

  /// Where the anchor is now is where the reader wants it. Scrolling does not
  /// need to call this: the anchor is measured in the thread's coordinates, so
  /// the reader moving is not a change. A new element under the top edge does.
  record(): void {
    this.#frames = 0;
    this.#recorded = this.#target.read();
  }

  /// Something may have changed the height of the thread.
  nudge(): void {
    if (this.#owned) return;
    this.#frames = ANCHOR_SETTLE_FRAMES;
    this.#schedule();
  }

  destroy(): void {
    this.#frames = 0;
    this.#recorded = null;
  }

  #schedule(): void {
    if (this.#queued || this.#frames <= 0) return;
    this.#queued = true;
    this.#frame(() => {
      this.#queued = false;
      this.#settle();
    });
  }

  #settle(): void {
    if (this.#frames <= 0) return;
    this.#frames -= 1;
    const recorded = this.#recorded;
    if (!this.#owned && recorded !== null) {
      const now = this.#target.read();
      if (now !== null) {
        const delta = anchorCorrection({ top: recorded }, { top: now }, false);
        if (delta !== 0) {
          this.#target.shift(delta);
          // The shift moves the viewport, not the thread, so the anchor is now
          // where the reader wants it. A move the browser clamped at either end
          // lands somewhere else; taking the reading back is what keeps the
          // next frame from chasing it.
          this.#recorded = this.#target.read() ?? recorded;
        }
      }
    }
    this.#schedule();
  }
}
