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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DocKind {
    Plan,
    Research,
    Validation,
    Uat,
    Context,
    Discussion,
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
        Self::ORDER.iter().position(|k| *k == self).unwrap()
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            DocKind::Plan => "plan",
            DocKind::Research => "research",
            DocKind::Validation => "validation",
            DocKind::Uat => "uat",
            DocKind::Context => "context",
            DocKind::Discussion => "discussion",
        }
    }

    /// Phase-level file name suffix; `Plan` is step-level and has no suffix.
    pub(crate) fn phase_suffix(self) -> Option<&'static str> {
        match self {
            DocKind::Plan => None,
            DocKind::Research => Some("RESEARCH.md"),
            DocKind::Validation => Some("VALIDATION.md"),
            DocKind::Uat => Some("UAT.md"),
            DocKind::Context => Some("CONTEXT.md"),
            DocKind::Discussion => Some("DISCUSSION-LOG.md"),
        }
    }
}
