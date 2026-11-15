// Night sky: gradient, stars, a moon with haze, and slow cloud bands.
//
// No bone moves here. The sky is the one plane that must not drift with the
// camera; it breathes in place (twinkle, haze) so the frame is never frozen.
(function () {
  const R = window.RIG;

  window.PARTS.sky = function (opts) {
    const o = Object.assign({ moonX: 380, moonY: 214, moonR: 58, horizon: 600 }, opts);
    const root = R.bone(null, 0, 0);

    let seed = 19;
    const random = function () {
      seed = (seed * 16807) % 2147483647;
      return seed / 2147483647;
    };
    const stars = [];
    for (let i = 0; i < 110; i++) {
      stars.push({
        x: random() * 1920,
        y: random() * o.horizon * 0.92,
        r: 0.5 + random() * 1.5,
        phase: random() * Math.PI * 2,
        speed: 0.4 + random() * 1.2,
      });
    }
    const clouds = [
      { x: 60, y: 150, w: 520, h: 46, speed: 7, alpha: 0.05 },
      { x: 860, y: 262, w: 700, h: 64, speed: 4, alpha: 0.045 },
      { x: 1420, y: 96, w: 430, h: 38, speed: 10, alpha: 0.04 },
    ];

    const part = R.part({
      bone: root,
      draw: function (ctx, _part, _bone, t) {
        const time = t || 0;
        const sky = ctx.createLinearGradient(0, 0, 0, o.horizon + 120);
        sky.addColorStop(0, "#04090f");
        sky.addColorStop(0.55, "#081722");
        sky.addColorStop(1, "#0f2b38");
        ctx.fillStyle = sky;
        ctx.fillRect(0, 0, 1920, 1080);

        for (const star of stars) {
          const twinkle = 0.35 + 0.45 * (0.5 + 0.5 * Math.sin(time * star.speed + star.phase));
          ctx.globalAlpha = twinkle;
          ctx.fillStyle = "#cfe6e2";
          ctx.beginPath();
          ctx.arc(star.x, star.y, star.r, 0, Math.PI * 2);
          ctx.fill();
        }
        ctx.globalAlpha = 1;

        const haze = ctx.createRadialGradient(
          o.moonX, o.moonY, o.moonR * 0.5,
          o.moonX, o.moonY, o.moonR * 5.2,
        );
        const breathe = 0.85 + 0.15 * Math.sin(time * 0.4);
        haze.addColorStop(0, "rgba(196,226,220," + 0.20 * breathe + ")");
        haze.addColorStop(0.45, "rgba(120,180,180,0.06)");
        haze.addColorStop(1, "rgba(80,140,150,0)");
        ctx.fillStyle = haze;
        ctx.fillRect(o.moonX - o.moonR * 5.2, o.moonY - o.moonR * 5.2, o.moonR * 10.4, o.moonR * 10.4);

        ctx.fillStyle = "#e6efe6";
        ctx.beginPath();
        ctx.arc(o.moonX, o.moonY, o.moonR, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = "rgba(30,60,70,0.10)";
        ctx.beginPath();
        ctx.arc(o.moonX - 18, o.moonY - 14, o.moonR * 0.22, 0, Math.PI * 2);
        ctx.arc(o.moonX + 16, o.moonY + 20, o.moonR * 0.16, 0, Math.PI * 2);
        ctx.fill();

        for (const cloud of clouds) {
          const drift = ((cloud.x + time * cloud.speed) % 2500) - 300;
          const shape = ctx.createLinearGradient(0, cloud.y, 0, cloud.y + cloud.h);
          shape.addColorStop(0, "rgba(170,205,205," + cloud.alpha + ")");
          shape.addColorStop(1, "rgba(170,205,205,0)");
          ctx.fillStyle = shape;
          ctx.beginPath();
          ctx.ellipse(drift, cloud.y, cloud.w * 0.5, cloud.h * 0.5, 0, 0, Math.PI * 2);
          ctx.ellipse(drift + cloud.w * 0.3, cloud.y + 10, cloud.w * 0.36, cloud.h * 0.4, 0, 0, Math.PI * 2);
          ctx.ellipse(drift - cloud.w * 0.28, cloud.y + 14, cloud.w * 0.3, cloud.h * 0.34, 0, 0, Math.PI * 2);
          ctx.fill();
        }
      },
    });

    return {
      bones: [root],
      parts: [part],
      z: 0,
    };
  };
})();
