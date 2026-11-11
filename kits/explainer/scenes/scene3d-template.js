// A 3D scene.
//
// The page (`kit/scene3d.html`) imports Three.js, creates the WebGL renderer
// and calls this module's `createScene`. Return `render(t)` to draw the 3D
// layer and `overlay(ctx, t)` to draw labels on the composited 2D canvas.
//
// Rules:
// - every motion and particle path is analytic and seeded: the same t must
//   render the same frame;
// - the camera is part of the shot, not a free-flying observer;
// - a 3D picture is still 示意 unless a real simulation produced it; say so
//   on screen and never call it CFD;
// - render it with `render_scene(..., page: "kit/scene3d.html")`.

export function createScene({ THREE, renderer, width, height, fps, duration }) {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0b1418);
  const camera = new THREE.PerspectiveCamera(38, width / height, 0.1, 100);
  camera.position.set(5, 4, 6);
  camera.lookAt(0, 1, 0);

  const cube = new THREE.Mesh(
    new THREE.BoxGeometry(1.6, 1.6, 1.6),
    new THREE.MeshBasicMaterial({ color: 0x7fe0d0, wireframe: true }),
  );
  cube.position.set(0, 1.2, 0);
  scene.add(cube);

  const render = (t) => {
    cube.rotation.y = t * 0.4;
    renderer.render(scene, camera);
  };

  const overlay = (ctx, t) => {
    ctx.save();
    ctx.fillStyle = "#f3ede1";
    ctx.font = '700 56px "Microsoft YaHei", sans-serif';
    ctx.textBaseline = "top";
    ctx.fillText("3D 场景模板", 80, 70);
    ctx.fillStyle = "#9fb6b4";
    ctx.font = '500 26px "Microsoft YaHei", sans-serif';
    ctx.fillText("示意：非仿真", 80, 145);
    ctx.restore();
  };

  return { render, overlay };
}
