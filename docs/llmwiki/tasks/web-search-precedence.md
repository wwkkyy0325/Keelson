# Task — web search follows the connected model

The user's rule: when the connected model is a cloud model, `web_search` tries
that model's own provider-side search first and only then falls back to the
separately configured backend.

## Steps

- [x] Replace `WebBackend::resolve` with `WebBackend::chain`, an ordered list —
      verify: `the_connected_model_comes_before_the_configured_backends`.
- [x] A DeepSeek chat model resolves to its own `/v1/tools/{search,fetch}` on
      the chat key — verify: `a_deepseek_key_is_not_tried_twice` keeps the
      tools-URL assertions.
- [x] A Volcengine Ark model resolves to the hosted `web_search` tool on
      `{origin}/api/v3/responses` — verify: the chain test asserts the endpoint
      and the model name.
- [x] `WebSearchTool` walks the chain: first non-empty answer wins, an empty
      answer passes on, every failure is reported together — verify: unit
      tests on the chain; a live call is still outstanding (see below).
- [x] Parse hosted answers: `url_citation` annotations and search-call
      `sources`, deduplicated; without citations the answer text is the single
      result — verify: `a_hosted_answer_yields_its_citations`,
      `a_hosted_answer_without_citations_still_reaches_the_model`.
- [x] `web_fetch` uses the first provider backend in the chain and downloads
      locally otherwise — verify: `serves_fetch` in the chain helper.
- [x] A saved key is skipped when the connected model already searches on that
      same origin and key — verify: `a_deepseek_key_is_not_tried_twice`.
- [x] Settings hint, doc comments, ADR 0038 amendment and the changelog state
      the new order.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test` — verify: green. **Blocked**: the permission mode of the
      session does not run the safety check, so no shell command executes.
- [ ] Live search on the active Ark preset (`deepseek-v4-1-flash-261017`,
      slot `volcengine`) and on a model without the hosted tool
      (`glm-5-3-flash-260828`) — verify: the first answers from the hosted
      call, the second falls through to the saved DeepSeek key. **Blocked**
      for the same reason; the fall-through keeps search working meanwhile.

## Notes

- Ark's hosted search is one model call per query, so it costs more than a
  plain search endpoint. A model whose provider rejects the `web_search` tool
  errors, and the next backend answers.
