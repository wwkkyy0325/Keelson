# 0005 — A maintained model catalog

Status: accepted

## Context

The first run asked for a provider, a base URL and a model id. That assumes
the user knows the wire protocol and the current model name of every provider.
The owner asked for the opposite: pick a current model from a list, paste one
key, start.

Models change faster than this repository. A hand-written list would be stale
within weeks, and a wrong base URL sends a key to the wrong host.

## Decision

- The model list comes from `https://models.dev/api.json`, an MIT-licensed,
  actively maintained database used by current coding agents. A
  byte-identical OpenCode mirror (`https://models.opencode.ai/api.json`) is
  tried next, and `KEELSON_CATALOG_URL` can point at a self-hosted proxy or
  mirror when both are unreachable.
- The desktop backend fetches it, keeps only tool-capable models, and filters
  it through a fixed provider allowlist in `desktop/src-tauri/src/catalog.rs`.
  Each allowed provider has a fixed wire kind, base URL and key hint.
- Deprecated models are dropped. Each family keeps only its newest model, and
  a six-month window relative to the provider's newest family hides stale
  families. Beta and alpha models stay: they are new, not old.
- The remote JSON supplies metadata only: model ids, names, context, prices
  and dates. It never supplies the endpoint, so a bad or compromised catalog
  entry cannot redirect an API key.
- A fetched catalog is cached at `<state>/model-catalog.json` for an hour.
  After that, the next open or refresh sends `If-None-Match` with the stored
  ETag; a 304 updates the timestamp without downloading the body, and a 200
  replaces the cache. A failed refresh keeps the cached list.
- A small bundled snapshot is used only when no cache exists and the first
  fetch fails. The picker does not surface cache timestamps or fetch sources;
  that is implementation detail.
- The first wizard page is model + key. Manual provider, endpoint and model
  fields remain behind an explicit disclosure for custom gateways.
- The selected model's input modalities are surfaced as badges (vision, PDF).
  They describe the model, not an image-upload feature; the app still sends
  text turns.
- The selected model's hard context capacity is stored with the connection and
  passed as `ModelOverride.context_window`. The profile still owns the
  working-set budget and the output reserve; the engine only raises the
  driver's capacity to the model's real window.
- The keychain stores one key. The config records which provider it belongs
  to, so switching providers requires a new key instead of silently sending
  the old one to a different host.

## Consequences

- The desktop now makes one non-model network request. It is metadata only,
  cached, and has no account or telemetry.
- New providers are a code change, not a data change. That is deliberate: a
  new wire protocol or auth scheme needs a provider implementation and a test,
  not an entry from the internet.
- Model-specific limits do not flow into the kernel yet; the profile still
  owns the context budget and output limit.
- If both sources disappear, the bundled snapshot and manual fields keep the
  app usable while the source is reconsidered. A mirror can only change model
  metadata; the provider endpoint still comes from `PROVIDERS`.
