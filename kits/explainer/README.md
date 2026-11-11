# Explainer kit

A starter project for the `explainer` rig. Copy it into the project once, then
write the script and the scenes for the topic.

```
kit/design.js       palette, type scale, motion helpers, the mascot
kit/scene.html      the 2D frame harness: draws t, posts one PNG per frame
kit/scene3d.html    the 3D harness: WebGL layer composited under 2D labels
kit/three.module.min.js  Three.js r160 (MIT), vendored for offline renders
kit/timeline.py     narration -> measured durations -> timeline + subtitles
kit/assemble.py     frames + voice -> scenes -> film, burns subtitles
kit/qa.py           data, blank frames, subtitles, video and loudness checks
data/spec.json      the only hand-written data (an example: 10x10 image, 3x3 kernel)
data/build.py       derives data/model.json
data/check.py       recomputes the numbers; qa.py runs it
data/check_claims.py  checks the evidence ledger; qa.py runs it
data/model.json     what the scenes draw
claims.json         one entry per claim: fact, interpretation or value
assets/             files the user supplied, with their license in assets.md
scenes/*.js         one file per scene: draw(ctx, t), or createScene for 3D
storyboard.example.json  the contract's shape; copy to storyboard.json
```

## Run it

```bash
python data/build.py          # spec -> model
python data/check.py          # independent recompute
python kit/timeline.py        # voice, measured durations, subtitles
# render each scene with the rig's render_scene tool, or:
#   render_scene(root, "01", duration_ms, 30)
python kit/assemble.py        # film
python kit/qa.py --video      # gates + contact sheet
```

## The rules the kit enforces

- A number on screen comes from `data/model.json`; scenes only draw it.
  `data/check.py` recomputes it, and `qa.py` fails the build on a mismatch.
- Duration is measured, not typed: `timeline.py` writes it from the audio.
- Subtitles never overlap; a cue is at most two lines of 28 characters, and
  content stays above the ~280 px subtitle band.
- The film is the size and length the storyboard says, has audio, and sits
  near -16 LUFS.
- A fact claim has two independent publishers or one primary source, with a
  verbatim quotation and an access date. An interpretation names its
  framework; a value judgment carries an on-screen label. A conflict keeps
  both sides.
- A scene that shows a fact fills `source_card`; `glossary` terms that appear
  on screen must be in that scene's narration; durations are never typed.
- Pictures are scenes plus user-supplied `assets/`; the rig does not download
  images, and every asset records its license.

## 3D scenes

A 3D scene uses `kit/scene3d.html` and exports `createScene({ THREE, renderer,
width, height, fps, duration })` returning `render(t)` and `overlay(ctx, t)`.
Render it with the tool's `page` argument:

```
render_scene(root, scene, duration_ms, fps, page: "kit/scene3d.html")
```

WebGL runs on SwiftShader in the headless browser, so no GPU is required. The
flow, camera and particles must be analytic and seeded, and a 3D picture is
labelled 示意 unless a real simulation produced it. Three.js is vendored at
r160 under the MIT license.

## The example data

`data/spec.json` is a convolutional-network demo so the kit works out of the
box. Replace the spec, `build.py`, `check.py` and `model.json` with the
topic's own data, and write the scenes against `window.DATA`.
