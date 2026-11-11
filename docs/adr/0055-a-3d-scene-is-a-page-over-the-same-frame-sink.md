# ADR 0055 — A 3D scene is a page over the same frame sink

## Status

Accepted.

## Context

Some explainers are spatial: a room, an air conditioner, cold air sinking and
warm air rising. A flat diagram can label that, but a camera and depth make
the loop legible. The rig already renders a page frame by frame and posts each
PNG back; the question was whether that path can carry WebGL, and what should
be fixed about a 3D scene.

Headless Edge renders WebGL through SwiftShader with
`--enable-unsafe-swiftshader`, which a live test confirmed at 1080p. Three.js
provides the scene graph, camera and points without a build step. A flow field
and a seeded particle set are deterministic, so a frame is reproducible and
checkable like any other.

## Decision

1. A 3D scene is `kit/scene3d.html`: it imports the vendored Three.js, creates
   a WebGL renderer on an offscreen canvas, and asks `scenes/<id>.js` for
   `createScene` returning `render(t)` and `overlay(ctx, t)`. The 3D layer is
   composited under the 2D labels, so the design kit and subtitles still
   apply.
2. `render_scene` takes an optional `page` (default `kit/scene.html`),
   restricted to an HTML file under `kit/`, and launches the browser with
   `--enable-unsafe-swiftshader` so WebGL works without a GPU.
3. Three.js r160 is vendored in the kit (MIT, license header retained)
   instead of fetched at render time, so a render is offline and pinned.
4. Motion is analytic and seeded. The flow field, camera path and particle
   seeds are part of the scene file, and the same `t` renders the same frame.
5. A 3D picture is labelled 示意 unless a real simulation produced it. The
   frame says "非 CFD 仿真"; a film that needs physical accuracy must cite the
   simulation or measurements it uses.

## Consequences

- The kit grows by about 670 KB (Three.js) and a 3D page; a 2D-only film pays
  nothing at runtime.
- WebGL runs on the CPU, so a long 3D shot renders slower than a flat one;
  the pilot rendered 11.8 s of 1080p in about 16 s.
- Three.js is the only new third-party dependency in the kit; it is scoped to
  the scene page and has no host code.
- A 3D scene is still not a simulation: the particles illustrate a model, and
  the label keeps the film honest.

## Alternatives rejected

- An external AI video service: a new network dependency and
  non-deterministic output, and the spatial model would still need to be
  authored.
- A 2.5D isometric canvas: cheaper, but a rotating camera and depth are the
  reason to use 3D at all.
- Blender or an offline renderer: a much heavier toolchain for a diagram that
  a few thousand points and a camera already carry.
- Fetching Three.js from a CDN at render time: a render must work offline and
  must not silently change version.
