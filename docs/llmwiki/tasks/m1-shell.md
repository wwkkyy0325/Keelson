# M1 — the shell host

Goal: a repository that loads rigs from files, validates them against the
pinned kernel, and can be checked with one command. No model, no turn loop.

## Steps

- [x] Repository skeleton, sibling to `Keel`, path dependencies on the pinned
      kernel.
      Verify: `cargo build` succeeds.
- [x] Rig schema in `src/rig.rs`: `name`, `title`, `description`, `ui`,
      `profile`, `[[extra_root]]`; unknown keys rejected.
      Verify: `rig::tests` — defaults, unknown key, relative root, bad name,
      unknown view.
- [x] Discovery over the search path, first directory wins, a broken file
      reported without hiding the rest.
      Verify: `discovery_keeps_the_first_rig_of_a_name`,
      `a_broken_rig_does_not_hide_the_others`.
- [x] `rig list`, `rig show <name>`, `--version`.
      Verify: `cargo run -- rig list` prints the coding rig;
      `cargo run -- rig show coding` resolves the profile path.
- [x] `doctor`: kernel handshake plus `keel-policy` load of every rig.
      Verify: `cargo run -- doctor` exits 0 and prints
      `profile coding (WorkspaceAuto)`.
- [x] Conformance probes for the pinned kernel.
      Verify: `cargo test` runs five probes, all pass.
- [x] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
      Verify: both clean.

## Not in M1

- Any model call, turn loop, session store.
- The terminal view and the `engine/` boundary test; they need `engine/` to
  exist first.
- `rig new`; it arrives with the engine, when a rig can actually run.
