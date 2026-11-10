# M6 — Research and Office rigs

## Goal

Split the non-coding work by output contract: `research` returns cited
answers with a source ledger, `office` returns documents. The old `general`
rig, which had no contract, is renamed `office` and stops being a catch-all
(ADR 0040).

## Steps

- [x] **1. Rename general to office** — `rigs/office.toml` →
      `profiles/office-live.toml` → `prompts/office.md`. The tool list gains
      `code_search` (content search) and `multi_edit` (several edits in one
      document); `git_diff` and `apply_edits` stay with the coding rig. The
      prompt gains the scenario rules: edit only the requested range, write
      deliverables to files, cite outside claims, ask on ambiguity, use a
      todo list, respect the kernel's context and clipping behaviour, and do
      not leave processes behind.
      Verify: `cargo run -- rig show office` and `cargo run -- doctor`.

- [x] **2. The research rig** — `rigs/research.toml` →
      `profiles/research-live.toml` → `prompts/research.md`. Tools: web
      search/fetch, content search, file read/write, shell, todo, history.
      No move or delete: research creates notes, it does not reorganise the
      user's files.
      Verify: the rig list shows `research` and its profile resolves.

- [x] **3. The evidence contract** — every factual sentence carries a source
      id; the reply ends with a ledger entry per source naming the title,
      URL, how it was read (page text, search snippet only, or a local file),
      the quotation and the claim it supports. Claims that could not be
      sourced are listed as unverified. Access boundaries are explicit: no
      bypassing captchas, login walls, paywalls or rate limits.

- [x] **4. UI names** — Chinese labels: `research` = 研究, `office` = 办公.

- [x] **5. Documentation** — ADR 0040 and the design document record the
      split by contract, and why a deck or video rig waits for artifact
      verification.

- [x] **6. Scenario evals** — `tests/office_eval.rs` (an edit changes only the
      requested text; a summary is written to the named file; an ambiguous
      request asks first) and `tests/research_eval.rs` (a conflict is reported
      with both sources and a quotation; an absent fact is not invented).
      Each asserts on workspace artifacts, not on prose quality.

- [x] **7. Wiring tests** — `tests/rig_tools.rs` opens each rig without
      calling a provider and checks the effective tool set and the loaded
      persona. This runs without a key; the live evals skip when none is
      configured.

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- doctor
cd desktop && npm test && npm run check && npm run build

# Live, one at a time (the source ledger and edits are the assertions):
DEEPSEEK_API_KEY=... cargo test --test office_eval -- --ignored --nocapture
DEEPSEEK_API_KEY=... cargo test --test research_eval -- --ignored --nocapture
```

The live runs are skipped when no endpoint is configured. A pass rate is
tracked per rig, not pooled: the two suites assert different contracts.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- doctor
cd desktop && npm test && npm run check && npm run build
```


## Not in this task

- Deck and video rigs. They arrive when the host can verify the artifact
  (open the file, render a page, probe duration and resolution).
- A better search backend. `web_search` quality is the other lever for
  research, and it is a host-level decision under ADR 0038.
- A scenario evaluation set for the two rigs; it follows as its own task.
