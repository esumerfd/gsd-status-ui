use crate::model::{DocKind, Phase, Plan, Stage, StateMeta, Step};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn find_planning_dir(start: &Path) -> Option<PathBuf> {
    let mut cur = start.canonicalize().ok()?;
    loop {
        let cand = cur.join(".planning");
        if cand.is_dir() {
            return Some(cand);
        }
        if !cur.pop() {
            return None;
        }
    }
}

// ───────────────────────────────────────────────────────────── STATE.md ──

pub(crate) fn load_state(planning: &Path) -> StateMeta {
    let mut meta = StateMeta::default();
    let path = planning.join("STATE.md");
    let body = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return meta,
    };

    let (front, rest) = split_frontmatter(&body);
    parse_frontmatter(front, &mut meta);

    meta.next_action = extract_section(rest, "Next Action");

    // Prefer PROJECT.md for the human-readable title; fall back to STATE.md's H1
    // with its leading "STATE:" / "ROADMAP:" tag stripped.
    if let Ok(p) = fs::read_to_string(planning.join("PROJECT.md")) {
        for line in p.lines() {
            if let Some(t) = line.strip_prefix("# ") {
                meta.project_title = strip_md(t).trim().to_string();
                break;
            }
        }
    }
    if meta.project_title.is_empty() {
        for line in rest.lines() {
            if let Some(t) = line.strip_prefix("# ") {
                meta.project_title = strip_md(t)
                    .trim()
                    .trim_start_matches("STATE:")
                    .trim()
                    .to_string();
                break;
            }
        }
    }
    meta
}

fn split_frontmatter(body: &str) -> (&str, &str) {
    let trimmed = body.trim_start_matches('\u{feff}');
    let rest = match trimmed.strip_prefix("---\n") {
        Some(r) => r,
        None => return ("", trimmed),
    };
    if let Some(end) = rest.find("\n---\n") {
        (&rest[..end], &rest[end + 5..])
    } else {
        ("", trimmed)
    }
}

