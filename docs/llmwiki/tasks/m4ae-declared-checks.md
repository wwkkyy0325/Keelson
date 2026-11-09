# M4ae — declared workspace checks

Goal: verify a workspace whose tooling is outside Cargo, Go, Python and Node
without guessing a command from a manifest.

## Shape

`.keelson/checks.toml` at the workspace root:

```toml
[[check]]
name = "jvm"
program = "java"
args = ["-version"]
```

- `name` is `[A-Za-z0-9_-]+` and unique; it becomes the id
  `workspace-check:<name>`.
- `program` is required and must be on the coding allowlist; a path inside the
  workspace is accepted by the verifier (M4ad).
- `args` defaults to empty. The command runs at the workspace root.
- Declared checks are appended after the detected ones and always run: the
  deepest-project rule only applies to detected candidates.
- A file that does not parse, is not UTF-8, carries an unknown key, or has a
  missing, duplicate or invalid name or an empty program produces one required
  item, `workspace-checks-file`, whose `Unverifiable` reason quotes
  `.keelson/checks.toml: <error>`. The turn is then not fully verified, which
  is the visible signal that the declared check did not run.
- The file is read when the engine opens, with the rest of check detection.

## Tests

- `a_declared_check_is_a_candidate`: id, description, required flag, program
  and args.
- `a_declared_check_always_runs`: a nested Cargo project and a declared check
  are both selected for a path inside the project.
- `a_declared_check_runs_and_fails_by_its_exit_code`: `rustc --version`
  passes and `rustc --no-such-flag` fails through
  `AcceptanceVerifier`.
- `a_checks_file_that_does_not_parse_is_a_failing_item`.
- `a_declared_check_needs_a_unique_name_and_program`: five invalid files.

## Verification

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
(60 lib tests, +5).

## Not in M4ae

- No working-directory field; the command runs at the workspace root.
- No confirmation card. A declared check is the project's own check, like the
  detected ones, and it runs after a turn that changed files. The program must
  still be allowlisted, and the file is the user's configuration: the persona
  forbids editing it to make a check pass.
- No replacement of a detected check; a declaration adds one.
