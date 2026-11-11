# Explainer kit

A starter project for the `explainer` rig. Copy it into the project once, then
write the script and the scenes for the topic.

```
kit/design.js       palette, type scale, motion helpers, the mascot
kit/scene.html      the frame harness: draws t, posts one PNG per frame
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
scenes/*.js         one file per scene, each a draw(ctx, t) function
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
- Subtitles never overlap; content stays above the bottom 140 pixels.
- The film is the size and length the storyboard says, has audio, and sits
  near -16 LUFS.
- A fact claim has two independent publishers or one primary source, with a
  quotation and an access date. An interpretation names its framework; a
  value judgment carries an on-screen label. A conflict keeps both sides.
- Pictures are scenes plus user-supplied `assets/`; the rig does not download
  images, and every asset records its license.

## The example data

`data/spec.json` is a convolutional-network demo so the kit works out of the
box. Replace the spec, `build.py`, `check.py` and `model.json` with the
topic's own data, and write the scenes against `window.DATA`.
