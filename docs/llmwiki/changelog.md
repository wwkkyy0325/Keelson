# Changelog

Append-only record of changes to Keelson. Kernel changes are recorded in the
Keel repository.

## 2026-11-05 — M1: the shell host

- New repository, sibling to `Keel`, consuming the pinned kernel `v0.1.0` by
  path during joint development.
- Rig schema: `name`, `title`, `description`, `ui`, `profile`,
  `[[extra_root]]`. Unknown keys are errors; a relative extra root is refused;
  a name must be a handle.
- Discovery over `$KEELSON_RIGS` or `./rigs` then `~/.keelson/rigs`, first
  directory wins, one broken file does not hide the rest.
- Commands: `rig list`, `rig show <name>`, `doctor`, `--version`.
- `doctor` prints the three version handshakes and loads every rig's profile
  through `keel-policy`.
- Conformance probes (`tests/kernel.rs`) for a text turn, a tool turn, a model
  failure, cancellation and the snapshot version.
- Copied from the reference host: the TUI specification, the coding profile
  and its prompt, and the failing-test demo workspace.

### Why

The kernel is frozen. A product on top is the next step, and the first
question is what the product is: a shell host whose rigs are files. That shape
keeps the agent thin and makes a second capability a configuration, not a code
path.
