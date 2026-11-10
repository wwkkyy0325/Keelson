# ADR 0043 — Desktop tools: look again, launch, and read controls

## Status

Accepted.

## Context

ADR 0042 gave the model a screen to look at. Three gaps remained for the
verification loop: an image is turn-scoped (a later turn cannot look at an
earlier screenshot), there is no way to start an application the check
needs, and pixels are a poor way to find a labelled control. The user
explicitly ruled out click-and-type automation ("one screenshot at a time is
too slow and too fragile"); these tools stay read-only apart from starting a
process.

## Decision

1. **`view_image(path)`** reads a PNG, JPEG, WebP or GIF from disk, resizes it
   to the same 1568 px bound as a capture when needed, and attaches it to the
   tool result. The read is a path permission: inside the workspace or a root
   it is free, outside it confirms. This is what makes a saved screenshot or
   an attached image viewable in a later turn.
2. **`launch_app(program, args)`** starts an application detached from the
   host (its own process group, no console) and returns the pid. The
   permission request is an `Execute` on the resolved program: a bare name is
   looked up on `PATH`, a relative path is resolved by the permission gate
   against the workspace. The process outlives the session; the user can
   close it.
3. **`wait_for_window(title)`** polls visible windows until a title contains
   the text, up to 30 s, and returns the window id and size. No permission:
   it reads titles, not pixels.
4. **`list_controls(window)`** walks the UI Automation control view of the
   window whose title matches, returning control type, name, class and screen
   bounds, bounded to 200 elements and six levels. No permission, but it is a
   read of the window content.
5. All four are host tools beside the capture tools, listed by every rig, and
   the kernel does not change. No clicks, no typing, no key injection: the
   agent looks and launches, the user acts.

## Consequences

- The verification loop can now be: launch → wait → capture or list controls
  → quote what is there → report, across more than one turn when
  `view_image` reopens the saved file.
- `launch_app` is the one mutating tool of the group; the confirmation names
  the executable. `full_auto` starts it without a prompt, like any other
  execution.
- `list_controls` is more reliable than pixels for labelled controls and
  returns nothing useful for custom-drawn surfaces; the fallback is the
  screenshot.
- UI Automation is desktop-only; the tools compile everywhere but return an
  error where the platform has no UIA.

## Alternatives rejected

- Click and type automation: the user rejected it as slow and fragile, and it
  is a much larger safety surface (ADR 0042's read-only boundary holds).
- Re-capturing to see an old image: wrong when the screen has changed and
  wasteful; the file already exists.
- Storing screenshots in the session so history can show them: the document
  would grow and the image is stale evidence (kernel ADR 0051).
- A UIA-only agent (no pixels): charts, canvases and rendered documents have
  no control tree; both views are needed.
