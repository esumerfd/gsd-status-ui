# Development

Building, releasing, the sample workspaces, and the code layout.
For what `gsd-status` is and how to install it, see [`README.md`](README.md).

## Releasing

`Cargo.toml` on `main` always holds the version the **next** release will cut,
so `gsd-status --version` on a dev build never reports a number a shipped build
already used. Run the release workflow manually and it:

1. tags `v<Cargo.toml version>` and builds that exact tree,
2. publishes the tarballs and rewrites the Homebrew formula,
3. **then** bumps `Cargo.toml` to the next minor on `main`.

Pass the optional `version` input to override step 1 — that is how you jump the
numbering forward (the workflow commits the version before tagging, so the tag
and the release agree). Pushing a `v*` tag by hand also releases; the bump job
skips when `main` is not sitting on the released version.
[`scripts/version.sh`](scripts/version.sh) holds the arithmetic and is covered
by [`tests/release_version.rs`](tests/release_version.rs).

## From source

```bash
make build      # cargo build --release
make install    # copy target/release/gsd-status to ~/bin
make run        # build + run against $PWD
```

Other targets: `make debug`, `make check`, `make fmt`, `make clean`. Run
`make help` for the full list.

## Try it against the sample workspaces

The repo ships four fictional `.planning/` trees under `sample/`, for manual
testing and screenshots:

```bash
cargo run -- sample/normal                          # mid-milestone: roadmap, phases, steps, todos
cargo run -- sample/research                        # pre-roadmap: research done, no ROADMAP.md yet
cargo run -- --plain sample/workstreams             # workstream federation: two workstreams, each scoped
cargo run -- --plain sample/project-and-workstreams # partially migrated: root project files + workstreams
```

See [`sample/README.md`](sample/README.md) for what each fixture exercises and
a link to its own README.

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

vendor/leaf/     vendored copy of the markdown/ratatui viewer leaf-adapter
                 wraps (see vendor/README.md for upstream + local patches)
sample/             four example .planning/ workspaces used in tests/cli.rs and
                    for manual TUI testing — see sample/README.md for an index
  normal/           mid-milestone: roadmap, phases, steps, todos
  research/         pre-roadmap: no ROADMAP.md yet (exercises the Project row)
  workstreams/      workstream federation: two independently scoped workstreams
  project-and-workstreams/  partially migrated: root project files + workstreams
```

`model.rs` / `planning.rs` / `report.rs` are a module split of the logic
currently still duplicated in `main.rs`; wiring `main.rs` to delegate to them
(plus the TUI) is in progress.

## Everyday commands

```bash
cargo check
cargo test
cargo fmt
```

`tests/cli.rs` runs the built binary end-to-end against `sample/normal/`.
`leaf-adapter/tests/doc_view.rs` exercises the doc panel renderer against a
`ratatui::TestBackend`.
