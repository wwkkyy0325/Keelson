# D2e — live model catalog

Goal: first run asks for a model and a key, nothing else. The model list comes
from a maintained catalog, refreshes automatically, and caches offline.

## Decisions

- Source: `https://models.dev/api.json`, the open model database used by
  current coding agents, then the byte-identical
  `https://models.opencode.ai/api.json` mirror. `KEELSON_CATALOG_URL` can
  replace the first source with a self-hosted proxy/mirror. Fetched by the
  Rust backend (no CORS dependency), filtered to a curated provider allowlist,
  cached for an hour and revalidated with its ETag.
- Only providers whose wire protocol this host can drive are shown. Each
  provider has a fixed kind, base URL and key hint in the backend; the remote
  JSON supplies model ids, names, context, prices and release dates.
- Only tool-capable models are shown; a coding rig cannot use a model without
  tool calls. Deprecated models are dropped, each family keeps its newest
  model, and a six-month window hides stale families.
- The first wizard page becomes model + key. The separate endpoint page is
  gone. Endpoint, provider and model fields stay behind an explicit "manual"
  disclosure for custom gateways.
- A bundled snapshot keeps the picker usable when the first fetch fails. The
  picker does not surface cache timestamps, fetch sources or refresh failures;
  those are implementation detail.
- The key remains one entry in the OS keychain. The config records which
  catalog provider it belongs to, so switching providers asks for a key
  instead of silently using the wrong one.
- Context: the selected model's hard capacity is stored with the connection
  and passed as `ModelOverride.context_window`. The profile keeps the
  working-set budget and output reserve; the driver uses the larger capacity.
- Modalities: image and PDF input are read from the catalog and shown as
  badges. They are capability labels; the app sends text turns only.
- The onboarding card is a fixed 640x580 frame. The model list fills the body
  instead of leaving a gap, and the footer with `上一步` / `下一步` sits on the
  same bottom line on every page. Later pages center their content in the body
  above that footer.
- Manual mode seeds its protocol, endpoint and model from the current catalog
  selection. Its label says `接口协议`, not `提供方`; the key line no longer
  prints an environment variable name. The manual fields sit in a panel that
  fills the model-list slot.
- Typography uses a display stack for headings (`Segoe UI Variable Display`
  with Chinese fallbacks), a text stack for body, and a mono stack for model
  ids and keys. The hierarchy is 1.5rem heading, .88rem description, .78rem
  field label, .9rem row title, .66rem model id.
- `ModelPicker` owns its own flex column: toolbar fixed, list `flex: 1` with
  `overflow-y: auto` and `overscroll-behavior: contain`. In the wizard it is
  passed `fill`; in settings it keeps a fixed 196px list. The stage grid uses
  `minmax(0, 1fr)` and the step has `min-height: 0`; without those, a grid
  `1fr` row keeps the step's min-content height and the stage scrolls instead
  of the list.
- The selected model row is marked with a check, not a persistent background,
  so hovering another row highlights only that row.

## Steps

- [x] Backend `catalog` module: fetch, filter, sort, cache and fallback.
      Verify: unit test on a sample payload; `cargo test`.
- [x] Backend command, config fields and key-provider tracking.
      Verify: `cargo clippy --all-targets -D warnings`.
- [x] `ModelPicker` component with search, provider filter, source and refresh.
      Verify: `npm run check`, a component test for the pure helpers.
- [x] Wizard: model + key on page one, three pages total, manual disclosure.
      Verify: `npm test`, `npm run build`; UI automation opens the picker.
- [x] Settings: same picker, refresh, key required when the provider changes.
      Verify: settings renders and saves in offline mode.
- [x] Docs, changelog, ADR 0005 and verification.

## Verification (2026-11-06)

- `cargo test` (root, including the context-override test),
  `cargo test --manifest-path desktop/src-tauri/Cargo.toml` (8 backend tests)
  and both clippy runs pass.
- Frontend: 11 tests, `svelte-check` 0 errors / 0 warnings, Vite build.
- The picker shows `识图` on DeepSeek V4.1 Flash and omits it on V4 Pro,
  matching the catalog modalities.
- The selected row is indicated by a check, not a persistent active
  background, so hovering another row highlights only that row.
- Switching to manual mode prefilled `接口协议 / OpenAI 兼容`,
  `https://api.deepseek.com` and `deepseek-flash` from the selected catalog
  model; switching back restored the same list selection. The card rectangle
  stayed fixed in both modes.
- The language and finish pages keep the `上一步` / `下一步` footer on the same
  bottom line; their content centers in the body above it. The card stayed a
  fixed 640x580 across all three pages.
- With a five-model provider, the list showed its own scrollbar inside the
  picker; the stage showed none and the header/footer stayed fixed.
- The scrollbar fix required `grid-template-rows: minmax(0, 1fr)` on the stage
  and `min-height: 0` on the step; `1fr` alone let the step's min-content
  height win and pushed the scrollbar to the stage.
- Page transitions are opacity-only. The old 6px translate and the stage
  scrollbar were the two sources of the slight twitch when changing pages.
- The default workspace is `~/Documents/Keelson` (or `~/Keelson`), not the
  user home. A stored home path migrates on open; the app created the folder
  and saved it as `lastWorkspace` in the verification run.
- UI automation with a temporary `KEELSON_HOME`:
  - the first page fetched the live catalog (source `models.dev`) and showed
    only current DeepSeek models with context and price;
  - the offline path reached the language step, the finish summary and the
    app shell;
  - settings opened on the saved offline mode, switched to the same picker,
    and refused to save a live model without a key (`请先填写密钥`).
- A second walkthrough against the real state confirmed the new cache version
  rejects the old unfiltered cache, refetches, and shows `DeepSeek · 2`:
  `DeepSeek V4.1 Flash 1017` and `DeepSeek V4 Pro`; the deprecated V4 Flash
  and Vision entries are gone.
- `curl -H 'If-None-Match: <etag>' https://models.dev/api.json` returns
  `304` with zero bytes, which is the revalidation path used after the cache
  hour.
- The OpenCode mirror returned the same 5,967,945 bytes and the same ETag as
  the official URL; its conditional GET also returns `304`. The app tries it
  when the canonical host fails.
- The first isolated run used `%TEMP%/keelson-d2e`; the real keychain was
  never written.

## Not in D2e

- Per-provider key storage. One key is stored; the config records its
  provider and the UI asks for a replacement when it changes.
- Per-model working-set budgets and output reserves. The profile still owns
  those; only the hard capacity comes from the catalog.
- Image or PDF attachments. The badges describe model capability; the
  composer and the kernel request path remain text-only.
- Telemetry or analytics for catalog choices.
