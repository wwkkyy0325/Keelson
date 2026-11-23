/// Render a turn's body only once it is near the viewport.
///
/// One turn is a markdown document plus, often, a hundred tool rows. A
/// conversation with 300 turns used to build all of them before the first
/// paint, and the browser laid every one of them out again on each reflow of
/// the thread. Only the turns around the viewport have to exist; the rest keep
/// their header and a stand-in of roughly the right height.
///
/// The stand-in is what keeps the scrollbar honest: a body that has never
/// rendered has no height, and a thread of collapsed turns cannot be scrolled
/// through. The estimate never has to be right, only close: it is gone the
/// moment the real body renders.
///
/// Nothing here touches the scroll position. A stand-in is given its height
/// once, at mount, and a body that renders above the reader is compensated by
/// the transcript's anchor keeper (lib/anchor.ts), which covers every cause of a
/// size change rather than just this one.

/// How far outside the viewport a turn is revealed, in pixels. One screen of
/// margin means a fast scroll does not outrun the render.
export const CULL_MARGIN_PX = 600;

/// A tool row's header line, even while folded.
const TOOL_ROW_PX = 24;
/// The result block (changes and verification) is one or two rows.
const RESULT_PX = 70;

/// Rendered pixels per character of answer, before anything has been measured.
/// Markdown is not prose: a table or a fenced block takes far more room per
/// character than a paragraph, so a fixed characters-per-line figure was off by
/// three on a real answer (512 px reserved for 2322 px rendered). The thread
/// measures its own turns and replaces this (see `learnDensity`).
export const INITIAL_DENSITY = 0.8;
/// Bounds on the learned figure: one odd turn must not resize the thread.
export const MIN_DENSITY = 0.2;
export const MAX_DENSITY = 4;
/// Turns measured before the running average is taken as the figure.
export const DENSITY_SAMPLES = 12;

/// Where the learned figure is kept between launches. It describes the
/// rendering — this stylesheet, this markdown — and not one conversation, so it
/// is worth handing to the next one: a stand-in that starts out close to the
/// real height is never resized under a reader, and the scrollbar does not have
/// to move as the thread fills in.
export const DENSITY_KEY = "keelson.transcript.density";

/// A stored figure, or null when there is none or it is not plausible.
export function parseDensity(raw: string | null | undefined): number | null {
  if (raw === null || raw === undefined || raw.trim() === "") return null;
  const value = Number.parseFloat(raw);
  if (!Number.isFinite(value)) return null;
  if (value < MIN_DENSITY || value > MAX_DENSITY) return null;
  return value;
}

/// The height to reserve for a body that has not rendered. `density` is pixels
/// per character of answer.
export function estimateTurnHeight(
  bodyChars: number,
  toolRows: number,
  hasResult: boolean,
  density = INITIAL_DENSITY,
): number {
  const chars = Number.isFinite(bodyChars) && bodyChars > 0 ? bodyChars : 0;
  const rows = Number.isFinite(toolRows) && toolRows > 0 ? Math.floor(toolRows) : 0;
  const perChar = Number.isFinite(density)
    ? Math.min(MAX_DENSITY, Math.max(MIN_DENSITY, density))
    : INITIAL_DENSITY;
  // The shortest turn still has to take some room, or a thread of empty turns
  // has no height to scroll through.
  const body = Math.max(22, chars * perChar);
  return Math.min(40_000, body + rows * TOOL_ROW_PX + (hasResult ? RESULT_PX : 0));
}

/// Folds one measured turn into the running figure. Only turns long enough to
/// be representative are counted, and only until the sample is complete: a
/// placeholder that keeps resizing while the reader scrolls is worse than a
/// slightly wrong one, and the later turns are revealed as they arrive anyway.
/// `measured` is how many turns have been measured so far.
export function learnDensity(
  measured: number,
  chars: number,
  renderedPx: number,
  density: number,
): number {
  if (renderedPx <= 0 || chars < 400) return density;
  if (measured >= DENSITY_SAMPLES) return density;
  const observed = Math.min(MAX_DENSITY, Math.max(MIN_DENSITY, renderedPx / chars));
  return (density * measured + observed) / (measured + 1);
}

/// How far past the reveal margin a rendered body keeps existing.
///
/// Revealing was one-way: a turn that had been read stayed rendered for the
/// life of the window. Reading a long thread therefore grew the DOM and the
/// JavaScript heap for as long as the window was open — one real conversation
/// reached 1.4 GB and a core at 100% — although the reader only ever sees one
/// screen of it. A body far behind the viewport is given back now, and the
/// stand-in it turns into is the height it actually had, so the thread does not
/// change size and the reading position does not move.
export const COLLAPSE_MARGIN_PX = 2400;

export type RevealDecision = "reveal" | "collapse" | "hold";

export type Box = { top: number; bottom: number };

/// Pixels between the viewport and a box; 0 when they overlap.
export function gapFromViewport(view: Box, rect: Box): number {
  if (!Number.isFinite(view.top) || !Number.isFinite(view.bottom)) return 0;
  if (!Number.isFinite(rect.top) || !Number.isFinite(rect.bottom)) return 0;
  return Math.max(0, view.top - rect.bottom, rect.top - view.bottom);
}

