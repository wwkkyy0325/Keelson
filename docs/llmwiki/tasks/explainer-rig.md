# Explainer rig

Goal: a fourth rig whose deliverable is a narrated explainer film, with the
film's claims and layout checked before anyone watches it.

## 1. Render tool — done

- [x] `render_scene(root, scene, duration_ms, fps)` serves the project on
      loopback, opens `kit/scene.html` in headless Edge or Chrome, and writes
      one PNG per frame posted back by the page.
- [x] A non-blocking listener with a blocking per-connection handler; a
      dropped speculative connection is not a render failure.
- Verify: `a_static_request_cannot_leave_the_root`; the live
      `a_scene_renders_and_a_file_probes` and
      `the_shipped_kit_renders_a_scene` (30 frames for a second at 30 fps).

## 2. Probe tool — done

- [x] `probe_media(path)` reports duration, size, video codec/geometry and
      audio streams through ffprobe.
- Verify: `a_probe_is_summarized`, `a_probe_without_audio_says_so`; the live
      test probes a film built from rendered frames.

## 3. The rig — done

- [x] `rigs/explainer.toml`, `profiles/explainer-live.toml` and
      `profiles/prompts/explainer.md`: the four gates, the measured timeline,
      the data contract, one term per concept, simplification labels, the
      subtitle band and the audio target.
- Verify: `the_explainer_rig_exposes_render_and_probe`; `keelson doctor`.

## 4. The kit — done

- [x] `kits/explainer/`: design system, frame harness, timeline, assembly,
      QA and a working `data/` contract (spec -> build -> model -> check).
- Verify: `data/build.py` and `data/check.py` agree on the example; the kit
  renders through the host tool in the live test.

## 5. Evidence and material — done

- [x] `claims.json` classifies every claim as fact, interpretation or value.
      `data/check_claims.py` requires two independent publishers or one
      primary source for a fact, a named basis for an interpretation, an
      on-screen label for a value, and both sides for a conflict. A scene
      names the claims it uses, so the chain is claim -> source -> scene.
- [x] `assets/` is the only picture material besides programmatic scenes;
      the rig does not download images, and every asset records its license.
- Verify: `the_claim_gate_needs_independent_sources` (one publisher fails,
  two pass, an unlabelled value fails).

## 6. Live proof — done

- [x] One turn on a real argumentative topic produced the ledger, the script
      and the storyboard; the gate passed, one dead citation was found, and a
      repair turn replaced it. 15 sources checked, none 404.
- Verify: `tests/explainer_eval.rs` (ignored; needs a key). The run exposed a
  kernel issue: `deepseek-flash` runs in thinking mode and its thinking block
  is not echoed back on a tool follow-up, so the run used `deepseek-chat`.

## 7. Documentation — done

- [x] ADR 0044; design and README list four rigs; changelog.

## 8. 3D scenes — done

- [x] `kit/scene3d.html` composes a WebGL layer under the 2D labels; Three.js
      r160 is vendored (MIT); `render_scene` takes `page` and launches the
      browser with SwiftShader. The flow field and camera are analytic and
      seeded, and a 3D picture is labelled 示意.
- Verify: `a_page_must_stay_under_the_kit`; the live 3D kit test; the AC
      airflow pilot (12 s, 1080p) passed frames, subtitles and loudness.

## 9. 3D boundaries and pilot polish — done

- [x] The camera fits the room to a safe rectangle (title and subtitle bands
      excluded) and recentres it vertically, so the near floor corner no
      longer clips and the room fills the band instead of sitting small and
      low. The fit repeats per frame as the camera drifts.
- [x] The flow is one closed stream function whose normal velocity vanishes on
      every wall, with an intake that returns air to the vent. The old clamp
      is gone; particles no longer stick to a ceiling plane.
- [x] Temperature relaxes to a spatial ambient field, so the colour shows
      where the air is rather than when the particle was born. A slow seeded
      noise breaks the cold/warm boundary into wisps.
- [x] Supply air leaves the vent with a decaying impulse, so the jet is
      visible instead of the plume appearing from nowhere.
- [x] Parcels draw as curved three-segment trails faded with vertex alpha;
      a dimmed colour painted black lines over the dark room.
- [x] The page squares the 3D layer, blurs it and adds it back before the
      labels: bright streaks glow, the walls do not fog, text stays sharp.
      The safe box shrinks five percent over the shot for a slow push-in.
- [x] The unit gained a display, a contact shadow and a vent detail; the floor
      and grid were lifted so the room does not float.
- [x] Labels are anchored to projected 3D points and their arrows come from
      the local field direction, so a label cannot contradict the flow.
- Verify: pilot re-rendered (355 frames, 38 s); consecutive-frame difference
      has no spike (mean 3.45, max 3.73 grey levels); the kit's live 3D test
      passes with the bloom page; QA passed at 12.00 s, 1080p, −18.04 LUFS;
      the film is 16 MB.
