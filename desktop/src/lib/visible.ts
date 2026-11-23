/// Whether the window is on screen at all.
///
/// Work that only refreshes the view — sampling streamed text, polling the
/// background rows, ticking the elapsed clock — is skipped while the window is
/// minimized or fully covered: a window nobody can see should not cost a core.
/// The kernel keeps running either way; this only stops the painting.

import { readable } from "svelte/store";

/// `document.hidden` is the browser's own answer and is what WebKit implements;
/// the visibility state is the fallback for an engine that only has that.
export function isHidden(
  doc: Pick<Document, "hidden" | "visibilityState"> | null | undefined,
): boolean {
  if (!doc) return false;
  if (typeof doc.hidden === "boolean") return doc.hidden;
  return doc.visibilityState === "hidden";
}

export const visible = readable(true, (set) => {
  if (typeof document === "undefined") return;
  set(!isHidden(document));
  const update = (): void => set(!isHidden(document));
  document.addEventListener("visibilitychange", update);
  // A window that is dragged between screens or restored fires `resume` on
  // some builds without a visibility change; asking again is cheap.
  window.addEventListener("focus", update);
  return () => {
    document.removeEventListener("visibilitychange", update);
    window.removeEventListener("focus", update);
  };
});
