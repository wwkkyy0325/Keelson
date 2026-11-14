// One scene, one idea — and it is a picture, not a slide.
//
// The page calls `draw(ctx, t, duration)` once per frame at a fixed rate and
// posts the frame back. Everything must be a pure function of `t`: the same
// `t` renders the same picture.
//
// The shape of a scene:
//   1. ground and air      — bg, hills, waves, particles
//   2. the subject         — figures, objects, a drawn path
//   3. the camera          — optional push in or pan around 1+2
//   4. the labels          — kicker, headline, body, footer
//
// A frame whose only content is a headline and a panel of text is a slide.
// If the narration says "the wave arrives", the frame shows the wave.
//
// Numbers come from `window.DATA` (derived from data/spec.json); never type a
// computed number into a scene. A photo the user supplied loads from
// /assets/<name>; never download one yourself.
//
// Style comes from D.C / D.alpha / D.fonts / D.panel / D.mascot only. A hex,
// rgb or font literal here pins the scene to one palette and breaks the
// series style. Content stays above the subtitle band (y > 920).
window.SCENE = {
  id: "scene-id",
  draw(ctx, t, duration) {
    const D = window.DESIGN;
    const C = D.C;
    const W = D.W;
    const H = D.H;
    const M = window.DATA;

    D.bg(ctx);

    // 1. The air, then the water. Layers, not one shape: each band is slower
    //    and paler than the one in front, which is what reads as depth.
    D.particles(ctx, t, { count: 36, speed: 24, to: H * 0.42, opacity: 0.16 });
    D.waves(ctx, t, {
      layers: 6,
      y: H * 0.42,
      amp: 52,
      gap: 36,
      speed: 0.8,
      colors: [C.accent2, C.accent2, C.accent, C.ink],
      opacity: 0.28,
    });

    // 2. The shore the people stand on, drawn over the water.
    ctx.save();
    ctx.beginPath();
    ctx.moveTo(-80, H + 80);
    for (let x = -80; x <= W + 80; x += 16) {
      ctx.lineTo(x, H * 0.78 + Math.sin(x * 0.0016 + t * 0.15) * 12);
    }
    ctx.lineTo(W + 80, H + 80);
    ctx.closePath();
    ctx.fillStyle = D.alpha(C.paperDeep, 0.98);
    ctx.fill();
    ctx.restore();

    // The subject: people walking. `scale` sets their size; `seed` keeps the
    // same cast for the whole shot.
    D.crowd(ctx, t, { count: 6, rows: 2, y: H * 0.86, speed: 58, seed: 3, scale: 1.7 });

    // A path that draws itself, for a process the narration is describing.
    const path = [];
    for (let i = 0; i <= 20; i++) {
      path.push(1180 + i * 34, 250 + Math.sin(i * 0.5) * 30);
    }
    D.stroke(ctx, path, D.clamp01(t / Math.min(2.2, duration)), {
      color: C.accent,
      width: 7,
    });

    // 3. Labels last, so nothing is painted over them.
    D.kicker(ctx, M && M.count ? `${M.count} 层` : "示意 · 一层一层");
    D.headline(ctx, ["一层一层推上来的", "不是同一道浪"], D.M, 190, 84);
    D.body(ctx, "每一层都比前一层慢，也比前一层淡。", D.M, 420, 42, C.muted);
    D.footer(ctx, "the film's title");
  },
};