/// Whether a turn `distancePx` outside the viewport keeps its body.
///
/// Two thresholds, not one: a body is revealed within `marginPx` and only given
/// back beyond `collapsePx`, so a turn sitting on the boundary does not build
/// and tear down its body on every scroll event.
export function decideReveal(
  distancePx: number,
  revealed: boolean,
  marginPx: number = CULL_MARGIN_PX,
  collapsePx: number = COLLAPSE_MARGIN_PX,
): RevealDecision {
  if (!Number.isFinite(distancePx)) return "hold";
  if (distancePx <= marginPx) return revealed ? "hold" : "reveal";
  if (revealed && distancePx > collapsePx) return "collapse";
  return "hold";
}

/// The margin an observer is built with, given whether the body is rendered.
///
/// An IntersectionObserver reports a target only when its intersection with the
/// expanded root *changes*. A turn already inside the wide collapse box reports
/// once, as it crosses that boundary, and never again while it crosses the
/// viewport. Observing at the collapse margin alone therefore left bodies
/// unbuilt as the reader scrolled back up: the only entry arrived 2400 px away,
/// the reveal margin wanted 600 px, nothing revealed it, and no second entry
/// ever came. Measured on a 40-turn thread in a real engine: 0 of 2 turns on
/// screen had a body, from the third wheel step upwards.
///
/// The margin follows the state: an unrendered body is watched where the reader
/// arrives (`marginPx`), a rendered one where the reader leaves (`collapsePx`).
/// The observer is rebuilt on a transition, which is once per crossing and not
/// once per scroll event.
export function observeMargin(
  revealed: boolean,
  marginPx: number = CULL_MARGIN_PX,
  collapsePx: number = COLLAPSE_MARGIN_PX,
): number {
  return revealed ? collapsePx : marginPx;
}

export type CullOptions = {
  /// The scrolling element. A turn is measured against it rather than against
  /// the window: a turn 3000 px up a thread is not visible only because the
  /// window is tall.
  root: HTMLElement | null;
  onReveal: () => void;
  /// The body is far behind the reader and can be given back.
  onCollapse?: () => void;
  /// Whether the body is rendered right now. The component owns that state; the
  /// observer only decides what it should be.
  isRevealed?: () => boolean;
  marginPx?: number;
  collapsePx?: number;
};

/// Reveals a turn when it comes near the viewport, and gives the body back when
/// it is left far behind.
export function cull(
  node: HTMLElement,
  options: CullOptions,
): { update: (next: CullOptions) => void; destroy: () => void } {
  let current = options;
  let observer: IntersectionObserver | null = null;
  let destroyed = false;
  /// The margin the live observer was built with. The observer has to be
  /// rebuilt when a transition changes it (see `observeMargin`).
  let observedMargin: number | null = null;

  function margin(): number {
    return current.marginPx ?? CULL_MARGIN_PX;
  }

  function collapseAt(): number {
    return current.collapsePx ?? COLLAPSE_MARGIN_PX;
  }

  /// The margin this turn should be observed at right now.
  function marginFor(): number {
    return observeMargin(current.isRevealed?.() ?? false, margin(), collapseAt());
  }

  /// Rebuild the observer when a reveal or a collapse changed the margin it
  /// watches. This runs once per crossing, not once per scroll event.
  function resync(): void {
    if (destroyed) return;
    if (observedMargin === marginFor()) return;
    observe();
  }

  /// The visible box, in viewport coordinates. The scroller is bound after the
  /// first render; before that the window is the viewport.
  function view(): Box | null {
    if (current.root !== null) {
      const box = current.root.getBoundingClientRect();
      return { top: box.top, bottom: box.bottom };
    }
    if (typeof window === "undefined") return null;
    if (!Number.isFinite(window.innerHeight)) return null;
    return { top: 0, bottom: window.innerHeight };
  }

  function judge(entry: IntersectionObserverEntry): void {
    if (destroyed) return;
    const box = view();
    if (box === null) {
      // No geometry to measure: fall back to the browser's own overlap test,
      // rather than leaving a turn that is on screen unrendered.
      if (entry.isIntersecting && !(current.isRevealed?.() ?? false)) {
        current.onReveal();
        resync();
      }
      return;
    }
    const distance = gapFromViewport(box, entry.boundingClientRect);
    const decision = decideReveal(
      distance,
      current.isRevealed?.() ?? false,
      margin(),
      collapseAt(),
    );
    if (decision === "reveal") current.onReveal();
    else if (decision === "collapse") current.onCollapse?.();
    else return;
    resync();
  }

  function observe(): void {
    observer?.disconnect();
    observer = null;
    if (destroyed) return;
    if (typeof IntersectionObserver === "undefined") {
      current.onReveal();
      return;
    }
    // The margin depends on whether a body is rendered: an unrendered one is
    // watched where the reader arrives, a rendered one where the reader leaves
    // (see `observeMargin`). The entry is judged against the viewport, not
    // against this expanded box.
    observedMargin = marginFor();
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) judge(entry);
      },
      { root: current.root, rootMargin: `${observedMargin}px 0px` },
    );
    observer.observe(node);
  }

  observe();

  return {
    update(next: CullOptions) {
      const rootChanged = next.root !== current.root;
      current = next;
      // The scroller is bound after the first render, so the first observation
      // is against the window; re-observing once it arrives is the whole point.
      if (rootChanged) observe();
    },
    destroy() {
      destroyed = true;
      observer?.disconnect();
      observer = null;
    },
  };
}
