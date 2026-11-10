# ADR 0042 — Screen tools hand the model pixels

## Status

Accepted.

## Context

The host can read and edit files but cannot see what the user sees. A user
asks the agent to verify that an application shows something; the answer is
on the screen, in a window, a region or a whole monitor. The configured model
(`deepseek-flash`) accepts images, and the kernel now carries them (kernel ADR
0051), so the missing half is capture.

Two questions had to be answered: who owns the capture, and what the host does
with the pixels. OCR was considered and rejected: it returns text without
layout, colour or relationships, introduces a language-pack dependency, and is
worse than either the model reading the image or the user describing it.

## Decision

1. Capture is a **host** tool. The kernel stays platform-free; the host adds
   `list_windows` and `screen_capture` to its registry the way it adds the web
   tools. Every rig may list them, and only rigs that list them see them.
2. `list_windows` returns id, application, title and size, and asks for no
   permission: it reads metadata, not pixels.
3. `screen_capture` takes `target` = `window` (id from the list or a title
   fragment), `region` (`x,y,w,h`) or `screen` (monitor index). It scales the
   longest side to 1568 px, writes the PNG to
   `<state>/projects/<hash>/screens/<session>/` and attaches the image to the
   tool result. The tool performs no recognition and no interpretation.
4. The permission request's path is that folder. It lies outside the
   workspace and its roots, so `workspace_auto` confirms with the target in
   the prompt, "allow this path for the session" grants the folder and
   silences later captures, and `full_auto` never asks. No new permission
   category was added.
5. Images live for the turn that captured them (kernel ADR 0051). The file
   stays in the folder so the user can open it; the session document keeps
   the text and the path.

## Consequences

- Verification reads pixels: the model quotes what it sees instead of
  trusting a text extraction. It must quote visible text; the tool
  description says so.
- Capture is confirmed by default in `workspace_auto`. That is the only place
  the privacy cost is paid, and it names the window it is about to read.
- Screens are not part of the workspace and never enter a checkpoint diff.
- The capture path uses xcap's GDI implementation. With its Windows Graphics
  Capture feature enabled, window capture timed out on this machine
  (`timed out waiting on channel`) for every window while monitor capture
  worked; without the feature, every window captured. Protected or
  GPU-only surfaces may still come out black; the tool reports a failure
  rather than pretending.
- A later turn cannot answer a question about an older screenshot without
  capturing again. That is intended: the screen has probably changed.

## Alternatives rejected

- OCR in the host: text without layout, a language-pack dependency, and a
  worse answer than vision or a user description.
- A screenshot tool that encodes the image into text: the model cannot decode
  it.
- Capturing continuously: a silent screen recorder is not what a verification
  step needs and is a much larger privacy surface.
- Driving the UI (clicks, typing): capture is read-only; automation of the
  user's windows is a separate decision with its own safety design.
