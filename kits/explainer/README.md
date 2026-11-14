# Explainer kit

A starter project for the `explainer` rig. The desktop seeds it into an empty
project when an explainer conversation opens, writing only files that are
missing; copying by hand is only needed for the CLI or a project kept outside
Keelson.

```
kit/design.js       palette, type scale, motion helpers, the mascot
kit/motifs.js       the moving picture: waves, hills, crowd, stroke, particles, camera
kit/scene.html      the 2D frame harness: draws t, posts one PNG per frame
kit/scene3d.html    the 3D harness: WebGL layer composited under 2D labels
kit/three.module.min.js  Three.js r160 (MIT), vendored for offline renders
kit/timeline.py     narration -> measured durations -> timeline + subtitles
kit/assemble.py     frames + voice -> scenes -> film, burns subtitles
kit/qa.py           data, blank frames, subtitles, video and loudness checks
GLOSSARY.md         English -> Chinese names for the kit's terms
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

## Series

A one-video project uses this folder as its root. A series uses the root for
the bible, the index and shared assets, and one self-contained copy of this
kit per episode:

```
series.md / series.json     the show and its episode index
assets/                     files shared by every episode
episodes/001-slug/          kit/, scenes/, data/, out/ and the episode docs
releases/001-slug/          film.mp4, subtitles, cover and publish.md
```

`new_episode(slug, title)` creates the episode folder and seeds this kit;
`archive_episode(episode, status)` copies the finished cut to `releases/`,
deletes `out/frames` and updates `series.json`. Do not copy the kit by hand.

## Styles

`kit/style.js` selects the look; `design.js` turns it into tokens and
helpers. A series records one style in `series.json`, and `new_episode`
copies the preset into the episode:

- `paper-editorial` — warm paper, ink type, vermilion accent, dot grid, cat.
- `warm-documentary` — muted film colour, grain, serif quotations, sun.
- `dark-technical` — near-black blueprint, cyan/amber, monospace, node.
- `bright-explainer` — white, saturated blocks, heavy sans, rounded cards.
- `swiss-minimal` — white/black/red, strict grid, square panels.

Scenes use only `D.C`, `D.alpha(color, opacity)`, `D.fonts`, `D.panel`,
`D.headline` and `D.mascot`; a literal colour or font in a scene pins it to one
palette. `copy kit/styles/<name>.js kit/style.js` is the only edit needed to
switch a single-video project.

## Run it

```bash
python data/build.py          # spec -> model
python data/check.py          # independent recompute
python kit/timeline.py        # voice, measured durations, subtitles
# render the whole film with the rig's render_film tool:
#   render_film(root)
python kit/assemble.py        # film
python kit/qa.py --video      # gates + contact sheet
```

## The rules the kit enforces

- A number on screen comes from `data/model.json`; scenes only draw it.
  `data/check.py` recomputes it, and `qa.py` fails the build on a mismatch.
  A schematic film makes no numeric claims and may omit `data/`;
  `window.DATA` is then `{}`.
- Duration is measured, not typed: `timeline.py` writes it from the audio.
- The scene page posts each frame as a PNG blob and reports its own failure to
  the sink (`/error`); a frame POST is retried three times. The renderer fails
  after ninety seconds without a new frame instead of waiting out its budget.
- Subtitles never overlap; a cue is at most two lines of 28 characters, and
  content stays above the ~280 px subtitle band.
- The film is the size and length the storyboard says, has audio, and sits
  near -16 LUFS.
- Subtitles burn with a CJK font. Under WSL, `assemble.py` points libass at
  `/mnt/c/Windows/Fonts`; if no CJK font is available it stops instead of
  burning tofu boxes.
- A fact claim has two independent publishers or one primary source, with a
  verbatim quotation and an access date. An interpretation names its
  framework; a value judgment carries an on-screen label. A conflict keeps
  both sides.
- A scene that shows a fact fills `source_card`; `glossary` terms that appear
  on screen must be in that scene's narration; durations are never typed.
- Pictures are scenes plus user-supplied `assets/`; the rig does not download
  images, and every asset records its license.

## Mixed films

A film can mix two registers. Every storyboard scene declares `asset_type`:
`canvas` (the default) is the flat design kit; `scene3d` is the WebGL page.
One tool renders the whole film after the timeline has measured it:

```
render_film(root)
```

It reads `storyboard.json` and `out/timeline.json`, takes each duration from
the timeline, and chooses `kit/scene.html` or `kit/scene3d.html` per scene.
`render_scene` remains the way to iterate on a single scene.

The split is editorial, not decorative:

- **2D carries the model.** Concepts, numbers, steps, comparisons, labels and
  any text that has to be read. It is the film's reading surface.
- **3D carries space.** Geometry, fields, flows, depth and a camera. It is the
  film's seeing surface, and it never carries a table or a sentence.
- Choose per section by the viewer's question: "where does it sit, where does
  it go" wants 3D; "what is the rule, how much, in what order" wants 2D.
- Alternate at topic boundaries, not mid-thought. Keep one 3D space per
  section, and reuse its model and camera across its shots.
- Unless the topic is spatial, keep 3D to about a third of the runtime; a
  spatial film can be mostly 3D, anchored by a few 2D pages.
- Both registers share one voice, one subtitle band, one type system and one
  measured timeline.

By topic:

| topic | shape |
|-------|-------|
| a spatial mechanism (air conditioner, engine, weather) | 3D anchors the space; 2D explains the loop, the numbers and the energy |
| an abstract model (a CNN, an algorithm, a statistic) | 2D throughout; 3D only for an object or a spatial metaphor, and label it |
| an argument (society, policy) | 2D only; the claim ledger is the spine |
| a product demo | 3D for the object; 2D for features, specs and labels |
| a place or a history | 2D for the narrative; 3D for a reconstructed place, if one exists |

The Chinese names for these terms are in [GLOSSARY.md](GLOSSARY.md).

## 3D scenes

A 3D scene uses `kit/scene3d.html` and exports `createScene({ THREE, renderer,
width, height, fps, duration })` returning `render(t)` and `overlay(ctx, t)`.
In a film, mark the scene `asset_type: "scene3d"`; `render_film` picks the
page. Use `render_scene` with the `page` argument only while iterating on that
scene:

```
render_scene(root, scene, duration_ms, fps, page: "kit/scene3d.html")
```

WebGL runs on SwiftShader in the headless browser, so no GPU is required. The
flow, camera and particles must be analytic and seeded, and a 3D picture is
labelled 示意 unless a real simulation produced it. Three.js is vendored at
r160 under the MIT license.

Boundaries are part of the scene:

- Fit the camera to a safe rectangle, title and subtitle bands excluded, and
  re-fit as the camera drifts. Do not let the near floor corner clip.
- Keep a closed room closed in the model: a stream function whose normal
  velocity vanishes at the walls, plus an intake that returns air to the vent.
  Clamping particles to a wall leaves a visible line of stuck particles.
- Anchor labels to projected 3D points and draw the arrows from the local
  field direction.
- The page squares the 3D layer, blurs it and adds it back before the labels,
  so bright streaks glow and the text stays sharp. Fade a trail with vertex
  alpha, not a dimmed colour: a dimmed colour paints black lines.

## The example data

`data/spec.json` is a convolutional-network demo so the kit works out of the
box. Replace the spec, `build.py`, `check.py` and `model.json` with the
topic's own data, and write the scenes against `window.DATA`.
