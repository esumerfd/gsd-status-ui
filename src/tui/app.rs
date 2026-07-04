//! Tab/step state machine. Pure state: no terminal, no leaf. The shell
//! owns the actual DocViews and creates one per OpenRequest returned here.
//!
//! Steps are a flat, roadmap-ordered list spanning ALL phases that have a
//! phase directory, so Ctrl-j/Ctrl-k walk seamlessly across phase
//! boundaries (e.g. Ctrl-k from the current phase's first step lands on the
//! previous phase's last step). Each step carries its phase context.

use crate::model::{DocKind, Phase, Step};
use crate::planning::{discover_steps, PhaseDocs};
use std::path::PathBuf;

/// One navigable step: a plan file plus the phase it belongs to.
#[derive(Debug, Clone)]
pub(crate) struct StepEntry {
    pub(crate) phase_id: String,
    pub(crate) docs: PhaseDocs,
    pub(crate) step: Step,
    /// 0-based position within its phase, and the phase's step count —
    /// for the footer's "step 02-02 (2/3)" display.
    pub(crate) pos_in_phase: usize,
    pub(crate) phase_steps: usize,
}

/// What the shell must open (create a DocView for) after a state change.
#[derive(Debug, PartialEq)]
pub(crate) struct OpenRequest {
    pub(crate) step: usize,
    pub(crate) kind: DocKind,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Focus {
    Status,
    Doc(DocKind),
}

#[derive(Debug, Clone, Default)]
struct TabSet {
    tabs: Vec<DocKind>,
    /// 0 = Status tab, 1..=tabs.len() = document tabs.
    focused: usize,
}

pub(crate) struct App {
    entries: Vec<StepEntry>,
    pub(crate) current: usize,
    tabsets: Vec<TabSet>,
    pub(crate) flash: Option<String>,
    pub(crate) quit: bool,
}

impl App {
    pub(crate) fn new(entries: Vec<StepEntry>) -> Self {
        let current = entries
            .iter()
            .position(|e| !e.step.checked)
            .unwrap_or(0);
        let tabsets = vec![TabSet::default(); entries.len()];
        Self {
            entries,
            current,
            tabsets,
            flash: None,
            quit: false,
        }
    }

    /// Flatten all phases (roadmap order) that have a phase directory.
    pub(crate) fn from_phases(phases: &[Phase]) -> Self {
        let mut entries = Vec::new();
        for phase in phases {
            let Some(dir) = phase.dir.as_deref() else {
                continue;
            };
            let steps = discover_steps(dir, &phase.plans);
            let count = steps.len();
            for (i, step) in steps.into_iter().enumerate() {
                entries.push(StepEntry {
                    phase_id: phase.id.clone(),
                    docs: PhaseDocs::new(dir),
                    step,
                    pos_in_phase: i,
                    phase_steps: count,
                });
            }
        }
        Self::new(entries)
    }

    pub(crate) fn current_entry(&self) -> Option<&StepEntry> {
        self.entries.get(self.current)
    }

