// The lamp's beam. It lives on a bone that sweeps, so the cone never needs
// its own animation: the tree moves it.
(function () {
  const R = window.RIG;

  window.PARTS.beam = function (opts) {
    const o = Object.assign({ bone: null, length: 1500, spread: 0.075, z: 70 }, opts);
    if (!o.bone) throw new Error("beam needs a bone");

    const part = R.part({
      bone: o.bone,
      draw: function (ctx) {
        ctx.save();
        ctx.globalCompositeOperation = "lighter";

        const cone = ctx.createLinearGradient(0, 0, o.length, 0);
        cone.addColorStop(0, "rgba(255,230,176,0.58)");
        cone.addColorStop(0.22, "rgba(255,208,134,0.22)");
        cone.addColorStop(0.62, "rgba(255,196,120,0.07)");
        cone.addColorStop(1, "rgba(255,190,110,0)");
        ctx.fillStyle = cone;
        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.lineTo(o.length, -Math.tan(o.spread) * o.length);
        ctx.lineTo(o.length, Math.tan(o.spread) * o.length);
        ctx.closePath();
        ctx.fill();

        const core = ctx.createLinearGradient(0, 0, o.length * 0.58, 0);
        core.addColorStop(0, "rgba(255,246,214,0.46)");
        core.addColorStop(1, "rgba(255,236,190,0)");
        ctx.fillStyle = core;
        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.lineTo(o.length * 0.58, -Math.tan(o.spread * 0.22) * o.length * 0.58);
        ctx.lineTo(o.length * 0.58, Math.tan(o.spread * 0.22) * o.length * 0.58);
        ctx.closePath();
        ctx.fill();

        const source = ctx.createRadialGradient(0, 0, 2, 0, 0, 74);
        source.addColorStop(0, "rgba(255,240,200,0.72)");
        source.addColorStop(1, "rgba(255,220,150,0)");
        ctx.fillStyle = source;
        ctx.fillRect(-74, -74, 148, 148);
        ctx.restore();
      },
    });

    return { bones: [], parts: [part], z: o.z };
  };
})();
