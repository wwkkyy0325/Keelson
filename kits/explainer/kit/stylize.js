// Plates: a photograph turned into a drawing, a painting or a print.
//
// Procedural motifs cannot draw a face, an animal or a nebula — that is a
// vocabulary limit, not a talent limit. A plate is the other half: take a real
// picture, restyle it once into something that belongs to the film, then move a
// camera over it. Restyling is a per-pixel pass, so it runs once in
// `SCENE.prepare()` and every frame just draws the cached canvas.
//
// A scene that uses a plate:
//
//   let plate;
//   window.SCENE = {
//     async prepare() {
//       const img = await STYLIZE.load("/assets/nebula.jpg");
//       plate = STYLIZE.oil(img, { levels: 7, scale: 0.5 });
//     },
//     draw(ctx, t) {
//       STYLIZE.drawPlate(ctx, plate, t, { zoom: [1.0, 1.12], pan: [0, -30] });
//     },
//   };
//
// `prepare()` is awaited by the page before the first frame; without it no
// frame is rendered on a plate that is still loading.
(function () {
  const S = {};

  function canvasOf(width, height) {
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(width));
    canvas.height = Math.max(1, Math.round(height));
    return canvas;
  }

  /** Loads an image from the project's `assets/` (or any URL the page can read). */
  S.load = function (src) {
    return new Promise((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () => reject(new Error("cannot load plate " + src));
      image.src = src;
    });
  };

  function scaled(image, scale) {
    const canvas = canvasOf(image.width * scale, image.height * scale);
    canvas.getContext("2d").drawImage(image, 0, 0, canvas.width, canvas.height);
    return canvas;
  }

  function posterize(ctx, width, height, levels) {
    const frame = ctx.getImageData(0, 0, width, height);
    const data = frame.data;
    const step = 255 / Math.max(1, levels - 1);
    for (let i = 0; i < data.length; i += 4) {
      data[i] = Math.round(data[i] / step) * step;
      data[i + 1] = Math.round(data[i + 1] / step) * step;
      data[i + 2] = Math.round(data[i + 2] / step) * step;
    }
    ctx.putImageData(frame, 0, 0);
  }

  function luminance(frame) {
    const { data } = frame;
    const grey = new Float32Array(data.length / 4);
    for (let i = 0, p = 0; i < data.length; i += 4, p++) {
      grey[p] = 0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2];
    }
    return grey;
  }

  /**
   * Painting: flatten the tones, keep the drawing, then put the paper back on
   * top. `levels` is how many tones survive; `edge` how hard the ink is.
   */
  S.oil = function (image, opts = {}) {
    const scale = opts.scale || 0.55;
    const levels = opts.levels || 7;
    const base = scaled(image, scale);
    const ctx = base.getContext("2d");
    posterize(ctx, base.width, base.height, levels);
    const out = canvasOf(image.width, image.height);
    const octx = out.getContext("2d");
    octx.imageSmoothingEnabled = true;
    octx.imageSmoothingQuality = "high";
    octx.drawImage(base, 0, 0, out.width, out.height);
    if (opts.ink !== false) {
      const edges = S.sketch(image, { scale, invert: true, strength: opts.edge || 1 });
      octx.globalAlpha = opts.ink === undefined ? 0.35 : opts.ink;
      octx.globalCompositeOperation = "multiply";
      octx.drawImage(edges, 0, 0, out.width, out.height);
      octx.globalAlpha = 1;
      octx.globalCompositeOperation = "source-over";
    }
    return out;
  };

  /**
   * Drawing: grayscale, Sobel edges. `invert` (the default) puts dark ink on
   * white paper, which is what `oil` multiplies back in; without it the edges
   * glow white on black.
   */
  S.sketch = function (image, opts = {}) {
    const scale = opts.scale || 0.55;
    const base = scaled(image, scale);
    const ctx = base.getContext("2d");
    const frame = ctx.getImageData(0, 0, base.width, base.height);
    const grey = luminance(frame);
    const width = base.width;
    const height = base.height;
    const out = ctx.createImageData(width, height);
    const invert = opts.invert !== false;
    const strength = opts.strength || 1;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const at = (xx, yy) =>
          grey[Math.min(height - 1, Math.max(0, yy)) * width + Math.min(width - 1, Math.max(0, xx))];
        const gx =
          -at(x - 1, y - 1) - 2 * at(x - 1, y) - at(x - 1, y + 1) +
          at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1);
        const gy =
          -at(x - 1, y - 1) - 2 * at(x, y - 1) - at(x + 1, y - 1) +
          at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1);
        let edge = Math.min(255, Math.sqrt(gx * gx + gy * gy) * strength);
        if (invert) edge = 255 - edge;
        const i = (y * width + x) * 4;
        out.data[i] = edge;
        out.data[i + 1] = edge;
        out.data[i + 2] = edge;
        out.data[i + 3] = 255;
      }
    }
    ctx.putImageData(out, 0, 0);
    const full = canvasOf(image.width, image.height);
    const fctx = full.getContext("2d");
    fctx.imageSmoothingEnabled = true;
    fctx.imageSmoothingQuality = "high";
    fctx.drawImage(base, 0, 0, full.width, full.height);
    return full;
  };

  /** Two inks: shadows in `dark`, lights in `light`. */
  S.duotone = function (image, dark, light, opts = {}) {
    const base = scaled(image, opts.scale || 0.6);
    const ctx = base.getContext("2d");
    const frame = ctx.getImageData(0, 0, base.width, base.height);
    const grey = luminance(frame);
    const d = hex(dark);
    const l = hex(light);
    for (let p = 0, i = 0; p < grey.length; p++, i += 4) {
      const k = grey[p] / 255;
      frame.data[i] = d[0] + (l[0] - d[0]) * k;
      frame.data[i + 1] = d[1] + (l[1] - d[1]) * k;
      frame.data[i + 2] = d[2] + (l[2] - d[2]) * k;
      frame.data[i + 3] = 255;
    }
    ctx.putImageData(frame, 0, 0);
    const full = canvasOf(image.width, image.height);
    full.getContext("2d").drawImage(base, 0, 0, full.width, full.height);
    return full;
  };

  function hex(color) {
    const match = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color);
    if (!match) return [0, 0, 0];
    let value = match[1];
    if (value.length === 3) value = value.split("").map((c) => c + c).join("");
    return [0, 2, 4].map((i) => parseInt(value.slice(i, i + 2), 16));
  }

  /**
   * Draws a prepared plate as the far plane of a shot: it covers the frame,
   * and the camera drifts across it so the still reads as motion.
   */
  S.drawPlate = function (ctx, plate, t, opts = {}) {
    const W = D_WIDTH();
    const H = D_HEIGHT();
    const zoom = opts.zoom || [1.02, 1.1];
    const pan = opts.pan || [0, 0];
    const p = Math.min(1, Math.max(0, t / (opts.duration || 6)));
    const z = zoom[0] + (zoom[1] - zoom[0]) * p;
    const dx = pan[0] * p;
    const dy = pan[1] * p;
    const cover = Math.max(W / plate.width, H / plate.height) * z;
    const w = plate.width * cover;
    const h = plate.height * cover;
    ctx.save();
    if (opts.alpha !== undefined) ctx.globalAlpha = opts.alpha;
    ctx.drawImage(plate, (W - w) / 2 + dx, (H - h) / 2 + dy, w, h);
    ctx.restore();
  };

  function D_WIDTH() {
    return window.DESIGN ? window.DESIGN.W : 1920;
  }

  function D_HEIGHT() {
    return window.DESIGN ? window.DESIGN.H : 1080;
  }

  window.STYLIZE = S;
})();
