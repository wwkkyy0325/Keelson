# Task — render tools declare the writes they request

## Goal

`render_scene` and `render_film` failed with "declared read-only but requests
write access", even in full auto because the dispatcher rejects the pairing
before the permission gate.

## Steps

- [x] Declare `Mutating` on `render_scene` and `render_film`; they write
  `out/frames/<scene>/`. Verify: `the_render_tools_declare_the_writes_they_request`
  in `src/engine/video.rs`.
- [x] Fix the same latent pairing on `screen_capture`; it writes its PNG to
  the capture folder. Verify: the risk assertion in
  `a_capture_asks_for_its_own_folder` in `src/engine/screen.rs`.
- [x] Resolve a relative `out` against the project root, not the app's cwd.
  Verify: `a_relative_out_belongs_to_the_project`.
- [x] Budget the render from the frame count and let the tool raise its call
  timeout above the session's 300 seconds. Verify:
  `a_tool_can_raise_its_timeout` in the kernel and the timeout assertion in
  `the_render_tools_declare_the_writes_they_request`.

## Notes

- With full auto the write runs without a prompt. With workspace auto, a write
  inside the workspace is confirmed and one outside it needs a grant.
- The film workflow is unchanged: `render_scene` for anchors, `view_image`
  for frames, `render_film` once the storyboard and timeline are final.
