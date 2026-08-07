//! Tab/step state machine. Pure state: no terminal, no leaf. The shell
//! owns the actual DocViews and creates one per OpenRequest returned here.
//!
//! Steps are a flat, roadmap-ordered list spanning ALL phases that have a
//! phase directory, so Ctrl-j/Ctrl-k walk seamlessly across phase
//! boundaries (e.g. Ctrl-k from the current phase's first step lands on the
//! previous phase's last step). Each step carries its phase context.

use crate::model::{Document, Other, OtherKind, Phase, QuickTask, Step, Todo};
use crate::planning::{
    discover_docs_sections, discover_documents, discover_root_documents, discover_steps,
    discover_task_documents, load_others, single_document, PhaseDocs,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A group of `.planning` markdown files surfaced as one navigable row between
/// the Roadmap and Phases sections. Each backs a `StepEntry` whose documents are
/// the group's files (first opens on Enter, all listed by the `o` picker).
///
/// Not a closed set: which folders exist is decided at runtime by
/// [`discover_docs_sections`], so a new `.planning` subfolder becomes a row with
/// no code change here. `id` is the folder name (or `"project"` for the root),
/// which is also the row's `step.id` — its identity across a live reload.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DocsFolder {
    id: String,
    title: String,
}

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
    /// the phase steps and before the todos. Its `documents` are the markdown
    /// files in the task's `.planning/quick/{id}-{slug}/` directory — the
    /// `-PLAN.md` at index 0, then any SUMMARY/CONTEXT docs.
    pub(crate) quick_task_title: Option<String>,
    /// True for the single synthetic entry that fronts the list when a
    /// project-level `ROADMAP.md` exists. Its document 0 is that file, so
    /// `open_doc(0)` opens the roadmap — mirroring how a todo reuses index 0.
    roadmap: bool,
    /// `Some(kind)` when this entry is the Intel or Research docs-folder row
    /// (between Roadmap and Phases). Its documents are the folder's files.
    docs_folder: Option<DocsFolder>,
    /// `Some(other)` when this entry is a note/idea/seed row in the Others
    /// section (below Todos). Its document 0 is the capture's markdown file.
    other: Option<Other>,
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

    /// The docs-folder this row surfaces, if it is one. Private to this module:
    /// callers outside need only the title, via [`StepEntry::docs_title`].
    fn docs_folder(&self) -> Option<&DocsFolder> {
        self.docs_folder.as_ref()
    }

    /// This row's display title when it is a docs-folder row (`"Intel"`,
    /// `"Reviews"`, …), else `None`. The one thing the shell needs to know about
    /// a docs row — it drives the tab title, the footer, and the body highlight,
    /// so none of them enumerate folders.
    pub(crate) fn docs_title(&self) -> Option<&str> {
        self.docs_folder.as_ref().map(|f| f.title.as_str())
    }

    pub(crate) fn is_other(&self) -> bool {
        self.other.is_some()
    }

    pub(crate) fn other_kind(&self) -> Option<OtherKind> {
        self.other.as_ref().map(|o| o.kind)
    }
}

/// What the status body should highlight for the current selection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Selected {
    /// The project-level Roadmap row (above the Phases list).
    Roadmap,
    /// A docs-folder row between Roadmap and Phases, carrying its title
    /// (`"Project"`, `"Intel"`, `"Reviews"`, …). One variant for all of them: the
    /// body highlight finds the row by its title, so no folder needs its own arm.
    Docs(String),
    /// The row for this phase id (a step belongs to it).
    Phase(String),
    /// The Nth active quick-task row (0-based, in render order).
    Task(usize),
    /// The Nth pending todo row (0-based, in render order).
    Todo(usize),
    /// The Nth Others row (note/idea/seed; 0-based, in render order).
    Other(usize),
}

