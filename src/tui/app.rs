//! Tab/step state machine. Pure state: no terminal, no leaf. The shell
//! owns the actual DocViews and creates one per OpenRequest returned here.
//!
//! Steps are a flat, roadmap-ordered list spanning ALL phases that have a
//! phase directory, so Ctrl-j/Ctrl-k walk seamlessly across phase
//! boundaries (e.g. Ctrl-k from the current phase's first step lands on the
//! previous phase's last step). Each step carries its phase context.

use crate::model::{Document, Phase, QuickTask, Step, Todo};
use crate::planning::{discover_documents, discover_steps, PhaseDocs};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One navigable step: its openable documents plus the phase it belongs to.
#[derive(Debug, Clone)]
pub(crate) struct StepEntry {
    pub(crate) phase_id: String,
    pub(crate) step: Step,
    /// Every document this entry can open, in canonical tab order: the plan
    /// (or the roadmap/todo file) first at index 0, then phase-level docs.
    /// A tab is identified by its index into this list, so any file can back a
    /// tab — not just a fixed enum of kinds.
    pub(crate) documents: Vec<Document>,
    /// 0-based position within its phase, and the phase's step count —
    /// for the footer's "step 02-02 (2/3)" display.
    pub(crate) pos_in_phase: usize,
    pub(crate) phase_steps: usize,
    /// `Some(title)` when this entry is a pending todo appended after the
    /// phase steps rather than a phase step. Its document 0 is the todo's
    /// markdown file, so `open_doc(0)` opens the todo.
    pub(crate) todo_title: Option<String>,
    /// `Some(title)` when this entry is an active quick task, appended after
    /// the phase steps and before the todos. Its `step.plan_path` is the
    /// task's `-PLAN.md` file, mirroring how a todo reuses its own markdown
    /// file — document-opening for tasks beyond that stays out of scope for
    /// Phase 2.
    pub(crate) quick_task_title: Option<String>,
    /// True for the single synthetic entry that fronts the list when a
    /// project-level `ROADMAP.md` exists. Its document 0 is that file, so
    /// `open_doc(0)` opens the roadmap — mirroring how a todo reuses index 0.
    roadmap: bool,
}

impl StepEntry {
    pub(crate) fn is_todo(&self) -> bool {
        self.todo_title.is_some()
    }

    pub(crate) fn is_task(&self) -> bool {
        self.quick_task_title.is_some()
    }

    pub(crate) fn is_roadmap(&self) -> bool {
        self.roadmap
    }
}

/// What the status body should highlight for the current selection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Selected {
    /// The project-level Roadmap row (above the Phases list).
    Roadmap,
    /// The row for this phase id (a step belongs to it).
    Phase(String),
    /// The Nth active quick-task row (0-based, in render order).
    Task(usize),
    /// The Nth pending todo row (0-based, in render order).
    Todo(usize),
}

/// What the shell must open (create a DocView for) after a state change.
/// `doc` is the index into the current entry's `documents`.
#[derive(Debug, PartialEq)]
pub(crate) struct OpenRequest {
    pub(crate) step: usize,
    pub(crate) doc: usize,
    pub(crate) path: PathBuf,
}

/// A tab is identified by the document index within its step entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Focus {
    Status,
    Doc(usize),
}

/// The Ctrl-o "open document" picker: the current step's existing documents
/// in canonical tab order. Each item is `(document index, file name)`.
#[derive(Debug)]
pub(crate) struct OpenDialog {
    pub(crate) items: Vec<(usize, String)>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, Default)]
struct TabSet {
    /// Open document indices (into the entry's `documents`), kept ascending so
    /// tabs stay in canonical order.
    tabs: Vec<usize>,
    /// 0 = Status tab, 1..=tabs.len() = document tabs.
    focused: usize,
}

pub(crate) struct App {
    entries: Vec<StepEntry>,
    pub(crate) current: usize,
    tabsets: Vec<TabSet>,
    dialog: Option<OpenDialog>,
    pub(crate) flash: Option<String>,
    pub(crate) quit: bool,
}

impl App {
    pub(crate) fn new(entries: Vec<StepEntry>) -> Self {
        // Default to the first unchecked *step*; the Roadmap row and todos
        // never grab the cursor on startup.
        let current = entries
            .iter()
            .position(|e| !e.is_todo() && !e.is_roadmap() && !e.is_task() && !e.step.checked)
            .unwrap_or(0);
        let tabsets = vec![TabSet::default(); entries.len()];
        Self {
            entries,
            current,
            tabsets,
            dialog: None,
            flash: None,
            quit: false,
        }
    }

    /// Flatten all phases in roadmap order. A phase with no step plans yet
    /// (or no phase directory at all) still gets one unchecked placeholder
    /// entry ("Step 1"), so an unstarted phase is selectable — and becomes
    /// the default once every real step before it is checked.
    /// Test-only convenience for building an App with no todos or tasks;
    /// production always goes through `from_phases_and_todos`.
    #[cfg(test)]
    pub(crate) fn from_phases(planning: &Path, phases: &[Phase]) -> Self {
        Self::from_phases_and_todos(planning, phases, &[], &[])
    }

