# Pre-roadmap sample GSD workspace

A fictional "Robot Tea Service" project stopped one step earlier than
[`sample/`](../sample/README.md): research is done and `REQUIREMENTS.md` is
written, but there is no `ROADMAP.md`, so no phases parse.

```bash
cargo run -- sample-research            # from the repo root: tabbed TUI (on a TTY)
cargo run -- --plain sample-research    # print-and-exit report
```

What it exercises:

| Thing | Where |
|---|---|
| The **Project** docs row | `PROJECT.md` + `REQUIREMENTS.md` at the `.planning` root |
| A workspace with no Roadmap row | no `ROADMAP.md`, so no phases and no Phases section |
| Research row alongside it | `research/` — 4 files |
| Empty milestone/status/progress header | no `STATE.md` |

The Project row only appears while the Roadmap row is absent. Add a `ROADMAP.md`
here and the Roadmap row takes over reaching the root docs (via `o`), and the
Project row disappears.
