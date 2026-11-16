# The ui rig

The second rig whose artifact is not a repository behaviour. A change here is
accepted by looking at a rendered page and measuring it, not by the project's
test command — which is why it is a rig and not a mode of the coding rig
(ADR 0040).

## Boundary

A profile (`profiles/ui-live.toml`), a persona (`profiles/prompts/ui.md`) and
the tools those two make visible. The repository tools stay: a layout change is
an edit to the same tree the coding rig would touch. What the rig adds is the
way to see the result.

It does not own a theme, a component library or a screenshot database. It does
not add per-rig behaviour to the app: the desktop shows the same picker it
showed before, and `rigs/ui.toml` is the only thing that has to exist.

## Tools

| Tool | What it gives the rig |
|---|---|
| `view_image` | opens a capture; the persona forbids naming one that was not opened |
| `screen_capture` | PNG of a window, by id or title fragment; includes the window chrome |
| `launch_app`, `wait_for_window` | a browser at `--window-size=390,844`, then its window |
| `render_scene` | canvas pages under `kit/` to a PNG sequence, at a fixed 1920×1080 window |
| `list_controls` | the accessibility tree, on Windows only (stripped elsewhere in `engine/mod.rs`) |
| `read_file`, `edit_file`, `multi_edit`, `apply_edits`, `git_diff`, `shell`, `todo`, `search_history` | the same editing loop as coding |

`render_scene` has no viewport parameter. Its browser is launched at
`--window-size=1920,1080`, and the page is fetched as
`/<page>?scene=<name>&duration=<sec>&fps=<fps>`, posting one PNG per frame to
`/frame?scene=…&index=0000`. A width therefore has to come from inside the
page: an iframe of the target width, or a container the layout keys off. One
still is a `duration_ms` and an `fps` whose product is 1.

## The contract

`profiles/prompts/ui.md` carries four requirements, and the reply is checked
against them:

1. A token file owns colour, type, space, radius, duration and easing. A
   component writes a token, never a literal. With no token file in the
   project, one is derived from the values already in use before any screen
   changes.
2. 390, 768 and 1440 CSS px, captured and opened. Misalignment is a grid
   defect, so `margin-left: -3px` to make one capture line up is refused.
3. States (default, hover, focus-visible, active, disabled, empty, loading,
   error) and a motion table: element, trigger, property, duration token,
   easing token. `prefers-reduced-motion: reduce` gets a static equivalent.
4. Verify: the project's own checks, then the three captures, then the states
   touched, then the measurements — no horizontal overflow, no text clipped by
   a fixed height, no control under 44px.

## Limits worth knowing

- There is no headless screenshot tool for arbitrary HTML. A real page is
  captured from a real window, so the run needs a display; `render_scene` is
  the only deterministic path and it draws to a canvas.
- A capture proves geometry, hierarchy, contrast and clipping. Hover, focus and
  drag are forced states, and the reply has to say so.
- `.keelson/checks.toml` runs at the workspace root on every verified turn and
  cannot name a working directory, so only seconds-fast static checks belong
  there: colour and size literals outside the token file, missing `alt` or
  label, a transition without a named duration. Overflow and alignment are
  measured from the rendered page, not from the source.
- The rig ships no evaluation set yet. ADR 0040 asks for one per rig; until
  then `tests/rig_tools.rs` and `tests/rig_contract.rs` check the wiring, not
  the quality.

## Next candidates

1. A `ui` eval: one fixture page, a recorded run of the coding rig and one of
   this rig, compared on the four contract points.
2. A deterministic `render_page` (headless browser `--screenshot` with a width)
   so a viewport matrix does not need a visible window.
3. The kit question: `kits/explainer` is seeded into a film project. If a `ui`
   kit is ever seeded the same way, the seeding must not key off the rig name
   in the app.
