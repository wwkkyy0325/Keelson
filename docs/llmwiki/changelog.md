# Changelog

Append-only record of changes to Keelson. Kernel changes are recorded in the
Keel repository.

## 2026-11-05 — M1: the shell host

- New repository, sibling to `Keel`, consuming the pinned kernel `v0.1.0` by
  path during joint development.
- Rig schema: `name`, `title`, `description`, `ui`, `profile`,
  `[[extra_root]]`. Unknown keys are errors; a relative extra root is refused;
  a name must be a handle.
- Discovery over `$KEELSON_RIGS` or `./rigs` then `~/.keelson/rigs`, first
  directory wins, one broken file does not hide the rest.
- Commands: `rig list`, `rig show <name>`, `doctor`, `--version`.
- `doctor` prints the three version handshakes and loads every rig's profile
  through `keel-policy`.
- Conformance probes (`tests/kernel.rs`) for a text turn, a tool turn, a model
  failure, cancellation and the snapshot version.
- Copied from the reference host: the TUI specification, the coding profile
  and its prompt, and the failing-test demo workspace.

### Why

The kernel is frozen. A product on top is the next step, and the first
question is what the product is: a shell host whose rigs are files. That shape
keeps the agent thin and makes a second capability a configuration, not a code
path.

## 2026-11-05 — M2: the engine

- `engine/`: profile and permission wiring, extra roots, the twelve built-in
  tools, the five context stages, model selection (mock, OpenAI, Anthropic)
  and one turn through `TurnDriver`.
- Sessions are one JSON document per session under
  `<state>/sessions/<id>.json`: kernel snapshot, task list, and one row per
  turn with its events. A session id is refused against a different workspace.
- `view/oneshot` prints the answer on stdout. `keelson run <rig>` uses the
  rig's `ui`; `transcript` and `chat` report that they arrive with M3.
- `rigs/script.toml`: the coding profile with the oneshot face, for scripts
  and pipes.
- `KEELSON_HOME` now governs both `rigs/` and the session store, as the
  discovery section already described; the M1 code only used `~/.keelson`.
- Boundary test: no file under `src/engine/` names a terminal library.

### Why

The host needs a turn loop before a face can render one. The engine returns a
neutral report and keeps the terminal out, so the transcript view in M3 is a
module over the same path, not a second engine. The store stays a single
document because no current view queries turns; a read model can be added over
the same snapshot when one does.

## 2026-11-05 — M3: the transcript view

- `engine::Engine`: a long-lived handle that builds the driver once, runs one
  turn at a time, persists after each turn and exposes events, tasks, roots,
  mode switches and interjections. The provider is wrapped when the view wants
  deltas; text reaches the view through a channel, not a kernel event.
- `view/transcript.rs`: the ported terminal UI — the block model, streaming,
  tool lines, inline permission prompts, process collapse, `/add-dir`,
  `/cost`, `/help`, `/roots`, `Ctrl-O`, `Shift+Tab` and page scrolling.
- `view/ui_text.rs`: the bilingual chrome table (zh/en, locale and OS
  detection, `--lang auto|zh|en`).
- `render/terminal.rs`: raw mode, the alternate screen, key polling and the
  restore-on-drop guard.
- `keelson run coding` opens the transcript; `keelson run script` keeps the
  oneshot face. A piped stdin is refused for the interactive view.
- Goldens copied from the reference host into `tests/fixtures/` with the
  fixture workspace renamed `keel` → `keelson`.

### Why

The engine and the store were tested but had no face. The transcript is the
face the design names first, and porting it reuses the reference host's
reducer and goldens instead of inventing a second UI. The mock provider changed
from a one-shot script to a repeating answer so one engine can serve many
turns; the kernel's own probes still cover tool dispatch.

The view owns layout and uses ratatui text types; `render/` owns the terminal
lifecycle. The enforced boundary is unchanged: no engine file names a terminal
library.

## 2026-11-06 — M3b: transcript daily use

- A tool line carries its target (`path=…`, `program=…`) and duration. A failed
  call stays visible after the turn folds, with its first error line on the
  same row; `Ctrl-O` still expands arguments and output.
- The finished-turn aggregate no longer prints token counts. `/cost` prints the
  turn total and the session total, and `/help` lists the keys and commands.
- `BackgroundStarted`, `BackgroundResult` and `SubAgentFinished` render one
  line each, in both languages.
- Permission prompts have English and Chinese goldens
  (`tui-prompt.en.txt`, `tui-prompt.zh.txt`); the key hints are unchanged.
- A 40-column render test; a winpty smoke covers Ctrl-C and CJK input.

### Why

The ported screen worked but still leaked process detail (tokens in the
aggregate), hid failed tools the user needs to act on, and had no line for
delegated or background work. These are the daily-use items the copied
specification had left open; closing them makes the transcript the face the
design describes without adding panels.

## 2026-11-06 — M3c: a bounded transcript

- `Ctrl-O` / `/details` expands only the most recent turn. The previous global
  toggle revealed every tool argument and output in the session at once.
- On each collapse, the process detail of earlier turns is released: tool
  arguments and output are cleared, while compact lines, answers, summaries
  and user messages remain. Failed tools stay visible with their error.
- The screen redraws only when something changed (a key, an interaction, a
  delta, a turn event) or while a turn runs; an idle screen no longer rebuilds
  the transcript every 80 ms.
- Tests pin the per-turn expansion, the release, and the existing goldens.

### Why

Expanding the whole history floods the screen and makes each frame cost grow
with the session. A per-turn expansion matches how the process is actually
read, and releasing old detail bounds both memory and the work a redraw does.
The block model already carried the turn boundary, so no cursor or selection
model was needed.
