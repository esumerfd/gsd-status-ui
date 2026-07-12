use crate::color;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(crate) struct StateMeta {
    pub(crate) milestone: String,
    pub(crate) milestone_name: String,
    pub(crate) status: String,
    pub(crate) last_updated: String,
    pub(crate) total_phases: u32,
    pub(crate) completed_phases: u32,
    pub(crate) total_plans: u32,
    pub(crate) completed_plans: u32,
    pub(crate) percent: u32,
    pub(crate) next_action: String,
    pub(crate) project_title: String,
}

/// A deferred work item captured under `.planning/todos/pending/`.
#[derive(Debug, Clone)]
pub(crate) struct Todo {
    pub(crate) title: String,
    pub(crate) area: Option<String>,
    /// Filename stem, used as a stable secondary sort key.
    pub(crate) slug: String,
    /// The todo's markdown file, opened when the todo is selected.
    pub(crate) path: PathBuf,
}

/// A quick task captured under `.planning/quick/{id}-{slug}/`.
#[derive(Debug, Clone)]
pub(crate) struct QuickTask {
    /// Leading `NNNNNN-xxx` directory-name token (e.g. `260709-aa1`).
    pub(crate) id: String,
    pub(crate) title: String,
    /// The quick-task directory, analogous to `Todo::path`.
    pub(crate) dir: PathBuf,
    pub(crate) status: QuickTaskStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QuickTaskStatus {
    InProgress,
    /// Raw `Status` string captured from a matching STATE.md row (D-04).
    Failed(String),
}

impl QuickTaskStatus {
    pub(crate) fn icon(&self) -> &'static str {
        match self {
            QuickTaskStatus::InProgress => "●",
            QuickTaskStatus::Failed(_) => "✗",
        }
    }
    pub(crate) fn label(&self) -> String {
        match self {
            QuickTaskStatus::InProgress => "in progress".to_string(),
            QuickTaskStatus::Failed(s) => s.clone(),
        }
    }
    pub(crate) fn color(&self) -> &'static str {
        match self {
            QuickTaskStatus::InProgress => color::YELLOW,
            QuickTaskStatus::Failed(_) => color::RED,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Phase {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) roadmap_checked: bool,
    pub(crate) plans: Vec<Plan>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) stage: Stage,
}

#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub(crate) name: String,
    pub(crate) checked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Stage {
    NotStarted,
    Discussing,
    Discussed,
    Planned,
    Executing,
    Executed,
    Verified,
}

impl Stage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Stage::NotStarted => "not started",
            Stage::Discussing => "discussing",
            Stage::Discussed => "discussed",
            Stage::Planned => "planned",
            Stage::Executing => "executing",
            Stage::Executed => "executed",
            Stage::Verified => "verified",
        }
    }
    pub(crate) fn color(self) -> &'static str {
        match self {
            Stage::NotStarted => color::GREY,
            Stage::Discussing | Stage::Discussed => color::MAGENTA,
            Stage::Planned => color::BLUE,
            Stage::Executing => color::YELLOW,
            Stage::Executed => color::CYAN,
            Stage::Verified => color::GREEN,
        }
    }
}

/// One executable plan within a phase (e.g. `02-01-PLAN.md`).
#[derive(Debug, Clone)]
pub(crate) struct Step {
    pub(crate) id: String,
    pub(crate) plan_path: PathBuf,
    pub(crate) checked: bool,
}

/// The document kinds a step's tab set can show, in canonical tab order.
///
/// `Roadmap` is special: it is a project-level document, not a step/phase doc.
/// It never appears in a phase step's tab set — only on the synthetic Roadmap
/// entry — so it is deliberately excluded from [`DocKind::ORDER`] and the `o`
/// open-document picker. Its `phase_suffix()` is `None`, so `path_for` resolves
/// it to the entry's `step.plan_path` (the workspace-root `ROADMAP.md`), exactly
/// as a pending todo reuses `Plan` to open its own markdown file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DocKind {
    Plan,
    Research,
    Validation,
    Uat,
    Context,
    Discussion,
    Roadmap,
}

impl DocKind {
    pub(crate) const ORDER: [DocKind; 6] = [
        DocKind::Plan,
        DocKind::Research,
        DocKind::Validation,
        DocKind::Uat,
        DocKind::Context,
        DocKind::Discussion,
    ];

    pub(crate) fn order_index(self) -> usize {
        // Roadmap is not in ORDER (it never mixes into a step's tab set); it
        // sorts after every step doc so the ordered-insert never panics.
        Self::ORDER
            .iter()
            .position(|k| *k == self)
            .unwrap_or(usize::MAX)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            DocKind::Plan => "plan",
            DocKind::Research => "research",
            DocKind::Validation => "validation",
            DocKind::Uat => "uat",
            DocKind::Context => "context",
            DocKind::Discussion => "discussion",
            DocKind::Roadmap => "roadmap",
        }
    }

    /// Phase-level file name suffix; `Plan` and `Roadmap` are resolved from the
    /// entry's `step.plan_path`, so they have no suffix.
    pub(crate) fn phase_suffix(self) -> Option<&'static str> {
        match self {
            DocKind::Plan | DocKind::Roadmap => None,
            DocKind::Research => Some("RESEARCH.md"),
            DocKind::Validation => Some("VALIDATION.md"),
            DocKind::Uat => Some("UAT.md"),
            DocKind::Context => Some("CONTEXT.md"),
            DocKind::Discussion => Some("DISCUSSION-LOG.md"),
        }
    }
}
