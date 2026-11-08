# M4j — a real task in a copied GameVault core

Goal: run the coding rig on a real, documented limitation from another
project, in a copy, without touching the original. This is a validation, not
a feature of Keelson.

## Task source

`GameVault/docs/llmwiki/tasks/phase2-saves.md`, "Known limitations":

> Root-level save files (VX Ace, Tyrano `.sav`) are not detected as a save
> directory, so those games report `backup_only` and are not backed up yet.

The copy is `D:/workspace-new/wwkkyy0325/keelson-task-gamevault`: GameVault's
`gamevault-core` crate plus the workspace manifest and lockfile. The UI and
`src-tauri` were removed; the real `GameVault` directory was only read. The
copy's baseline was **282 passing tests**.

## Turn 1 — detection

Prompt: add root-level `.rvdata2`/`.sav` detection and tests; keep directory
behavior unchanged; do not change existing tests.

- **182.65 s**; host `cargo test` passed; **286 tests** (282 + 4 new).
- Added `SaveLayout.save_file`/`save_file_relative`, `detect_root_save_files`,
  extension matching and four tests. Existing tests were byte-identical.
- The model stopped at detection: `save_profiles` had no column for a file
  target, so those games were still not backed up. It said so explicitly.
- Every `shell` call was denied, so it could not run `cargo test` itself; the
  host's automatic check is what proved the build.

## Turn 2 — end to end

Prompt: extend `SaveProfile` and the `save_profiles` schema with a v3
migration, thread the file target through repo/service/backup so a root-level
save is snapshotted after exit and restorable, add end-to-end tests; version
assertions may change with the migration; do not weaken behavioral tests.

- **261.85 s**; the turn ended `VerificationFailed`. The suite ran **293
  tests: 292 passed, 1 failed**.
- The failure was the model's own new test
  `ensure_profile_persists_a_root_level_save_file_target`: on Windows it
  expected `root.join("Game/Save01.rvdata2")`, which mixes the separator,
  while the production path uses `\`. The production code was correct.
- The model could not run `cargo test` (all `shell` calls were denied) and
  ended without claiming a green run. The host's verification was the only
  executor.
- A one-line human fix in the copy — `variant_dir.join("Save01.rvdata2")` —
  makes the suite **293 passed, 0 failed**. The feature is complete:
  `SaveProfile.save_file_relative`, schema v3, repo column/upsert, file-target
  `ensure_profile`/`post_exit` backup, restore, and the end-to-end tests.
- Six core files changed: `domain/save.rs`, `db/migrations.rs`, `db/repo.rs`,
  `db/mod.rs`, `saves/detect.rs`, `saves/service.rs`.

## Turn 3 — self-verification with approved execution

To test the missing execution path, the one-line human fix was reverted (the
suite was back to 292/293), and the real-task harness was given an
`ApproveExecution` interaction port: it answers a permission approval with
`GrantScope::Once`, like a user clicking "allow once" on a trusted copy.

Prompt: the suite has one failing test; run `cargo test`, find it, fix it
without weakening the assertion, and report the real result.

- **69.47 s**; shell calls now succeeded (`cargo test`, `cargo fmt --check`,
  `cargo clippy --workspace`, `git log`/`git diff`).
- The model ran the suite, read the failing assertion, and changed the test
  to `root.join("Game").join("Save01.rvdata2")` — a platform-neutral join.
- It ran the targeted service tests, the full suite, fmt and clippy; **293
  passed**. The host's automatic check also passed.
- The model fixed its own Windows path bug once it could execute; no human
  edit was needed for this run.

## Findings

1. **The rig can do a real end-to-end foreign feature.** Across two turns it
   added detection, a schema migration, persistence, file backup/restore and
   tests in a codebase it had never seen. The host check caught the one
   remaining failure.
2. **A platform-specific test assertion was the only failure.** The generated
   test used a mixed path separator on Windows; the production code was
   correct. A one-line fix made 293 tests pass. This is a test-authoring
   lesson, not a production defect.
3. **Execution approval is the difference between blind and self-verifying.**
   With the deferring port, the model could not run `cargo test` and could not
   see the assertion values. With `ApproveExecution` (allow once), it ran the
   suite, saw the Windows path mismatch, fixed its own test, and reached 293
   passes in 69 s. A desktop user approves the card; the harness now simulates
   that for a trusted copy.
4. **Denied calls lost their arguments in the transcript.** The dispatcher's
   `reject` path emitted `ToolStarted { args_preview: None }`, so every denied
   `shell` row showed no command. Fixed in Keel: the redacted args preview is
   now included for invisible, unknown, invalid and denied calls; a dispatch
   test asserts `text=hi` survives a denial.
5. **A green host check is not product acceptance.** Turn 1 was green while
   the user-facing limitation remained; turn 2 was the end-to-end task. The
   acceptance prompt, not the test count, decides what is done.

## Follow-ups

- Add a persona/test guideline: build expected paths with `Path::join`, never
  mixed-separator strings, especially in cross-platform tests.
- Keep `ApproveExecution` manual and scoped to a trusted copy; it approves
  every permission request, and `shell` is not sandboxed.
- Commit the Keel denied-call fix, the harness and this task doc.

## Not in M4j

- Merging the copy back into the real project; the copy is for inspection at
  `D:/workspace-new/wwkkyy0325/keelson-task-gamevault`.
- A general permission policy for unattended runs; the harness approval is a
  test-only simulation.
