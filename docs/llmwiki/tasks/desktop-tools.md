# Desktop tools — look again, launch, and read controls

## Goal

Complete the observation loop for verification: reopen a saved image, start
an application and wait for its window, and read labelled controls without
pixels. No clicks or typing.

## Steps

- [x] **1. `view_image`** — reads PNG/JPEG/WebP/GIF by path, scales a large
      image to 1568 px, attaches it to the tool result. The read goes through
      the normal path permission, so an image in the workspace or a read root
      is free and anything else confirms.

- [x] **2. `launch_app` and `wait_for_window`** — a detached process with its
      own group and no console, returning the pid; the permission is an
      `Execute` on the resolved executable (bare names are found on `PATH`,
      relative paths resolve against the workspace). `wait_for_window` polls
      titles up to 30 s and returns the id and size.

- [x] **3. `list_controls`** — walks the UI Automation control view of the
      window whose title matches: control type, name, class and screen bounds,
      bounded to 200 elements and six levels. Window titles are matched the
      same way `wait_for_window` does.

- [x] **4. Rigs and prompts** — every rig lists the four tools; the office and
      coding prompts say to use them before asking the user to describe a
      program's state.

## Verification

```bash
cd /d/workspace-new/wwkkyy0325/Keelson
cargo test && cargo clippy --all-targets -- -D warnings
cargo test --lib desktop::tests::the_control_tree_of_a_window_is_listed -- --ignored
cargo test --lib desktop::tests::the_desktop_tools_work_end_to_end -- --ignored
DEEPSEEK_API_KEY=... cargo test --test desktop_eval -- --ignored --nocapture
```

Live acceptance: ask the agent to launch Notepad, wait for its window, capture
it and quote the text; then ask it to `view_image` the saved screenshot in a
later turn; then ask `list_controls` for a window with labelled controls and
check the names and bounds.

## Not in this task

- Mouse and keyboard injection (ADR 0043). The agent does not act on the
  user's windows.
- A UIA search ("find the button named Save and return its bounds") beyond the
  listing; a later tool can build on the same walk.
