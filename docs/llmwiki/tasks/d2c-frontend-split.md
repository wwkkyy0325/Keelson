# D2c — component split and a stable page stage

Goal: remove the layout jump between wizard pages and make the desktop
frontend a set of independent components instead of one controller file.

## Decisions

- The wizard renders all four steps in one CSS grid cell. The card height is
  the height of the tallest step, so switching steps cannot reflow the card.
  Steps fade and translate in place with `opacity`/`transform`; there is no
  Svelte `in/out` transition that removes the outgoing element.
- The controller (`App.svelte`) owns only boot, setup, connection and the
  settings overlay. It does not render chat or wizard markup.
- Components:
  - `Wizard` — first-run pages, validation, offline path, finish.
  - `Chat` — app shell, session actions, error banner.
  - `AppHeader` — workspace chip, language toggle, settings button.
  - `Transcript` — turns, running turn, keyset pagination and scroll anchor.
  - `Turn` — one finished turn and its tool rows.
  - `Composer` — draft, Enter-to-send, send/cancel.
  - `PermissionCard` — one pending kernel interaction.
  - `SettingsPanel` — one-page slide-over settings.
- Shared state is only the session reducer store and the i18n locale store.
  Components that perform persistence (`AppHeader`, `SettingsPanel`, `Wizard`)
  call the API directly; no component reaches into another's local state.
- Settings remain one page; the paged wizard is first-run only.

## Steps

- [x] Wizard grid stage with stable card height and CSS-only page transition.
      Verify: step 0 and the language step have the same card bounds in the
      UI-automation screenshots.
- [x] Split the frontend into the components above; `App.svelte` is a
      controller.
      Verify: `svelte-check` 0 errors / 0 warnings; `npm run build`.
- [x] Keep the Chinese-first i18n and the one-page settings panel.
      Verify: UI automation switched the settings page to English.
- [x] Docs, changelog and this task record.
      Verify: `npm test`, `npm run check`, `npm run build`.

## Verification (2026-11-06)

- `svelte-check`: 0 errors, 0 warnings across 184 files.
- `npm test`: 7 reducer tests pass.
- `npm run build`: succeeds.
- UI automation: a clean config opens the wizard; step 0 and the language
  step render the same card rectangle; offline mode reaches the app; sending
  `hello` renders the streamed mock answer and the `✓ echo` row through the
  split `Chat`/`Transcript`/`Turn`/`Composer` components.
- The settings panel is one page, and its English switch updates the strings
  in place.

## Tradeoff

The card is as tall as the tallest step, so short steps have some empty space
at the bottom. That is the cost of zero reflow. If it becomes annoying, the
next step is a measured height transition on the stage with a `ResizeObserver`,
not a return to unmounted `in/out` transitions.

## Not in D2c

- Component-level browser tests; the reducer tests and UI automation cover the
  state path.
- A real-key onboarding run (the offline path is what automation exercises).
- Floating window, tray and global shortcut.
