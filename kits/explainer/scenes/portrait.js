// A portrait cut from a reference plate.
//
// A face is not drawn here. The scene requires a headshot in `assets/`; the
// plate is restyled once (on the GPU when WebGL2 is there), then cut into a
// head and a body, and each piece gets a bone. Without a plate the scene
// renders a refusal slate rather than a guessed face.
window.SCENE = (function () {
  const R = window.RIG;
  const components = [];
  const bones = [];
  let ready = false;
  let missing = null;
  let lastTime = 0;

  function add(component) {
    components.push(component);
    for (const bone of component.bones) bones.push(bone);
  }

  async function loadPlate() {
    for (const name of ["portrait.jpg", "portrait.jpeg", "portrait.png", "portrait.webp"]) {
      try {
        const photo = await STYLIZE.load("/assets/" + name);
        return PARTS.fitPlate(photo, 760, 980);
      } catch (error) {
        // Try the next name; an absent file is the normal case here.
      }
    }
    return null;
  }

  function drawRefusal(ctx, reason) {
    const background = ctx.createLinearGradient(0, 0, 0, 1080);
    background.addColorStop(0, "#171310");
    background.addColorStop(1, "#0b0908");
    ctx.fillStyle = background;
    ctx.fillRect(0, 0, 1920, 1080);

    ctx.strokeStyle = "rgba(224,163,74,0.55)";
    ctx.lineWidth = 2;
    ctx.setLineDash([12, 10]);
    ctx.strokeRect(510, 210, 900, 660);
    ctx.setLineDash([]);

    ctx.fillStyle = "#e0a34a";
    ctx.font = "600 44px 'Microsoft YaHei','Noto Sans SC',sans-serif";
    ctx.textAlign = "center";
    ctx.fillText(reason === "trace" ? "描边失败" : "缺少参照图", 960, 470);

    ctx.fillStyle = "#c9c4bb";
    ctx.font = "24px 'Microsoft YaHei','Noto Sans SC',sans-serif";
    if (reason === "trace") {
      ctx.fillText("背景太杂，分不出主体轮廓。换一张背景干净的参照图。", 960, 530);
    } else {
      ctx.fillText("把一张有授权的头照放到 assets/portrait.jpg", 960, 530);
    }
    ctx.fillStyle = "#8b8f98";
    ctx.font = "20px 'Microsoft YaHei','Noto Sans SC',sans-serif";
    ctx.fillText("这个 rig 不画脸：搬参照图 → 描边 → 修 → 拆件 → 绑骨。", 960, 580);
    ctx.textAlign = "left";
  }

  return {
    async prepare() {
      await PARTS.load("/kit/parts/portrait.js");
      const plate = await loadPlate();
      if (!plate) {
        missing = "plate";
        return;
      }
      const painted = window.STYLIZE_GL
        ? STYLIZE_GL.oil(plate, { levels: 10, ink: 0.7 })
        : STYLIZE.oil(plate, { levels: 10, ink: 0.5 });

      // Trace the reference, then cut at the traced neck. A picture whose
      // background cannot be separated is refused rather than approximated.
      const silhouette = TRACE.silhouette(painted, { threshold: 44 });
      if (!silhouette) {
        missing = "trace";
        return;
      }
      const neck = TRACE.neck(silhouette);
      const pieces = TRACE.pieces(painted, silhouette, neck.y);
      add(PARTS.portrait({
        head: pieces.head,
        body: pieces.body,
        neck: { x: neck.centre, y: neck.y },
        x: 580,
        // Low enough that the plate's bottom edge sits outside the frame.
        y: 150,
      }));
      ready = true;
    },

    draw(ctx, t) {
      if (missing) {
        drawRefusal(ctx, missing);
        return;
      }

      const background = ctx.createLinearGradient(0, 0, 0, 1080);
      background.addColorStop(0, "#171310");
      background.addColorStop(0.6, "#100d0b");
      background.addColorStop(1, "#0b0908");
      ctx.fillStyle = background;
      ctx.fillRect(0, 0, 1920, 1080);

      for (let i = 0; i < 7; i++) {
        const phase = t * (0.05 + i * 0.012) + i * 1.7;
        const x = 960 + Math.cos(phase) * (420 + i * 60);
        const y = 520 + Math.sin(phase * 1.3) * (260 + i * 30);
        const radius = 120 + i * 46;
        const glow = ctx.createRadialGradient(x, y, 0, x, y, radius);
        glow.addColorStop(0, "rgba(150,96,64,0.055)");
        glow.addColorStop(1, "rgba(150,96,64,0)");
        ctx.fillStyle = glow;
        ctx.fillRect(x - radius, y - radius, radius * 2, radius * 2);
      }

      if (!ready) return;

      let dt = t - lastTime;
      if (dt <= 0 || dt > 0.5) dt = 1 / 30;
      lastTime = t;

      for (const component of components) {
        if (component.update) component.update(t, dt);
      }
      R.solve(bones);
      for (const component of components) {
        for (const part of component.parts) R.drawPart(ctx, part, t);
      }

      const vignette = ctx.createRadialGradient(960, 520, 300, 960, 520, 1000);
      vignette.addColorStop(0, "rgba(0,0,0,0)");
      vignette.addColorStop(1, "rgba(5,3,2,0.62)");
      ctx.fillStyle = vignette;
      ctx.fillRect(0, 0, 1080 * 16 / 9, 1080);
    },
  };
})();