fn parse_frontmatter(front: &str, meta: &mut StateMeta) {
    let mut in_progress = false;
    for raw in front.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let indented = line.starts_with("  ") || line.starts_with('\t');
        if !indented {
            in_progress = false;
        }
        if let Some((key, value)) = split_kv(line) {
            let key = key.trim();
            let value = strip_quotes(value.trim());
            match (in_progress, key) {
                (_, "progress") if value.is_empty() => in_progress = true,
                (false, "milestone") => meta.milestone = value.to_string(),
                (false, "milestone_name") => meta.milestone_name = value.to_string(),
                (false, "status") => meta.status = value.to_string(),
                (false, "last_updated") => meta.last_updated = value.to_string(),
                (true, "total_phases") => meta.total_phases = value.parse().unwrap_or(0),
                (true, "completed_phases") => meta.completed_phases = value.parse().unwrap_or(0),
                (true, "total_plans") => meta.total_plans = value.parse().unwrap_or(0),
                (true, "completed_plans") => meta.completed_plans = value.parse().unwrap_or(0),
                (true, "percent") => meta.percent = value.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(':')?;
    Some((line[..idx].trim_start(), &line[idx + 1..]))
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn extract_section(body: &str, heading: &str) -> String {
    let mut collecting = false;
    let mut buf = String::new();
    for line in body.lines() {
        let is_heading = line.trim_start().starts_with('#');
        let heading_text = line.trim_start_matches('#').trim();
        if is_heading {
            if collecting {
                break;
            }
            if heading_text.eq_ignore_ascii_case(heading) {
                collecting = true;
                continue;
            }
        } else if collecting {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    buf.trim().to_string()
}

fn strip_md(s: &str) -> String {
    s.replace("**", "").replace('`', "")
}

// ─────────────────────────────────────────────────────────── ROADMAP.md ──

pub(crate) fn load_phases(planning: &Path) -> Vec<Phase> {
    let roadmap_path = planning.join("ROADMAP.md");
    let body = match fs::read_to_string(&roadmap_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let top_level = parse_phase_index(&body);
    let plans_per_phase = parse_phase_plans(&body);

    let phase_dirs = scan_phase_dirs(planning);

    let mut phases = Vec::new();
    for (id, title, checked) in top_level {
        let plans = plans_per_phase.get(&id).cloned().unwrap_or_default();
        let dir = phase_dirs
            .iter()
            .find(|p| dir_matches_phase(p, &id))
            .cloned();
        let stage = infer_stage(dir.as_deref(), &plans, checked);
        phases.push(Phase {
            id,
            title,
            roadmap_checked: checked,
            plans,
            dir,
            stage,
        });
    }
    phases
}

fn parse_phase_index(body: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut in_phases = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## ") {
            in_phases = trimmed.eq_ignore_ascii_case("## Phases");
            continue;
        }
        if !in_phases {
            continue;
        }
        if let Some(item) = parse_phase_index_line(trimmed) {
            out.push(item);
        }
    }
    out
}

fn parse_phase_index_line(line: &str) -> Option<(String, String, bool)> {
    let rest = line.strip_prefix("- ")?;
    let (checked, rest) = if let Some(r) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        (true, r)
    } else if let Some(r) = rest.strip_prefix("[ ] ") {
        (false, r)
    } else {
        return None;
    };
    let bold = rest.strip_prefix("**")?;
    let end = bold.find("**")?;
    let header = &bold[..end];
    let phase_part = header.strip_prefix("Phase ")?;
    let colon = phase_part.find(':')?;
    let id = phase_part[..colon].trim().to_string();
    let title = phase_part[colon + 1..].trim().to_string();
    Some((id, title, checked))
}

fn parse_phase_plans(body: &str) -> HashMap<String, Vec<Plan>> {
    let mut map: HashMap<String, Vec<Plan>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("### Phase ") {
            let colon = rest.find(':').unwrap_or(rest.len());
            let id = rest[..colon].trim().to_string();
            current = Some(id);
            continue;
        }
        if trimmed.starts_with("## ") {
            current = None;
            continue;
        }
        let Some(id) = current.as_ref() else { continue };
        if let Some(plan) = parse_plan_line(trimmed) {
            map.entry(id.clone()).or_default().push(plan);
        }
    }
    map
}

fn parse_plan_line(line: &str) -> Option<Plan> {
    let rest = line.strip_prefix("- ")?;
    let (checked, rest) = if let Some(r) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        (true, r)
    } else if let Some(r) = rest.strip_prefix("[ ] ") {
        (false, r)
    } else {
        return None;
    };
    if !rest.contains("PLAN.md") {
        return None;
    }
    let name = rest
        .split('—')
        .next()
        .unwrap_or(rest)
        .trim()
        .trim_end_matches(".md")
        .trim_end_matches("-PLAN")
        .to_string();
    Some(Plan { name, checked })
}

// ──────────────────────────────────────────────────────── phase scanning ──

fn scan_phase_dirs(planning: &Path) -> Vec<PathBuf> {
    let phases_root = planning.join("phases");
    let Ok(entries) = fs::read_dir(&phases_root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn dir_matches_phase(dir: &Path, phase_id: &str) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let leading: String = name
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let normalized_dir = leading.trim_start_matches('0');
    let normalized_id = phase_id.trim_start_matches('0');
    if normalized_dir.is_empty() && normalized_id.is_empty() {
        return true;
    }
    normalized_dir.eq_ignore_ascii_case(normalized_id)
}

fn infer_stage(dir: Option<&Path>, plans: &[Plan], roadmap_checked: bool) -> Stage {
    if roadmap_checked {
        return Stage::Verified;
    }
    let Some(dir) = dir else {
        return Stage::NotStarted;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Stage::NotStarted;
    };
    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();

    let has = |suffix: &str| names.iter().any(|n| n.ends_with(suffix));
    let count = |suffix: &str| names.iter().filter(|n| n.ends_with(suffix)).count();

    if has("-VERIFICATION.md") {
        return Stage::Verified;
    }
    let summaries = count("-SUMMARY.md");
    let plan_files = count("-PLAN.md");
    if plan_files > 0 && summaries >= plan_files {
        return Stage::Executed;
    }
    if plan_files > 0 && summaries > 0 {
        return Stage::Executing;
    }
    if plan_files > 0 {
        return Stage::Planned;
    }
    if !plans.is_empty() {
        return Stage::Discussed;
    }
    if has("-CONTEXT.md") {
        return Stage::Discussed;
    }
    if names.iter().any(|n| n.contains("DISCUSS-CHECKPOINT")) {
        return Stage::Discussing;
    }
    Stage::NotStarted
}

// ─────────────────────────────────────────── steps & document discovery ──

/// Resolves document paths for one phase directory.
#[derive(Debug, Clone)]
pub(crate) struct PhaseDocs {
    pub(crate) dir: PathBuf,
    /// Leading alphanumeric prefix of the dir name, e.g. "02".
    pub(crate) prefix: String,
}

impl PhaseDocs {
    pub(crate) fn new(dir: &Path) -> Self {
        let prefix = dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| {
                n.chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
            })
            .unwrap_or_default();
        Self {
            dir: dir.to_path_buf(),
            prefix,
        }
    }

    pub(crate) fn path_for(&self, kind: DocKind, step: &Step) -> PathBuf {
        match kind.phase_suffix() {
            None => step.plan_path.clone(),
            Some(suffix) => self.dir.join(format!("{}-{}", self.prefix, suffix)),
        }
    }
}

/// Enumerate a phase's steps from its `NN-MM-PLAN.md` files, ordered by id.
/// `roadmap_plans` supplies the checked state (matched by plan name).
pub(crate) fn discover_steps(phase_dir: &Path, roadmap_plans: &[Plan]) -> Vec<Step> {
    let Ok(entries) = fs::read_dir(phase_dir) else {
        return Vec::new();
    };
    let mut steps: Vec<Step> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let id = name.strip_suffix("-PLAN.md")?;
            // Step plans are NN-MM (two dash-separated numeric segments).
            let mut parts = id.split('-');
            let is_step = parts.next().is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
                && parts.next().is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
                && parts.next().is_none();
            if !is_step {
                return None;
            }
            let checked = roadmap_plans
                .iter()
                .any(|p| p.name == id && p.checked);
            Some(Step {
                id: id.to_string(),
                plan_path: path.clone(),
                checked,
            })
        })
        .collect();
    steps.sort_by(|a, b| a.id.cmp(&b.id));
    steps
}

