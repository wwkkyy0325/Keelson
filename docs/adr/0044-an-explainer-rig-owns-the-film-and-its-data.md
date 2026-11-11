# ADR 0044 — An explainer rig owns the film and its data

## Status

Accepted.

## Context

The three existing rigs produce code, cited answers and documents. A fourth
output is a narrated explainer film: a script, a storyboard, programmatic
scenes and a voiceover, assembled locally. Earlier attempts produced films
that were mostly text and simple charts, and that could show a number in one
shot that disagreed with the same number in another.

A film has two properties the other rigs do not. Its artifact is a media file,
which can be probed but not read like a document; and its pictures make
claims, so a wrong number or a misleading diagram is a defect even when the
file plays. The host already had the pieces to check it: headless Edge can
render a scene, `view_image` can look at a frame, and ffprobe can report a
finished file.

## Decision

1. `explainer` is a rig whose deliverable is the film file, with
   `script.md` and `storyboard.json` as its working artifacts. The stages are
   not separate rigs: a conversation keeps one rig (ADR 0039), and the
   iteration loop spans all of them.
2. The rig stops at four gates: the script, the storyboard, the style anchor
   and contact sheet, and a preview of the first thirty seconds. The full film
   is rendered only after the preview is approved.
3. Duration is measured, not authored. The storyboard carries narration and
   visual intent; the timeline comes from the generated audio, and the
   subtitles are shifted onto the same timeline.
4. Numbers have one source. `data/spec.json` is hand-written; `data/build.py`
   derives `data/model.json`; scenes draw it and never compute or type a
   result. `data/check.py` recomputes the claims independently and `qa.py`
   runs it.
5. The host gains two tools: `render_scene` renders a project's scene to a
   PNG sequence through a headless browser and a loopback frame sink, and
   `probe_media` reports duration, geometry and streams through ffprobe.
   Neither tool edits, assembles or understands content.
6. A starter kit ships in `kits/explainer/`: the design system, the frame
   harness, the timeline, assembly, QA and an example `data/` contract. It is
   copied into a project once; the rig is still only a file plus tools.

## Consequences

- The host stays thin: no per-rig host branch, no video editor in the engine.
  The script, scenes and data live in the project and are editable by hand.
- A film cannot pass its checks with a wrong convolution, an overlapping
  subtitle or no audio, and the same checks run for every topic.
- The rig needs a local browser (Edge or Chrome), ffmpeg/ffprobe, Python with
  `edge-tts`, and Pillow. A missing tool is reported by the tool, not guessed.
- The art is programmatic: vector scenes and a mascot, not photographic or
  generated footage. Stock and image APIs remain a separate, later decision
  with their own ADR.

## Alternatives rejected

- Separate `script` and `video` rigs: the conversation would break in the
  middle of the loop, and a prose script alone has no verifiable artifact.
- A generic `video` rig with per-medium prompts: it would verify nothing, the
  failure ADR 0040 exists to prevent.
- Calling an external AI video service in v1: a new network dependency and
  nondeterministic output before the local pipeline was proven.
- Rendering through Remotion: a heavier project dependency for a frame model
  the kit already implements.
