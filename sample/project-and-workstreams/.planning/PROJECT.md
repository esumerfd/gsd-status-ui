# Sample Project — Neighborhood Tool Library

A demo GSD workspace exercising the partially-migrated shape: root-level
project files (this `PROJECT.md`, `ROADMAP.md`, a root phase, root research,
and a root todo) coexist with two workstreams, alpha and beta, that have
started taking over planning. Alpha has grown its own `research/` directory
but no `ROADMAP.md` of its own; beta has grown a full `ROADMAP.md` and its own
`todos/`.

## Goals

- A root project half-migrated to workstreams, not fully either way.
- Alpha exercises the shadowing and split-brain gaps.
- Beta exercises the invisible-scoped-todos gap, and is the healthy roadmap
  control arm proving the other two gaps are about federation, not a broken
  fixture.

## Constraints

- Tools only. Reservation and notification logic never actually run — this is
  a planning-document fixture, not an app.
