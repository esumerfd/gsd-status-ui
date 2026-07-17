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

/// A deferred work item captured under `.planning/todos/pending/` (or, once
/// resolved, `.planning/todos/completed/`).
#[derive(Debug, Clone)]
pub(crate) struct Todo {
    pub(crate) title: String,
    pub(crate) area: Option<String>,
    /// Filename stem, used as a stable secondary sort key.
    pub(crate) slug: String,
    /// The todo's markdown file, opened when the todo is selected.
    pub(crate) path: PathBuf,
    /// True when loaded from `todos/completed/` — only surfaced when the
    /// "show completed" toggle is on.
    pub(crate) completed: bool,
}

/// A lightweight capture surfaced in the Others section: a note, idea, or seed
/// markdown file under `.planning/{notes,ideas,seeds}/`. One row per file,
/// tagged with its [`OtherKind`].
#[derive(Debug, Clone)]
pub(crate) struct Other {
    pub(crate) title: String,
    pub(crate) kind: OtherKind,
    /// Filename stem, used as a stable secondary sort key and nav identity.
    pub(crate) slug: String,
    /// The capture's markdown file, opened when the row is selected.
    pub(crate) path: PathBuf,
}

/// The three capture folders combined into the single Others section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OtherKind {
    Note,
    Idea,
    Seed,
}

impl OtherKind {
    /// Render/scan order: Notes, then Ideas, then Seeds.
    pub(crate) const ALL: [OtherKind; 3] = [OtherKind::Note, OtherKind::Idea, OtherKind::Seed];

    /// The `.planning` subfolder this kind is captured into.
    pub(crate) fn dir(self) -> &'static str {
        match self {
            OtherKind::Note => "notes",
            OtherKind::Idea => "ideas",
            OtherKind::Seed => "seeds",
        }
    }

    /// The lowercase tag (used for nav step-id prefixes and doc labels).
    pub(crate) fn label(self) -> &'static str {
        match self {
            OtherKind::Note => "note",
            OtherKind::Idea => "idea",
            OtherKind::Seed => "seed",
        }
    }

    /// The capitalized name used to prefix a row ("Note:", "Idea:", "Seed:")
    /// and to title the tab/footer.
    pub(crate) fn title(self) -> &'static str {
        match self {
            OtherKind::Note => "Note",
            OtherKind::Idea => "Idea",
            OtherKind::Seed => "Seed",
        }
    }
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
    /// A finished task (passing/blank STATE.md status). Hidden by default;
    /// only surfaced when the "show completed" toggle is on.
    Completed,
}

impl QuickTaskStatus {
    pub(crate) fn icon(&self) -> &'static str {
        match self {
            QuickTaskStatus::InProgress => "●",
            QuickTaskStatus::Failed(_) => "✗",
            QuickTaskStatus::Completed => "✓",
        }
    }
    pub(crate) fn label(&self) -> String {
        match self {
            QuickTaskStatus::InProgress => "in progress".to_string(),
            QuickTaskStatus::Failed(s) => s.clone(),
            QuickTaskStatus::Completed => "completed".to_string(),
        }
    }
    pub(crate) fn color(&self) -> &'static str {
        match self {
            QuickTaskStatus::InProgress => color::YELLOW,
            QuickTaskStatus::Failed(_) => color::RED,
            QuickTaskStatus::Completed => color::GREEN,
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

/// The canonical document kinds, in tab order. Used as a *classifier* over
/// discovered filenames (see [`DocKind::classify`]) — no longer the identity of
/// a tab. `Plan` heads the order (the step's own plan always opens first); the
/// rest are phase-level docs. The project-level roadmap and any file that
/// doesn't classify are handled outside this enum, by the discovery layer.
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

    /// Position in canonical tab order. Every variant is in [`ORDER`], so the
    /// fallback is unreachable and only guards against future additions.
    pub(crate) fn order_index(self) -> usize {
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
        }
    }

    /// Phase-level kinds that a discovered filename can be fuzzily classified
    /// into, in canonical tab order. `Plan` (resolved from the step's own
    /// plan path) and `Roadmap` (project-level) are deliberately excluded.
    const CLASSIFIABLE: [(DocKind, &'static str); 5] = [
        (DocKind::Research, "RESEARCH"),
        (DocKind::Validation, "VALIDATION"),
        (DocKind::Uat, "UAT"),
        (DocKind::Context, "CONTEXT"),
        (DocKind::Discussion, "DISCUSSION"),
    ];

    /// Fuzzily classify a phase-level document's kind token — the filename with
    /// the phase prefix and `.md` extension stripped (e.g. `"VERIFICATION"`,
    /// `"DISCUSSION-LOG"`, `"RESERCH"`) — into a known [`DocKind`], or `None`
    /// when nothing fits well enough.
    ///
    /// This is the "fuzzy match on the name to ensure best fit" step: a known
    /// doc keeps its canonical tab slot even if its filename varies slightly,
    /// while a genuinely unknown doc (e.g. `VERIFICATION`, `SECURITY`) returns
    /// `None` so the caller can append it after the known docs. The threshold
    /// is deliberately high enough that near-but-distinct names — most notably
    /// `VERIFICATION` vs `VALIDATION` — do not collide.
    pub(crate) fn classify(token: &str) -> Option<DocKind> {
        let norm = normalize_kind_token(token);
        if norm.is_empty() {
            return None;
        }
        let mut best: Option<(DocKind, f64)> = None;
        for (kind, keyword) in Self::CLASSIFIABLE {
            let score = token_match_score(&norm, keyword);
            if score >= CLASSIFY_THRESHOLD && best.is_none_or(|(_, b)| score > b) {
                best = Some((kind, score));
            }
        }
        best.map(|(kind, _)| kind)
    }
}