/// The step to select on startup: first unchecked plan, else the first step.
pub(crate) fn initial_step(steps: &[Step]) -> usize {
    steps.iter().position(|s| !s.checked).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_phase_dir() -> PathBuf {
        PathBuf::from("sample/.planning/phases/02-coffee-acquisition")
    }

    fn sample_plans() -> Vec<Plan> {
        vec![
            Plan { name: "02-01".into(), checked: true },
            Plan { name: "02-02".into(), checked: false },
            Plan { name: "02-03".into(), checked: false },
        ]
    }

    #[test]
    fn discovers_steps_in_order_with_checked_state() {
        let steps = discover_steps(&sample_phase_dir(), &sample_plans());
        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["02-01", "02-02", "02-03"]);
        assert!(steps[0].checked);
        assert!(!steps[1].checked);
    }

    #[test]
    fn initial_step_is_first_unchecked() {
        let steps = discover_steps(&sample_phase_dir(), &sample_plans());
        assert_eq!(initial_step(&steps), 1); // 02-02
    }

    #[test]
    fn initial_step_falls_back_to_first_when_all_checked() {
        let plans: Vec<Plan> = sample_plans()
            .into_iter()
            .map(|mut p| { p.checked = true; p })
            .collect();
        let steps = discover_steps(&sample_phase_dir(), &plans);
        assert_eq!(initial_step(&steps), 0);
    }

    #[test]
    fn resolves_step_and_phase_doc_paths() {
        let docs = PhaseDocs::new(&sample_phase_dir());
        assert_eq!(docs.prefix, "02");
        let steps = discover_steps(&sample_phase_dir(), &sample_plans());
        let step = &steps[1];

        let plan = docs.path_for(DocKind::Plan, step);
        assert!(plan.ends_with("02-02-PLAN.md"));
        assert!(plan.exists());

        for (kind, file) in [
            (DocKind::Research, "02-RESEARCH.md"),
            (DocKind::Validation, "02-VALIDATION.md"),
            (DocKind::Uat, "02-UAT.md"),
            (DocKind::Context, "02-CONTEXT.md"),
            (DocKind::Discussion, "02-DISCUSSION-LOG.md"),
        ] {
            let p = docs.path_for(kind, step);
            assert!(p.ends_with(file), "{kind:?} -> {}", p.display());
            assert!(p.exists(), "{} should exist", p.display());
        }
    }

    #[test]
    fn phase_level_docs_ignore_non_step_plan_files() {
        // 01-navigation-skeleton has 01-01-PLAN.md only; VERIFICATION and
        // SUMMARY files must not be mistaken for steps.
        let steps = discover_steps(
            &PathBuf::from("sample/.planning/phases/01-navigation-skeleton"),
            &[],
        );
        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["01-01"]);
    }
}