    pub(crate) fn tabs(&self) -> &[DocKind] {
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

    /// Open (or focus) a document tab for the current step. Returns an
    /// OpenRequest when a new tab was added; the shell must then create the
    /// DocView (and call `remove_tab` if that fails).
    pub(crate) fn open_doc(&mut self, kind: DocKind) -> Option<OpenRequest> {
        self.flash = None;
        let Some(entry) = self.entries.get(self.current) else {
            self.flash = Some("no active phase step".into());
            return None;
        };
        let path = entry.docs.path_for(kind, &entry.step);
        if !path.exists() {
            self.flash = Some(format!(
                "no {} document ({})",
                kind.label(),
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
            return None;
        }
        let step_idx = self.current;
        let set = &mut self.tabsets[step_idx];
        if let Some(pos) = set.tabs.iter().position(|k| *k == kind) {
            set.focused = pos + 1;
            return None;
        }
        let insert_at = set
            .tabs
            .iter()
            .position(|k| k.order_index() > kind.order_index())
            .unwrap_or(set.tabs.len());
        set.tabs.insert(insert_at, kind);
        set.focused = insert_at + 1;
        Some(OpenRequest {
            step: step_idx,
            kind,
            path,
        })
    }

    /// Move to a later (`+1`) or earlier (`-1`) step, crossing phase
    /// boundaries. If the target step has no open tabs, its plan document is
    /// auto-opened.
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
        self.current = target as usize;
        if self.tabs().is_empty() {
            return self.open_doc(DocKind::Plan);
        }
        None
    }

    /// Close the focused document tab. Returns the (step, kind) whose view
    /// the shell should drop. Closing the Status tab is a no-op.
    pub(crate) fn close_current(&mut self) -> Option<(usize, DocKind)> {
        let step_idx = self.current;
        let set = self.tabsets.get_mut(step_idx)?;
        if set.focused == 0 {
            return None;
        }
        let kind = set.tabs.remove(set.focused - 1);
        set.focused = set.focused.min(set.tabs.len());
        Some((step_idx, kind))
    }

    /// Called by the shell when creating a DocView failed after open_doc.
    pub(crate) fn remove_tab(&mut self, step: usize, kind: DocKind, reason: String) {
        if let Some(set) = self.tabsets.get_mut(step) {
            if let Some(pos) = set.tabs.iter().position(|k| *k == kind) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sample_phases() -> Vec<Phase> {
        crate::planning::load_phases(Path::new("sample/.planning"))
    }

    fn sample_app() -> App {
        let app = App::from_phases(&sample_phases());
        let ids: Vec<&str> = app.entries.iter().map(|e| e.step.id.as_str()).collect();
        assert_eq!(ids, ["01-01", "02-01", "02-02", "02-03"]);
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
    fn ctrl_k_walks_back_across_the_phase_boundary() {
        let mut app = sample_app();

        // 02-02 -> 02-01 (same phase), plan auto-opens.
        let req = app.change_step(-1).expect("auto-open 02-01 plan");
        assert!(req.path.ends_with("02-01-PLAN.md"));

        // 02-01 -> 01-01: crosses into phase 1's last (only) step.
        let req = app.change_step(-1).expect("auto-open 01-01 plan");
        assert!(req.path.ends_with("01-01-PLAN.md"));
        let entry = app.current_entry().unwrap();
        assert_eq!(entry.phase_id, "1");
        assert_eq!((entry.pos_in_phase, entry.phase_steps), (0, 1));

        // 01-01 is the very first step.
        assert!(app.change_step(-1).is_none());
        assert!(app.flash.as_deref().unwrap().contains("first step"));

        // And forward again crosses back into phase 2 (tabs preserved).
        assert!(app.change_step(1).is_none()); // 02-01 kept its plan tab
        assert_eq!(app.current_entry().unwrap().step.id, "02-01");
    }

    #[test]
    fn open_inserts_in_canonical_order_regardless_of_open_order() {
        let mut app = sample_app();
        assert!(app.open_doc(DocKind::Discussion).is_some());
        assert!(app.open_doc(DocKind::Uat).is_some());
        assert!(app.open_doc(DocKind::Research).is_some());
        assert!(app.open_doc(DocKind::Plan).is_some());
        assert_eq!(
            app.tabs(),
            [
                DocKind::Plan,
                DocKind::Research,
                DocKind::Uat,
                DocKind::Discussion
            ]
        );
        // Last opened (Plan) is focused.
        assert_eq!(app.focus(), Focus::Doc(DocKind::Plan));
    }

    #[test]
    fn reopening_focuses_without_duplicating() {
        let mut app = sample_app();
        assert!(app.open_doc(DocKind::Research).is_some());
        assert!(app.open_doc(DocKind::Context).is_some());
        assert!(app.open_doc(DocKind::Research).is_none()); // no new request
        assert_eq!(app.tabs(), [DocKind::Research, DocKind::Context]);
        assert_eq!(app.focus(), Focus::Doc(DocKind::Research));
    }

    #[test]
    fn step_change_preserves_tabsets_and_autoopens_plan_on_empty() {
        let mut app = sample_app();
        app.open_doc(DocKind::Research);
        app.open_doc(DocKind::Validation);

        // Later step (02-03): empty tab set -> plan auto-opens.
        let req = app.change_step(1).expect("auto-open plan");
        assert_eq!(req.kind, DocKind::Plan);
        assert!(req.path.ends_with("02-03-PLAN.md"));
        assert_eq!(app.tabs(), [DocKind::Plan]);
        assert_eq!(app.focus(), Focus::Doc(DocKind::Plan));

        // Back to 02-02: its tabs are intact, no auto-open.
        assert!(app.change_step(-1).is_none());
        assert_eq!(app.tabs(), [DocKind::Research, DocKind::Validation]);
    }

    #[test]
    fn step_change_past_the_last_step_flashes() {
        let mut app = sample_app();
        app.change_step(1); // 02-03 (last — phase 3 has no directory)
        assert!(app.change_step(1).is_none());
        assert!(app.flash.as_deref().unwrap().contains("last step"));
    }

    #[test]
    fn closing_last_tab_falls_back_to_status() {
        let mut app = sample_app();
        app.open_doc(DocKind::Plan);
        let closed = app.close_current().expect("closed");
        assert_eq!(closed.1, DocKind::Plan);
        assert!(app.tabs().is_empty());
        assert_eq!(app.focus(), Focus::Status);
        // Closing on the Status tab is a no-op.
        assert!(app.close_current().is_none());
    }

    #[test]
    fn missing_document_flashes_and_adds_no_tab() {
        // Phase 1 (01-navigation-skeleton) has no RESEARCH doc.
        let phases = sample_phases();
        let mut app = App::from_phases(&phases[..1]);
        assert_eq!(app.current_entry().unwrap().step.id, "01-01");
        assert!(app.open_doc(DocKind::Research).is_none());
        assert!(app.tabs().is_empty());
        assert!(app.flash.as_deref().unwrap().contains("research"));
    }

    #[test]
    fn focus_cycles_through_status_and_tabs() {
        let mut app = sample_app();
        app.open_doc(DocKind::Plan);
        app.open_doc(DocKind::Context);
        app.focus_slot(1);
        assert_eq!(app.focus(), Focus::Status);
        app.focus_next();
        assert_eq!(app.focus(), Focus::Doc(DocKind::Plan));
        app.focus_next();
        assert_eq!(app.focus(), Focus::Doc(DocKind::Context));
        app.focus_next(); // wraps
        assert_eq!(app.focus(), Focus::Status);
        app.focus_prev(); // wraps back
        assert_eq!(app.focus(), Focus::Doc(DocKind::Context));
    }

    #[test]
    fn no_steps_anywhere_is_survivable() {
        let mut app = App::new(Vec::new());
        assert_eq!(app.focus(), Focus::Status);
        assert!(app.open_doc(DocKind::Plan).is_none());
        assert!(app.flash.is_some());
        assert!(app.change_step(1).is_none());
        assert!(app.close_current().is_none());
        app.focus_next(); // must not panic
    }
}
