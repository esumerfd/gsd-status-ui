---
created: 2026-08-17T19:24:47.604Z
title: Add support for md files that contain XML tags
area: general
severity: minor
files:
  - vendor/leaf/src/markdown/mod.rs
  - leaf-adapter/src/lib.rs
---

## Problem

GSD phase-plan markdown files use bare XML-style structural tags (`<objective>`,
`<execution_context>`, `<context>`, `<tasks>`, `<task type="auto">`, `<name>`, etc.)
to delimit sections. When such a file is opened in the leaf-based doc viewer, entire
tagged sections silently vanish from the rendered output instead of showing as text.

Root cause (confirmed via a throwaway repro in `leaf-adapter/tests/doc_view.rs` using
`DocView::open`/`render`): `vendor/leaf/src/markdown/mod.rs` (`parse_markdown_with_width`)
drives a `pulldown_cmark::Parser` and matches on `MdEvent` variants, but has no arm for
`MdEvent::Html`/`InlineHtml` — unhandled events fall into a catch-all `_ => {}` and are
dropped.

CommonMark's HTML-block grammar (type 7) treats any line that's just a bare open tag —
tag name made only of letters/digits/hyphens — as the start of an HTML block that
swallows everything up to the next blank line. Tags like `<objective>`, `<context>`,
and `<tasks>` (with all nested `<task>`/`<name>`/`<action>` children) match this and get
classified as HTML, so the renderer discards them entirely — not garbled, just gone.

Tags with underscores in the name (e.g. `<execution_context>`, `<success_criteria>`)
don't match CommonMark's tag-name grammar (no underscores allowed), so they fall through
as literal paragraph text instead — which is why some tagged sections show up dimly as
raw text with visible angle brackets while others disappear completely. This
inconsistency is what makes the bug confusing to spot.

Example file that reproduces it: `wk-i18n/.planning/phases/06-poc-pendo-language-visitor-metadata/06-01-PLAN.md`
(a GSD phase plan) — `<objective>`, `<context>`, and the whole `<tasks>` tree are missing
from the rendered panel; only `<execution_context>` and `<success_criteria>` show, with
tags visible.

## Solution

Add an `MdEvent::Html`/`InlineHtml` arm to `parse_markdown_with_width` in
`vendor/leaf/src/markdown/mod.rs` that renders raw HTML/XML-like block content as literal
styled text (tags visible, no styling assumptions) instead of silently dropping it —
mirroring the fallback behavior underscore-named tags already get by accident.

Since `vendor/leaf` is a vendored third-party crate (RivoLink/leaf), consider whether the
fix belongs upstream (PR to RivoLink/leaf) vs. a local patch to the vendored copy.
Alternative/complementary options worth considering:
- Pre-process source text in `leaf-adapter` (`load()` in `leaf-adapter/src/lib.rs`)
  to escape bare `<tag>` lines before handing off to `leaf::viewer::parse`, so
  pulldown-cmark never classifies them as HTML.
- Document that GSD-style plan files should fence structural tags in a ```` ```xml ````
  code block if they need to render reliably in this viewer, as a stopgap.