    /// Like `from_phases`, but appends one navigable entry per active quick
    /// task, then one per pending todo, after all phase steps, so j/k walks
    /// steps, then tasks, then todos. `planning` locates the workspace-root
    /// `ROADMAP.md` for the leading Roadmap entry.
    pub(crate) fn from_phases_and_todos(
        planning: &Path,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> Self {
        Self::new(Self::build_entries(planning, phases, quick_tasks, todos))
    }

    /// The flattened roadmap-then-steps-then-tasks-then-todos entry list.
    /// Shared by construction and by `refresh` (the periodic reload), so both
    /// see the same ordering. A leading Roadmap entry fronts the list
    /// whenever phases exist (i.e. a `ROADMAP.md` parsed), mirroring the
    /// report's row.
    fn build_entries(
        planning: &Path,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> Vec<StepEntry> {
        let mut entries = Vec::new();
        if !phases.is_empty() {
            let roadmap_path = planning.join("ROADMAP.md");
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: String::new(),
                    plan_path: roadmap_path.clone(),
                    checked: false,
                },
                documents: vec![Document {
                    path: roadmap_path,
                    label: "roadmap".into(),
                }],
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: None,
                quick_task_title: None,
                roadmap: true,
            });
        }
        for phase in phases {
            let dir = phase.dir.as_deref();
            let prefix = PhaseDocs::new(dir.unwrap_or_else(|| Path::new(""))).prefix;
            let steps = dir
                .map(|d| discover_steps(d, &phase.plans))
                .unwrap_or_default();
            if steps.is_empty() {
                let step = Step {
                    id: "1".into(),
                    plan_path: PathBuf::new(),
                    checked: false,
                };
                let documents = dir
                    .map(|d| discover_documents(d, &prefix, &step))
                    .unwrap_or_default();
                entries.push(StepEntry {
                    phase_id: phase.id.clone(),
                    step,
                    documents,
                    pos_in_phase: 0,
                    phase_steps: 1,
                    todo_title: None,
                    quick_task_title: None,
                    roadmap: false,
                });
                continue;
            }
            let count = steps.len();
            for (i, step) in steps.into_iter().enumerate() {
                let documents = dir
                    .map(|d| discover_documents(d, &prefix, &step))
                    .unwrap_or_default();
                entries.push(StepEntry {
                    phase_id: phase.id.clone(),
                    step,
                    documents,
                    pos_in_phase: i,
                    phase_steps: count,
                    todo_title: None,
                    quick_task_title: None,
                    roadmap: false,
                });
            }
        }
        for task in quick_tasks {
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: task.id.clone(),
                    plan_path: task.dir.join(format!("{}-PLAN.md", task.id)),
                    checked: false,
                },
                documents: Vec::new(),
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: None,
                quick_task_title: Some(task.title.clone()),
                roadmap: false,
            });
        }
        for todo in todos {
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: todo.slug.clone(),
                    plan_path: todo.path.clone(),
                    checked: false,
                },
                documents: vec![Document {
                    path: todo.path.clone(),
                    label: "plan".into(),
                }],
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: Some(todo.title.clone()),
                quick_task_title: None,
                roadmap: false,
            });
        }
        entries
    }

    /// Rebuild the entry list from freshly loaded phases + todos (the periodic
    /// reload), so navigation bounds track a workspace that changed on disk —
    /// e.g. a todo captured while the TUI is open. The current selection is
    /// preserved by identity (or clamped into range if it vanished), and each
    /// surviving entry keeps its open-document tab set. Returns a map from old
    /// entry index to new index for the entries that survived, so the shell can
    /// remap its per-entry DocViews.
    pub(crate) fn refresh(
        &mut self,
        planning: &Path,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> HashMap<usize, usize> {
        // `(phase_id, step.id)` is a stable identity: step ids are phase-scoped
        // and unique, todo entries carry their (unique) slug as the step id,
        // quick-task entries carry their (unique) id as the step id, and the
        // single Roadmap entry is the unique `("", "")`.
        let key = |e: &StepEntry| (e.phase_id.clone(), e.step.id.clone());
        let selected = self.entries.get(self.current).map(&key);

        let new_entries = Self::build_entries(planning, phases, quick_tasks, todos);
        let new_index: HashMap<(String, String), usize> = new_entries
            .iter()
            .enumerate()
            .map(|(i, e)| (key(e), i))
            .collect();

        let mut new_tabsets = vec![TabSet::default(); new_entries.len()];
        let mut remap = HashMap::new();
        for (old_i, entry) in self.entries.iter().enumerate() {
            if let Some(&new_i) = new_index.get(&key(entry)) {
                new_tabsets[new_i] = self.tabsets[old_i].clone();
                remap.insert(old_i, new_i);
            }
        }

        self.current = selected
            .and_then(|k| new_index.get(&k).copied())
            .unwrap_or_else(|| self.current.min(new_entries.len().saturating_sub(1)));
        self.entries = new_entries;
        self.tabsets = new_tabsets;
        remap
    }

    /// What the status body should highlight for the current selection:
    /// the phase row for a step, or the todo row for a todo entry.
    pub(crate) fn selection(&self) -> Option<Selected> {
        let entry = self.entries.get(self.current)?;
        if entry.is_roadmap() {
            Some(Selected::Roadmap)
        } else if entry.is_task() {
            let ordinal = self.entries[..self.current]
                .iter()
                .filter(|e| e.is_task())
                .count();
            Some(Selected::Task(ordinal))
        } else if entry.is_todo() {
            let ordinal = self.entries[..self.current]
                .iter()
                .filter(|e| e.is_todo())
                .count();
            Some(Selected::Todo(ordinal))
        } else {
            Some(Selected::Phase(entry.phase_id.clone()))
        }
    }

    pub(crate) fn current_entry(&self) -> Option<&StepEntry> {
        self.entries.get(self.current)
    }

    /// The selected todo's title, or `None` when the selection is a phase
    /// step. Backs the `c` "copy todo name" key.
    pub(crate) fn current_todo_title(&self) -> Option<&str> {
        self.entries.get(self.current)?.todo_title.as_deref()
    }

    pub(crate) fn tabs(&self) -> &[usize] {
        self.tabsets
            .get(self.current)
            .map(|t| t.tabs.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn focus(&self) -> Focus {
        let Some(set) = self.tabsets.get(self.current) else {
            return Focus::Status;
        };
        if set.focused == 0 {
            Focus::Status
        } else {
            Focus::Doc(set.tabs[set.focused - 1])
        }
    }

    /// The document backing a `(step, doc index)` pair, if any. Used by the
    /// shell to label tabs and resolve paths.
    pub(crate) fn document(&self, step: usize, doc: usize) -> Option<&Document> {
        self.entries.get(step)?.documents.get(doc)
    }

    /// Open (or focus) the document at index `doc` for the current step.
    /// Returns an OpenRequest when a new tab was added; the shell must then
    /// create the DocView (and call `remove_tab` if that fails). Document
    /// indices are canonical order, so inserting them ascending keeps the tab
    /// row ordered: plan first, known kinds next, unmatched files last.
    pub(crate) fn open_doc(&mut self, doc: usize) -> Option<OpenRequest> {
        self.flash = None;
        let Some(entry) = self.entries.get(self.current) else {
            self.flash = Some("no active phase step".into());
            return None;
        };
        let Some(document) = entry.documents.get(doc) else {
            self.flash = Some("no document for this step".into());
            return None;
        };
        let path = document.path.clone();
        if !path.exists() {
            self.flash = Some(format!(
                "no {} document ({})",
                document.label,
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
            return None;
        }
        let step_idx = self.current;
        let set = &mut self.tabsets[step_idx];
        if let Some(pos) = set.tabs.iter().position(|d| *d == doc) {
            set.focused = pos + 1;
            return None;
        }
        let insert_at = set
            .tabs
            .iter()
            .position(|d| *d > doc)
            .unwrap_or(set.tabs.len());
        set.tabs.insert(insert_at, doc);
        set.focused = insert_at + 1;
        Some(OpenRequest {
            step: step_idx,
            doc,
            path,
        })
    }

    /// Move to a later (`+1`) or earlier (`-1`) step, crossing phase
    /// boundaries. Navigation preserves the current mode:
    /// - from the Status tab (browsing) the selection just moves — nothing
    ///   opens and focus stays on Status;
    /// - from a document tab (viewer mode) the target step's docs get focus,
    ///   auto-opening its plan when the step has no open tabs.
    pub(crate) fn change_step(&mut self, delta: i32) -> Option<OpenRequest> {
        self.flash = None;
        if self.entries.is_empty() {
            self.flash = Some("no steps in any phase".into());
            return None;
        }
        let target = self.current as i32 + delta;
        if target < 0 {
            self.flash = Some("already at the first step".into());
            return None;
        }
        if target as usize >= self.entries.len() {
            self.flash = Some("already at the last step".into());
            return None;
        }
        let browsing = matches!(self.focus(), Focus::Status);
        self.current = target as usize;
        if browsing {
            self.tabsets[self.current].focused = 0;
            return None;
        }
        if self.tabs().is_empty() {
            // Document 0 is the entry's primary file (plan / roadmap / todo).
            return self.open_doc(0);
        }
        let set = &mut self.tabsets[self.current];
        if set.focused == 0 {
            set.focused = 1;
        }
        None
    }

    /// Section ordinal for grouping entries: Roadmap(0), Phases(1), Tasks(2),
    /// Todos(3). Entries are built in this order, so each section is
    /// contiguous.
    fn section_key(e: &StepEntry) -> u8 {
        if e.is_roadmap() {
            0
        } else if e.is_task() {
            2
        } else if e.is_todo() {
            3
        } else {
            1
        }
    }

    /// Start index of each contiguous section present, in entry order.
    fn section_bounds(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut last: Option<u8> = None;
        for (i, e) in self.entries.iter().enumerate() {
            let k = Self::section_key(e);
            if last != Some(k) {
                starts.push(i);
                last = Some(k);
            }
        }
        starts
    }

    /// First index of each phase (distinct consecutive `phase_id` among phase
    /// steps; roadmap/todo rows are not phases).
    fn phase_starts(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut last: Option<&str> = None;
        for (i, e) in self.entries.iter().enumerate() {
            if e.is_roadmap() || e.is_todo() || e.is_task() {
                last = None;
                continue;
            }
            let pid = e.phase_id.as_str();
            if last != Some(pid) {
                starts.push(i);
                last = Some(pid);
            }
        }
        starts
    }

    /// Move the selection to `idx` in browsing mode (Status focus).
    fn set_browsing(&mut self, idx: usize) {
        self.current = idx;
        if let Some(set) = self.tabsets.get_mut(idx) {
            set.focused = 0;
        }
    }

    /// `g` / `G` — jump the selection to the first / last entry.
    pub(crate) fn select_first(&mut self) {
        self.flash = None;
        if !self.entries.is_empty() {
            self.set_browsing(0);
        }
    }

    pub(crate) fn select_last(&mut self) {
        self.flash = None;
        if !self.entries.is_empty() {
            self.set_browsing(self.entries.len() - 1);
        }
    }

    /// `d` / `u` — jump to the next / previous section (Roadmap / Phases /
    /// Todos). Going up first snaps to the top of the current section, then to
    /// the previous section's top.
    pub(crate) fn select_section(&mut self, delta: i32) {
        self.flash = None;
        if self.entries.is_empty() {
            return;
        }
        let starts = self.section_bounds();
        if delta > 0 {
            match starts.iter().copied().find(|&s| s > self.current) {
                Some(next) => self.set_browsing(next),
                None => self.flash = Some("already at the last section".into()),
            }
        } else {
            let cur_start = starts
                .iter()
                .copied()
                .rev()
                .find(|&s| s <= self.current)
                .unwrap_or(0);
            if self.current > cur_start {
                self.set_browsing(cur_start);
            } else {
                match starts.iter().copied().rev().find(|&s| s < cur_start) {
                    Some(prev) => self.set_browsing(prev),
                    None => self.flash = Some("already at the first section".into()),
                }
            }
        }
    }

    /// `J` / `K` — jump to the next / previous phase's first step. Steps within
    /// a phase are skipped; a roadmap/todo row anchors to the adjacent phase.
    pub(crate) fn select_phase(&mut self, delta: i32) {
        self.flash = None;
        // Roadmap, Tasks, and Todos rows have no phases, so J/K there behave
        // like j/k (move one row) rather than jumping into the Phases section.
        let on_phase = self
            .entries
            .get(self.current)
            .is_some_and(|e| !e.is_roadmap() && !e.is_todo() && !e.is_task());
        if !on_phase {
            self.change_step(delta);
            return;
        }
        let starts = self.phase_starts();
        let anchor = starts
            .iter()
            .copied()
            .rev()
            .find(|&s| s <= self.current)
            .unwrap_or(self.current);
        let target = if delta > 0 {
            starts.iter().copied().find(|&s| s > anchor)
        } else {
            starts.iter().copied().rev().find(|&s| s < anchor)
        };
        match target {
            Some(t) => self.set_browsing(t),
            // No next/prev phase: keep flowing in the same direction (down into
            // Todos, up onto the Roadmap) so the user needn't release Shift at
            // the section boundary.
            None => {
                self.change_step(delta);
            }
        }
    }

    pub(crate) fn dialog(&self) -> Option<&OpenDialog> {
        self.dialog.as_ref()
    }

    /// Index of the synthetic Roadmap entry, if a `ROADMAP.md` exists.
    pub(crate) fn roadmap_index(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.is_roadmap())
    }

    /// Jump the cursor to the Roadmap entry and open (or focus) its tab. Used by
    /// the global `R` peek; the caller stashes the prior location and restores
    /// it with [`restore_location`] on Esc.
    pub(crate) fn open_roadmap_peek(&mut self) -> Option<OpenRequest> {
        let idx = self.roadmap_index()?;
        self.current = idx;
        self.open_doc(0)
    }

    /// Restore a `(current, focus)` pair captured before an `R` peek: move the
    /// cursor back and re-focus the tab (or Status) that was active.
    pub(crate) fn restore_location(&mut self, current: usize, focus: Focus) {
        if current >= self.entries.len() {
            return;
        }
        self.current = current;
        let slot = match focus {
            Focus::Status => 0,
            Focus::Doc(doc) => self.tabsets[current]
                .tabs
                .iter()
                .position(|d| *d == doc)
                .map(|p| p + 1)
                .unwrap_or(0),
        };
        self.tabsets[current].focused = slot;
    }

    /// Open the Ctrl-o picker listing the current step's existing documents.
    pub(crate) fn open_dialog(&mut self) {
        self.flash = None;
        let Some(entry) = self.entries.get(self.current) else {
            self.flash = Some("no active phase step".into());
            return;
        };
        if entry.is_roadmap() {
            self.flash = Some("press Enter or R to open the roadmap".into());
            return;
        }
        let items: Vec<(usize, String)> = entry
            .documents
            .iter()
            .enumerate()
            .filter_map(|(doc, document)| {
                if !document.path.exists() {
                    return None;
                }
                let name = document.path.file_name()?.to_string_lossy().into_owned();
                Some((doc, name))
            })
            .collect();
        if items.is_empty() {
            self.flash = Some("no documents for this step".into());
            return;
        }
        self.dialog = Some(OpenDialog { items, selected: 0 });
    }

    pub(crate) fn close_dialog(&mut self) {
        self.dialog = None;
    }

    pub(crate) fn dialog_move(&mut self, delta: i32) {
        if let Some(dialog) = self.dialog.as_mut() {
            let last = dialog.items.len().saturating_sub(1) as i32;
            dialog.selected = (dialog.selected as i32 + delta).clamp(0, last) as usize;
        }
    }

    /// Open the selected document and close the dialog. As with `open_doc`,
    /// Some(request) means the shell must create the DocView.
    pub(crate) fn dialog_select(&mut self) -> Option<OpenRequest> {
        let dialog = self.dialog.take()?;
        let (doc, _) = dialog.items.get(dialog.selected)?;
        self.open_doc(*doc)
    }

    /// Close the focused document tab. Returns the (step, doc index) whose view
    /// the shell should drop. Closing the Status tab is a no-op.
    pub(crate) fn close_current(&mut self) -> Option<(usize, usize)> {
        let step_idx = self.current;
        let set = self.tabsets.get_mut(step_idx)?;
        if set.focused == 0 {
            return None;
        }
        let doc = set.tabs.remove(set.focused - 1);
        set.focused = set.focused.min(set.tabs.len());
        Some((step_idx, doc))
    }

    /// Called by the shell when creating a DocView failed after open_doc.
    pub(crate) fn remove_tab(&mut self, step: usize, doc: usize, reason: String) {
        if let Some(set) = self.tabsets.get_mut(step) {
            if let Some(pos) = set.tabs.iter().position(|d| *d == doc) {
                set.tabs.remove(pos);
                set.focused = set.focused.min(set.tabs.len());
            }
        }
        self.flash = Some(reason);
    }

    pub(crate) fn focus_next(&mut self) {
        if let Some(set) = self.tabsets.get_mut(self.current) {
            set.focused = (set.focused + 1) % (set.tabs.len() + 1);
        }
    }

    pub(crate) fn focus_prev(&mut self) {
        if let Some(set) = self.tabsets.get_mut(self.current) {
            set.focused = (set.focused + set.tabs.len()) % (set.tabs.len() + 1);
        }
    }

    /// Focus tab N, where 1 = Status and 2.. = document tabs.
    pub(crate) fn focus_slot(&mut self, n: usize) {
        if let Some(set) = self.tabsets.get_mut(self.current) {
            if n >= 1 && n <= set.tabs.len() + 1 {
                set.focused = n - 1;
            }
        }
    }

    /// Test-only: the document index for a given label within `step`'s entry.
    #[cfg(test)]
    pub(crate) fn doc_id(&self, step: usize, label: &str) -> usize {
        self.entries[step]
            .documents
            .iter()
            .position(|d| d.label == label)
            .unwrap_or_else(|| panic!("no {label} document at step {step}"))
    }

    /// Test-only: labels of the current step's open tabs, in tab order.
    #[cfg(test)]
    pub(crate) fn tab_labels(&self) -> Vec<String> {
        let entry = &self.entries[self.current];
        self.tabsets[self.current]
            .tabs
            .iter()
            .map(|&d| entry.documents[d].label.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sample_planning() -> &'static Path {
        Path::new("sample/.planning")
    }

    fn sample_phases() -> Vec<Phase> {
        crate::planning::load_phases(sample_planning())
    }

    fn sample_app() -> App {
        let app = App::from_phases(sample_planning(), &sample_phases());
        let ids: Vec<&str> = app.entries.iter().map(|e| e.step.id.as_str()).collect();
        // Leading "" is the synthetic Roadmap entry; "1" is the phase-3
        // placeholder (no plans yet).
        assert_eq!(ids, ["", "01-01", "02-01", "02-02", "02-03", "1"]);
        app
    }

    #[test]
    fn starts_on_status_tab_of_first_unchecked_step_across_phases() {
        let app = sample_app();
        let entry = app.current_entry().unwrap();
        assert_eq!(entry.step.id, "02-02");
        assert_eq!(entry.phase_id, "2");
        assert_eq!((entry.pos_in_phase, entry.phase_steps), (1, 3));
        assert_eq!(app.focus(), Focus::Status);
        assert!(app.tabs().is_empty());
    }

    #[test]
    fn status_browsing_moves_selection_without_opening_docs() {
        let mut app = sample_app();

        // From the status tab, j/k only move the selection.
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.current_entry().unwrap().step.id, "02-01");
        assert_eq!(app.focus(), Focus::Status);
        assert!(app.tabs().is_empty());

        // Crossing the phase boundary is still just browsing.
        assert!(app.change_step(-1).is_none());
        let entry = app.current_entry().unwrap();
        assert_eq!(entry.phase_id, "1");
        assert_eq!((entry.pos_in_phase, entry.phase_steps), (0, 1));
        assert_eq!(app.focus(), Focus::Status);

        // 01-01 is the first phase step; k moves up onto the Roadmap row.
        assert!(app.change_step(-1).is_none());
        assert!(app.current_entry().unwrap().is_roadmap());
        assert_eq!(app.selection(), Some(Selected::Roadmap));
        assert_eq!(app.focus(), Focus::Status);

        // The Roadmap row is the very first entry.
        assert!(app.change_step(-1).is_none());
        assert!(app.flash.as_deref().unwrap().contains("first step"));
    }

    #[test]
    fn enter_opens_the_plan_and_viewer_stepping_stays_in_viewer() {
        let mut app = sample_app();
        app.change_step(-1);
        app.change_step(-1); // browsing on 01-01, still status

        // Enter: open the plan (document 0), entering viewer mode.
        let req = app.open_doc(0).expect("open 01-01 plan");
        assert!(req.path.ends_with("01-01-PLAN.md"));
        assert_eq!(app.focus(), Focus::Doc(0));

        // Ctrl-j from viewer mode: keep viewing — the next step's plan
        // auto-opens because its tab set is empty.
        let req = app.change_step(1).expect("auto-open 02-01 plan");
        assert!(req.path.ends_with("02-01-PLAN.md"));
        assert_eq!(app.focus(), Focus::Doc(0));

        // Back to 01-01: its plan tab is retained and refocused.
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        assert_eq!(app.focus(), Focus::Doc(0));
    }

    #[test]
    fn status_browsing_onto_a_step_with_tabs_stays_on_status() {
        let mut app = sample_app();
        let research = app.doc_id(app.current, "research");
        app.open_doc(research); // 02-02 now has a tab, viewer focus
        app.focus_slot(1); // back to status
        app.change_step(1); // 02-03
        assert_eq!(app.focus(), Focus::Status);
        app.change_step(-1); // back onto 02-02, which has an open tab
        assert_eq!(
            app.focus(),
            Focus::Status,
            "browsing from status must not jump into a doc"
        );
        assert_eq!(app.tab_labels(), ["research"], "tab set preserved");
    }

    #[test]
    fn open_inserts_in_canonical_order_regardless_of_open_order() {
        let mut app = sample_app();
        let cur = app.current;
        assert!(app.open_doc(app.doc_id(cur, "discussion")).is_some());
        assert!(app.open_doc(app.doc_id(cur, "uat")).is_some());
        assert!(app.open_doc(app.doc_id(cur, "research")).is_some());
        assert!(app.open_doc(app.doc_id(cur, "plan")).is_some());
        assert_eq!(app.tab_labels(), ["plan", "research", "uat", "discussion"]);
        // Last opened (plan) is focused.
        assert_eq!(app.focus(), Focus::Doc(app.doc_id(cur, "plan")));
    }

    #[test]
    fn reopening_focuses_without_duplicating() {
        let mut app = sample_app();
        let cur = app.current;
        let research = app.doc_id(cur, "research");
        assert!(app.open_doc(research).is_some());
        assert!(app.open_doc(app.doc_id(cur, "context")).is_some());
        assert!(app.open_doc(research).is_none()); // no new request
        assert_eq!(app.tab_labels(), ["research", "context"]);
        assert_eq!(app.focus(), Focus::Doc(research));
    }

    #[test]
    fn step_change_preserves_tabsets_and_autoopens_plan_on_empty() {
        let mut app = sample_app();
        let cur = app.current;
        app.open_doc(app.doc_id(cur, "research"));
        app.open_doc(app.doc_id(cur, "validation"));

        // Later step (02-03): empty tab set -> plan (doc 0) auto-opens.
        let req = app.change_step(1).expect("auto-open plan");
        assert_eq!(req.doc, 0);
        assert!(req.path.ends_with("02-03-PLAN.md"));
        assert_eq!(app.tab_labels(), ["plan"]);
        assert_eq!(app.focus(), Focus::Doc(0));

        // Back to 02-02: its tabs are intact, no auto-open.
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.tab_labels(), ["research", "validation"]);
    }

    #[test]
    fn step_change_past_the_last_step_flashes() {
        let mut app = sample_app();
        app.change_step(1); // 02-03
        app.change_step(1); // phase 3 placeholder (last)
        assert_eq!(app.current_entry().unwrap().phase_id, "3");
        assert!(app.change_step(1).is_none());
        assert!(app.flash.as_deref().unwrap().contains("last step"));
    }

    #[test]
    fn closing_last_tab_falls_back_to_status() {
        let mut app = sample_app();
        app.open_doc(0); // plan
        let closed = app.close_current().expect("closed");
        assert_eq!(closed.1, 0);
        assert!(app.tabs().is_empty());
        assert_eq!(app.focus(), Focus::Status);
        // Closing on the Status tab is a no-op.
        assert!(app.close_current().is_none());
    }

    #[test]
    fn missing_document_flashes_and_adds_no_tab() {
        // Phase 1 (01-navigation-skeleton) has no RESEARCH doc.
        let phases = sample_phases();
        let mut app = App::from_phases(sample_planning(), &phases[..1]);
        // Phase 1's only step is checked, so the default lands on the Roadmap
        // row; step down onto 01-01.
        app.change_step(1);
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        // 01-01's documents are only [plan, verification] — no research exists,
        // so index 5 (a would-be discussion slot) is out of range and no-ops.
        assert!(app.open_doc(5).is_none());
        assert!(app.tabs().is_empty());
        assert!(app.flash.as_deref().unwrap().contains("no document"));
    }

    #[test]
    fn focus_cycles_through_status_and_tabs() {
        let mut app = sample_app();
        let cur = app.current;
        let plan = app.doc_id(cur, "plan");
        let context = app.doc_id(cur, "context");
        app.open_doc(plan);
        app.open_doc(context);
        app.focus_slot(1);
        assert_eq!(app.focus(), Focus::Status);
        app.focus_next();
        assert_eq!(app.focus(), Focus::Doc(plan));
        app.focus_next();
        assert_eq!(app.focus(), Focus::Doc(context));
        app.focus_next(); // wraps
        assert_eq!(app.focus(), Focus::Status);
        app.focus_prev(); // wraps back
        assert_eq!(app.focus(), Focus::Doc(context));
    }

    fn sample_todos() -> Vec<crate::model::Todo> {
        crate::planning::load_todos(Path::new("sample/.planning"), false)
    }

    fn todo(slug: &str, title: &str) -> crate::model::Todo {
        crate::model::Todo {
            title: title.into(),
            area: None,
            slug: slug.into(),
            path: std::path::PathBuf::from(format!("{slug}.md")),
            completed: false,
        }
    }

    fn sample_quick_tasks() -> Vec<QuickTask> {
        crate::planning::load_quick_tasks(sample_planning(), false)
    }

    #[test]
    fn quick_tasks_are_inserted_between_phase_steps_and_todos() {
        let app = App::from_phases_and_todos(
            sample_planning(),
            &sample_phases(),
            &sample_quick_tasks(),
            &sample_todos(),
        );
        // 1 roadmap + 5 steps + 4 active tasks + 3 todos.
        assert_eq!(app.entries.len(), 13);
        let last_phase_idx = 5; // the phase-3 placeholder
        let first_todo_idx = 10;
        for e in &app.entries[(last_phase_idx + 1)..first_todo_idx] {
            assert!(e.is_task(), "expected a task row: {e:?}");
            assert!(!e.is_todo());
        }
        assert!(!app.entries[last_phase_idx].is_task());
        assert!(app.entries[first_todo_idx].is_todo());
        assert!(!app.entries[first_todo_idx].is_task());
    }

    #[test]
    fn refresh_appends_a_new_todo_so_nav_can_reach_it() {
        // Start with no todos and walk to the very last entry.
        let mut app = App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &[]);
        let last = app.entries.len() - 1;
        while app.current < last {
            app.change_step(1);
        }
        assert!(app.change_step(1).is_none());
        assert!(app.flash.as_deref().unwrap().contains("last step"));

        // A timed reload picks up a freshly captured todo.
        app.refresh(
            sample_planning(),
            &sample_phases(),
            &[],
            &[todo("2026-07-09-new-todo", "Fresh todo")],
        );

        // The down-movement bound now derives from the grown list: j descends
        // onto the appended todo instead of clamping against the stale length.
        assert!(app.change_step(1).is_none());
        let entry = app.current_entry().unwrap();
        assert!(entry.is_todo(), "cursor should reach the new todo row");
        assert_eq!(entry.todo_title.as_deref(), Some("Fresh todo"));
    }

    #[test]
    fn refresh_preserves_the_current_selection_by_identity() {
        let mut app = App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &[]);
        app.change_step(-1); // browse to 02-01
        assert_eq!(app.current_entry().unwrap().step.id, "02-01");

        // Reload that only appends a todo must not move the cursor off 02-01.
        app.refresh(
            sample_planning(),
            &sample_phases(),
            &[],
            &[todo("2026-07-09-new-todo", "Fresh todo")],
        );
        let entry = app.current_entry().unwrap();
        assert_eq!(entry.step.id, "02-01");
        assert_eq!(entry.phase_id, "2");
    }

    #[test]
    fn refresh_remaps_open_tabs_to_surviving_entries() {
        let mut app = App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &[]);
        // Open a doc on the current step (02-02), then reload with a new todo.
        app.open_doc(app.doc_id(app.current, "research"));
        let before = app.current;
        assert_eq!(app.tab_labels(), ["research"]);

        let remap = app.refresh(
            sample_planning(),
            &sample_phases(),
            &[],
            &[todo("2026-07-09-new-todo", "Fresh todo")],
        );

        // The step kept its position and its open tab survived; the remap
        // reports the (unchanged, here) old->new index so the shell can move
        // its DocViews.
        assert_eq!(remap.get(&before), Some(&app.current));
        assert_eq!(app.tab_labels(), ["research"]);
    }

    #[test]
    fn refresh_clamps_selection_when_entries_shrink() {
        // Select the last entry, then reload a workspace with fewer entries.
        let mut app = App::from_phases_and_todos(
            sample_planning(),
            &sample_phases(),
            &[],
            &[todo("t", "Gone soon")],
        );
        app.current = app.entries.len() - 1;
        assert!(app.current_entry().unwrap().is_todo());

        app.refresh(sample_planning(), &sample_phases(), &[], &[]);
        assert!(
            app.current < app.entries.len(),
            "selection must stay in range after the list shrinks"
        );
    }

    #[test]
    fn todos_are_appended_after_steps_and_default_skips_them() {
        let app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        // Default lands on the first unchecked real step, never the Roadmap
        // row or a todo.
        assert!(!app.current_entry().unwrap().is_todo());
        assert!(!app.current_entry().unwrap().is_roadmap());
        assert_eq!(app.current_entry().unwrap().step.id, "02-02");
        // 1 roadmap + 5 steps + 3 todos.
        assert_eq!(app.entries.len(), 9);
        assert!(app.entries[0].is_roadmap());
        assert!(app.entries[6].is_todo());
        assert!(app.entries[8].is_todo());
    }

    #[test]
    fn stepping_reaches_todos_and_enter_opens_the_todo_md() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        // 02-02 (idx 2) -> 02-03 -> phase-3 placeholder -> first todo (idx 5).
        app.change_step(1);
        app.change_step(1);
        app.change_step(1);
        assert!(app.current_entry().unwrap().is_todo());
        let req = app.open_doc(0).expect("open todo md");
        assert!(
            req.path.ends_with("2026-07-07-signed-build.md"),
            "{}",
            req.path.display()
        );
    }

    #[test]
    fn current_todo_title_is_some_only_on_a_todo() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        // Starts on a real step.
        assert!(app.current_todo_title().is_none());
        // Walk to the first todo (02-02 -> 02-03 -> placeholder -> todo0).
        app.change_step(1);
        app.change_step(1);
        app.change_step(1);
        assert_eq!(
            app.current_todo_title(),
            Some("Official signed build process for pr-monitor apps")
        );
    }

    #[test]
    fn selection_reports_phase_for_steps_and_ordinal_for_todos() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        assert_eq!(app.selection(), Some(Selected::Phase("2".into())));
        app.change_step(1);
        app.change_step(1);
        app.change_step(1); // first todo
        assert_eq!(app.selection(), Some(Selected::Todo(0)));
        app.change_step(1); // second todo
        assert_eq!(app.selection(), Some(Selected::Todo(1)));
    }

    #[test]
    fn phase_without_steps_gets_a_placeholder_entry() {
        let app = App::from_phases(sample_planning(), &sample_phases());
        let last = app.entries.last().expect("placeholder entry");
        assert_eq!(last.phase_id, "3");
        assert_eq!(last.step.id, "1");
        assert!(!last.step.checked, "an unstarted phase is unchecked");
        assert_eq!((last.pos_in_phase, last.phase_steps), (0, 1));
    }

    #[test]
    fn starts_on_the_unstarted_phase_when_all_steps_are_checked() {
        // Mark every real step checked; the phase-3 placeholder must win.
        let mut app = App::from_phases(sample_planning(), &sample_phases());
        for entry in app.entries.iter_mut() {
            if entry.phase_id != "3" {
                entry.step.checked = true;
            }
        }
        let app = App::new(app.entries);
        let entry = app.current_entry().unwrap();
        assert_eq!(entry.phase_id, "3");
        assert_eq!(entry.step.id, "1");
    }

    #[test]
    fn no_steps_anywhere_is_survivable() {
        let mut app = App::new(Vec::new());
        assert_eq!(app.focus(), Focus::Status);
        assert!(app.open_doc(0).is_none());
        assert!(app.flash.is_some());
        assert!(app.change_step(1).is_none());
        assert!(app.close_current().is_none());
        app.focus_next(); // must not panic
        app.open_dialog(); // no docs to list
        assert!(app.dialog().is_none());
    }

    #[test]
    fn open_dialog_lists_existing_docs_in_canonical_order() {
        let mut app = sample_app(); // current step 02-02: all six docs exist
        app.open_dialog();
        let dialog = app.dialog().expect("dialog open");
        let names: Vec<&str> = dialog.items.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "02-02-PLAN.md",
                "02-RESEARCH.md",
                "02-VALIDATION.md",
                "02-UAT.md",
                "02-CONTEXT.md",
                "02-DISCUSSION-LOG.md",
            ]
        );
        assert_eq!(dialog.selected, 0);
    }

    #[test]
    fn unmatched_verification_doc_is_openable_after_the_plan() {
        // The reported bug: 01-VERIFICATION.md was unopenable. It must now show
        // in the picker after the plan and open on selection.
        let phases = sample_phases();
        let mut app = App::from_phases(sample_planning(), &phases[..1]);
        app.change_step(1); // off the Roadmap row onto 01-01

        app.open_dialog();
        let names: Vec<String> = app
            .dialog()
            .expect("dialog open")
            .items
            .iter()
            .map(|(_, n)| n.clone())
            .collect();
        assert_eq!(names, ["01-01-PLAN.md", "01-VERIFICATION.md"]);

        let verification = app.doc_id(app.current, "verification");
        let req = app.open_doc(verification).expect("verification opens");
        assert!(req.path.ends_with("01-VERIFICATION.md"));
        assert_eq!(app.tab_labels(), ["verification"]);
    }

    #[test]
    fn open_dialog_omits_missing_docs() {
        // Phase 1 has only its plan and a VERIFICATION doc on disk — the
        // canonical kinds that don't exist (research, validation, uat, …) must
        // not appear, but any file that does exist is listed.
        let phases = sample_phases();
        let mut app = App::from_phases(sample_planning(), &phases[..1]);
        app.change_step(1); // off the Roadmap row onto 01-01
        app.open_dialog();
        let dialog = app.dialog().expect("dialog open");
        let names: Vec<&str> = dialog.items.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["01-01-PLAN.md", "01-VERIFICATION.md"]);
        for missing in ["RESEARCH", "VALIDATION", "UAT", "CONTEXT", "DISCUSSION"] {
            assert!(
                !names.iter().any(|n| n.contains(missing)),
                "missing canonical doc {missing} must be omitted"
            );
        }
    }

    #[test]
    fn dialog_moves_clamp_and_select_opens_the_doc() {
        let mut app = sample_app();
        app.open_dialog();
        app.dialog_move(-1); // clamps at top
        assert_eq!(app.dialog().unwrap().selected, 0);
        app.dialog_move(1);
        app.dialog_move(1); // -> validation (item index 2)
        let validation = app.doc_id(app.current, "validation");
        let req = app.dialog_select().expect("open request");
        assert_eq!(req.doc, validation);
        assert!(req.path.ends_with("02-VALIDATION.md"));
        assert!(app.dialog().is_none(), "dialog closes on select");
        assert_eq!(app.focus(), Focus::Doc(validation));
        app.open_dialog();
        for _ in 0..20 {
            app.dialog_move(1); // clamps at bottom
        }
        assert_eq!(app.dialog().unwrap().selected, 5);
    }

    #[test]
    fn dialog_select_of_open_doc_focuses_without_new_request() {
        let mut app = sample_app();
        let research = app.doc_id(app.current, "research");
        app.open_doc(research);
        app.open_dialog();
        app.dialog_move(1); // research
        assert!(app.dialog_select().is_none(), "already open -> focus only");
        assert_eq!(app.focus(), Focus::Doc(research));
    }

    #[test]
    fn dialog_close_cancels_without_side_effects() {
        let mut app = sample_app();
        app.open_dialog();
        app.dialog_move(1);
        app.close_dialog();
        assert!(app.dialog().is_none());
        assert!(app.tabs().is_empty());
        assert_eq!(app.focus(), Focus::Status);
    }

    #[test]
    fn roadmap_entry_fronts_the_list_and_opens_the_project_roadmap() {
        let mut app = sample_app();
        assert_eq!(app.roadmap_index(), Some(0));
        assert!(app.entries[0].is_roadmap());

        // Select the Roadmap row and open its document 0 -> ROADMAP.md.
        app.current = 0;
        assert_eq!(app.selection(), Some(Selected::Roadmap));
        let req = app.open_doc(0).expect("open roadmap");
        assert_eq!(req.doc, 0);
        assert!(req.path.ends_with("ROADMAP.md"), "{}", req.path.display());
        assert_eq!(app.focus(), Focus::Doc(0));
    }

    #[test]
    fn r_peek_opens_roadmap_and_restore_returns_to_prior_location() {
        let mut app = sample_app();
        // Viewing the 02-02 plan (the default selection).
        let start = app.current;
        app.open_doc(0);
        assert_eq!(app.focus(), Focus::Doc(0));

        // Stash the location, then peek the roadmap.
        let ret = (app.current, app.focus());
        let req = app.open_roadmap_peek().expect("peek opens roadmap");
        assert!(req.path.ends_with("ROADMAP.md"));
        assert!(app.current_entry().unwrap().is_roadmap());
        assert_eq!(app.focus(), Focus::Doc(0));

        // Restoring returns to the prior step and its focused doc.
        app.restore_location(ret.0, ret.1);
        assert_eq!(app.current, start);
        assert_eq!(app.focus(), Focus::Doc(0));
    }

    #[test]
    fn select_first_and_last_jump_to_the_ends() {
        let mut app = sample_app(); // entries: roadmap, 01-01, 02-01, 02-02, 02-03, ph3
        app.select_last();
        assert_eq!(app.current, app.entries.len() - 1);
        assert_eq!(app.current_entry().unwrap().phase_id, "3");
        assert_eq!(app.focus(), Focus::Status);
        app.select_first();
        assert_eq!(app.current, 0);
        assert!(app.current_entry().unwrap().is_roadmap());
    }

    #[test]
    fn select_section_walks_roadmap_phases_todos() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        app.select_first(); // Roadmap
        assert!(app.current_entry().unwrap().is_roadmap());

        app.select_section(1); // -> Phases (first step)
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        app.select_section(1); // -> Todos (first todo)
        assert!(app.current_entry().unwrap().is_todo());
        app.select_section(1); // last section: stay + flash
        assert!(app.current_entry().unwrap().is_todo());
        assert!(app.flash.as_deref().unwrap().contains("last section"));

        // From mid-Phases, up snaps to the top of Phases, then to Roadmap.
        app.current = 3; // 02-02, mid Phases
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        app.select_section(-1);
        assert!(app.current_entry().unwrap().is_roadmap());
        app.select_section(-1);
        assert!(app.flash.as_deref().unwrap().contains("first section"));
    }

    #[test]
    fn select_phase_jumps_phase_to_phase() {
        let mut app = sample_app(); // default 02-02 (phase 2)
        app.select_phase(1); // -> phase 3 (its placeholder)
        assert_eq!(app.current_entry().unwrap().phase_id, "3");
        app.select_phase(-1); // -> phase 2 first step (02-01)
        assert_eq!(app.current_entry().unwrap().step.id, "02-01");
        assert_eq!(app.current_entry().unwrap().phase_id, "2");
        app.select_phase(-1); // -> phase 1 (01-01)
        assert_eq!(app.current_entry().unwrap().phase_id, "1");
        // Past the first phase, K flows one row up onto the Roadmap (no todos
        // here means J past the last phase would clamp like j at the end).
        app.select_phase(-1);
        assert!(app.current_entry().unwrap().is_roadmap());
    }

    #[test]
    fn select_phase_falls_back_to_single_step_off_the_phases_section() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        // entries: roadmap(0), 01-01(1)…02-03(4), ph3(5), todo0(6), todo1(7), todo2(8)

        // In Todos, K moves one row up (todo → todo), not into the Phases section.
        app.current = 7; // todo1
        app.select_phase(-1);
        assert_eq!(app.current, 6);
        assert!(app.current_entry().unwrap().is_todo());

        // J on a todo moves one row down.
        app.select_phase(1);
        assert_eq!(app.current, 7);
        assert!(app.current_entry().unwrap().is_todo());

        // On the Roadmap row, K clamps like k at the top (no phase jump).
        app.select_first();
        app.select_phase(-1);
        assert!(app.current_entry().unwrap().is_roadmap());
        assert!(app.flash.as_deref().unwrap().contains("first step"));

        // From the last phase, J flows down into the Todos section…
        app.current = 5; // phase 3 placeholder (the last phase)
        app.select_phase(1);
        assert_eq!(app.current, 6);
        assert!(app.current_entry().unwrap().is_todo());

        // …and from the first phase, K flows up onto the Roadmap row.
        app.current = 1; // 01-01 (first phase)
        app.select_phase(-1);
        assert_eq!(app.current, 0);
        assert!(app.current_entry().unwrap().is_roadmap());
    }

    #[test]
    fn no_roadmap_entry_when_there_are_no_phases() {
        let app = App::from_phases_and_todos(sample_planning(), &[], &[], &[]);
        assert_eq!(app.roadmap_index(), None);
        assert!(app.entries.first().is_none_or(|e| !e.is_roadmap()));
    }
}
