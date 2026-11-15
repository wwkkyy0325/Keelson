// One wave band. A chain of bones runs across the frame; every bone carries
// the next, so a small angle at the root becomes a swell at the far end. The
// band fills from its crest down past the bottom of the frame, which is what
// lets six of them stack without a seam.
(function () {
  const R = window.RIG;

  function shade(hex, amount) {
    const value = hex.replace("#", "");
    const full = value.length === 3 ? value.split("").map(function (c) { return c + c; }).join("") : value;
    const channels = [0, 2, 4].map(function (at) {
      const channel = parseInt(full.slice(at, at + 2), 16);
      return Math.max(0, Math.min(255, Math.round(channel * (1 + amount))));
    });
    return "rgb(" + channels.join(",") + ")";
  }

  window.PARTS.wave = function (opts) {
    const o = Object.assign({
      y: 700,
      segments: 18,
      span: 2280,
      speed: 0.6,
      amplitude: 0.05,
      phase: 0,
      color: "#0e2c39",
      crest: "#8fd2c9",
      crestAlpha: 0.22,
      foam: 0.45,
      z: 10,
    }, opts);

    const world = R.bone(null, 0, 0);
    const root = R.bone(null, -180, o.y);
    const bones = [world, root];
    const step = o.span / o.segments;
    let parent = root;
    for (let i = 0; i < o.segments; i++) {
      const bone = R.bone(parent, step, 0, step);
      bones.push(bone);
      parent = bone;
    }

    const part = R.part({
      // The chain's points are already in world space, so the part is drawn
      // in a frame at the origin rather than in the root bone's frame.
      bone: world,
      draw: function (ctx, _part, _bone, t) {
        const points = bones.map(function (bone) { return { x: bone.world.x, y: bone.world.y }; });
        const left = points[0];
        const right = points[points.length - 1];
        points.unshift({ x: left.x - step, y: left.y - (points[1].y - left.y) });
        points.push({ x: right.x + step, y: right.y + (right.y - points[points.length - 2].y) });

        let crestY = 0;
        for (const point of points) crestY += point.y;
        crestY /= points.length;

        // The gradient is anchored to the moving crest, not the frame, so
        // every band keeps a lit lip even as it rises and falls.
        const body = ctx.createLinearGradient(0, crestY - 12, 0, crestY + 280);
        body.addColorStop(0, shade(o.color, 0.22));
        body.addColorStop(0.22, o.color);
        body.addColorStop(1, shade(o.color, -0.5));

        ctx.beginPath();
        ctx.moveTo(points[0].x, points[0].y);
        for (let i = 1; i < points.length - 1; i++) {
          const midX = (points[i].x + points[i + 1].x) / 2;
          const midY = (points[i].y + points[i + 1].y) / 2;
          ctx.quadraticCurveTo(points[i].x, points[i].y, midX, midY);
        }
        const last = points[points.length - 1];
        ctx.lineTo(last.x, last.y);
        ctx.lineTo(last.x, 1140);
        ctx.lineTo(points[0].x, 1140);
        ctx.closePath();
        ctx.fillStyle = body;
        ctx.fill();

        function crestPath() {
          ctx.beginPath();
          ctx.moveTo(points[0].x, points[0].y);
          for (let i = 1; i < points.length - 1; i++) {
            const midX = (points[i].x + points[i + 1].x) / 2;
            const midY = (points[i].y + points[i + 1].y) / 2;
            ctx.quadraticCurveTo(points[i].x, points[i].y, midX, midY);
          }
          ctx.lineTo(last.x, last.y);
        }

        ctx.save();
        ctx.strokeStyle = o.crest;
        ctx.lineJoin = "round";
        ctx.globalAlpha = o.crestAlpha * 0.45;
        ctx.lineWidth = 9;
        crestPath();
        ctx.stroke();
        ctx.globalAlpha = o.crestAlpha;
        ctx.lineWidth = 2.2;
        crestPath();
        ctx.stroke();
        ctx.restore();

        if (o.foam > 0) {
          ctx.save();
          ctx.fillStyle = "#e6f4ef";
          for (let i = 2; i < points.length - 2; i += 2) {
            const point = points[i];
            const lift = Math.sin(t * 1.7 + i * 0.8);
            if (lift < 0.25) continue;
            ctx.globalAlpha = o.foam * (lift - 0.25) * 0.9;
            ctx.beginPath();
            ctx.ellipse(point.x, point.y - 2, 26 + 12 * lift, 3.2, 0, 0, Math.PI * 2);
            ctx.fill();
          }
          ctx.restore();
        }
      },
    });

    return {
      bones: bones,
      parts: [part],
      z: o.z,
      update: function (t) {
        root.angle = Math.sin(t * o.speed * 0.8 + o.phase) * o.amplitude * 0.9;
        for (let i = 1; i < bones.length; i++) {
          bones[i].angle =
            Math.sin(t * o.speed * 1.7 - i * 0.55 + o.phase) * o.amplitude +
            Math.sin(t * o.speed * 0.6 + i * 0.23 + o.phase * 1.7) * o.amplitude * 0.6;
        }
      },
    };
  };
})();
