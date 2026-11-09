# M4ad — run a venv interpreter

Goal: make the Python check run the interpreter the code is written against,
instead of failing before the check starts.

## What was wrong

The host prefers `<project>/.venv/Scripts/python.exe` (or `bin/python`). The
kernel's `AcceptanceVerifier` rejects any program that contains a path
separator, so the item was refused with `program ... is not allowlisted` and
counted as a required failure. Any Python project with a venv could not pass
its own check, regardless of the code.

The gap was invisible until now because the copied repositories used by the
earlier live runs had their `.venv` left out.

## What changed

- Kernel (`Keel`): `AcceptanceVerifier::program_allowed`. A bare name is
  unchanged. An absolute path is accepted when it has no `..`, starts with the
  workspace root, is a file, and its file name alone is allowlisted. The shell
  tool's allowlist is untouched, so a model-initiated `shell` call still
  cannot name a path.
- Contract tests: a workspace path whose name is allowlisted runs (the copied
  test binary stands in for an interpreter), a path outside the workspace is
  refused, a workspace path with an unlisted name is refused.
- ADR 0030 records the contract.

## Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`: pass (the verification suite is 10 tests, +3).
- Manual end-to-end: a temporary project with a real `python -m venv .venv`
  and one `unittest` module ran through the verifier with the venv
  interpreter and passed (`Ran 1 test ... OK`). The same scratch check fails
  with `not allowlisted` when the kernel change is stashed, which is the
  before/after evidence.
- Host: no code change; the existing `a_project_venv_is_preferred` test still
  asserts the venv path reaches the item.

## Not in M4ad

- No PATH or environment injection into a check; the interpreter is chosen by
  path only.
- A check still runs at the workspace root; the venv path is the only program
  that the host names by path.
