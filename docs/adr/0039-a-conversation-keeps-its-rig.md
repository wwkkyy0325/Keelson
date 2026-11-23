# ADR 0039 — A conversation keeps the rig it started with

## Status

Accepted.

## Context

The desktop chose the rig from `DesktopConfig.rig` every time it opened a
conversation. The session document recorded the rig, but opening a session
ignored that field, so changing the setting silently changed the tool set and
persona of conversations already in progress. It also made the rig chip under
the composer a button that opened settings, which promised a change it could
not deliver to the open conversation.

A rig decides the tools and the system prompt, and both are held by the engine
for as long as the conversation is open. A conversation is one thread with one
persona: a rig switch in the middle would mean rebuilding the engine against
the existing history and changing the persona mid-thread.

## Decision

1. The rig is fixed when the conversation's first turn is written. `open_inner`
   runs the rig recorded in the session document; `DesktopConfig.rig` is the
   default for a conversation that has no recorded rig yet.
2. The empty conversation is the only place a one-off choice is made. While a
   conversation has no turns and no turn is running, the status line shows a
   rig select. Picking a rig saves it as the default and reopens the blank
   conversation under the same id, so the first turn is written with it and
   anything typed in the composer survives.
3. After the first turn the rig segment is plain text. It no longer opens
   settings; the header gear does.
4. A recorded rig whose file has been deleted falls back to the setting.

## Consequences

- Changing the setting no longer touches an open conversation, and this is
  visible: the status line names the rig that is actually running.
- Choosing a rig in an empty conversation also makes it the default for the
  next new conversation. The choice is sticky rather than a hidden one-off
  state, so there is one place to see what will run.
- There is no way to switch a conversation's rig. To use another rig, start
  another conversation.

## Alternatives rejected

- Switching mid-conversation: the engine would have to be rebuilt against the
  history, and a persona that changes mid-thread is not a feature.
- Keeping the choice only in the frontend: it would be lost on restart, and
  the session document already records the rig.
- Making the setting the source of truth and pinning only in the UI: the
  document is what survives a restart, and the host has no reason to distrust
  it.

## Amendment — the chosen rig is the one that runs

`open_inner` returned the already-open engine for any conversation the window
had open, and it did so before resolving the rig. `chooseRig` reopens the same
id, so that early return kept the engine opened with the old rig: the status
line named the old rig, the first turn ran with it, and the document recorded
it, while the setting already held the new one. Point 2 of the decision was
therefore true only on paper.

A blank conversation — no document, no streaming turn, no background command —
is now rebuilt under the same id, so the rig the status line names is the rig
that runs the first turn. A conversation with a document, or with work in
flight, keeps its engine, which is the rest of the rule.

The status line's rig comes from the view that opened the conversation, not
from a separately fetched status: a status answer also names the conversation
it describes, and one that arrives after the window has moved on is dropped.
