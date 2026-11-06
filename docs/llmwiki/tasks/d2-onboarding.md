# D2 — first-run onboarding and the Chinese interface

Goal: a first launch that asks for the key first, walks one page at a time,
then enters the app directly. Chinese is the default interface; English is a
switch, not a separate build. The workspace is chosen inside the app.

## Decisions

- Four wizard pages: key → model endpoint → language → finish. Only the key is
  required; the endpoint page is prefilled and can be skipped by continuing.
- The key is stored in the OS keychain (Windows Credential Manager / macOS
  Keychain). `desktop.json` holds only non-secret settings.
- The engine accepts a `ModelOverride` (provider, base URL, model, key), so the
  desktop does not rewrite profiles or set process environment variables.
- The wizard is offline-capable: "先用离线模式" saves the mock provider and
  enters the app without a key.
- Language lives in one dictionary and one `locale` store. Chinese is the
  default; the header has a 中/EN switch and every string goes through `t()`.
- The workspace is not part of onboarding. The app opens the remembered
  workspace (home on first run) and the header chip opens a folder picker.
  Sessions are scoped by a workspace hash, so switching directories cannot
  collide with or be refused by another workspace's session.
- After onboarding the app auto-connects. The gear opens a **one-page
  slide-over settings panel** (model/key, language, workspace info, save and
  reconnect); the paged wizard is only for the first run.

## Steps

- [x] `ModelOverride` in `OpenRequest`; the profile still supplies tools,
  persona and context, while the desktop supplies provider/endpoint/model/key.
  Verify: `cargo test`.
- [x] Desktop config in `desktop.json`, key in the OS keychain, setup commands
  (`setup_state`, `save_key`, `save_model`, `save_locale`,
  `complete_onboarding`).
  Verify: `cargo clippy` and the desktop test for stable workspace keys.
- [x] Workspace-scoped session documents under
  `<state>/sessions/<workspace-hash>/default.json`; `open_session` builds the
  model override and updates `lastWorkspace`.
  Verify: `workspace_keys_are_stable_and_distinct`.
- [x] Wizard with per-page transitions, progress, show/hide key, offline path,
  language cards and a finish summary.
  Verify: `npm run check`, `npm run build`.
- [x] Chinese-first i18n with an English dictionary and header toggle.
  Verify: UI automation shows Chinese chrome; the `EN` toggle is present.
- [x] In-app workspace picker (Tauri dialog plugin) and a single-page
  settings slide-over with model/key, language, workspace info and
  save-and-reconnect.
  Verify: build and UI automation opening the settings page and switching it
  to English.
- [x] Docs, changelog and this task record.
  Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `npm test`, `npm run check`, `npm run build`.

## Verification (2026-11-06)

- Root: 65 unit tests, 1 boundary test and 5 kernel probes pass.
- Desktop: the workspace-key unit test passes; clippy is clean.
- Frontend: 7 reducer tests pass; `svelte-check` reports 0 errors, 0 warnings;
  Vite builds ~72 KB JS and ~7 KB CSS.
- UI automation against the real window: started from a clean config, captured
  the Chinese key page, chose offline mode, advanced through the language and
  finish pages, entered the app, sent `hello`, and captured the streamed
  `mock turn complete` and the `✓ echo` row.
- The same run opened the gear: the settings page is one dense page (model
  and key, language segments, workspace info), and switching it to English
  updated all strings in place.
- Not exercised with a real key: the keyring write/read path compiles and is
  used only when the user enters a key; the offline path skipped it.
- Not yet exercised: the folder picker (the dialog plugin and capability are
  wired), and a real-model turn through the wizard.

## Not in D2

- Floating, frameless, always-on-top window; tray; global shortcut.
- Multiple providers per keystroke, key validation against the endpoint, and
  a keychain delete flow.
- Markdown rendering and live tool lines while a turn runs.
