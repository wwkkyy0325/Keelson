# ADR 0056 — A film renders from the storyboard

## Status

Accepted.

## Context

The explainer kit can draw a film on paper and in a 3D room. A mixed film uses
both in one timeline. Until now each scene was rendered with its own
`render_scene` call, and the caller had to pass the page: one wrong page and a
3D scene is drawn flat, or a 2D scene asks for WebGL. The choice of register is
a property of the film, not of the call; it already lives in the storyboard's
`asset_type`.

The durations also belong to the film: `kit/timeline.py` measures them from
the narration and writes `out/timeline.json`. A renderer that takes durations
from the caller can drift from the voice.

## Decision

1. `render_film(root)` reads `storyboard.json` and `out/timeline.json`,
   validates both, and renders every scene in storyboard order.
2. `asset_type` picks the page: `canvas` (the default) is `kit/scene.html`,
   `scene3d` is `kit/scene3d.html`. Any other value is refused.
3. The duration of a scene is the timeline's measured value; it is not a tool
   argument. `fps` may come from the storyboard's `format` or the caller.
4. `render_scene` remains for iterating on one scene, where a page argument is
   still useful. `render_film` is the path for a finished film.
5. The tool renders only. `kit/assemble.py` and `kit/qa.py` still own the film
   and its gates.

## Consequences

- One call renders a mixed film; the register is declared once, in the
  storyboard, and the renderer cannot disagree with it.
- The tool refuses a film whose scenes are missing from the timeline, whose
  ids are not plain, whose ids repeat, or whose durations are unusable; the
  error names the scene.
- A 3D scene needs `kit/scene3d.html` and the vendored Three.js; a 2D scene
  needs the kit's `data/model.json`, because `kit/scene.html` loads it.
- A failure leaves the frames already written for earlier scenes; re-running
  clears each scene's own folder.

## Alternatives rejected

- A shell loop over `render_scene`: no permission path for the frames, and the
  page would be duplicated in prose.
- A per-scene `page` field in the storyboard: two sources of truth for the
  register; `asset_type` is the semantic one.
- One page that switches on `asset_type`: a page cannot know the film, and it
  would merge two different harnesses.
- Reading durations from the storyboard: the timeline measures them; a typed
  duration drifts from the voice.
