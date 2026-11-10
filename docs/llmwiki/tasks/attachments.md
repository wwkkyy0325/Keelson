# Attachments — files in the composer

## Goal

A file dropped on the window or picked with the paperclip becomes a path the
agent can read, in every rig, without a kernel change. Images are accepted as
files but are not sent to the model (it has no vision channel).

## Steps

- [x] **1. Copy the file** — choosing a file only records its source path;
      `attach_files` runs on send and copies each path into
      `<state>/attachments/<workspace-key>/<session-id>/`. An unchanged file
      reuses its copy, a changed one is numbered, and a file over 50 MB or a
      batch over 10 is refused. Every file in a batch is validated before the
      first copy.
      Verify: `an_unchanged_file_is_reused_and_a_changed_one_is_numbered`,
      `an_oversized_attachment_is_refused`.

- [x] **2. Make it readable** — the folder is added as a read-only root when
      the conversation opens, so reads work without a prompt and writes are
      refused. It lives outside the workspace, so checkpoints never list it.

- [x] **3. Composer** — paperclip (dialog plugin) and a window-level native
      drop, chips with a remove control, and a send that appends an
      `Attachments (read-only):` block of paths to the message. A send
      restores the chips if the backend rejects it.

- [x] **4. Lifecycle** — deleting a conversation removes its attachment
      folder.

- [x] **5. Documentation** — ADR 0041 records why copies, why the state
      directory, why a read-only root, and what an image currently is.

- [x] **6. Folders** — a dropped or picked directory is copied as a bounded
      snapshot: 200 files, 200 MB, 8 levels, 400 MB per batch; symlinks and
      `.git` are skipped. Past the bounds the copy is refused and the
      suggestion is to open the folder as the workspace, since attachments are
      read-only. An unchanged folder reuses its snapshot; a changed one is
      numbered.

## Verification

```bash
cd desktop
npm run check && npm test && npm run build
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Plus a manual drop: the chip appears, the message carries the path, and the
agent can read the file; the workspace and its diff stay clean.

## Not in this task

- Images as model input. It needs typed message content in the kernel, both
  adapters, and a vision model.
- Pasting an image from the clipboard: the clipboard gives bytes without a
  path, and the file tools work on paths.
