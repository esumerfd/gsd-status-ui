# Partially-migrated sample GSD workspace

A fictional "Neighborhood Tool Library" project, stuck halfway between a flat
GSD workspace and a fully workstream-scoped one: root project files
(`PROJECT.md`, `ROADMAP.md`, `phases/`, `research/`, `todos/`) coexist with two
workstreams, alpha and beta, that have each started taking over their own
planning to a different degree.

```bash
cargo run -- sample/project-and-workstreams                    # default: alpha, tabbed TUI
cargo run -- --plain sample/project-and-workstreams            # default: alpha, print-and-exit
cargo run -- --ws beta --plain sample/project-and-workstreams  # beta, print-and-exit
```

What it exercises — the tests over this fixture are **characterization
tests**, pinning gsd-status's current federation behavior rather than
asserting what it should do:

| Gap | Where | What it shows |
|---|---|---|
| Folder shadowing is total, not a union | alpha, focused (default) | Alpha's own `research/C.md` replaces the root's `research/{A,B}.md` — the Research row counts 1 file, not 3 |
| Roadmap split-brain | alpha, focused (default) | Alpha has no scoped `ROADMAP.md`, so there is no Roadmap row and no Phases section, even though the root `ROADMAP.md` and root `phases/` both exist; the root roadmap stays openable via the Project docs row |
| A scoped `todos/` is invisible | beta, `--ws beta` | The todo under `workstreams/beta/todos/pending/` renders nowhere; the root todo (hard-rooted) still does |
| Healthy comparison arm | beta, `--ws beta` | Beta has its own `ROADMAP.md`, so its Roadmap row and Phases section both render normally — proof the other three rows are about federation, not a broken fixture |

These three gaps are captured, not fixed, by explicit scope decision (see
the plan that added this fixture).
