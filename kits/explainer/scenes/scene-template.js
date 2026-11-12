// One scene, one idea.
//
// The page draws this file's `draw(ctx, t, duration)` for every frame at a
// fixed rate, then posts it back. Use only `window.DESIGN` for style and
// `window.DATA` for numbers; do not type a computed number into a scene.
//
// A photo or screenshot the user supplied loads from /assets/<name>:
//   const image = new Image();
//   image.src = "/assets/photo.jpg";
// Wait for it before the first frame. Never download an image yourself.
//
// Rules of the house:
// - content stays above the subtitle band (y > 920 is subtitles);
// - one headline, one secondary line, one picture;
// - motion is a slide, a fade or a cut, eased with the kit's helpers;
// - style comes from D.C/D.alpha/D.fonts/D.panel/D.mascot only. A hex, rgb or
//   font literal in this file pins the scene to one palette and breaks the
//   series style.
window.SCENE = {
  id: "scene-id",
  draw(ctx, t, duration) {
    const D = window.DESIGN;
    const C = D.C;
    const M = window.DATA;
    D.bg(ctx);
    D.kicker(ctx, "01 / a short label");
    D.headline(ctx, ["One idea", "in one line"], D.M, 320, 92);
    D.body(ctx, "A sentence the narration also says.", D.M, 600, 46, C.muted);
    // Draw the picture from M here.
    D.footer(ctx, "the film's title");
  },
};
