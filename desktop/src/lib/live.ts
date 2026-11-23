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
