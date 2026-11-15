// A night sea and a lighthouse, assembled from parts.
//
// Six wave bands are bone chains: each band's angle at the root carries the
// whole band, so a 0.05 rad bend at the root is a swell by the right edge.
// The tower sways on its base bone, its pennant is a spring, and the beam is
// a cone on a bone that sweeps.
window.SCENE = (function () {
  const R = window.RIG;
  const components = [];
  const bones = [];
  let ready = false;
  let lastTime = 0;
  let frame = null;

  function add(component) {
    components.push(component);
    for (const bone of component.bones) bones.push(bone);
  }

  return {
    async prepare() {
      await Promise.all([
        PARTS.load("/kit/parts/sky.js"),
        PARTS.load("/kit/parts/waves.js"),
        PARTS.load("/kit/parts/rock.js"),
        PARTS.load("/kit/parts/tower.js"),
        PARTS.load("/kit/parts/beam.js"),
      ]);

      add(PARTS.sky({ moonX: 372, moonY: 206 }));
      add(PARTS.wave({ y: 566, speed: 0.30, amplitude: 0.012, phase: 0.0, color: "#0b202b", crest: "#86c9c1", crestAlpha: 0.20, foam: 0.16, z: 10 }));
      add(PARTS.wave({ y: 622, speed: 0.42, amplitude: 0.016, phase: 1.1, color: "#0d2634", crestAlpha: 0.24, foam: 0.24, z: 20 }));
      add(PARTS.wave({ y: 692, speed: 0.55, amplitude: 0.020, phase: 2.3, color: "#0f2d3c", crestAlpha: 0.28, foam: 0.32, z: 30 }));
      add(PARTS.wave({ y: 772, speed: 0.70, amplitude: 0.024, phase: 3.4, color: "#123545", crestAlpha: 0.32, foam: 0.42, z: 40 }));
      add(PARTS.wave({ y: 864, speed: 0.90, amplitude: 0.028, phase: 4.7, color: "#154050", crestAlpha: 0.36, foam: 0.52, z: 50 }));
      add(PARTS.wave({ y: 966, speed: 1.15, amplitude: 0.032, phase: 5.9, color: "#194b5d", crestAlpha: 0.40, foam: 0.62, z: 60 }));
      add(PARTS.rock({ x: 1460, y: 648, z: 80 }));

      const tower = PARTS.tower({ x: 1460, y: 646, height: 330, z: 90 });
      add(tower);
      add(PARTS.beam({ bone: tower.beam, length: 1560, spread: 0.08, z: 70 }));

      components.sort(function (a, b) { return a.z - b.z; });
      ready = true;
    },

    drawScene(ctx, t) {
      if (!ready) return;
      for (const component of components) {
        if (component.update) component.update(t);
      }
      R.solve(bones);

      // Springs step in fixed slices so a dropped frame does not change how
      // far the pennant swings.
      let dt = t - lastTime;
      if (dt <= 0 || dt > 0.5) dt = 1 / 30;
      lastTime = t;
      const steps = Math.min(4, Math.max(1, Math.round(dt * 60)));
      const slice = dt / steps;
      for (let i = 0; i < steps; i++) {
        for (const component of components) {
          if (component.step) component.step(slice);
        }
      }

      for (const component of components) {
        for (const part of component.parts) R.drawPart(ctx, part, t);
      }

      const vignette = ctx.createRadialGradient(960, 560, 360, 960, 560, 1140);
      vignette.addColorStop(0, "rgba(0,0,0,0)");
      vignette.addColorStop(1, "rgba(2,7,10,0.55)");
      ctx.fillStyle = vignette;
      ctx.fillRect(0, 0, 1920, 1080);
    },

    draw(ctx, t) {
      if (!ready) return;
      if (!frame) {
        frame = document.createElement("canvas");
        frame.width = 1920;
        frame.height = 1080;
      }
      const scene = frame.getContext("2d");
      this.drawScene(scene, t);
      const painted = window.STYLIZE_GL
        ? STYLIZE_GL.oil(frame, { radius: 2, levels: 18, ink: 0.28, scale: 0.6 })
        : null;
      if (painted) ctx.drawImage(painted, 0, 0, 1920, 1080);
      else ctx.drawImage(frame, 0, 0);
    },
  };
})();
