# 0084 — A record names the build that wrote it

## Context

On 2026-11-24 two builds sat on disk ten minutes apart: one without the tail fix
in ADR 0083, one with it. Telling them apart took `shasum` of three binaries, a
`ps` line for the start time, and the presence of a `partial` field in a note —
all of it outside the records themselves. Every telemetry file the app writes
was silent about which build produced it:

- `<state>/perf/turn-<conversation>-<seq>-<phase>.json` carried `seq`, `phase`,
  `at`, the counters, the timers and the DOM counts. No version, no commit.
- The step log's `turns` table carried the model, the provider, the rig, the
  workspace and the input. No version, no commit.

The obvious fix — a version field — does not work here. `Cargo.toml`,
`desktop/src-tauri/Cargo.toml`, `tauri.conf.json` and `desktop/package.json` all
say `0.1.0`, and nothing bumps them; forty accounts of two behaviours would
carry the same version. The kernel makes it worse: `KERNEL_VERSION` is
`CARGO_PKG_VERSION` of `keel-core`, also `0.1.0`, and the kernel is a path
dependency from a *different* checkout, so the host's commit says nothing about
the kernel code a turn actually ran on.

So the question "is this record from the build I just fixed?" had no answer
inside the record, and the evidence that did answer it died with the process
that a restart killed.

## Decision

The binary stamps the build into every record it writes, and says which code it
was built from rather than which number it carries.

1. **The stamp is `keelson <version> <commit> keel <commit>`.** The host's short
   commit, the kernel's short commit, and `-dirty` on either when the files that
   make that program were not committed. `unknown` when the build had no
   checkout to ask. Built in `desktop/src-tauri/build.rs`, read through
   `crate::build::ID`.
2. **The build script declares what the stamp is computed from**: the app's
   source trees, the frontend sources and output, and each checkout's
   `.git/HEAD`, `.git/index` and the ref HEAD names. `tauri-build` declares its
   own inputs, which switches cargo's default watch of the package off, so an
   unwatched stamp would name whichever build ran last — the one case where a
   stamp is worse than none.
3. **The host stamps the window's account, not the window.** `perf::write`
   parses the report the webview sent and adds `build` before writing. The
   window is the same binary and cannot know its own commit; the host knows it.
   A report that is not a JSON object is written as it arrived, because the
   account is evidence and losing it to a stamp is the worse failure.
4. **The step log records it per turn.** `turns` gains `build TEXT` and
   `event_format INTEGER`, filled from `crate::build::ID` and
   `keel_core::EVENT_FORMAT_VERSION`. An older `journal.db` is migrated in
   place with `ALTER TABLE ... ADD COLUMN`, and the rows already in it read
   `NULL` — no stamp is inferred for a turn written before the stamp existed.
5. **The event format stays separate from the commit.** `steps.data` holds
   kernel events in the kernel's persisted shape; a reader that pairs a row with
   an event needs `EVENT_FORMAT_VERSION`, which the kernel bumps on its own
   schedule, not the commit that happened to link it.
6. **The running process can be asked.** `build_id` returns the same stamp
   without a turn behind it: a record names the build only after a turn has
   written one, and a window that has just launched has written nothing. The
   command is read-only, so the settings panel asks on open.

What is *not* stamped, deliberately: the engine's turn documents under
`sessions/*.turns/` belong to the kernel's format (a host field there would be a
host branch in a kernel file), and the progress documents under
`records/progress/` are the user's.

## Consequences

- A record answers "which build?" by itself. `grep '"build"' perf/*.json` and
  `SELECT build, count(*) FROM turns GROUP BY build` separate two behaviours
  without a binary, a process id or an mtime.
- A build from an uncommitted tree says `-dirty`, which is the honest answer and
  the common one while a fix is being tried. A docs-only commit does not change
  the stamp: the dirty check is restricted to the paths that go into the app.
- The stamp replays nothing. It was never able to name a build that was deleted
  from disk, but it now names one that no longer exists at all — the two builds
  of 2026-11-24 differ by ten minutes and a `shasum` nobody wrote down.
- A `turns` row from an older log stays `NULL` forever. That is the intended
  reading: the log has no way to know, and a guess would defeat the field.
- The settings panel shows the stamp in Diagnostics, so the process in front of
  the user can be named even when it is too early (or too stuck) for a record.
- Watching the app's source trees makes the build script re-run on every source
  change, which recompiles the crate the change would have recompiled anyway.
  The extra work is two `git` calls.
