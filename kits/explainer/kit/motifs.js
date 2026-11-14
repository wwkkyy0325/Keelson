// Motifs: the moving picture layer.
//
// The text helpers in `design.js` (headline, body, panel, pill) are stills. A
// scene composed only from them reads as a slide no matter how it is
// arranged. These helpers draw what moves underneath the labels — water,
// ground, people, a drawn path — so a scene has something happening in the
// frame even when the narration is one sentence.
//
// Every helper is a pure function of (t, seed): the same t renders the same
// frame, which is what the renderer relies on. Colours come from the style
// tokens (`D.C`, `D.alpha`); a scene never passes a literal.
(function () {
  const D = window.DESIGN;
  const { C, W, H } = D;

  /** Deterministic 0..1 sequence, so a crowd keeps its shape between frames. */
  function rng(seed) {
    let state = (seed >>> 0) || 1;
    return function () {
      state = (state * 1664525 + 1013904223) >>> 0;
      return state / 4294967296;
    };
  }

  function line(ctx, x1, y1, x2, y2) {
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.stroke();
  }

  /**
   * Layered water. Each band is the same wave drawn lower, slower and paler,
   * so the stack reads as depth rather than as repetition.
   *
   * opts: layers, amp, gap, y (first crest), speed, phase, opacity, colors
   */
  function waves(ctx, t, opts = {}) {
    const layers = opts.layers || 5;
    const amp = opts.amp === undefined ? 54 : opts.amp;
    const gap = opts.gap === undefined ? 56 : opts.gap;
    const base = opts.y === undefined ? H * 0.54 : opts.y;
    const speed = opts.speed === undefined ? 0.55 : opts.speed;
    const phase = opts.phase || 0;
    const colors = opts.colors || [C.accent2, C.accent, C.accent3, C.ink];
    for (let i = layers - 1; i >= 0; i--) {
      const depth = i / Math.max(1, layers - 1);
      const crest = base + i * gap;
      const a = amp * (0.5 + 0.5 * (1 - depth));
      const k = 0.0042 * (1 + depth * 0.6);
      const drift = t * speed * (1 - depth * 0.5) + phase + i * 0.9;
      ctx.save();
      ctx.beginPath();
      ctx.moveTo(-80, H + 80);
      for (let x = -80; x <= W + 80; x += 12) {
        const y =
          crest +
          Math.sin(x * k + drift) * a +
          Math.sin(x * k * 2.7 + drift * 0.7 + i) * a * 0.26;
        ctx.lineTo(x, y);
      }
      ctx.lineTo(W + 80, H + 80);
      ctx.closePath();
      ctx.fillStyle = D.alpha(
        colors[i % colors.length],
        opts.opacity === undefined ? 0.22 - depth * 0.07 : opts.opacity,
      );
      ctx.fill();
      // A lighter line on the crest is what makes the stack read as water
      // rather than as bands of colour.
      if (opts.crest !== false) {
        ctx.strokeStyle = D.alpha(opts.crestColor || C.cream, 0.5);
        ctx.lineWidth = Math.max(1, 3 - depth * 2);
        ctx.beginPath();
        for (let x = -80; x <= W + 80; x += 12) {
          const y =
            crest +
            Math.sin(x * k + drift) * a +
            Math.sin(x * k * 2.7 + drift * 0.7 + i) * a * 0.26;
          if (x === -80) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
      ctx.restore();
    }
  }

  /**
   * Ground silhouettes: rolling shapes that drift at a parallax speed, back
   * layers first. Use it to put the scene somewhere instead of on paper.
   *
   * opts: layers, amp, wavelength, y, speed, opacity, color
   */
  function hills(ctx, t, opts = {}) {
    const layers = opts.layers || 3;
    const amp = opts.amp === undefined ? 90 : opts.amp;
    const wavelength = opts.wavelength || 1400;
    const base = opts.y === undefined ? H * 0.66 : opts.y;
    const speed = opts.speed === undefined ? 14 : opts.speed;
    const color = opts.color || C.ink;
    for (let i = layers - 1; i >= 0; i--) {
      const depth = i / Math.max(1, layers - 1);
      const k = (Math.PI * 2) / (wavelength * (0.7 + depth * 0.9));
      const drift = t * speed * (0.4 + depth * 0.8) + i * 240;
      ctx.save();
      ctx.beginPath();
      ctx.moveTo(-80, H + 80);
      for (let x = -80; x <= W + 80; x += 16) {
        const y =
          base + i * 46 + Math.sin(x * k + drift) * amp * (0.5 + depth * 0.5);
        ctx.lineTo(x, y);
      }
      ctx.lineTo(W + 80, H + 80);
      ctx.closePath();
      ctx.fillStyle = D.alpha(color, (opts.opacity || 0.12) * (1.4 - depth));
      ctx.fill();
      ctx.restore();
    }
  }

  /**
   * One walking figure. `phase` advances with distance travelled, so the same
   * call from a crowd and from a solo walker keeps the same stride.
   *
   * opts: color, stride, weight, flip
   */
  function walker(ctx, x, y, scale, phase, opts = {}) {
    const color = opts.color || C.ink;
    const stride = opts.stride === undefined ? 0.6 : opts.stride;
    const s = scale;
    const foot = y;
    const hip = -30 * s;
    const shoulder = -62 * s;
    const head = -76 * s;
    const a = Math.sin(phase) * stride;
    const b = Math.sin(phase + Math.PI) * stride;
    ctx.save();
    ctx.translate(x, foot);
    if (opts.flip) ctx.scale(-1, 1);
    ctx.rotate(Math.sin(phase) * 0.03);
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineCap = "round";
    // A silhouette, not a stick: the torso and the limbs are heavy enough to
    // read as a body at a glance.
    const limb = Math.max(2.5, (opts.weight || 1) * 7 * s);
    ctx.lineWidth = limb * 1.2;
    line(ctx, 0, hip, Math.sin(a) * 26 * s, 0);
    line(ctx, 0, hip, Math.sin(b) * 26 * s, 0);
    ctx.lineWidth = limb * 2.6;
    line(ctx, 0, hip + 2 * s, 0, shoulder);
    ctx.lineWidth = limb;
    line(ctx, 0, shoulder + 6 * s, Math.sin(b) * 20 * s, -24 * s);
    line(ctx, 0, shoulder + 6 * s, Math.sin(a) * 20 * s, -24 * s);
    ctx.beginPath();
    ctx.arc(0, head, 10.5 * s, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  /**
   * People crossing the frame. Depth is a scale and an alpha, not a second
   * drawing: back rows are smaller, paler and slower.
   *
   * opts: count, y, speed, seed, scale, rows, dir, color
   */
  function crowd(ctx, t, opts = {}) {
    const seed = opts.seed || 7;
    const random = rng(seed);
    const rows = opts.rows || 2;
    const count = opts.count || 8;
    const baseY = opts.y === undefined ? H * 0.78 : opts.y;
    const speed = opts.speed === undefined ? 46 : opts.speed;
    const dir = opts.dir === -1 ? -1 : 1;
    const color = opts.color || C.ink;
    const unit = opts.scale === undefined ? 1 : opts.scale;
    const span = W + 260;
    const people = [];
    for (let i = 0; i < count * rows; i++) {
      people.push({
        start: random() * span,
        lane: Math.floor(random() * rows),
        scale: 0.72 + random() * 0.6,
        pace: 0.7 + random() * 0.6,
        bob: random() * Math.PI * 2,
      });
    }
    people.sort((p, q) => q.lane - p.lane);
    for (const p of people) {
      const depth = p.lane / Math.max(1, rows - 1 || 1);
      const s = p.scale * unit * (1 - depth * 0.22);
      const travelled = (p.start + t * speed * p.pace * dir + span) % span;
      const x = dir > 0 ? travelled - 130 : W + 130 - travelled;
      const y =
        baseY - p.lane * 54 * unit + Math.sin(t * 2.2 * p.pace + p.bob) * 2;
      ctx.save();
      ctx.globalAlpha = 0.92 - depth * 0.3;
      walker(ctx, x, y, s, x / (26 * s), { color, flip: dir > 0 });
      ctx.restore();
    }
  }

  /**
   * Draw-on stroke: a polyline revealed up to `progress` by arc length, the
   * way a pen would. Points are flat: [x0, y0, x1, y1, ...].
   *
   * opts: color, width, cap, dashTail
   */
  function stroke(ctx, points, progress, opts = {}) {
    const p = D.clamp01(progress);
    if (p <= 0 || points.length < 4) return;
    const segments = [];
    let total = 0;
    for (let i = 0; i + 3 < points.length; i += 2) {
      const dx = points[i + 2] - points[i];
      const dy = points[i + 3] - points[i + 1];
      const len = Math.hypot(dx, dy);
      segments.push({ x: points[i], y: points[i + 1], dx, dy, len });
      total += len;
    }
    let left = total * p;
    ctx.save();
    ctx.strokeStyle = opts.color || C.ink;
    ctx.lineWidth = opts.width || 4;
    ctx.lineCap = opts.cap || "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(points[0], points[1]);
    for (const seg of segments) {
      if (left <= 0) break;
      const f = Math.min(1, left / seg.len);
      ctx.lineTo(seg.x + seg.dx * f, seg.y + seg.dy * f);
      left -= seg.len;
    }
    ctx.stroke();
    ctx.restore();
  }

  /**
   * Slow drift and fall. `count` is fixed by the seed, so a particle does not
   * appear mid-shot.
   *
   * opts: count, seed, speed, size, color, opacity, from, to
   */
  function particles(ctx, t, opts = {}) {
    const random = rng(opts.seed || 11);
    const count = opts.count || 90;
    const speed = opts.speed === undefined ? 18 : opts.speed;
    const size = opts.size === undefined ? 2.4 : opts.size;
    const from = opts.from === undefined ? 0 : opts.from;
    const to = opts.to === undefined ? H : opts.to;
    const travel = to - from;
    ctx.save();
    ctx.fillStyle = D.alpha(opts.color || C.ink, opts.opacity === undefined ? 0.22 : opts.opacity);
    for (let i = 0; i < count; i++) {
      const x0 = random() * W;
      const sway = 8 + random() * 26;
      const rate = 0.4 + random() * 0.9;
      const y = from + ((random() * travel + t * speed * rate) % travel);
      const x = x0 + Math.sin(t * 0.7 * rate + i) * sway;
      ctx.beginPath();
      ctx.arc(x, y, size * (0.6 + random() * 0.8), 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }

  /**
   * A camera around a drawing: push in, pan or rotate while `draw` runs. The
   * transform is anchored at the frame centre, so a scene keeps its margins.
   *
   * opts: from {x, y, zoom, rot}, to {x, y, zoom, rot}, ease
   */
  function camera(ctx, t, opts, draw) {
    const from = Object.assign({ x: 0, y: 0, zoom: 1, rot: 0 }, opts.from || {});
    const to = Object.assign(from, opts.to || {});
    const p = D.clamp01((opts.ease || D.easeInOut)(t));
    const mix = (a, b) => a + (b - a) * p;
    ctx.save();
    ctx.translate(W / 2 + mix(from.x, to.x), H / 2 + mix(from.y, to.y));
    ctx.rotate(mix(from.rot, to.rot));
    const zoom = mix(from.zoom, to.zoom);
    ctx.scale(zoom, zoom);
    ctx.translate(-W / 2, -H / 2);
    draw(ctx);
    ctx.restore();
  }

  Object.assign(D, {
    waves,
    hills,
    walker,
    crowd,
    stroke,
    particles,
    camera,
    rng,
  });
})();
