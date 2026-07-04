# Plan: Interactive Document Review Tabs for gsd-status

**Status:** DRAFT — for review
**Date:** 2026-07-04
**Example GSD project used for development/testing:** `~/GoogleDrive/edward/Personal/work/wk-voice-agent`

---

## 1. Goal

Extend `gsd-status` from a print-and-exit status report into an interactive
terminal application that:

1. Owns the terminal and shows the current status view as the first panel.
2. Lets the user open the current phase's planning documents with keystroke
   chords (`o p`, `o r`, `o c`, `o v`, `o d`).
3. Shows each opened document in its own tab, rendered as markdown via the
   [leaf](https://github.com/rivolink/leaf) viewer, linked into the single
   `gsd-status` executable through a `leaf-adapter` layer.
4. Maintains a per-step tab set: `j`/`k` move between the steps (plans) of the
   current phase, and each step remembers which tabs were open for it.

## 2. Current State

- `gsd-status-ui` is a single-file (`src/main.rs`, ~760 lines), **zero-dependency**
  Rust binary. It parses `.planning/STATE.md`, `ROADMAP.md`, and phase
  directories, then prints a colored report and exits.
- Loading (`load_state`, `load_phases`, `infer_stage`) is already cleanly
  separated from rendering (`render(&mut impl Write, ...)`) — good seams for
  the refactor.
- Leaf (v1.26.0) is a **bin-only crate**: no `[lib]` target, so it cannot be
  consumed as a Cargo dependency as-is. It uses ratatui 0.30, crossterm 0.29,
  pulldown-cmark 0.12, syntect 5.2 — plus reqwest/sha2/semver for its
  self-update feature, which we do **not** want compiled into gsd-status.
  Its source is well modularized: `markdown/`, `render/`, `theme/`, `app/`.

## 3. Domain Model

### 3.1 Phase, step, and document kinds

For the current phase (first non-verified phase), the phase directory (e.g.
`.planning/phases/01-workflow-registry/`) contains two classes of documents:

| Scope | File pattern | Example | Keystroke |
|---|---|---|---|
| **Step** (one per plan) | `NN-MM-PLAN.md` | `01-01-PLAN.md`, `01-02-PLAN.md` | `o p` |
| **Phase** | `NN-RESEARCH.md` | `01-RESEARCH.md` | `o r` |
| **Phase** | `NN-CONTEXT.md` | `01-CONTEXT.md` | `o c` |
| **Phase** | `NN-VALIDATION.md` | `01-VALIDATION.md` | `o v` |
| **Phase** | `NN-DISCUSSION-LOG.md` | `01-DISCUSSION-LOG.md` | `o d` |
| **Phase** (future) | requirements/spec doc | — | reserved |

A **step** = one `NN-MM-PLAN.md` file. Steps are ordered by their `MM` index.
The initially-selected step is the first unchecked plan of the current phase
(falling back to the first step).

Phase-level docs (research, context, …) are the *same file* regardless of
which step is selected, but they are opened *into the current step's tab set*
— consistent with the per-step tab model below.

### 3.2 Tab model

- The **Status tab** is always present, always leftmost, and cannot be closed.
- Each step owns a **TabSet**: an ordered list of open document tabs.
- Only one step's TabSet is visible at a time (requirement: no two steps' tabs
  ever show together).
