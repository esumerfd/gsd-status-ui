# Sample GSD workspace for manual testing

A fictional "Robot Coffee Service" project with a `.planning/` tree shaped
like a real GSD workspace.

```bash
cargo run -- sample            # from the repo root: tabbed TUI (on a TTY)
cargo run -- --plain sample    # print-and-exit report
```

What it exercises:

| Thing | Where |
|---|---|
| One phase per stage, so every stage colour is on screen | Phases 1-9 (below) |
| Steps to browse with `j`/`k` (open with `Enter`) | `02-01`, `02-02`, `02-03` |
| All document tab kinds | `02-{RESEARCH,VALIDATION,UAT,CONTEXT,DISCUSSION-LOG}.md` + per-step plans |
| Missing-doc flash message | Phase 1 has no research/uat/etc. docs |
| Scroll testing (long doc, tables, code fences) | `02-02-PLAN.md` |
| Bare structural tags (`<objective>`, `<task>`) don't vanish | `02-02-PLAN.md` (GSD-style tagged appendix) |
| Structural tags convert to a nested heading outline — nesting depth, attribute stripping, name casing, the six-level cap, and the fenced contrast | `09-01-PLAN.md` (GSD-style tagged appendix) |
| Root docs behind the Roadmap row (`o` picker) | `PROJECT.md`, `REQUIREMENTS.md`, `STATE.md` |

## One phase per stage

Each stage paints its row a different colour, so the sample keeps one phase
sitting in every stage — a colour with no phase behind it is a colour nobody can
eyeball. `sample_workspace_has_a_phase_in_every_stage` guards the arrangement.
Phase 9 shares the planned stage with Phase 5 on purpose: it earns its place
by exercising the document viewer's structural-tag heading conversion, not by
adding a colour, so the eight-row stage table below stays one phase per stage
and needs no new row.

| Stage | Colour | Icon | Phase | What puts it there |
|---|---|---|---|---|
| Verified | green | `✓` | 1 Navigation Skeleton | roadmap `[x]` + `01-VERIFICATION.md` |
| Executing | yellow | `●` | 2 Coffee Acquisition | 3 plans, 1 summary |
| NotStarted | grey | `·` | 3 Delivery Etiquette | roadmap row only, no directory |
| Executed | cyan | `●` | 4 Milk Steaming | 2 plans, 2 summaries, no verification |
| Planned | sky blue | `◐` | 5 Cup Inventory | 2 plans, no summaries |
| Discussed | magenta | `◌` | 6 Order Queue | `06-CONTEXT.md` |
| Discussing | magenta | `◌` | 7 Multi-Floor Delivery | `07-DISCUSS-CHECKPOINT.md` |
| Abandoned | grey | `⊘` | 8 Voice Ordering | roadmap `[~]` |

Verified and abandoned phases are settled, so 1 and 8 are hidden until you press
`H` — that also makes the Roadmap row read `Phases 2/9`. Phase 2 stays the first
unsettled phase, so it remains the "current" one.

The Tasks section covers its three status colours the same way (in progress =
yellow, verification failed = red, completed = green, the last also behind `H`).
The milestone `status:` in `STATE.md` is single-valued, so only one of its
colours can show at a time.

For the pre-roadmap state — where those root docs get their own **Project** row
because there's no Roadmap row yet — use `sample-research/` instead.
