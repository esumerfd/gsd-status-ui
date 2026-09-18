# Workstream-aware sample GSD workspace

A fictional "Workstream Sample Project" exercising workstream federation:
root-scoped shared content (`todos/`, `notes/`, `research/`, `PROJECT.md`)
plus four workstreams, each with their own `STATE.md`/`ROADMAP.md`/`phases/`:
alpha and beta are live, gamma is `milestone complete`, and zeta is
`archived`. The last two exist to exercise how the `S` picker orders and
hides finished work.

```bash
cargo run -- --plain sample/workstreams                # default: active-workstream pointer (beta)
cargo run -- --ws alpha --plain sample/workstreams      # focus alpha explicitly
cargo run -- --ws beta --plain sample/workstreams       # focus beta explicitly
```

What it exercises:

| Thing | Where |
|---|---|
| Default workstream resolution via the `active-workstream` pointer file | `.planning/active-workstream` (set to `beta`) |
| `--ws` flag overriding the pointer | `--ws alpha` |
| `GSD_WORKSTREAM` env var overriding the pointer | set `GSD_WORKSTREAM=alpha` |
| Each workstream renders its own phases only | alpha's `Alpha First Phase` vs. beta's `Beta First Phase` |
| Root-scoped content federated into every workstream | `notes/`, `research/`, `todos/`, root `PROJECT.md` |
| Unknown `--ws` value exits 2 and lists known workstreams | `--ws nope` |
| `--ws` rejects path traversal | `--ws ../../etc` |

For the partially-migrated case — where root project files and workstreams
coexist rather than root content being purely federated in — use
[`../project-and-workstreams/`](../project-and-workstreams/README.md) instead.
