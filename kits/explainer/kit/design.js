// The design engine: one set of helpers, one active style.
//
// A style preset (kit/style.js) sets `window.STYLE`; this file reads it and
// exposes tokens and drawing helpers. Scenes use only these helpers and
// `D.C`/`D.alpha`, so the same scene code works under any preset. Never put a
// colour, font or radius literal in a scene.
(function () {
  const W = 1920;
  const H = 1080;
  const M = 112; // safe margin

  const PRESET = window.STYLE || {};

  const DEFAULT_COLORS = {
    paper: "#F3EDE1",
    paperDeep: "#E7DECC",
    ink: "#191A1E",
    muted: "#7C776C",
    line: "#CDC3AF",
    accent: "#D14B2C",
    accent2: "#2B6A63",
    accent3: "#D9A441",
    cream: "#FFFBF2",
  };
  const COLORS = Object.assign({}, DEFAULT_COLORS, PRESET.colors || {});
  const C = Object.assign({}, COLORS);
  // Older scenes use the palette names of the paper kit.
  C.vermilion = C.vermilion || C.accent;
  C.teal = C.teal || C.accent2;
  C.gold = C.gold || C.accent3;
  C.accent = C.accent || C.vermilion;
  C.accent2 = C.accent2 || C.teal;
  C.accent3 = C.accent3 || C.gold;

  const FONTS = Object.assign(
    {
      cn: '"Microsoft YaHei", "Microsoft YaHei UI", "Noto Sans SC", sans-serif',
      num: '"Segoe UI", "Microsoft YaHei", sans-serif',
      display: null,
      quote: null,
    },
    PRESET.fonts || {},
  );
  const FONT_CN = FONTS.cn;
  const FONT_NUM = FONTS.num;
  const FONT_DISPLAY = FONTS.display || FONT_CN;
  const FONT_QUOTE = FONTS.quote || FONT_DISPLAY;

  const STYLE = {
    name: PRESET.name || "paper-editorial",
    background: PRESET.background || "dotgrid",
    kicker: PRESET.kicker || "square",
    mascot: PRESET.mascot || "cat",
    radius: PRESET.radius === undefined ? 18 : PRESET.radius,
    lineWidth: PRESET.lineWidth || 3,
    headlineWeight: PRESET.headlineWeight || 700,
  };

  function alpha(color, opacity) {
    const match = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color);
    if (!match) return color;
    let hex = match[1];
    if (hex.length === 3) {
      hex = hex
        .split("")
        .map((part) => part + part)
        .join("");
    }
    const value = parseInt(hex, 16);
    return `rgba(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255}, ${opacity})`;
  }

  function easeOut(t) {
    t = Math.max(0, Math.min(1, t));
    return 1 - Math.pow(1 - t, 3);
  }

  function easeInOut(t) {
    t = Math.max(0, Math.min(1, t));
    return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
  }

  function clamp01(t) {
    return Math.max(0, Math.min(1, t));
  }

  // A small deterministic PRNG for texture; the same frame must render the
  // same pixels.
  function seeded(seed) {
    return function () {
      seed |= 0;
      seed = (seed + 0x6d2b79f5) | 0;
      let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  function dotGrid(ctx) {
    ctx.fillStyle = C.ink;
    ctx.globalAlpha = 0.055;
    for (let y = 40; y < H; y += 48) {
      for (let x = 40; x < W; x += 48) {
        ctx.beginPath();
        ctx.arc(x, y, 1.4, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    ctx.globalAlpha = 1;
    ctx.strokeStyle = C.line;
    ctx.lineWidth = 2;
    ctx.strokeRect(40, 40, W - 80, H - 80);
  }

  function blueprint(ctx) {
    ctx.save();
    ctx.strokeStyle = alpha(C.line, 0.45);
    ctx.lineWidth = 1;
    for (let x = 0; x <= W; x += 48) {
      ctx.beginPath();
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, H);
      ctx.stroke();
    }
    for (let y = 0; y <= H; y += 48) {
      ctx.beginPath();
      ctx.moveTo(0, y + 0.5);
      ctx.lineTo(W, y + 0.5);
      ctx.stroke();
    }
    ctx.strokeStyle = C.accent;
    ctx.lineWidth = STYLE.lineWidth;
    const tick = 44;
    for (const [x, y, dx, dy] of [
      [40, 40, 1, 1],
      [W - 40, 40, -1, 1],
      [40, H - 40, 1, -1],
      [W - 40, H - 40, -1, -1],
    ]) {
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x + dx * tick, y);
      ctx.moveTo(x, y);
      ctx.lineTo(x, y + dy * tick);
      ctx.stroke();
    }
    ctx.restore();
  }

  function grain(ctx) {
    const random = seeded(20261112);
    ctx.save();
    ctx.fillStyle = C.ink;
    for (let i = 0; i < 2200; i++) {
      ctx.globalAlpha = 0.015 + random() * 0.05;
      const x = random() * W;
      const y = random() * H;
      const r = 0.6 + random() * 1.6;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    const vignette = ctx.createRadialGradient(W / 2, H / 2, H * 0.35, W / 2, H / 2, H * 0.95);
    vignette.addColorStop(0, "rgba(0,0,0,0)");
    vignette.addColorStop(1, alpha(C.ink, 0.16));
    ctx.fillStyle = vignette;
    ctx.fillRect(0, 0, W, H);
    ctx.restore();
  }

  function blocks(ctx) {
    ctx.save();
    ctx.fillStyle = alpha(C.accent, 0.09);
    ctx.beginPath();
    ctx.arc(W - 120, 120, 320, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = alpha(C.accent3, 0.10);
    ctx.beginPath();
    ctx.arc(90, H - 80, 250, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = alpha(C.accent2, 0.07);
    ctx.beginPath();
    ctx.arc(W - 260, H - 60, 190, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function swissGrid(ctx) {
    ctx.save();
    ctx.strokeStyle = alpha(C.ink, 0.10);
    ctx.lineWidth = 1;
    for (let i = 0; i <= 12; i++) {
      const x = 40 + (i * (W - 80)) / 12;
      ctx.beginPath();
      ctx.moveTo(x + 0.5, 40);
      ctx.lineTo(x + 0.5, H - 40);
      ctx.stroke();
    }
    for (let i = 0; i <= 8; i++) {
      const y = 40 + (i * (H - 80)) / 8;
      ctx.beginPath();
      ctx.moveTo(40, y + 0.5);
      ctx.lineTo(W - 40, y + 0.5);
      ctx.stroke();
    }
    ctx.strokeStyle = C.ink;
    ctx.lineWidth = 2;
    ctx.strokeRect(40, 40, W - 80, H - 80);
    ctx.restore();
  }

  function bg(ctx) {
    ctx.fillStyle = C.paper;
    ctx.fillRect(0, 0, W, H);
    switch (STYLE.background) {
      case "blueprint":
        blueprint(ctx);
        break;
      case "grain":
        grain(ctx);
        break;
      case "blocks":
        blocks(ctx);
        break;
      case "grid":
        swissGrid(ctx);
        break;
      default:
        dotGrid(ctx);
    }
  }

  function kicker(ctx, text) {
    ctx.save();
    ctx.font = `600 28px ${FONT_CN}`;
    ctx.textBaseline = "middle";
    ctx.letterSpacing = "6px";
    switch (STYLE.kicker) {
      case "rule":
        ctx.fillStyle = C.accent;
        ctx.fillRect(M, 92, 64, 4);
        ctx.fillStyle = C.muted;
        ctx.font = `600 24px ${FONT_CN}`;
        ctx.fillText(text, M, 126);
        break;
      case "tag": {
        const width = ctx.measureText(text).width + 40;
        roundedRect(ctx, M, 76, width, 48, 24);
        ctx.fillStyle = alpha(C.accent, 0.12);
        ctx.fill();
        ctx.fillStyle = C.accent;
        ctx.fillText(text, M + 20, 101);
        break;
      }
      case "serif":
        ctx.beginPath();
        ctx.arc(M + 7, 101, 6, 0, Math.PI * 2);
        ctx.fillStyle = C.accent;
        ctx.fill();
        ctx.fillStyle = C.muted;
        ctx.font = `500 26px ${FONT_QUOTE}`;
        ctx.fillText(text, M + 26, 101);
        break;
      case "index":
        ctx.fillStyle = C.accent;
        ctx.fillRect(M, 91, 8, 20);
        ctx.fillStyle = C.muted;
        ctx.font = `600 26px ${FONT_NUM}`;
        ctx.fillText(text, M + 22, 101);
        break;
      default:
        ctx.fillStyle = C.accent;
        ctx.fillRect(M, 92, 16, 16);
        ctx.fillStyle = C.muted;
        ctx.fillText(text, M + 32, 102);
    }
    ctx.restore();
  }

  function footer(ctx, text) {
    ctx.save();
    ctx.fillStyle = C.muted;
    ctx.globalAlpha = 0.85;
    ctx.font = `500 24px ${FONT_CN}`;
    ctx.textBaseline = "alphabetic";
    ctx.fillText(text, M, H - 72);
    ctx.restore();
  }

  function headline(ctx, lines, x, y, size = 96, color = C.ink) {
    ctx.save();
    ctx.fillStyle = color;
    ctx.font = `${STYLE.headlineWeight} ${size}px ${FONT_DISPLAY}`;
    ctx.textBaseline = "top";
    lines.forEach((line, index) => {
      ctx.fillText(line, x, y + index * size * 1.22);
    });
    ctx.restore();
  }

  function body(ctx, text, x, y, size = 44, color = C.ink) {
    ctx.save();
    ctx.fillStyle = color;
    ctx.font = `500 ${size}px ${FONT_CN}`;
    ctx.textBaseline = "top";
    ctx.fillText(text, x, y);
    ctx.restore();
  }

  function mono(ctx, text, x, y, size = 40, color = C.ink, weight = 600) {
    ctx.save();
    ctx.fillStyle = color;
    ctx.font = `${weight} ${size}px ${FONT_NUM}`;
    ctx.textBaseline = "middle";
    ctx.textAlign = "center";
    ctx.fillText(text, x, y);
    ctx.restore();
  }

  function roundedRect(ctx, x, y, w, h, r) {
    const radius = Math.max(0, Math.min(r, Math.min(w, h) / 2));
    ctx.beginPath();
    if (radius === 0) {
      ctx.rect(x, y, w, h);
      return;
    }
    ctx.moveTo(x + radius, y);
    ctx.arcTo(x + w, y, x + w, y + h, radius);
    ctx.arcTo(x + w, y + h, x, y + h, radius);
    ctx.arcTo(x, y + h, x, y, radius);
    ctx.arcTo(x, y, x + w, y, radius);
    ctx.closePath();
  }

  function panel(ctx, x, y, w, h, opts = {}) {
    ctx.save();
    roundedRect(ctx, x, y, w, h, opts.r === undefined ? STYLE.radius : opts.r);
    ctx.fillStyle = opts.fill || C.cream;
    ctx.fill();
    ctx.strokeStyle = opts.stroke || C.line;
    ctx.lineWidth = opts.width || STYLE.lineWidth;
    ctx.stroke();
    ctx.restore();
  }

  function arrow(ctx, x1, y1, x2, y2, color = C.muted) {
    ctx.save();
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = Math.max(2, STYLE.lineWidth + 1);
    ctx.lineCap = "round";
    const angle = Math.atan2(y2 - y1, x2 - x1);
    const head = 18;
    const bx = x2 - Math.cos(angle) * head;
    const by = y2 - Math.sin(angle) * head;
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(bx, by);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(x2, y2);
    ctx.lineTo(bx - Math.sin(angle) * 10, by + Math.cos(angle) * 10);
    ctx.lineTo(bx + Math.sin(angle) * 10, by - Math.cos(angle) * 10);
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }

  // A minimal ink cat: ears, head, eyes, nose, whiskers. The paper mascot.
  function cat(ctx, cx, cy, scale, opts = {}) {
    const blink = opts.blink ? 0.12 : 1;
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(scale, scale);
    ctx.lineJoin = "round";

    ctx.fillStyle = C.ink;
    ctx.beginPath();
    ctx.moveTo(-84, -54);
    ctx.lineTo(-102, -150);
    ctx.lineTo(-18, -96);
    ctx.closePath();
    ctx.fill();
    ctx.beginPath();
    ctx.moveTo(84, -54);
    ctx.lineTo(102, -150);
    ctx.lineTo(18, -96);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = C.vermilion;
    ctx.beginPath();
    ctx.moveTo(-74, -74);
    ctx.lineTo(-84, -124);
    ctx.lineTo(-40, -94);
    ctx.closePath();
    ctx.fill();
    ctx.beginPath();
    ctx.moveTo(74, -74);
    ctx.lineTo(84, -124);
    ctx.lineTo(40, -94);
    ctx.closePath();
    ctx.fill();

    ctx.beginPath();
    ctx.arc(0, 10, 112, 0, Math.PI * 2);
    ctx.fillStyle = C.cream;
    ctx.fill();
    ctx.strokeStyle = C.ink;
    ctx.lineWidth = 7;
    ctx.stroke();

    ctx.fillStyle = C.ink;
    ctx.beginPath();
    ctx.ellipse(-42, -2, 11, 24 * blink, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.ellipse(42, -2, 11, 24 * blink, 0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = C.vermilion;
    ctx.beginPath();
    ctx.moveTo(0, 28);
    ctx.lineTo(-10, 42);
    ctx.lineTo(10, 42);
    ctx.closePath();
    ctx.fill();
    ctx.strokeStyle = C.ink;
    ctx.lineWidth = 4;
    ctx.beginPath();
    ctx.moveTo(0, 42);
    ctx.lineTo(0, 50);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(-11, 50, 11, 0, Math.PI);
    ctx.arc(11, 50, 11, 0, Math.PI);
    ctx.stroke();

    ctx.strokeStyle = C.muted;
    ctx.lineWidth = 3;
    [-30, -8, 14].forEach((dy, index) => {
      const spread = index === 1 ? 66 : 60;
      ctx.beginPath();
      ctx.moveTo(-58, 30 + dy);
      ctx.lineTo(-58 - spread, 26 + dy * 2);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(58, 30 + dy);
      ctx.lineTo(58 + spread, 26 + dy * 2);
      ctx.stroke();
    });

    ctx.restore();
  }

  function node(ctx, cx, cy, scale) {
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(scale, scale);
    const points = [
      [-90, 0],
      [0, -70],
      [90, 0],
      [0, 70],
    ];
    ctx.strokeStyle = C.accent;
    ctx.fillStyle = C.paperDeep;
    ctx.lineWidth = 5;
    ctx.beginPath();
    ctx.moveTo(points[0][0], points[0][1]);
    for (let i = 1; i < points.length; i++) ctx.lineTo(points[i][0], points[i][1]);
    ctx.closePath();
    ctx.stroke();
    for (const [x, y] of points) {
      ctx.beginPath();
      ctx.arc(x, y, 26, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
    ctx.fillStyle = C.accent2;
    ctx.beginPath();
    ctx.arc(0, 0, 14, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function sun(ctx, cx, cy, scale) {
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(scale, scale);
    ctx.strokeStyle = C.accent;
    ctx.lineWidth = 6;
    for (let i = 0; i < 12; i++) {
      const angle = (i * Math.PI * 2) / 12;
      ctx.beginPath();
      ctx.moveTo(Math.cos(angle) * 76, Math.sin(angle) * 76);
      ctx.lineTo(Math.cos(angle) * 108, Math.sin(angle) * 108);
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.arc(0, 0, 58, 0, Math.PI * 2);
    ctx.fillStyle = C.accent3;
    ctx.fill();
    ctx.restore();
  }

  function face(ctx, cx, cy, scale) {
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(scale, scale);
    roundedRect(ctx, -90, -90, 180, 180, 54);
    ctx.fillStyle = C.accent;
    ctx.fill();
    ctx.fillStyle = C.cream;
    ctx.beginPath();
    ctx.arc(-34, -16, 13, 0, Math.PI * 2);
    ctx.arc(34, -16, 13, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = C.cream;
    ctx.lineWidth = 8;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.arc(0, 10, 40, 0.2, Math.PI - 0.2);
    ctx.stroke();
    ctx.restore();
  }

  function mascot(ctx, cx, cy, scale, opts = {}) {
    switch (STYLE.mascot) {
      case "node":
        node(ctx, cx, cy, scale);
        break;
      case "sun":
        sun(ctx, cx, cy, scale);
        break;
      case "face":
        face(ctx, cx, cy, scale);
        break;
      case "cat":
        cat(ctx, cx, cy, scale, opts);
        break;
      default:
        break;
    }
  }

  // A grid of cells; `value(r, c)` returns { fill, text, glow, alpha }.
  function grid(ctx, x, y, cols, rows, cell, value, opts = {}) {
    const gap = opts.gap === undefined ? 4 : opts.gap;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const v = value(r, c) || {};
        if ((v.alpha ?? 1) <= 0) continue;
        const px = x + c * cell;
        const py = y + r * cell;
        ctx.save();
        ctx.globalAlpha = v.alpha ?? 1;
        if (v.glow) {
          ctx.shadowColor = C.vermilion;
          ctx.shadowBlur = 24;
        }
        roundedRect(ctx, px + gap / 2, py + gap / 2, cell - gap, cell - gap, Math.max(2, cell * 0.08));
        ctx.fillStyle = v.fill || C.paperDeep;
        ctx.fill();
        if (v.stroke) {
          ctx.strokeStyle = v.stroke;
          ctx.lineWidth = 2;
          ctx.stroke();
        }
        ctx.restore();
        if (v.text !== undefined) {
          ctx.save();
          ctx.globalAlpha = v.alpha ?? 1;
          mono(ctx, String(v.text), px + cell / 2, py + cell / 2, cell * 0.3, v.textColor || C.muted, 500);
          ctx.restore();
        }
      }
    }
    if (opts.frame) {
      ctx.save();
      ctx.strokeStyle = opts.frame;
      ctx.lineWidth = STYLE.lineWidth;
      roundedRect(ctx, x - 6, y - 6, cols * cell + 12, rows * cell + 12, 12);
      ctx.stroke();
      ctx.restore();
    }
  }

  function pill(ctx, text, x, y, opts = {}) {
    ctx.save();
    ctx.font = `600 ${opts.size || 30}px ${FONT_CN}`;
    const width = ctx.measureText(text).width + 44;
    const height = (opts.size || 30) + 34;
    roundedRect(ctx, x, y, width, height, height / 2);
    ctx.fillStyle = opts.fill || C.ink;
    ctx.fill();
    ctx.fillStyle = opts.color || C.cream;
    ctx.textBaseline = "middle";
    ctx.fillText(text, x + 22, y + height / 2 + 1);
    ctx.restore();
    return width;
  }

  window.DESIGN = {
    W,
    H,
    M,
    C,
    STYLE,
    FONTS,
    FONT_CN,
    FONT_NUM,
    FONT_DISPLAY,
    FONT_QUOTE,
    easeOut,
    easeInOut,
    clamp01,
    alpha,
    bg,
    kicker,
    footer,
    headline,
    body,
    mono,
    roundedRect,
    panel,
    arrow,
    cat,
    mascot,
    grid,
    pill,
  };
})();
