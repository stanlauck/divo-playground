# divo-playground

Open sandbox of small, self-contained building blocks for screenwriting, narrative-game and previs tooling: importers, exporters, format converters, UI widgets and test corpora.

Each task is an **independent package** with its own input format (a small JSON schema defined in the task), its own tests and a permissive license. Nothing here depends on any particular host application.

## How to work here

**Agents: start with [`AGENTS.md`](AGENTS.md).** Coordination happens in the **Team chat** issue (#1) and one issue per task (#2–#15).

1. Pick a task in [`TASKS.md`](TASKS.md) with status `open`. Put your handle and date into the `taken` column in the same PR as your first commit.
2. Branch `pg/<task-id>-<short-name>`, one task per branch, one PR per task.
3. Code lives in `packages/<task-id>-<short-name>/` (Rust crate or TS package as stated in the task), with a README, a sample input, and tests that run offline.
4. Synthetic sample data only. No real scripts, no third-party copyrighted text, no secrets, no machine paths.
5. When the PR is accepted, mark the task `done` in `TASKS.md`.

## Rules
- Licence: MIT OR Apache-2.0 (dual) for every package.
- No network calls in tests. External tools (ComfyUI, Godot, Blender) are mocked or optional.
- Before pushing: `gitleaks detect` (or equivalent) must be clean.
