# Screen tools — window, region and monitor capture

## Goal

The model can look at what the user sees: enumerate windows, capture one, a
region or a whole monitor, and read the image in the same turn. No OCR; the
model's vision is the reader, and the user describes the picture when the
model cannot see it.

## Steps

- [x] **1. Kernel image channel** — `ImagePart`, `Message.images` and
      `ToolResult.images`, in-memory only (Keel ADR 0051). The driver attaches
      a result's images to its tool message; both adapters encode them; the
      context estimate charges 1024 tokens per image.
      Verify: `a_tool_image_reaches_the_next_model_call`, the OpenAI and
      Anthropic wire tests.

- [x] **2. Host capture tools** — `list_windows` and `screen_capture`
      (`src/engine/screen.rs`), registered for every engine and visible only
      where a rig lists them. Targets: window (id or title), region
      (`x,y,w,h`), screen (monitor index). The PNG is scaled to 1568 px and
      written to `<state>/projects/<hash>/screens/<session>/`.
      Verify: region parsing, the permission request names the folder and
      the target, a 2000 px image is scaled (unit tests); a window and a
      monitor capture were taken on this machine (ignored tests).

- [x] **3. Permission** — the screens folder is the scope, outside the
      workspace: `workspace_auto` confirms, the session grant silences later
      captures, `full_auto` does not ask. No new permission category.

- [x] **4. Rigs** — `list_windows` and `screen_capture` are listed by the
      coding, office and research profiles.

## Verification

```bash
cd /d/workspace-new/wwkkyy0325/Keelson
cargo test && cargo clippy --all-targets -- -D warnings
cargo test --lib screen::tests::a_window_can_be_listed_and_captured -- --ignored
cargo test --lib screen::tests::a_monitor_can_be_captured -- --ignored
```

Live acceptance: put a unique sentence in a Notepad window, ask the agent to
capture that window and quote the sentence; repeat with a region around the
sentence and with a full-monitor capture. The model must quote the text
exactly; a paraphrase or an invented line is a failure.

## Not in this task

- OCR or any other recognition in the host.
- UI automation (clicks, typing): capture is read-only.
- Screens in the session document: the image is turn-scoped, the path is
  kept.
