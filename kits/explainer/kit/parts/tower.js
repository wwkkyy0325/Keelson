// The lighthouse: body, gallery, lantern and a pennant on a spring.
//
// The body sways a fraction of a degree on its base bone; the lantern rides
// the top; the pennant is a spring chain rooted at the lantern, so it keeps
// moving after the tower has stopped.
(function () {
  const R = window.RIG;

  window.PARTS.tower = function (opts) {
    const o = Object.assign({ x: 1460, y: 646, height: 330, z: 90 }, opts);
    const base = R.bone(null, o.x, o.y);
    const world = R.bone(null, 0, 0);
    const body = R.bone(base, 0, 0, o.height);
    const lamp = R.bone(body, 0, -o.height - 26, 0);
    const beam = R.bone(body, 0, -o.height - 38, 1400);

    const height = o.height;
    const bottomHalf = 76;
    const topHalf = 46;

    function towerPath(ctx, inset) {
      const bottom = bottomHalf - (inset || 0);
      const top = topHalf - (inset || 0);
      ctx.beginPath();
      ctx.moveTo(-bottom, 0);
      ctx.lineTo(bottom, 0);
      ctx.lineTo(top, -height);
      ctx.lineTo(-top, -height);
      ctx.closePath();
    }

    const bodyPart = R.part({
      bone: body,
      draw: function (ctx) {
        ctx.save();
        towerPath(ctx);
        ctx.clip();

        const stone = ctx.createLinearGradient(-bottomHalf, 0, bottomHalf, 0);
        stone.addColorStop(0, "#5d5147");
        stone.addColorStop(0.62, "#c9bda6");
        stone.addColorStop(1, "#8a7d6a");
        ctx.fillStyle = stone;
        ctx.fillRect(-90, -height - 10, 180, height + 20);

        const bands = 5;
        const band = height / bands;
        for (let i = 0; i < bands; i++) {
          if (i % 2 === 0) continue;
          ctx.fillStyle = "rgba(126,52,42,0.82)";
          ctx.fillRect(-90, -height + i * band, 180, band);
        }

        const shade = ctx.createLinearGradient(-bottomHalf, 0, bottomHalf, 0);
        shade.addColorStop(0, "rgba(4,12,18,0.55)");
        shade.addColorStop(0.7, "rgba(4,12,18,0)");
        shade.addColorStop(1, "rgba(4,12,18,0.35)");
        ctx.fillStyle = shade;
        ctx.fillRect(-90, -height - 10, 180, height + 20);
        ctx.restore();

        ctx.strokeStyle = "rgba(232,240,232,0.12)";
        ctx.lineWidth = 1.4;
        towerPath(ctx, 6);
        ctx.stroke();
      },
    });

    const galleryPart = R.part({
      bone: body,
      oy: -height,
      draw: function (ctx) {
        ctx.fillStyle = "#131f26";
        ctx.fillRect(-topHalf - 18, -18, (topHalf + 18) * 2, 18);
        ctx.strokeStyle = "#2b3d45";
        ctx.lineWidth = 3;
        for (let i = -3; i <= 3; i++) {
          const x = i * 14;
          ctx.beginPath();
          ctx.moveTo(x, -18);
          ctx.lineTo(x, -40);
          ctx.stroke();
        }
        ctx.beginPath();
        ctx.moveTo(-topHalf - 18, -40);
        ctx.lineTo(topHalf + 18, -40);
        ctx.stroke();
      },
    });

    const lanternPart = R.part({
      bone: body,
      oy: -height - 18,
      draw: function (ctx, _part, _bone, t) {
        const glow = 0.75 + 0.25 * Math.sin(t * 2.1);
        const halo = ctx.createRadialGradient(0, -34, 6, 0, -34, 158);
        halo.addColorStop(0, "rgba(255,214,140," + 0.55 * glow + ")");
        halo.addColorStop(0.4, "rgba(255,190,110,0.16)");
        halo.addColorStop(1, "rgba(255,180,100,0)");
        ctx.globalCompositeOperation = "lighter";
        ctx.fillStyle = halo;
        ctx.fillRect(-170, -200, 340, 340);

        ctx.globalCompositeOperation = "source-over";
        ctx.fillStyle = "#f6d79a";
        ctx.fillRect(-30, -66, 60, 50);
        ctx.fillStyle = "rgba(255,244,214,0.9)";
        ctx.beginPath();
        ctx.arc(0, -42, 13, 0, Math.PI * 2);
        ctx.fill();

        ctx.strokeStyle = "#1b262c";
        ctx.lineWidth = 5;
        ctx.strokeRect(-32, -68, 64, 54);
        ctx.beginPath();
        ctx.moveTo(0, -68);
        ctx.lineTo(0, -14);
        ctx.stroke();

        ctx.fillStyle = "#16232a";
        ctx.beginPath();
        ctx.moveTo(-40, -68);
        ctx.quadraticCurveTo(0, -104, 40, -68);
        ctx.closePath();
        ctx.fill();
        ctx.fillStyle = "#22323a";
        ctx.beginPath();
        ctx.arc(0, -100, 5, 0, Math.PI * 2);
        ctx.fill();
      },
    });

    const pennant = R.spring({ count: 5, length: 17, stiffness: 0.62, damping: 0.12, gravity: 60, ox: 36, oy: -height - 42 });
    const flagPart = R.part({
      bone: world,
      draw: function (ctx) {
        const points = pennant.points;
        if (points.length < 3) return;
        ctx.fillStyle = "#b0493a";
        ctx.beginPath();
        ctx.moveTo(points[0].x, points[0].y);
        for (let i = 1; i < points.length; i++) {
          ctx.lineTo(points[i].x, points[i].y);
        }
        ctx.lineTo(points[points.length - 1].x, points[points.length - 1].y + 12);
        for (let i = points.length - 2; i >= 0; i--) {
          ctx.lineTo(points[i].x, points[i].y + 12);
        }
        ctx.closePath();
        ctx.fill();
      },
    });

    return {
      bones: [base, world, body, lamp, beam],
      parts: [bodyPart, galleryPart, lanternPart, flagPart],
      z: o.z,
      lamp: lamp,
      beam: beam,
      update: function (t) {
        body.angle = Math.sin(t * 0.35) * 0.004 + Math.sin(t * 0.11 + 1.2) * 0.002;
        beam.angle = -Math.PI / 2 + Math.sin(t * 0.55) * 1.25;
      },
      step: function (dt) {
        R.stepSpringOnBone(pennant, body, dt);
      },
    };
  };
})();