/// Minimum similarity for [`DocKind::classify`] to accept a match. Chosen so
/// single-character typos still classify (`RESERCH` → Research, ratio ≈ 0.88)
/// while distinct names stay unmatched (`VERIFICATION` vs `VALIDATION` ≈ 0.42).
const CLASSIFY_THRESHOLD: f64 = 0.72;

/// Uppercase the token and drop everything but ASCII letters, so digits,
/// dashes, and case never affect classification (`"Discussion-Log"` →
/// `"DISCUSSIONLOG"`).
fn normalize_kind_token(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Similarity of a normalized token against a canonical keyword in `[0.0, 1.0]`.
/// Containment (only for keywords of 4+ letters, so short ones like `UAT` don't
/// over-match) scores high; otherwise a Levenshtein ratio is used.
fn token_match_score(norm: &str, keyword: &str) -> f64 {
    if norm == keyword {
        return 1.0;
    }
    if keyword.len() >= 4 && (norm.contains(keyword) || keyword.contains(norm)) {
        return 0.95;
    }
    let dist = levenshtein(norm, keyword);
    let max = norm.len().max(keyword.len());
    if max == 0 {
        return 0.0;
    }
    1.0 - (dist as f64 / max as f64)
}

/// Classic Levenshtein edit distance over ASCII bytes (tokens are normalized to
/// A–Z, so byte length equals char length).
fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// A single openable document within a step's tab set: a concrete file plus the
/// label its tab shows when no view title is available yet. Discovery resolves
/// these from the actual files on disk (see `planning::discover_documents`), so
/// any file can back a tab — not just the fixed [`DocKind`] set.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Document {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_matches_exact_canonical_tokens() {
        assert_eq!(DocKind::classify("RESEARCH"), Some(DocKind::Research));
        assert_eq!(DocKind::classify("VALIDATION"), Some(DocKind::Validation));
        assert_eq!(DocKind::classify("UAT"), Some(DocKind::Uat));
        assert_eq!(DocKind::classify("CONTEXT"), Some(DocKind::Context));
    }

    #[test]
    fn classify_is_case_and_separator_insensitive() {
        // Real GSD discussion doc is "DISCUSSION-LOG"; the dash and the trailing
        // LOG must not stop it from landing in the Discussion slot.
        assert_eq!(
            DocKind::classify("DISCUSSION-LOG"),
            Some(DocKind::Discussion)
        );
        assert_eq!(DocKind::classify("research"), Some(DocKind::Research));
    }

    #[test]
    fn classify_tolerates_single_char_typos() {
        assert_eq!(DocKind::classify("RESERCH"), Some(DocKind::Research));
    }

    #[test]
    fn classify_rejects_distinct_unknown_docs() {
        // The reported bug: VERIFICATION must NOT be mistaken for VALIDATION —
        // it is unknown and belongs at the end of the tab set.
        assert_eq!(DocKind::classify("VERIFICATION"), None);
        assert_eq!(DocKind::classify("SECURITY"), None);
        assert_eq!(DocKind::classify(""), None);
    }
}
