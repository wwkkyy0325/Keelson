# ADR 0038 — Web tools use the provider's key

## Status

Accepted and implemented in the engine. Released with Keelson 0.1.0.

## Context

The host could not read or search the web. The two honest designs are a
provider-side tool or a local client, and a local search backend needs a key
from every user (Brave, Tavily, Exa, SearXNG), which is friction most people
will not pay. Claude Code avoids it because Anthropic's API runs the search
server-side on the account's key; opencode's search is Exa behind an opt-in
environment flag.

DeepSeek now serves raw Search and Fetch endpoints on the same key and base
URL as the chat API: `POST {base}/v1/tools/search` and
`POST {base}/v1/tools/fetch`, billed per successful request. That removes the
extra credential for the provider this host currently uses.

## Decision

1. Two host tools, registered by the engine, not the kernel:
   - `web_search` calls the provider's search endpoint. It is registered only
     when the configured provider exposes one (DeepSeek today) or when the
     settings carry a SearXNG base URL; a profile may list it for every
     provider, and the engine drops it from the visible set when neither
     exists. The SearXNG path calls `{url}/search?q=...&format=json`, so the
     instance must enable its JSON format; it needs no key.
   - `web_fetch` always exists. With a provider fetch endpoint it returns the
     page as Markdown; otherwise it downloads the URL and reduces HTML to
     text locally, with SSRF guards (no loopback or private hosts), a 30 s
     timeout, a 2 MiB body cap and a 60k-character output cap.
2. Both are read-only and ask for no confirmation. The rig's `tools.visible`
   list is the opt-in: only rigs that name them can use them.
3. The provider key stays where it already is (the OS keychain, passed to the
   engine as a model override); the tools hold it in memory only and never
   log or return it.
4. This ADR amends the rule that the models.dev catalog is the only non-model
   network request: the web tools are the second, and they exist only when a
   rig lists them.

## Consequences

- A DeepSeek session gets search and fetch with no setup at all; a local
  model gets `web_fetch`, and `web_search` when the settings name a SearXNG
  instance — the deployment brings its own search service and the host only
  stores its address. Without either, `web_search` silently drops from the
  tool list instead of failing validation.
- A provider endpoint always wins over the configured URL, so the same
  setting can stay in place when switching between a local model and
  DeepSeek.
- The provider bills search and fetch per successful request; failed or empty
  requests are free.
- Read-only network tools plus file access form an exfiltration path (a file's
  text can be put in a URL). The boundary is the rig: a rig without the web
  tools cannot reach the network, and `web_fetch` is visible only where a
  profile names it.
- The local fetch does not resolve DNS before connecting, so a hostname that
  points at a private address is not caught; this is the same trust model as
  the provider fetch.

## Amendment — 2026-11-10

- The provider fetch call is bounded at 60 s: the provider fetches the page
  server-side, and a slow origin can exceed the 30 s bound that remains
  right for a direct download. A live fetch of a government page was
  aborted at 30.0 s and succeeded on the retry at 28.4 s.
- A failed request appends the reqwest source chain to the message
  (`...: operation timed out`), so a model can tell a timeout from a bad URL.
- The tools URL is built from the API origin, not from the chat base path.
  The desktop's Anthropic-compatible provider uses
  `https://api.deepseek.com/anthropic`, and appending `/v1/tools/...` to it
  produced a path the tools are not served from; `/v1` was already stripped
  and the compatibility path was not.

## Amendment — 2026-11-11

- A saved DeepSeek key is also the web-tools backend when the chat model is a
  local server with no endpoint of its own. The host passes it as
  `OpenRequest::search_key`; it wins over the SearXNG URL. The key stays in
  memory and is never written to a rig or a session document.
