// The cliff the lighthouse stands on, plus the rocks breaking the near water.
// One bone holds the mass; the silhouette is drawn in its frame so a future
// camera move can drift the whole headland without redrawing it.
(function () {
  const R = window.RIG;

  window.PARTS.rock = function (opts) {
    const o = Object.assign({ x: 1460, y: 648, z: 80 }, opts);
    const root = R.bone(null, o.x, o.y);

    const headland = [
      [-250, 430], [-232, 210], [-196, 96], [-150, 44], [-118, 12],
      [-72, -8], [-34, 2], [8, -16], [58, -2], [118, 28], [168, 92],
      [206, 214], [236, 430],
    ];
    const nearRocks = [
      { x: 420, y: 1010, s: 1 },
      { x: 980, y: 1046, s: 0.7 },
      { x: 760, y: 992, s: 0.45 },
    ];

    const part = R.part({
      bone: root,
      draw: function (ctx) {
        ctx.save();
        ctx.fillStyle = "#07131a";
        ctx.beginPath();
        headland.forEach(function (point, index) {
          if (index === 0) ctx.moveTo(point[0], point[1]);
          else ctx.lineTo(point[0], point[1]);
        });
        ctx.lineTo(280, 520);
        ctx.lineTo(-300, 520);
        ctx.closePath();
        ctx.fill();

        ctx.strokeStyle = "rgba(132,186,184,0.22)";
        ctx.lineWidth = 2.5;
        ctx.beginPath();
        ctx.moveTo(-232, 210);
        ctx.lineTo(-196, 96);
        ctx.lineTo(-150, 44);
        ctx.lineTo(-118, 12);
        ctx.lineTo(-72, -8);
        ctx.lineTo(-34, 2);
        ctx.lineTo(8, -16);
        ctx.lineTo(58, -2);
        ctx.lineTo(118, 28);
        ctx.stroke();

        for (const rock of nearRocks) {
          const width = 120 * rock.s;
          const height = 46 * rock.s;
          const left = rock.x - o.x;
          const top = rock.y - o.y;
          ctx.fillStyle = "#061018";
          ctx.beginPath();
          ctx.moveTo(left - width, top);
          ctx.lineTo(left - width * 0.45, top - height);
          ctx.lineTo(left + width * 0.15, top - height * 0.72);
          ctx.lineTo(left + width * 0.7, top - height * 0.3);
          ctx.lineTo(left + width, top);
          ctx.closePath();
          ctx.fill();
          ctx.strokeStyle = "rgba(130,190,185,0.18)";
          ctx.lineWidth = 1.6;
          ctx.stroke();
        }
        ctx.restore();
      },
    });

    return { bones: [root], parts: [part], z: o.z };
  };
})();