- Tab **ordering is fixed by kind**, not by open order:

  `requirements → plan → research → validation → context → discussion`

  Opening a doc inserts its tab at the canonical position. Opening an
  already-open kind just focuses its tab (no duplicates).

  > ⚠️ **Open question:** your ordering list ("requirements, plan, validation,
  > context, discussion") omitted *research* even though `o r` opens it. I've
  > slotted research after plan — confirm or correct.

- **Step navigation:** `j` = later step, `k` = earlier step. On arrival:
  - If the target step's TabSet is non-empty → show it, focus its last-focused tab.
  - If empty → auto-open that step's PLAN document as the first (and focused)
    document tab.
- Closing a tab removes it from the step's TabSet; closing the last doc tab
  falls back to focusing the Status tab.

### 3.3 Proposed key map

| Key | Context | Action |
|---|---|---|
| `o` then `p/r/c/v/d` | any tab | Open (or focus) plan / research / context / validation / discussion |
| `j` / `k` | any tab | Later / earlier step (swaps visible TabSet) |
| `Tab` / `Shift-Tab` (or `l`/`h`) | any tab | Cycle focus right / left through visible tabs |
| `1..9` | any tab | Jump to tab N (1 = Status) |
| `x` | doc tab | Close current tab |
| `↑`/`↓`, `PgUp`/`PgDn`, `g`/`G` | doc tab | Scroll document |
| `Esc` | mid-chord | Cancel a pending `o` chord |
| `q`, `Ctrl-C` | any tab | Quit (restore terminal) |

> ⚠️ **Deliberate conflict resolution:** leaf itself uses `j`/`k` for
> scrolling. Since `j`/`k` are reserved here for step navigation, document
> scrolling uses arrows/PgUp/PgDn. If that feels wrong in use, an alternative
> is `J`/`K` (shift) for steps and `j`/`k` for scroll — flag preference at review.

The `o` chord is implemented as a tiny pending-prefix state with a visual hint
in the footer (`o- p:plan r:research c:context v:validation d:discussion`),
making future chord families (your "additional keystroke types") cheap to add.

### 3.4 Screen layout

```
┌ Status │ 01-01-PLAN.md │ 01-RESEARCH.md ────────────────────┐  ← tab bar
│                                                              │
│   (active tab content: status panel or leaf-rendered doc)    │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ Phase 01 · step 01-01 (1/2)   o:open j/k:step x:close q:quit │  ← footer
└──────────────────────────────────────────────────────────────┘
```

The footer always shows the current phase + step position so `j`/`k` context
is never ambiguous.

## 4. Architecture

### 4.1 Workspace restructure

Convert the repo to a Cargo workspace:

```
gsd-status-ui/
├── Cargo.toml                 # [workspace] members = ["gsd-status", "leaf-adapter"]
├── vendor/leaf/               # git submodule (see §4.3)
├── crates? no — keep flat:
├── gsd-status/                # the binary crate (current code, split into modules)
│   └── src/
│       ├── main.rs            # arg parsing, tty detection, mode dispatch
│       ├── model.rs           # StateMeta, Phase, Plan, Stage, Step, DocKind
│       ├── planning.rs        # load_state, load_phases, doc discovery
│       ├── report.rs          # existing print-and-exit renderer (kept!)
│       └── tui/
│           ├── mod.rs         # event loop, terminal setup/teardown
│           ├── app.rs         # App state: tabs, tab sets, current step, chord state
│           ├── tabs.rs        # TabSet, DocKind ordering/insertion, focus rules
│           └── status_panel.rs# status view rendered as a ratatui widget
└── leaf-adapter/              # wrapper crate isolating all leaf coupling
    └── src/lib.rs             # DocView: open/close/render/handle_key/scroll
```

### 4.2 Two run modes (compatibility preserved)

- **Interactive (new default):** stdout is a TTY → enter the tabbed TUI
  (crossterm raw mode + alternate screen, so the user's scrollback is
  untouched on exit).
- **Plain (existing behavior):** stdout is not a TTY, or `--plain` /
  `--no-tui` given → print the current report and exit. This keeps
  `gsd-status | less`, scripts, and CI output working unchanged.

### 4.3 Leaf integration — options and recommendation

Leaf has no `[lib]` target, so "add submodule and depend on it" doesn't work
directly. Three options:

| | Approach | Pros | Cons |
|---|---|---|---|
| **A** | Submodule upstream leaf; `leaf-adapter` includes selected leaf source files via `#[path]` module includes (`markdown/`, `render/`, `theme/` only) | No fork to maintain; pin exact SHA; excludes updater/reqwest automatically | Fragile: leaf-internal refactors break the includes; leaf's modules may have cross-deps on `config`/`app` we'd have to stub |
| **B** ✅ | Fork leaf, add a minimal `src/lib.rs` exposing `markdown`, `render`, `theme` (feature-gated so the updater/reqwest/CLI stay out of the lib build); submodule the **fork**; `leaf-adapter` takes a normal path dependency `leaf = { path = "../vendor/leaf", default-features = false, features = ["render"] }` | Clean Cargo story; one executable; upstream syncs are a rebase of a ~2-file patch; the lib.rs change is upstreamable as a PR to rivolink/leaf | Requires maintaining a small fork until/unless upstream accepts the patch |
| **C** | Don't link leaf; depend on the same crates (ratatui, pulldown-cmark, syntect) and write a minimal markdown widget, using leaf as reference | Zero coupling to leaf internals | Re-implements rendering; loses leaf's polish (themes, code highlight, tables); contradicts the stated goal |

**Recommendation: B.** It's the only option that gives a supported Cargo
dependency graph *and* single-executable linking *and* a path back to
upstream. The fork patch is deliberately tiny: `lib.rs` + `Cargo.toml`
feature flags; everything else stays pristine so `git merge upstream/main`
stays cheap.

**Version pinning consequence:** gsd-status's TUI must use the same
`ratatui`/`crossterm` versions as leaf (0.30 / 0.29) so the adapter can pass
`Frame`/`Rect` types across the boundary. The workspace pins these once in
`[workspace.dependencies]`.

### 4.4 The leaf-adapter contract

`leaf-adapter` is the *only* crate that imports leaf types. gsd-status talks
to this trait, so the viewer is swappable (and option C remains a fallback if
B hits a wall):

```rust
pub struct DocView { /* wraps leaf's parsed doc + render/scroll state */ }

impl DocView {
    pub fn open(path: &Path) -> Result<DocView, DocViewError>;  // parse + theme
    pub fn title(&self) -> &str;                                // file name for the tab
    pub fn render(&mut self, frame: &mut Frame, area: Rect);    // draw into a tab body
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome;  // scroll keys it consumes
    pub fn reload(&mut self) -> Result<(), DocViewError>;       // re-read file from disk
}
```

Open/close of a "view panel within a tab" maps to constructing/dropping a
`DocView`; gsd-status owns the tab chrome, the adapter owns everything inside
the tab body.

Scope guard: leaf features we do **not** adopt: its file picker, search,
config file, editor integration, self-update. Only markdown → styled
scrollable panel.

## 5. Implementation Phases (strict red-green-refactor per task)

Each phase lands as its own PR-able unit with tests written first. The pure
state machine (tabs, steps, chords) is designed to be testable without a real
terminal — ratatui's `TestBackend` covers the rendering assertions.

### Phase 1 — Restructure without behavior change
1. **RED:** golden-output test: run current binary logic against a fixture
   `.planning/` tree (copied from wk-voice-agent's shape) and snapshot the
   plain report. Test fails until the harness exists.
2. **GREEN/REFACTOR:** split `main.rs` into `model` / `planning` / `report`
   modules; workspace-ify. Snapshot must stay byte-identical (minus color
   when `NO_COLOR`).
3. Add `--plain` flag + TTY detection dispatch (TUI mode stubbed to fall back
   to plain).

### Phase 2 — Step & document discovery
1. **RED:** unit tests against fixture phase dirs: enumerate steps
   (`01-01`, `01-02`) from `NN-MM-PLAN.md` files; resolve each DocKind to a
   path (present and missing cases); initial step = first unchecked plan.
2. **GREEN:** implement `Step`, `DocKind`, discovery in `planning.rs`.

### Phase 3 — Tab state machine (no rendering yet)
1. **RED:** unit tests for `App`/`TabSet`: canonical insertion order;
   open-focuses-existing; per-step tab persistence across `j`/`k`; empty step
   auto-opens plan; close-last-tab focuses Status; chord state (`o` →
   pending → resolve/cancel); missing-doc open is a no-op + status message.
2. **GREEN:** implement `tui/app.rs`, `tui/tabs.rs` as a pure
   `fn handle(event) -> ()` state machine over an injected doc-opener trait
   (so tests need no filesystem or leaf).

### Phase 4 — TUI shell
1. **RED:** `TestBackend` tests: tab bar shows Status + open docs in order
   with focus highlight; footer shows phase/step and key hints; status panel
   content matches the plain report's data.
2. **GREEN:** event loop (crossterm read → app.handle → draw), raw
   mode/alternate screen setup with panic-hook teardown (never leave the
   user's terminal broken), status panel as a widget reusing `report`'s data.

### Phase 5 — Leaf submodule + adapter
1. Fork `rivolink/leaf`; add `lib.rs` + feature flags; add as submodule at
   `vendor/leaf`; wire `[workspace.dependencies]` ratatui/crossterm to leaf's
   versions. Verify `cargo build` produces one static binary and that
   reqwest/updater code is absent from the dependency graph
   (`cargo tree -e no-dev | grep -c reqwest` == 0).
2. **RED:** adapter tests: `DocView::open` on a fixture markdown file renders
   non-empty styled buffer via `TestBackend`; scroll keys change the visible
   region; `open` on a missing file returns `Err`.
3. **GREEN:** implement `DocView` over leaf's `markdown`/`render`/`theme`.

### Phase 6 — Integration + polish
1. **RED:** end-to-end `TestBackend` test: synthetic key sequence
   `o p, o r, j, o v, k` yields the expected tab bars at each step.
2. **GREEN:** wire adapter into the tab body; doc `reload` on tab focus (docs
   change on disk as GSD agents run — cheap freshness win).
3. Manual UAT against wk-voice-agent's live `.planning/` tree; update README
   and `--help`.

## 6. Risks & Open Questions

1. **Leaf internal coupling (main risk):** leaf's `render`/`markdown` modules
   may depend on its `config`/`app` structs more deeply than the directory
   layout suggests. Mitigation: Phase 5 starts with a 1-hour spike compiling
   just those modules behind the lib feature; if the web of dependencies is
   too tangled, fall back to option C (same crates, minimal own renderer)
   behind the *unchanged* `DocView` contract — Phases 1–4 are unaffected.
2. **Research tab position** — see §3.2 open question.
3. **`j`/`k` semantics** — steps (as specced) vs. muscle-memory scrolling; see §3.3.
4. **Steps beyond the current phase:** should `j` at the last step of the
   current phase cross into the next phase's steps? Assumed **no** for v1
   (current phase only).
5. **"Requirements" doc:** named in your tab ordering but no file pattern
   exists yet in GSD phases. Reserved a DocKind slot and (future) keystroke;
   needs a file-pattern decision when it becomes real.
6. **Submodule + npm-style distribution:** anyone building from source now
   needs `git clone --recurse-submodules`. Documented in README; `make build`
   will init submodules automatically.

## 7. Out of Scope (v1)

- Editing documents (view-only; leaf's editor integration not wired).
- Watching files for live updates (manual reload-on-focus only).
- Leaf's file picker, search-in-document, theme switching UI, self-update.
- Multi-phase browsing, milestone history.
