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
| Verified phase | Phase 1 (roadmap checked + `01-VERIFICATION.md`) |
| Executing phase (the "current" phase) | Phase 2 — 3 plans, 1 summary |
| Untouched phase | Phase 3 (no directory) |
| Steps for `Alt-j`/`Alt-k` navigation | `02-01`, `02-02`, `02-03` |
| All document tab kinds | `02-{RESEARCH,VALIDATION,UAT,CONTEXT,DISCUSSION-LOG}.md` + per-step plans |
| Missing-doc flash message | Phase 1 has no research/uat/etc. docs |
| Scroll testing (long doc, tables, code fences) | `02-02-PLAN.md` |
