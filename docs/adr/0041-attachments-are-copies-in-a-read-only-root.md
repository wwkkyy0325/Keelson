# ADR 0041 — Attachments are copies in a read-only root

## Status

Accepted.

## Context

The composer could only send text. Working with a document meant moving it
into the workspace first, and a file dropped from Downloads had no path the
agent could reach. The host needed a way to hand a file to the conversation.

Three questions had to be answered: where the copy lives, how the agent is
allowed to reach it, and what an image means.

## Decision

1. **A dropped or picked file is copied, not referenced, and the copy is
   made when the message is sent.** Choosing a file only records its source
   path in the composer, so browsing or removing a chip has no side effect,
   and the original is read once, at send time.
2. **The copy lives in the host state directory:**
   `<state>/attachments/<workspace-key>/<session-id>/`. It is not written into
   the workspace. A file in the workspace would appear in the turn's
   checkpoint diff as a change the turn made, and undo would delete the
   user's file.
3. **The attachment folder is a read-only root**, added when the conversation
   opens. The agent can read and list it, cannot write to it, and reads need
   no per-file approval. The same source path is one entry in the composer;
   sending an unchanged file again reuses the existing copy, and only a
   changed file with the same name gets a number (`notes (2).txt`).
4. **The paths travel in the message text.** The host appends a
   `Attachments (read-only):` block naming the copied paths, so the transcript
   and the model see the same list after a restart, with no new session
   schema.
5. **Limits:** 50 MB per file, 10 files per message. A batch is validated
   before anything is copied, so a rejected file leaves no partial set.
6. **Deleting a conversation deletes its attachment folder.**
7. **An image is just a file today.** The kernel's `Message.content` is a
   string and both adapters send text blocks only, so no model can see an
   image. An attached image is reachable through `shell` (probe, OCR), not
   through vision. Image understanding needs typed content blocks in the
   kernel, adapter support and a vision model — a separate decision.

## Consequences

- Dropping a file works with every rig, without a kernel change.
- The agent discovers the file only through the path in the message; it does
  not auto-read the bytes. That keeps context cost proportional to what the
  turn actually needs.
- Attachments survive restarts and are removed with their conversation.
- A copied file is a second copy on disk. The composer treats one source path
  as one attachment, and an unchanged file reuses its copy, so repeated drops
  or sends do not grow the folder. A changed file keeps a numbered snapshot
  next to the original copy.

## Alternatives rejected

- Referencing the original path: breaks when the file moves, and every read
  outside the workspace would prompt.
- Copying into the workspace: pollutes the user's project, appears in
  checkpoints, and undo would remove it.
- Passing file contents inline in the message: unbounded context cost, and
  binary formats cannot be represented as text.
- Images as base64 in the message text: the model cannot see them; it only
  spends tokens on a blob.
