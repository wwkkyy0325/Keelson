// The design kit: one palette, one type scale, one motion vocabulary.
// Scenes draw on a 1920x1080 canvas and may only use these helpers.
(function () {
  const W = 1920;
  const H = 1080;
  const M = 112; // safe margin

  const C = {
    paper: "#F3EDE1",
    paperDeep: "#E7DECC",
    ink: "#191A1E",
    muted: "#7C776C",
    line: "#CDC3AF",
    vermilion: "#D14B2C",
    teal: "#2B6A63",
    gold: "#D9A441",
    cream: "#FFFBF2",
  };

  const FONT_CN = '"Microsoft YaHei", "Microsoft YaHei UI", "Noto Sans SC", sans-serif';
  const FONT_NUM = '"Segoe UI", "Microsoft YaHei", sans-serif';

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

  function bg(ctx) {
    ctx.fillStyle = C.paper;
    ctx.fillRect(0, 0, W, H);
    // A printed dot grid, deterministic and very quiet.
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
    // Page frame.
    ctx.strokeStyle = C.line;
    ctx.lineWidth = 2;
    ctx.strokeRect(40, 40, W - 80, H - 80);
  }

  function kicker(ctx, text) {
    ctx.save();
    ctx.fillStyle = C.vermilion;
    ctx.fillRect(M, 92, 16, 16);
    ctx.fillStyle = C.muted;
    ctx.font = `600 28px ${FONT_CN}`;
    ctx.letterSpacing = "6px";
    ctx.textBaseline = "middle";
    ctx.fillText(text, M + 32, 102);
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
    ctx.font = `700 ${size}px ${FONT_CN}`;
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
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.arcTo(x + w, y, x + w, y + h, r);
    ctx.arcTo(x + w, y + h, x, y + h, r);
    ctx.arcTo(x, y + h, x, y, r);
    ctx.arcTo(x, y, x + w, y, r);
    ctx.closePath();
  }

  function panel(ctx, x, y, w, h, opts = {}) {
    ctx.save();
    roundedRect(ctx, x, y, w, h, opts.r || 18);
    ctx.fillStyle = opts.fill || C.cream;
    ctx.fill();
    ctx.strokeStyle = opts.stroke || C.line;
    ctx.lineWidth = opts.width || 3;
    ctx.stroke();
    ctx.restore();
  }

  function arrow(ctx, x1, y1, x2, y2, color = C.muted) {
    ctx.save();
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = 4;
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

  // A minimal ink cat: ears, head, eyes, nose, whiskers. The mascot of the kit.
  function cat(ctx, cx, cy, scale, opts = {}) {
    const blink = opts.blink ? 0.12 : 1;
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(scale, scale);
    ctx.lineJoin = "round";

    // ears
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

    // head
    ctx.beginPath();
    ctx.arc(0, 10, 112, 0, Math.PI * 2);
    ctx.fillStyle = C.cream;
    ctx.fill();
    ctx.strokeStyle = C.ink;
    ctx.lineWidth = 7;
    ctx.stroke();

    // eyes
    ctx.fillStyle = C.ink;
    ctx.beginPath();
    ctx.ellipse(-42, -2, 11, 24 * blink, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.ellipse(42, -2, 11, 24 * blink, 0, 0, Math.PI * 2);
    ctx.fill();

    // nose and mouth
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

    // whiskers
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
      ctx.lineWidth = 3;
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
    FONT_CN,
    FONT_NUM,
    easeOut,
    easeInOut,
    clamp01,
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
    grid,
    pill,
  };
})();
