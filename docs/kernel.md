# The kernel Keelson is built on

Keelson is a host. It does not contain a turn loop, a permission gate or a
model adapter of its own; those come from the Keel crates.

## Pinned version

| What | Value |
|---|---|
| Kernel release | `v0.2.12` |
| `KERNEL_VERSION` | `0.2.12` |
| `EVENT_FORMAT_VERSION` | `1` |
| `SNAPSHOT_VERSION` | `1` |
| Source during joint development | `path = "../Keel/crates/keel-*"` |
| Source for a frozen build | `git = "file:///D:/workspace-new/wwkkyy0325/Keel", tag = "v0.2.12"` |

`Cargo.toml` uses path dependencies while the kernel and this host change
together. A release build switches every `keel-*` entry to the tag and never to
`main`.

## Consumed surface

- `keel-core`: `Session`, `SessionSnapshot`, `Tool`, `ToolArgs`, `ToolResult`,
  `ModelProvider`, `ContextStage`, `Policy`, `PermissionGate`,
  `InteractionPort`, `Event`, `EventSink`, `KernelError`, `ErrorCode`,
  `KERNEL_VERSION`, `EVENT_FORMAT_VERSION`, `SNAPSHOT_VERSION`.
- `keel-policy`: `load_profile`, `from_profile_str`, `RuntimeConfig`.
- `keel-runtime`: `TurnDriver`, `TurnDriverConfig`, `ContextBuilder` and the
  standard stages, `ToolRegistry`, `PermissionEngine`, `EventLog`,
  `InteractionBroker`, the built-in tools, `LocalTrashStore`,
  `LocalBackgroundTasks`, `ReadLedger`.
- `keel-model-openai`, `keel-model-anthropic`: the two providers.

Anything else in the kernel is out of scope until a rig needs it.

## Upgrading

The kernel repository owns the checklist
(`docs/kernel-upgrade.md` there); this repository owns the steps that follow
it:

1. Change the pinned tag or path in `Cargo.toml`.
2. `cargo update -p keel-core` (and the other `keel-*` packages), then build.
3. `cargo test` — including the conformance probes in `tests/kernel.rs`,
   which drive a real driver with a mock model.
4. Run `keelson doctor`; it prints the three version handshakes and loads
   every rig's profile.
5. If `SNAPSHOT_VERSION` changed, migrate stored sessions before restoring
   them. Migration is this repository's job, not the kernel's.
6. Run one real turn per rig. A mock run does not catch a behavior change.

## Conformance probes

`keel-testkit` is a dev-dependency. `tests/kernel.rs` holds the probes: a text
turn, a tool turn, a model failure, cancellation, the snapshot version and the
event-name list. They prove the kernel still behaves as documented; they do not
prove Keelson's own wiring, which needs its own tests.
