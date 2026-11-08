# M4aa — ignore-aware search and glob

Goal: stop walking generated trees a project already ignores.

## Kernel

- `walk_workspace` uses `ignore::WalkBuilder` with `require_git(false)`,
  `hidden(false)`, `.gitignore` / `.ignore` / git excludes, a deterministic
  file-name sort and the existing depth limit.
- `code_search` and `glob` run the walk on a blocking thread; cancellation and
  the result and file caps still apply.
- The fixed build-directory filter remains; an explicitly chosen root is not
  pruned.
- ADR 0029.

## Verification

- `code_search_respects_gitignore`: an ignored directory is not searched.
- `glob_respects_gitignore`: an ignored directory is not listed.
- The existing build-directory and search tests pass.
- Keel: 115 lib tests, 39 tool tests (+2), clippy; `cargo deny check licenses`
  passes.
- Keelson: 51 lib tests, polyglot eval 11.65 s, cross-file eval 8.86 s,
  desktop 28.

## Not in M4aa

- A file can still be read by an explicit path even if ignored.
- Negation and nested ignore rules are handled by the `ignore` crate; no extra
  policy.
