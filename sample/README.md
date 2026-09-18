# Sample GSD workspaces

Fictional `.planning/` trees used by `tests/cli.rs` and for manual TUI
testing. Each subdirectory is its own self-contained workspace.

| Directory | Why it exists | Run it |
|---|---|---|
| [`normal/`](normal/README.md) | Mid-milestone: roadmap, phases, steps, todos — the main fixture exercising most rendering and navigation | `cargo run -- sample/normal` |
| [`research/`](research/README.md) | Pre-roadmap: research done, no `ROADMAP.md` yet, so the **Project** row carries `PROJECT.md`/`REQUIREMENTS.md` | `cargo run -- sample/research` |
| [`workstreams/`](workstreams/README.md) | Workstream-aware federation: two workstreams, each with their own `ROADMAP.md`/`STATE.md`/`phases/` | `cargo run -- --plain sample/workstreams` |
| [`project-and-workstreams/`](project-and-workstreams/README.md) | The partially-migrated case — root project files and workstreams coexisting. Tests over it are characterization tests pinning current federation behavior, not assertions of desired behavior | `cargo run -- --plain sample/project-and-workstreams` |