/// A contiguous run of rows that `d`/`u` treat as one unit. `Docs` carries the
/// folder id because the docs rows are discovered at runtime — there is no fixed
/// number of them, so a section cannot be a plain ordinal. Only equality between
/// adjacent rows matters, so `PartialEq` is enough.
#[derive(Debug, Clone, PartialEq)]
enum Section {
    Docs(String),
    Phases,
    Tasks,
    Todos,
    Others,
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
            .position(|e| {
                !e.is_todo()
                    && !e.is_roadmap()
                    && !e.is_task()
                    && !e.is_other()
                    && e.docs_folder().is_none()
                    && !e.step.checked
            })
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
    /// `ROADMAP.md` for the leading Roadmap entry, which every phase here is
    /// taken to be under — production lists a subset, so it uses
    /// `with_roadmap_row`.
    #[cfg(test)]
    pub(crate) fn from_phases_and_todos(
        planning: &Path,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> Self {
        Self::with_roadmap_row(planning, !phases.is_empty(), phases, quick_tasks, todos)
    }

    /// `from_phases_and_todos` for callers that list only some of the phases:
    /// `has_roadmap` states outright whether the panel drew its Roadmap row,
    /// which stops tracking the phase list once completed phases are hidden —
    /// a fully verified roadmap keeps the tally with no phase rows beneath it.
    pub(crate) fn with_roadmap_row(
        planning: &Path,
        has_roadmap: bool,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> Self {
        Self::new(Self::build_entries(
            planning,
            has_roadmap,
            phases,
            quick_tasks,
            todos,
        ))
    }

    /// The flattened roadmap-then-steps-then-tasks-then-todos entry list.
    /// Shared by construction and by `refresh` (the periodic reload), so both
    /// see the same ordering. A leading Roadmap entry fronts the list whenever
    /// the report drew one, mirroring it.
    fn build_entries(
        planning: &Path,
        has_roadmap: bool,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> Vec<StepEntry> {
        let mut entries = Vec::new();
        if has_roadmap {
            let roadmap_path = planning.join("ROADMAP.md");
            // The Roadmap row is the UI's window onto every `.planning` root
            // doc: ROADMAP.md stays at index 0 (so open_doc(0)/Enter/R open it),
            // and the other root docs follow so the picker can reach them.
            let mut documents = discover_root_documents(planning);
            if documents.is_empty() {
                documents = single_document(roadmap_path.clone(), "roadmap");
            }
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: String::new(),
                    plan_path: roadmap_path,
                    checked: false,
                },
                documents,
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: None,
                quick_task_title: None,
                roadmap: true,
                docs_folder: None,
                other: None,
            });
        }
        // One docs-folder row per group of `.planning` markdown: Project (only
        // without a Roadmap row, which already reaches the root docs), then
        // Intel, Research, and every other subfolder discovered at runtime. They
        // sit between the Roadmap row and the phases so j/k, section jumps, and
        // the open flow reach them like any other row. The same discovery backs
        // the report's rows, so a row here always has a line to highlight.
        //
        // `step.id` is the folder id and `phase_id` stays empty, which is the
        // identity `refresh_with_roadmap_row` keys on — so a docs row keeps its
        // selection and open tabs across a reload that adds or removes an
        // unrelated folder.
        for section in discover_docs_sections(planning, !has_roadmap) {
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: section.id.clone(),
                    plan_path: section.documents[0].path.clone(),
                    checked: false,
                },
                documents: section.documents,
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: None,
                quick_task_title: None,
                roadmap: false,
                docs_folder: Some(DocsFolder {
                    id: section.id,
                    title: section.title,
                }),
                other: None,
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
                    docs_folder: None,
                    other: None,
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
                    docs_folder: None,
                    other: None,
                });
            }
        }
        for task in quick_tasks {
            // A task's openable docs are the markdown files in its directory:
            // its `-PLAN.md` (document 0, opened by Enter) plus any SUMMARY /
            // CONTEXT / etc. reachable from the `o` picker.
            let documents = discover_task_documents(&task.dir, &task.id);
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: task.id.clone(),
                    plan_path: task.dir.join(format!("{}-PLAN.md", task.id)),
                    checked: false,
                },
                documents,
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: None,
                quick_task_title: Some(task.title.clone()),
                roadmap: false,
                docs_folder: None,
                other: None,
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
                documents: single_document(todo.path.clone(), "plan"),
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: Some(todo.title.clone()),
                quick_task_title: None,
                roadmap: false,
                docs_folder: None,
                other: None,
            });
        }
        // Others: notes/ideas/seeds, below the todos. Each is a single file, so
        // document 0 is that file (Enter/o open it). The step id is prefixed by
        // kind so identically-named files in different folders stay unique.
        for other in load_others(planning) {
            entries.push(StepEntry {
                phase_id: String::new(),
                step: Step {
                    id: format!("{}-{}", other.kind.label(), other.slug),
                    plan_path: other.path.clone(),
                    checked: false,
                },
                documents: single_document(other.path.clone(), other.kind.label()),
                pos_in_phase: 0,
                phase_steps: 1,
                todo_title: None,
                quick_task_title: None,
                roadmap: false,
                docs_folder: None,
                other: Some(other),
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
    #[cfg(test)]
    pub(crate) fn refresh(
        &mut self,
        planning: &Path,
        phases: &[Phase],
        quick_tasks: &[QuickTask],
        todos: &[Todo],
    ) -> HashMap<usize, usize> {
        self.refresh_with_roadmap_row(planning, !phases.is_empty(), phases, quick_tasks, todos)
    }

    /// `refresh` for callers listing only some of the phases — see
    /// `with_roadmap_row` for why the Roadmap row needs saying out loud.
    pub(crate) fn refresh_with_roadmap_row(
        &mut self,
        planning: &Path,
        has_roadmap: bool,
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

        let new_entries = Self::build_entries(planning, has_roadmap, phases, quick_tasks, todos);
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
        } else if let Some(title) = entry.docs_title() {
            Some(Selected::Docs(title.to_string()))
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
        } else if entry.is_other() {
            let ordinal = self.entries[..self.current]
                .iter()
                .filter(|e| e.is_other())
                .count();
            Some(Selected::Other(ordinal))
        } else {
            Some(Selected::Phase(entry.phase_id.clone()))
        }
    }

    pub(crate) fn current_entry(&self) -> Option<&StepEntry> {
        self.entries.get(self.current)
    }

    /// The selected todo's, quick task's, or Others row's title, or `None` when
    /// the selection is a phase step or the roadmap row. Backs the `c` "copy
    /// name" key for todos, tasks, and notes/ideas/seeds.
    pub(crate) fn current_copyable_title(&self) -> Option<&str> {
        let entry = self.entries.get(self.current)?;
        entry
            .todo_title
            .as_deref()
            .or(entry.quick_task_title.as_deref())
            .or(entry.other.as_ref().map(|o| o.title.as_str()))
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

    /// Find the `(step, doc)` index pair whose document path equals `path`,
    /// scanning every entry's `documents` in row order. Used by the find-a-
    /// requirement feature to turn a resolved file path back into a
    /// selectable row/tab pair.
    pub(crate) fn locate_document(&self, path: &Path) -> Option<(usize, usize)> {
        for (step, entry) in self.entries.iter().enumerate() {
            if let Some(doc) = entry.documents.iter().position(|d| d.path == path) {
                return Some((step, doc));
            }
        }
        None
    }

    /// Select `step` then open (or focus) its document at `doc` — the
    /// combination `locate_document` feeds into to jump straight to a
    /// resolved requirement definition. Delegates to `open_doc` so tab
    /// ordering and the missing-file flash stay in one place.
    pub(crate) fn select_document(&mut self, step: usize, doc: usize) -> Option<OpenRequest> {
        self.current = step;
        self.open_doc(doc)
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

    /// Section ordinal for grouping entries: Roadmap or Project(0), Intel(1),
    /// Research(2), Phases(3), Tasks(4), Todos(5), Others(6). Entries are built
    /// in this order, so each section is contiguous — Roadmap and Project share
    /// an ordinal because only one of them is ever drawn.
    /// Which section a row belongs to, for grouping entries so `d`/`u` step
    /// section by section. Entries are built in section order, so each section is
    /// contiguous and `section_bounds` need only compare adjacent rows.
    ///
    /// Every docs folder is its own section — there is no fixed count of them, so
    /// this cannot be an ordinal. The Roadmap row and the Project row both map to
    /// `Docs("project")`: only one of them is ever drawn.
    fn section_key(e: &StepEntry) -> Section {
        if e.is_roadmap() {
            Section::Docs("project".to_string())
        } else if let Some(folder) = e.docs_folder() {
            Section::Docs(folder.id.clone())
        } else if e.is_task() {
            Section::Tasks
        } else if e.is_todo() {
            Section::Todos
        } else if e.is_other() {
            Section::Others
        } else {
            Section::Phases
        }
    }

    /// Start index of each contiguous section present, in entry order.
    fn section_bounds(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut last: Option<Section> = None;
        for (i, e) in self.entries.iter().enumerate() {
            let k = Self::section_key(e);
            if last.as_ref() != Some(&k) {
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
            if e.is_roadmap()
                || e.is_todo()
                || e.is_task()
                || e.is_other()
                || e.docs_folder().is_some()
            {
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
        let on_phase = self.entries.get(self.current).is_some_and(|e| {
            !e.is_roadmap()
                && !e.is_todo()
                && !e.is_task()
                && !e.is_other()
                && e.docs_folder().is_none()
        });
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

    /// How many `change_step(1)` moves it takes to walk from the default landing
    /// row (02-02) to the row just past the Phases section — the first task, or
    /// the first todo when no tasks are loaded. The sample carries a phase in
    /// every stage, so the tail of that section is long: 02-03, the phase-3
    /// placeholder, 04-01/02, 05-01/02, then the phase-6/7/8 placeholders.
    const STEPS_BELOW_02_02: usize = 10;

    fn sample_phases() -> Vec<Phase> {
        crate::planning::load_phases(sample_planning())
    }

    fn sample_app() -> App {
        let app = App::from_phases(sample_planning(), &sample_phases());
        let ids: Vec<&str> = app.entries.iter().map(|e| e.step.id.as_str()).collect();
        // Leading "" is the synthetic Roadmap entry; "intel"/"research"/"reviews"
        // are the docs-folder rows (intel and research pinned first, then every
        // other `.planning` subfolder holding markdown, name-sorted — `debug/` is
        // owned by the Todos section, so it gets no row); each bare "1" is a
        // placeholder for a phase with no plans yet (phases 3, 6, 7, and 8 — the
        // sample carries one phase per stage); the trailing note-/idea-/seed- rows
        // are the Others section.
        assert_eq!(
            ids,
            [
                "",
                "intel",
                "research",
                "reviews",
                "01-01",
                "02-01",
                "02-02",
                "02-03",
                "1",
                "04-01",
                "04-02",
                "05-01",
                "05-02",
                "1",
                "1",
                "1",
                "note-2026-07-08-grinder-timing",
                "note-2026-07-09-milk-frother",
                "idea-latte-art-mode",
                "seed-SEED-001-mobile-ordering",
            ]
        );
        app
    }

    #[test]
    fn an_unowned_docs_folder_becomes_a_row_whose_files_open_from_the_picker() {
        let mut app = sample_app();
        let idx = app
            .entries
            .iter()
            .position(|e| e.docs_title() == Some("Reviews"))
            .expect("a Reviews docs-folder row");
        assert_eq!(app.entries[idx].step.id, "reviews");

        app.set_browsing(idx);
        app.open_dialog();
        let names: Vec<&str> = app
            .dialog()
            .expect("picker open on the Reviews row")
            .items
            .iter()
            .map(|(_, n)| n.as_str())
            .collect();
        assert!(
            names.contains(&"STK-EXAMPLE-pass-rate-audit.md"),
            "picker items: {names:?}"
        );
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

        // 01-01 is the first phase step; k moves up through the docs-folder rows
        // — Reviews, Research, Intel — before reaching the Roadmap row.
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.selection(), Some(Selected::Docs("Reviews".into())));
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.selection(), Some(Selected::Docs("Research".into())));
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.selection(), Some(Selected::Docs("Intel".into())));
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
        app.select_last(); // the final Others row
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
        // row; select the 01-01 step by identity (Intel/Research rows precede it
        // and the Others rows trail it).
        app.current = app
            .entries
            .iter()
            .position(|e| e.step.id == "01-01")
            .unwrap();
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
        // 1 roadmap + 3 docs-folder rows + 12 steps + 4 tasks + 4 todos (incl.
        // the active debug session) + 4 others.
        assert_eq!(app.entries.len(), 28);
        let last_phase_idx = 15; // the phase-8 placeholder
        let first_todo_idx = 20;
        for e in &app.entries[(last_phase_idx + 1)..first_todo_idx] {
            assert!(e.is_task(), "expected a task row: {e:?}");
            assert!(!e.is_todo());
        }
        assert!(!app.entries[last_phase_idx].is_task());
        assert!(app.entries[first_todo_idx].is_todo());
        assert!(!app.entries[first_todo_idx].is_task());
    }

    #[test]
    fn task_row_opens_its_plan_and_picker_lists_task_docs() {
        let mut app = App::from_phases_and_todos(
            sample_planning(),
            &sample_phases(),
            &sample_quick_tasks(),
            &[],
        );
        app.current = app
            .entries
            .iter()
            .position(|e| e.is_task())
            .expect("a task row");

        // Enter (open_doc(0)) opens the task's PLAN.md — the reported bug was
        // that tasks had no documents so this flashed instead.
        let req = app.open_doc(0).expect("task plan opens");
        assert!(
            req.path.to_string_lossy().ends_with("-PLAN.md"),
            "{}",
            req.path.display()
        );

        // The `o` picker lists the task's docs (at least its plan).
        app.open_dialog();
        let names: Vec<String> = app
            .dialog()
            .expect("dialog open on a task row")
            .items
            .iter()
            .map(|(_, n)| n.clone())
            .collect();
        assert!(
            names.iter().any(|n| n.ends_with("-PLAN.md")),
            "picker lists the task plan: {names:?}"
        );
    }

    #[test]
    fn refresh_appends_a_new_todo_so_nav_can_reach_it() {
        // Start with no todos; the list already ends with the Others rows.
        let mut app = App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &[]);
        let before = app.entries.len();

        // A timed reload picks up a freshly captured todo.
        app.refresh(
            sample_planning(),
            &sample_phases(),
            &[],
            &[todo("2026-07-09-new-todo", "Fresh todo")],
        );

        // The list grew and the new todo is reachable by browsing down from the
        // top — the down-movement bound derives from the grown list.
        assert_eq!(app.entries.len(), before + 1);
        let idx = app
            .entries
            .iter()
            .position(|e| e.todo_title.as_deref() == Some("Fresh todo"))
            .expect("new todo present after reload");
        app.select_first();
        while app.current < idx {
            assert!(app.change_step(1).is_none());
        }
        assert_eq!(app.current, idx, "cursor reaches the new todo row");
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
        // Select the (soon-removed) todo, then reload a workspace without it.
        let mut app = App::from_phases_and_todos(
            sample_planning(),
            &sample_phases(),
            &[],
            &[todo("t", "Gone soon")],
        );
        app.current = app.entries.iter().position(|e| e.is_todo()).unwrap();
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
        // 1 roadmap + 3 docs-folder rows + 12 steps + 4 todos (incl. the active
        // debug session) + 4 others.
        assert_eq!(app.entries.len(), 24);
        assert!(app.entries[0].is_roadmap());
        assert!(app.entries[16].is_todo());
        assert!(app.entries[19].is_todo());
        // The Others rows trail the todos.
        assert!(app.entries[20].is_other());
        assert!(app.entries[22].is_other());
    }

    #[test]
    fn stepping_reaches_todos_and_enter_opens_the_todo_md() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        // Walk off the end of the Phases section: 02-02 -> 02-03 -> the phase-3
        // placeholder -> 04-01/02 -> 05-01/02 -> the phase-6/7/8 placeholders
        // -> the first todo.
        for _ in 0..STEPS_BELOW_02_02 {
            app.change_step(1);
        }
        assert!(app.current_entry().unwrap().is_todo());
        let req = app.open_doc(0).expect("open todo md");
        assert!(
            req.path.ends_with("2026-07-07-signed-build.md"),
            "{}",
            req.path.display()
        );
    }

    #[test]
    fn current_copyable_title_is_some_only_on_a_todo_or_task() {
        let mut app = App::from_phases_and_todos(
            sample_planning(),
            &sample_phases(),
            &sample_quick_tasks(),
            &sample_todos(),
        );
        // Starts on a real step.
        assert!(app.current_copyable_title().is_none());
        // Walk off the Phases section onto the first task row.
        for _ in 0..STEPS_BELOW_02_02 {
            app.change_step(1);
        }
        assert_eq!(app.current_copyable_title(), Some("Add dark-mode toggle"));
        // Walk past the 4 tasks onto the first todo.
        for _ in 0..4 {
            app.change_step(1);
        }
        assert_eq!(
            app.current_copyable_title(),
            Some("Official signed build process for pr-monitor apps")
        );
    }

    #[test]
    fn selection_reports_phase_for_steps_and_ordinal_for_todos() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        assert_eq!(app.selection(), Some(Selected::Phase("2".into())));
        for _ in 0..STEPS_BELOW_02_02 {
            app.change_step(1); // walk off the Phases section onto the first todo
        }
        assert_eq!(app.selection(), Some(Selected::Todo(0)));
        app.change_step(1); // second todo
        assert_eq!(app.selection(), Some(Selected::Todo(1)));
    }

    #[test]
    fn phase_without_steps_gets_a_placeholder_entry() {
        let app = App::from_phases(sample_planning(), &sample_phases());
        // Phase 3 has no steps; it still gets one placeholder entry (the Others
        // rows trail it, so it is no longer the final entry).
        let placeholder = app
            .entries
            .iter()
            .find(|e| e.phase_id == "3")
            .expect("placeholder entry");
        assert_eq!(placeholder.step.id, "1");
        assert!(!placeholder.step.checked, "an unstarted phase is unchecked");
        assert_eq!((placeholder.pos_in_phase, placeholder.phase_steps), (0, 1));
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
        app.current = app
            .entries
            .iter()
            .position(|e| e.step.id == "01-01")
            .unwrap();

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
        app.current = app
            .entries
            .iter()
            .position(|e| e.step.id == "01-01")
            .unwrap();
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
    fn roadmap_row_picker_lists_all_planning_root_docs() {
        // The Ctrl-o / o picker on the Roadmap row surfaces every `.planning`
        // root markdown file, not just ROADMAP.md — with ROADMAP.md pinned
        // first (so open_doc(0)/Enter/R still open the roadmap).
        let mut app = sample_app();
        app.current = 0; // Roadmap row
        app.open_dialog();
        let dialog = app.dialog().expect("dialog open on the roadmap row");
        let names: Vec<&str> = dialog.items.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            names,
            ["ROADMAP.md", "PROJECT.md", "REQUIREMENTS.md", "STATE.md"]
        );
        assert_eq!(dialog.selected, 0);
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
        let mut app = sample_app();
        app.select_last();
        assert_eq!(app.current, app.entries.len() - 1);
        // The last entry is the final Others row (a seed).
        assert_eq!(
            app.current_entry().unwrap().other_kind(),
            Some(OtherKind::Seed)
        );
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

        app.select_section(1); // -> Intel
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Intel"));
        app.select_section(1); // -> Research
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Research"));
        app.select_section(1); // -> Reviews (its own section)
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Reviews"));
        app.select_section(1); // -> Phases (first step)
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        app.select_section(1); // -> Todos (first todo)
        assert!(app.current_entry().unwrap().is_todo());
        app.select_section(1); // -> Others (first note/idea/seed)
        assert!(app.current_entry().unwrap().is_other());
        app.select_section(1); // last section: stay + flash
        assert!(app.current_entry().unwrap().is_other());
        assert!(app.flash.as_deref().unwrap().contains("last section"));

        // From mid-Phases, up snaps to the top of Phases, then back through the
        // Reviews, Research, and Intel rows to the Roadmap.
        app.current = 6; // 02-02, mid Phases
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Reviews"));
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Research"));
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Intel"));
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
        // Past the first phase, K flows one row up onto the last docs-folder row
        // (the row directly above the Phases section).
        app.select_phase(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Reviews"));
    }

    #[test]
    fn select_phase_falls_back_to_single_step_off_the_phases_section() {
        let mut app =
            App::from_phases_and_todos(sample_planning(), &sample_phases(), &[], &sample_todos());
        // entries: roadmap(0), intel(1), research(2), reviews(3), 01-01(4)…
        // 02-03(7), ph3(8), 04-01(9), 04-02(10), 05-01(11), 05-02(12), ph6(13),
        // ph7(14), ph8(15), todo0(16), todo1(17), todo2(18), todo3(19, the
        // active debug session)

        // In Todos, K moves one row up (todo → todo), not into the Phases section.
        app.current = 17; // todo1
        app.select_phase(-1);
        assert_eq!(app.current, 16);
        assert!(app.current_entry().unwrap().is_todo());

        // J on a todo moves one row down.
        app.select_phase(1);
        assert_eq!(app.current, 17);
        assert!(app.current_entry().unwrap().is_todo());

        // On the Roadmap row, K clamps like k at the top (no phase jump).
        app.select_first();
        app.select_phase(-1);
        assert!(app.current_entry().unwrap().is_roadmap());
        assert!(app.flash.as_deref().unwrap().contains("first step"));

        // From the last phase, J flows down into the Todos section…
        app.current = 15; // phase 8 placeholder (the last phase)
        app.select_phase(1);
        assert_eq!(app.current, 16);
        assert!(app.current_entry().unwrap().is_todo());

        // …and from the first phase, K flows up onto the last docs-folder row
        // (the row directly above the Phases section).
        app.current = 4; // 01-01 (first phase)
        app.select_phase(-1);
        assert_eq!(app.current, 3);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Reviews"));
    }

    fn planning_with_docs_folders(intel: bool, research: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(
            p.join("ROADMAP.md"),
            "## Phases\n\n- [ ] **Phase 1: Skeleton** - x.\n",
        )
        .unwrap();
        if intel {
            std::fs::create_dir_all(p.join("intel")).unwrap();
            std::fs::write(p.join("intel/ARCHITECTURE.md"), "# a\n").unwrap();
            std::fs::write(p.join("intel/STACK.md"), "# s\n").unwrap();
        }
        if research {
            std::fs::create_dir_all(p.join("research")).unwrap();
            std::fs::write(p.join("research/SUMMARY.md"), "# s\n").unwrap();
        }
        dir
    }

    #[test]
    fn intel_and_research_rows_sit_between_roadmap_and_phases() {
        let dir = planning_with_docs_folders(true, true);
        let phases = crate::planning::load_phases(dir.path());
        let app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        // roadmap(0), intel(1), research(2), phase-1 placeholder(3)
        assert!(app.entries[0].is_roadmap());
        assert_eq!(app.entries[1].docs_title(), Some("Intel"));
        assert_eq!(app.entries[2].docs_title(), Some("Research"));
        assert!(app.entries[3].docs_title().is_none());
        assert_eq!(app.entries[3].phase_id, "1");
        // Default selection skips the docs-folder rows and lands on the phase.
        assert!(app.current_entry().unwrap().docs_title().is_none());
        assert_eq!(app.current_entry().unwrap().phase_id, "1");
    }

    #[test]
    fn intel_row_opens_first_file_and_picker_lists_all() {
        let dir = planning_with_docs_folders(true, true);
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        app.current = 1; // Intel row
        assert_eq!(app.selection(), Some(Selected::Docs("Intel".into())));

        let req = app.open_doc(0).expect("open first intel file");
        assert!(
            req.path.ends_with("ARCHITECTURE.md"),
            "{}",
            req.path.display()
        );

        app.focus_slot(1); // back to status so the dialog opens on the row
        app.open_dialog();
        let names: Vec<&str> = app
            .dialog()
            .expect("dialog open on intel row")
            .items
            .iter()
            .map(|(_, n)| n.as_str())
            .collect();
        assert_eq!(names, ["ARCHITECTURE.md", "STACK.md"]);
    }

    #[test]
    fn omits_docs_folder_row_when_its_folder_is_absent() {
        let dir = planning_with_docs_folders(false, true);
        let phases = crate::planning::load_phases(dir.path());
        let app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        assert!(
            app.entries.iter().all(|e| e.docs_title() != Some("Intel")),
            "no Intel row when intel/ is absent"
        );
        assert!(app
            .entries
            .iter()
            .any(|e| e.docs_title() == Some("Research")));
    }

    #[test]
    fn select_section_walks_roadmap_intel_research_phases() {
        let dir = planning_with_docs_folders(true, true);
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        app.select_first();
        assert!(app.current_entry().unwrap().is_roadmap());
        app.select_section(1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Intel"));
        app.select_section(1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Research"));
        app.select_section(1);
        assert_eq!(app.current_entry().unwrap().phase_id, "1");
    }

    #[test]
    fn refresh_preserves_selection_on_a_docs_folder_row() {
        let dir = planning_with_docs_folders(true, true);
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        app.current = 2; // Research row
        app.refresh(dir.path(), &phases, &[], &[]);
        assert_eq!(
            app.current_entry().unwrap().docs_title(),
            Some("Research"),
            "the Research selection survives a reload by identity"
        );
    }

    /// A workspace with the pinned intel/research folders plus `reviews/` — a
    /// folder no section owns, so it is discovered at runtime rather than pinned.
    /// Two phases, so "snap to the top of the Phases section" is distinguishable
    /// from "jump to the previous section".
    ///
    /// entries: roadmap(0), intel(1), research(2), reviews(3), phase1(4), phase2(5)
    fn planning_with_a_discovered_folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(
            p.join("ROADMAP.md"),
            "## Phases\n\n- [ ] **Phase 1: Skeleton** - x.\n- [ ] **Phase 2: Body** - y.\n",
        )
        .unwrap();
        std::fs::create_dir_all(p.join("intel")).unwrap();
        std::fs::write(p.join("intel/ARCHITECTURE.md"), "# a\n").unwrap();
        std::fs::create_dir_all(p.join("research")).unwrap();
        std::fs::write(p.join("research/SUMMARY.md"), "# s\n").unwrap();
        std::fs::create_dir_all(p.join("reviews")).unwrap();
        std::fs::write(
            p.join("reviews/STK-EXAMPLE-pass-rate-audit.md"),
            "# audit\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn select_section_treats_each_discovered_folder_as_its_own_section() {
        let dir = planning_with_a_discovered_folder();
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        app.current = 1; // Intel
        app.select_section(1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Research"));
        app.select_section(1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Reviews"));
        // A discovered folder is its own section, so the next jump leaves the
        // docs rows entirely and lands in Phases.
        app.select_section(1);
        assert_eq!(app.current_entry().unwrap().phase_id, "1");
        assert!(app.current_entry().unwrap().docs_title().is_none());
    }

    #[test]
    fn select_section_walks_back_up_through_every_discovered_folder() {
        let dir = planning_with_a_discovered_folder();
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        app.current = 5; // phase 2 — mid Phases
                         // Up first snaps to the top of Phases…
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().phase_id, "1");
        // …then steps up one docs folder at a time.
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Reviews"));
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Research"));
        app.select_section(-1);
        assert_eq!(app.current_entry().unwrap().docs_title(), Some("Intel"));
        app.select_section(-1);
        assert!(app.current_entry().unwrap().is_roadmap());
    }

    #[test]
    fn refresh_preserves_selection_and_tabs_on_a_discovered_docs_folder_row() {
        let dir = planning_with_a_discovered_folder();
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        app.current = 3; // Reviews
        app.open_doc(0).expect("open the review document");
        assert!(!app.tabs().is_empty());

        // A folder appearing elsewhere must not disturb this row's identity.
        std::fs::create_dir_all(dir.path().join("adr")).unwrap();
        std::fs::write(dir.path().join("adr/0001-choice.md"), "# adr\n").unwrap();
        app.refresh(dir.path(), &phases, &[], &[]);

        assert_eq!(
            app.current_entry().unwrap().docs_title(),
            Some("Reviews"),
            "the Reviews selection survives a reload by identity"
        );
        assert!(
            !app.tabs().is_empty(),
            "and its open tab survives with it: {:?}",
            app.tabs()
        );
    }

    #[test]
    fn omits_a_discovered_docs_folder_row_when_it_holds_no_markdown() {
        let dir = planning_with_a_discovered_folder();
        // An empty folder, and one holding only non-markdown: neither is
        // openable, so neither earns a row.
        std::fs::create_dir_all(dir.path().join("archive")).unwrap();
        std::fs::create_dir_all(dir.path().join("exports")).unwrap();
        std::fs::write(dir.path().join("exports/data.csv"), "a,b\n").unwrap();

        let phases = crate::planning::load_phases(dir.path());
        let app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        assert!(
            app.entries
                .iter()
                .all(|e| e.docs_title() != Some("Archive")),
            "no row for an empty folder"
        );
        assert!(
            app.entries
                .iter()
                .all(|e| e.docs_title() != Some("Exports")),
            "no row for a folder with no markdown"
        );
        assert!(app
            .entries
            .iter()
            .any(|e| e.docs_title() == Some("Reviews")));
    }

    /// A workspace that finished research but has no ROADMAP.md yet, so no
    /// phases parse and the Roadmap row can't front the entry list.
    fn planning_before_roadmap() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(p.join("PROJECT.md"), "# Demo\n").unwrap();
        std::fs::write(p.join("REQUIREMENTS.md"), "# Reqs\n").unwrap();
        std::fs::create_dir_all(p.join("research")).unwrap();
        std::fs::write(p.join("research/STACK.md"), "# s\n").unwrap();
        dir
    }

    #[test]
    fn project_row_reaches_root_docs_when_no_roadmap_exists_yet() {
        let dir = planning_before_roadmap();
        let app = App::from_phases_and_todos(dir.path(), &[], &[], &[]);
        assert_eq!(app.entries[0].docs_title(), Some("Project"));
        let names: Vec<String> = app.entries[0]
            .documents
            .iter()
            .map(|d| d.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["PROJECT.md", "REQUIREMENTS.md"]);
        assert_eq!(app.entries[1].docs_title(), Some("Research"));
        // Nothing else can hold the cursor, so the Project row takes it.
        assert_eq!(app.selection(), Some(Selected::Docs("Project".into())));
    }

    #[test]
    fn omits_project_row_when_the_roadmap_row_carries_root_docs() {
        let dir = planning_with_docs_folders(true, true);
        let phases = crate::planning::load_phases(dir.path());
        let app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        assert!(app.entries[0].is_roadmap());
        assert!(
            app.entries
                .iter()
                .all(|e| e.docs_title() != Some("Project")),
            "the Roadmap row already reaches every root doc"
        );
    }

    fn planning_with_others() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(
            p.join("ROADMAP.md"),
            "## Phases\n\n- [ ] **Phase 1: Skeleton** - x.\n",
        )
        .unwrap();
        std::fs::create_dir_all(p.join("notes")).unwrap();
        std::fs::write(p.join("notes/2026-07-10-grinder.md"), "# Grinder\n").unwrap();
        std::fs::create_dir_all(p.join("ideas")).unwrap();
        std::fs::write(p.join("ideas/latte.md"), "# Latte art\n").unwrap();
        std::fs::create_dir_all(p.join("seeds")).unwrap();
        std::fs::write(p.join("seeds/SEED-001-mobile.md"), "# Mobile orders\n").unwrap();
        dir
    }

    #[test]
    fn others_rows_are_appended_after_todos_and_default_skips_them() {
        let dir = planning_with_others();
        let phases = crate::planning::load_phases(dir.path());
        let others = crate::planning::load_others(dir.path());
        let app = App::from_phases_and_todos(
            dir.path(),
            &phases,
            &[],
            &[todo("2026-07-07-a-todo", "A todo")],
        );
        // roadmap(0), 01-01(1), todo(2), note(3), idea(4), seed(5)
        assert_eq!(others.len(), 3);
        let tail: Vec<Option<crate::model::OtherKind>> = app
            .entries
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|e| e.other_kind())
            .collect();
        assert_eq!(
            tail,
            [
                Some(crate::model::OtherKind::Note),
                Some(crate::model::OtherKind::Idea),
                Some(crate::model::OtherKind::Seed),
            ]
        );
        // The todo sits just before the first Other row.
        let first_other = app.entries.iter().position(|e| e.is_other()).unwrap();
        assert!(app.entries[first_other - 1].is_todo());
        // Default never lands on an Other row.
        assert!(!app.current_entry().unwrap().is_other());
    }

    #[test]
    fn other_row_opens_its_file_and_reports_ordinal() {
        let dir = planning_with_others();
        let phases = crate::planning::load_phases(dir.path());
        let mut app = App::from_phases_and_todos(dir.path(), &phases, &[], &[]);
        let idea_idx = app
            .entries
            .iter()
            .position(|e| e.other_kind() == Some(crate::model::OtherKind::Idea))
            .unwrap();
        app.current = idea_idx;
        assert_eq!(app.selection(), Some(Selected::Other(1))); // note(0), idea(1)
        assert_eq!(app.current_copyable_title(), Some("Latte art"));
        let req = app.open_doc(0).expect("open the idea file");
        assert!(req.path.ends_with("latte.md"), "{}", req.path.display());
    }

    #[test]
    fn no_roadmap_entry_when_there_are_no_phases() {
        let app = App::from_phases_and_todos(sample_planning(), &[], &[], &[]);
        assert_eq!(app.roadmap_index(), None);
        assert!(app.entries.first().is_none_or(|e| !e.is_roadmap()));
    }

    #[test]
    fn locate_document_finds_the_step_and_doc_index_for_a_path() {
        let app = sample_app();
        // Roadmap row (entries[0]) lists ROADMAP, PROJECT, REQUIREMENTS, STATE
        // in that order — REQUIREMENTS.md is document index 2.
        let path = sample_planning().join("REQUIREMENTS.md");

        let found = app.locate_document(&path);

        assert_eq!(found, Some((0, 2)));
    }

    #[test]
    fn locate_document_of_an_unknown_path_is_none() {
        let app = sample_app();
        let path = sample_planning().join("NOPE.md");

        assert_eq!(app.locate_document(&path), None);
    }

    #[test]
    fn select_document_moves_current_step_and_opens_the_tab() {
        let mut app = sample_app();
        assert_ne!(app.current, 0, "sanity: not already on the Roadmap row");

        let req = app
            .select_document(0, 2)
            .expect("REQUIREMENTS.md exists and is not already open");

        assert_eq!(app.current, 0);
        assert!(
            req.path.ends_with("REQUIREMENTS.md"),
            "{}",
            req.path.display()
        );
        assert_eq!(app.selection(), Some(Selected::Roadmap));
    }
}
