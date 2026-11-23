// Whether the transcript keeps following the newest row. Pure, so the rule can
// be checked without a browser (follow.test.ts).

export interface ScrollSample {
  top: number;
  height: number;
}

/// How close to the end counts as "at the bottom". A wheel notch is smaller
/// than this band, which is why reaching the band alone does not mean the user
/// is still following.
export const FOLLOW_BAND_PX = 80;

export function atBottom(
  top: number,
  height: number,
  viewport: number,
  band = FOLLOW_BAND_PX,
): boolean {
  return height - top - viewport < band;
}

/// A scroll event can mean the user moved up, or that the content shrank under
/// a position the browser had to clamp: a background row that finished and
/// disappeared, a fold that closed, a placeholder that collapsed. The second
/// kind says nothing about where the user wants to look, so it must not stop
/// the follow — that is how the newest tool rows used to stay below the fold
/// for the rest of a turn.
///
/// A wheel releases the follow before the browser scrolls (Transcript.onWheel).
/// This covers the rest: a drag, a keyboard page, a trackpad flick.
export function followAfterScroll(
  previous: ScrollSample,
  top: number,
  height: number,
  viewport: number,
  band = FOLLOW_BAND_PX,
): boolean {
  if (height >= previous.height && top < previous.top) return false;
  return atBottom(top, height, viewport, band);
}
