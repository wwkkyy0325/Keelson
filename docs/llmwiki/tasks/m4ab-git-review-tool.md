# M4ab — read-only git review

Goal: let the model inspect status and the unstaged diff without a shell
confirmation.

## What changed

- Host tool `git_diff`: `mode = "status"` or `"diff"` (default), optional
  `path`. It is read-only, so the permission engine allows it without a card.
- It runs `git -c color.ui=false --no-optional-locks -C <root> ...`; the diff
  uses `--no-ext-diff`. A 15 s timeout and a 32 KiB cap bound it.
- A directory that is not a repository returns `not a git repository`.
- Registered in the host and exposed by both coding profiles; the persona
  asks the model to review the tree before finishing.

## Verification

- Unit tests: the diff shows `-old` / `+new`; status with a path omits other
  files; a non-repository message; an unknown mode is an argument error.
- Keelson: 55 lib tests (+4), clippy. Polyglot eval 11.36 s; cross-file eval
  12.39 s; desktop 28.

## Not in M4ab

- No stage, commit or history operations; those remain shell or user work.
- The tool reads the workspace's own repository, not the checkpoint shadow
  repository.
