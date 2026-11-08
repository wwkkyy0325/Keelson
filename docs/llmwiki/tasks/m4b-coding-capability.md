# M4b — coding rig capability pass

Goal: close the gaps the M4 live eval could not expose because the demo is
small. The suite must edit and inspect a real repository without hitting a
token cap, an unreadable file range or a search that cannot find a symbol.

## Decisions

- The profile owns the working set; the catalog owns hard capacity. A model
  with a smaller window reduces the working set, never raises it above the
  profile.
- The output reserve is a profile value. 8192 tokens covers a normal edit;
  the catalog's output limit is not wired yet.
- `read_file` gains `offset` and `limit` in lines. A ranged read returns the
  exact bytes of the range, with no line-number decoration, so the model can
  pass the text to `edit_file` unchanged. CRLF is preserved.
- `code_search` gains `case_sensitive` and a file-name `glob`. It stays a
  substring search; ripgrep is not a dependency.
- Background shell is wired to the kernel's `LocalBackgroundTasks`. The
  process is killed when the engine drops, which is the session boundary the
  persona already promises.
- Verification detection is an ordered table: Cargo, Go, Python, Node. A
  manifest alone is not enough for Node; `package.json` must declare a test
  script.
- `echo` leaves the coding profiles' visible tools. It remains registered for
  the kernel tests and the offline fixture.

## Steps

- [x] Raise the coding output reserve and working set; clamp to the model
      window.
      Verify: both profiles carry 8192 / 64000; a 32k model clamps the working
      set; the existing 1M override test still passes.
- [x] Keel `read_file` line ranges.
      Verify: range read, CRLF preservation, offset past EOF, invalid offset;
      `keel-runtime` tests pass.
- [x] Keel `code_search` case sensitivity and glob.
      Verify: a case-sensitive query skips a different-case match; a glob
      keeps only matching files; the existing search test still passes.
- [x] Wire `LocalBackgroundTasks` in the host.
      Verify: the registry registers the shell tool with background support
      and the driver receives it; `cargo test` and the kernel background tests
      pass.
- [x] Verification table for Go, Node and nested Python tests.
      Verify: host tests pin each manifest, the no-test-script case and the
      Cargo priority.
- [x] Remove `echo` from the coding profiles and update the docs.
      Verify: profiles load; root, desktop and frontend suites pass; the live
      eval still repairs the demo.

## Verification

- Profiles: `coding` and `coding-live` both use `max_output_tokens = 8192`
  and `budget_tokens = 64000`. `Engine::open` clamps the working set to
  `model capacity - output reserve` when the catalog knows a smaller window;
  `a_small_model_window_reduces_the_working_set` pins a 32k model to 23808
  tokens of working set.
- Keel `read_file`: `offset`/`limit` are 1-based lines. A range returns its
  exact bytes (CRLF preserved, no line numbers); an offset past the end
  reports the line count; zero is `InvalidArguments`. The 8 MiB cap still
  bounds the whole read first.
- Keel `code_search`: `case_sensitive` is a boolean (a string is rejected);
  `glob` supports `*` and `?`, matches the file name unless the pattern
  contains a separator, and then matches the workspace-relative path. Test:
  `Hello` with `case_sensitive` and `*.rs` returns only the `main.rs` match.
  `keel-runtime` tool tests: 22 passed.
- Background: the host builds `LocalBackgroundTasks` under the project's
  `outputs/background`, registers the shell tool through `with_background`,
  and passes the registry to `TurnDriverConfig.background`. The kernel
  background tests pass.
- Verification: `workspace_check` is Cargo → Go → Python (recursive to depth
  2, skipping build and cache directories) → Node (`package.json` with a
  non-empty `scripts.test` that is not npm's `no test specified` placeholder).
  Tests pin each case, the missing test script and Cargo priority.
- `echo` is no longer visible in either coding profile; it stays registered
  for the kernel tests and the mock fixture.
- Suites: root 24 tests + 1 ignored eval + boundary + 5 kernel probes, clippy;
  desktop 25 tests, clippy; frontend 16 tests, `svelte-check` 0/0, build.
  `cargo run -- doctor` loads both rigs.
- The live eval passed on 2026-11-08 in 19.61 s with the new limits; the model
  repaired the demo and its post-turn `unittest` check passed.
- Not exercised: a live UI walkthrough of a background command, and a real
  large-repository search. Both have unit or kernel coverage but no visual or
  large-scale run yet.

## Not in M4b

- Regex or ripgrep search, gitignore semantics.
- Wiring the catalog's output limit into `ModelOverride`.
- A diff view in the transcript.
- `spawn_agent`, the suite picker and a second user-facing rig.
