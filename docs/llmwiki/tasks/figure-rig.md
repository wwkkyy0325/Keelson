# Figure rig

A film rig whose scenes are assembled from components, with bones at every
moving point and springs for secondary motion.

## Steps

- [x] `kit/rig.js`: bones with world transforms, two-bone IK (`ik2`), parts
      drawn in a bone's frame, Verlet spring chains with a pinned root.
- [x] `kit/parts.js`: registry and on-demand loader. A component returns
      `{ bones, parts, z, update(t), step(dt) }`.
- [x] Four components under `kit/parts/`: `sky`, `waves`, `rock`, `tower`,
      each registering itself. `still.html` (temporary) rendered fixed times
      for inspection.
- [x] `scenes/lighthouse.js`: six wave bands (19 bones each), a swaying tower,
      a sweeping beam bone and a pennant spring.
      Verify: two headless stills at t=1.2 and t=2.8; the beam moved, the
      waves stacked, the pennant trailed.
- [x] `rigs/figure.toml`, `profiles/figure-live.toml`,
      `profiles/prompts/figure.md`. `tests/rig_tools.rs` now opens the figure
      rig and checks its tool set and persona.
- [x] The kit embeds through `build.rs`, so `parts/` ships with the seed.

## Not done

- The portrait piece: needs a licence-clear headshot, a plate split along its
  joints, and each piece bound to a bone.
- GPU stylization (`stylize-gl.js`). The scene is fully procedural today, so
  nothing in it needs a per-pixel pass yet.
- The waves still read as flat bands; they need value separation between
  neighbouring bands and a texture pass.
