# M5 — A second rig and a rig fixed per conversation

## Goal

Prove the host is a shell: one non-coding rig built from a file, with no
engine change, and a conversation that keeps the rig it started with.

## Steps

- [x] **1. The office rig** — `rigs/office.toml` → `profiles/office-live.toml`
      → `prompts/office.md`. Tools drop the code-only ones (`git_diff`,
      `apply_edits`); the persona covers documents, writing and file
      organisation rather than a codebase. No engine change.
      Verify: `cargo run -- rig show office` and `cargo run -- doctor`.

- [x] **2. A conversation keeps its rig** — `open_inner` reads the rig recorded
      in the session document and runs it; the setting chooses the rig only
      for a conversation that has no recorded rig yet (a new one). A recorded
      rig whose file is gone falls back to the setting.
      Verify: a desktop test opens a session with rig A, changes the setting
      to B and reopens it: the session still runs A.

- [x] **3. The setting is a default** — `settings.rigHint` says the setting is
      what new conversations start with, and a saved change no longer claims
      to change the open conversation.

- [x] **4. Choose the rig in an empty conversation** — while a conversation has
      no turns and is not running, the status line shows a rig select; picking
      one saves the default and reopens the blank conversation, so the first
      turn is written under that rig. After the first turn the same slot is
      plain text.
      Verify: pick `office` in a new conversation, send one message: the
      status line shows `办公` and the session document records `office`;
      reopening the conversation after switching the setting to `coding-live`
      still shows `office`.

- [x] **5. One less dead control** — the status line no longer opens settings
      (the header gear does); the rig segment is text, or a select while the
      conversation is empty. The permission chip stays interactive.

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- doctor
cd desktop
npm test && npm run check && npm run build
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```

Plus a real turn under `office` on a copy of a project.

## Not in this task

- Switching the rig mid-conversation. The running engine already holds the
  tool set and persona; a switch would mean rebuilding it against the existing
  history, and a persona that changes mid-thread is not a feature.
- A rig picker anywhere else. Settings holds the default; the empty
  conversation is the only place a one-off choice is made.

## Follow-up

M6 renamed this rig to `office` and split `research` out of it; see
`docs/llmwiki/tasks/m6-research-office-rigs.md` and ADR 0040.
