# Evals

Keelson's evaluations live in `tests/` as cargo integration tests
(`explainer_eval.rs`, `explainer_film_eval.rs`, `explainer_kit.rs`,
`real_task_eval.rs`, and the rest). They drive the desktop/kernel stack and
record what the agent produced; the working directories they ran in are kept
for inspection rather than rebuilt, because the runs cost model calls and time.

## Run workspaces

`evals/data/workspaces/` holds those directories — `explainer-lab/` (nine
explainer runs), `keel-video/`, and the `keelson-task-gamevault` /
`keelson-task-valveguard` copies used by `m4j` and `m4k`. `INDEX.md` in that
directory says which run produced each one and which note cites it. The
directory is gitignored (`/evals/data/`).

Rendered frames and Cargo `target/` directories were deleted when the
workspaces were collected here; the authored inputs (`storyboard.json`,
`scenes/`, `kit/`, `script.json`, `render.py`, manifests and sources) were kept,
so a re-render rebuilds them.

Task notes written before the move still quote the old
`D:\workspace-new\wwkkyy0325\<name>` path. That is a record of where the run
happened, so it was left as written.

`git clean -xdf` deletes ignored files and would take these workspaces with it.
Use `git clean -df` when cleaning this repository.
