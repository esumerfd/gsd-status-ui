# gsd-status-ui

![gsd-status-ui banner](assets/banner.png)

A terminal status view for [GSD](https://github.com/open-gsd/gsd-core) planning
workspaces — point it at a project directory and it reports which phase you're
on, how many plans are done, and what to run next.

## What it reads

GSD ([open-gsd/gsd-core](https://github.com/open-gsd/gsd-core)) is a slash-command
workflow system that drives phase-based project planning through Claude Code. As
a project moves through discussion, planning, execution, and verification, GSD
writes and updates a `.planning/` directory: `PROJECT.md`, `ROADMAP.md`,
`STATE.md`, and per-phase `PLAN.md` / `SUMMARY.md` / `VERIFICATION.md` documents.

`gsd-status-ui` doesn't drive that workflow — it's a read-only viewer over the
`.planning/` tree GSD produces. It parses those files and renders a summary of
project progress and a suggested next command, without needing gsd-core itself
installed.

## Usage

```bash
gsd-status [path]
```

If `path` is omitted, it walks up from the current directory looking for a
`.planning/` directory. Output looks like:

```
╭─ GSD STATUS ────────────────────────────────────────────────╮
  Robot Coffee Service
  /path/to/project/.planning
  milestone: M1 (v1)    status: executing
  progress:  ████████░░░░░░░░░░░░░░░░  33%  (1/3 phases · 1/4 plans)
╰─────────────────────────────────────────────────────────────╯

  Phases
  ───────────────────────────────────────────────────────────
  ✓  Phase 1   Navigation Skeleton                 —        verified
  ●  Phase 2   Coffee Acquisition                3/3 plans  executing
  ·  Phase 3   Delivery                             —       not started

  Next
  ───────────────────────────────────────────────────────────
  ...
    /gsd-execute-phase 2       continue executing remaining plans
    /gsd-progress              show concrete next step
    /gsd-help                  list all GSD commands
```

Honors `NO_COLOR`; colored output is skipped automatically when stdout isn't a
terminal.

An interactive TUI mode (step/tab navigation over a phase's Plan, Research,
Validation, Context, and Discussion documents, backed by the `leaf-adapter`
crate) is under active development — see [Project layout](#project-layout).

## Build & install

```bash
make build      # cargo build --release
make install    # copy target/release/gsd-status to ~/bin
make run        # build + run against $PWD
```

Other targets: `make debug`, `make check`, `make fmt`, `make clean`. Run
`make help` for the full list.

## Try it against the sample workspace

The repo ships a fictional `.planning/` tree (`sample/`) shaped like a real GSD
project, for manual testing and screenshots:

```bash
cargo run -- sample
```

See [`sample/README.md`](sample/README.md) for what each phase in it exercises.

## Project layout

```
src/
  main.rs        current CLI entry point (report rendering, self-contained)
  color.rs       ANSI color constants
  model.rs       domain model: StateMeta, Phase, Plan, Stage, Step, DocKind
  planning.rs    .planning/ parser: STATE.md, ROADMAP.md, phase directory scan
  report.rs      plain-text status report renderer
  tui/app.rs     step/tab navigation state machine for the interactive TUI

leaf-adapter/    isolates gsd-status from `leaf`: renders a markdown file into
                 a scrollable ratatui doc panel. The only crate that touches
                 leaf types.

vendor/leaf/     git submodule — the markdown/ratatui viewer leaf-adapter wraps
sample/          example .planning/ workspace used in tests/cli.rs and for
                 manual TUI testing
```

`model.rs` / `planning.rs` / `report.rs` are a module split of the logic
currently still duplicated in `main.rs`; wiring `main.rs` to delegate to them
(plus the TUI) is in progress.

## Development

```bash
cargo check
cargo test
cargo fmt
```

`tests/cli.rs` runs the built binary end-to-end against `sample/`.
`leaf-adapter/tests/doc_view.rs` exercises the doc panel renderer against a
`ratatui::TestBackend`.
