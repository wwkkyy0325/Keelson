// Trace a reference: outline, silhouette, and a cut at the neck.
//
// The order matters and it is the only order that works for a complex
// subject: bring the picture, trace its edge, clean the trace, split it at a
// joint, then bind the pieces. Nothing here invents a shape — the silhouette
// comes from the picture's own pixels.
//
// The silhouette test is a background-distance threshold, which is why the
// reference should have a reasonably plain background. When it fails, the
// scene says so instead of guessing.
(function () {
  const T = {};

  function canvasOf(width, height) {
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(width));
    canvas.height = Math.max(1, Math.round(height));
    return canvas;
  }

  /** White ink on transparent, from a Sobel edge over the luminance. */
  T.outline = function (image, opts) {
    const o = Object.assign({ strength: 1.4 }, opts);
    const base = canvasOf(image.width, image.height);
    const ctx = base.getContext("2d");
    ctx.drawImage(image, 0, 0);
    const frame = ctx.getImageData(0, 0, base.width, base.height);
    const data = frame.data;
    const width = base.width;
    const height = base.height;
    const grey = new Float32Array(width * height);
    for (let i = 0, p = 0; i < data.length; i += 4, p++) {
      grey[p] = 0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2];
    }
    const out = ctx.createImageData(width, height);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const at = function (xx, yy) {
          return grey[
            Math.min(height - 1, Math.max(0, yy)) * width +
            Math.min(width - 1, Math.max(0, xx))
          ];
        };
        const gx =
          -at(x - 1, y - 1) - 2 * at(x - 1, y) - at(x - 1, y + 1) +
          at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1);
        const gy =
          -at(x - 1, y - 1) - 2 * at(x, y - 1) - at(x + 1, y - 1) +
          at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1);
        const edge = Math.min(255, Math.sqrt(gx * gx + gy * gy) * o.strength);
        const i = (y * width + x) * 4;
        out.data[i] = 255;
        out.data[i + 1] = 245;
        out.data[i + 2] = 230;
        out.data[i + 3] = edge;
      }
    }
    ctx.putImageData(out, 0, 0);
    return base;
  };

  /**
   * Foreground mask by distance from the border colour. Returns the mask as
   * 0/1 bytes plus its bounds, or null when the picture has no usable
   * separation.
   */
  T.silhouette = function (image, opts) {
    const o = Object.assign({ threshold: 46, clean: true }, opts);
    const base = canvasOf(image.width, image.height);
    const ctx = base.getContext("2d");
    ctx.drawImage(image, 0, 0);
    const width = base.width;
    const height = base.height;
    const data = ctx.getImageData(0, 0, width, height).data;

    // The background is read from the two top corners. A full border sample
    // is wrong the moment the subject reaches the bottom of the frame — a
    // shoulder line does exactly that, and the average lands on the subject.
    let r = 0, g = 0, b = 0, count = 0;
    const patch = function (x0, y0) {
      for (let y = y0; y < y0 + 12; y++) {
        for (let x = x0; x < x0 + 12; x++) {
          const i = (Math.min(height - 1, y) * width + Math.min(width - 1, x)) * 4;
          r += data[i];
          g += data[i + 1];
          b += data[i + 2];
          count++;
        }
      }
    };
    patch(4, 4);
    patch(width - 16, 4);
    r /= count;
    g /= count;
    b /= count;

    let mask = new Uint8Array(width * height);
    for (let p = 0, i = 0; p < mask.length; p++, i += 4) {
      const dr = data[i] - r;
      const dg = data[i + 1] - g;
      const db = data[i + 2] - b;
      mask[p] = Math.sqrt(dr * dr + dg * dg + db * db) > o.threshold ? 1 : 0;
    }

    if (o.clean) {
      const cleaned = new Uint8Array(width * height);
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          let sum = 0;
          for (let dy = -1; dy <= 1; dy++) {
            for (let dx = -1; dx <= 1; dx++) {
              const xx = Math.min(width - 1, Math.max(0, x + dx));
              const yy = Math.min(height - 1, Math.max(0, y + dy));
              sum += mask[yy * width + xx];
            }
          }
          cleaned[y * width + x] = sum >= 5 ? 1 : 0;
        }
      }
      mask = cleaned;

      // One pixel of erosion: the edge of the mask carries a little of the
      // background colour, and a blue fringe on dark hair is the first thing
      // the eye finds.
      const eroded = new Uint8Array(width * height);
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          if (!mask[y * width + x]) continue;
          const left = x > 0 ? mask[y * width + x - 1] : 0;
          const right = x < width - 1 ? mask[y * width + x + 1] : 0;
          const up = y > 0 ? mask[(y - 1) * width + x] : 0;
          const down = y < height - 1 ? mask[(y + 1) * width + x] : 0;
          eroded[y * width + x] = left && right && up && down ? 1 : 0;
        }
      }
      mask = eroded;
    }

    let left = width, right = -1, top = height, bottom = -1, area = 0;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        if (!mask[y * width + x]) continue;
        area++;
        if (x < left) left = x;
        if (x > right) right = x;
        if (y < top) top = y;
        if (y > bottom) bottom = y;
      }
    }
    if (right < left || bottom < top) return null;
    const share = area / (width * height);
    if (share < 0.04 || share > 0.92) return null;

    return { mask, width, height, left, right, top, bottom, canvas: base };
  };

  /**
   * The narrowest row between the chin and the chest, measured across the
   * silhouette. That row is the neck; the cut goes there.
   */
  T.neck = function (silhouette) {
    const { mask, width, top, bottom } = silhouette;
    const from = Math.round(top + (bottom - top) * 0.3);
    const to = Math.round(top + (bottom - top) * 0.62);
    let best = { y: Math.round((from + to) / 2), width: Infinity, centre: width / 2 };
    for (let y = from; y <= to; y++) {
      let left = width, right = -1;
      for (let x = 0; x < width; x++) {
        if (!mask[y * width + x]) continue;
        if (x < left) left = x;
        if (x > right) right = x;
      }
      if (right < left) continue;
      const span = right - left;
      if (span < best.width) best = { y: y, width: span, centre: (left + right) / 2 };
    }
    return best;
  };

  /**
   * Two pieces, cut at `neckY`: the head above and the body below, each an
   * RGBA canvas the size of the plate with the silhouette baked in. Nothing
   * outside the silhouette survives.
   */
  T.pieces = function (plate, silhouette, neckY) {
    const width = plate.width;
    const height = plate.height;
    const source = plate.getContext("2d").getImageData(0, 0, width, height);
    const scaleX = silhouette.width / width;
    const scaleY = silhouette.height / height;

    const make = function (from, to) {
      const canvas = canvasOf(width, height);
      const ctx = canvas.getContext("2d");
      const frame = ctx.createImageData(width, height);
      for (let y = from; y < to; y++) {
        const maskRow = Math.min(silhouette.height - 1, Math.round(y * scaleY)) * silhouette.width;
        for (let x = 0; x < width; x++) {
          const target = (y * width + x) * 4;
          if (!silhouette.mask[maskRow + Math.min(silhouette.width - 1, Math.round(x * scaleX))]) continue;
          const sourceAt = (y * width + x) * 4;
          frame.data[target] = source.data[sourceAt];
          frame.data[target + 1] = source.data[sourceAt + 1];
          frame.data[target + 2] = source.data[sourceAt + 2];
          frame.data[target + 3] = 255;
        }
      }
      ctx.putImageData(frame, 0, 0);
      return canvas;
    };

    // The head keeps a few rows below the joint. Without that overlap a two
    // pixel bob opens a hairline across the neck.
    const cut = Math.round(neckY);
    return {
      head: make(0, Math.min(height, cut + 10)),
      body: make(cut, height),
      neckY: cut,
    };
  };

  window.TRACE = T;
})();
