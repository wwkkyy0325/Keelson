/// How often the streaming blocks may be re-rendered while an answer grows.
///
/// Every sample parses the markdown, sanitizes the HTML and lays the replaced
/// block out again. Measured in a browser on realistic markdown (headings,
/// tables, fenced code, lists), one sample costs:
///
/// | answer | parse + sanitize + layout |
/// |--------|---------------------------|
/// | 1 KB   | 0.6 ms                    |
/// | 5 KB   | 2.1 ms                    |
/// | 20 KB  | 7.8 ms                    |
/// | 50 KB  | 18.5 ms                   |
/// | 100 KB | 37.9 ms                   |
///
/// At a fixed 80 ms window a 100 KB answer spends about half of one core on
/// re-rendering itself, and the frame it blocks is 38 ms long. The window
/// therefore follows the length: the sample stays a tenth of the time, the
/// floor keeps short answers smooth, and the ceiling keeps a very long answer
/// from looking frozen.
export const LIVE_FLUSH_MIN_MS = 80;
export const LIVE_FLUSH_MAX_MS = 320;

/// Roughly the measured cost per character, so a sample is ~10% of the time.
const MS_PER_CHAR = 0.0035;

export function flushDelay(chars: number): number {
  if (!Number.isFinite(chars) || chars <= 0) return LIVE_FLUSH_MIN_MS;
  const scaled = Math.round(chars * MS_PER_CHAR);
  return Math.min(LIVE_FLUSH_MAX_MS, Math.max(LIVE_FLUSH_MIN_MS, scaled));
}

/// Everything that can move the bottom of the transcript: a block that was just
/// rendered, a new tool row, a new turn, a new background row, an error — and
/// the running turn itself.
///
/// The running turn is the one that made a send look lost. The live card is
/// appended the moment the turn starts, with the user's text in it, but nothing
/// in this figure changed when it appeared: the turn count stays where it was
/// until the turn is persisted, and the answer, the tool rows and the
/// background rows are all still empty. The follow therefore did not scroll,
/// and a message sent with the thread already at its end landed below the fold
/// — the transcript looked as if the send had gone nowhere until the finished
/// turn raised the count and the follow caught up.
///
/// The source is the drawing, not the store: reading `scrollHeight` per delta
/// would lay out every rendered turn once per chunk instead of once per
/// rendered change, so the caller passes lengths and ids, not elements.
export type BottomParts = {
  /// Characters of the answer being streamed.
  answerChars: number;
  /// Characters of the thinking block, when it is open.
  reasoningChars: number;
  /// Persisted turns in the transcript.
  turns: number;
  /// The running turn's sequence, or null when none runs. A fresh turn brings a
  /// new number, which is what moves the bottom when the card appears.
  runningSeq: number | null;
  /// Characters of the running turn's input, so a card rebuilt from the
  /// engine's view at a different length is not mistaken for the same one.
  runningInputChars: number;
  /// Tool rows of the running turn.
  tools: number;
  /// Background rows under the transcript.
  background: number;
  /// The error banner, empty when there is none.
  error: string;
};

export function bottomKey(parts: BottomParts): string {
  return [
    parts.answerChars,
    parts.reasoningChars,
    parts.turns,
    parts.runningSeq ?? 0,
    parts.runningInputChars,
    parts.tools,
    parts.background,
    parts.error,
  ].join(":");
}
