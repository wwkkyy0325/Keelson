# Backlog

Cases that are not scheduled yet, with the evidence and the first step.

## Session document growth

Queued 2026-11-10. Task: `tasks/session-document-growth.md`.

Step 4 is open: the kernel snapshot. Steps 1–3 are done — new turns store no
event stream, tool output and thinking are capped, and the send path no longer
parses the document.

A live 8-turn conversation reached 1.5 MB on disk. Every send parses the
document (`is_archived`) and every turn rewrites it; a parse measured 21 ms
in a debug build at 874k characters. Measured composition (characters):
tool outputs 192k, kernel snapshot 318k, per-turn events 142k, thinking
134k, answers 10k. Dropping the events alone saves about 17%; the host-owned
tool output and thinking caps are the larger win, and the kernel snapshot is
a separate, kernel-side question.

First step: replace the stored per-turn events with the metrics the host
actually reads (usage, the last context point), keeping the events as a
read-only fallback for old documents.
