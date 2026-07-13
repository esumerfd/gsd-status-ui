use crate::model::{
    DocKind, Phase, Plan, QuickTask, QuickTaskStatus, Stage, StateMeta, Step, Todo,
};
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
    // STATE.md may not exist yet (e.g. a freshly scaffolded project still in the
    // research phase). Treat it as empty so the PROJECT.md title below is still read.
    let body = fs::read_to_string(planning.join("STATE.md")).unwrap_or_default();

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

// ──────────────────────────────────────────────────────────────── todos ──

/// Load pending todos from `.planning/todos/pending/*.md`, sorted by filename
/// (date-prefixed, so chronological). A missing dir yields an empty list.
pub(crate) fn load_todos(planning: &Path) -> Vec<Todo> {
    let dir = planning.join("todos").join("pending");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut todos: Vec<Todo> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
        .filter_map(|path| parse_todo(&path))
        .collect();
    todos.sort_by(|a, b| a.slug.cmp(&b.slug));
    todos
}

fn parse_todo(path: &Path) -> Option<Todo> {
    let slug = path.file_stem()?.to_str()?.to_string();
    let body = fs::read_to_string(path).ok()?;
    let (front, rest) = split_frontmatter(&body);

    let mut title: Option<String> = None;
    let mut area: Option<String> = None;
    for raw in front.lines() {
        let line = raw.trim_end();
        // Skip indented lines (e.g. list items under `files:`).
        if line.starts_with("  ") || line.starts_with('\t') {
            continue;
        }
        if let Some((key, value)) = split_kv(line) {
            let value = strip_quotes(value.trim()).trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.trim() {
                "title" => title = Some(value),
                "area" => area = Some(value),
                _ => {}
            }
        }
    }

    let title = title
        .or_else(|| first_h1(rest))
        .unwrap_or_else(|| title_from_slug(&slug));
    Some(Todo {
        title,
        area,
        slug,
        path: path.to_path_buf(),
    })
}

fn first_h1(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| strip_md(t).trim().to_string()))
        .filter(|t| !t.is_empty())
}

/// Derive a human title from a `YYYY-MM-DD-some-words` filename stem.
fn title_from_slug(slug: &str) -> String {
    let is_date_prefixed = {
        let mut parts = slug.splitn(4, '-');
        matches!(
            (parts.next(), parts.next(), parts.next()),
            (Some(y), Some(m), Some(d))
                if y.len() == 4 && m.len() == 2 && d.len() == 2
                    && y.bytes().chain(m.bytes()).chain(d.bytes()).all(|b| b.is_ascii_digit())
        )
    };
    let rest = if is_date_prefixed {
        slug.splitn(4, '-').nth(3).unwrap_or(slug)
    } else {
        slug
    };
    rest.replace('-', " ")
}

// ────────────────────────────────────────────────────────── quick tasks ──

/// Load quick tasks from `.planning/quick/{id}-{slug}/` directories, sorted by
/// id. A missing `quick/` dir yields an empty list. Each task's visibility and
/// status is decided by cross-referencing STATE.md's "Quick Tasks Completed"
/// table (see `parse_quick_completions`): no matching row keeps the task
/// in-progress (D-03a); a matching row with a passing/blank Status hides the
/// task (D-02); a matching row with a non-passing Status keeps the task
/// visible as `Failed(raw_status)` (D-03b/D-04).
pub(crate) fn load_quick_tasks(planning: &Path) -> Vec<QuickTask> {
    let dir = planning.join("quick");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let completions = parse_quick_completions(planning);
    let mut tasks: Vec<QuickTask> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|path| parse_quick_task(&path))
        .filter_map(|mut task| {
            let dir_name = task.dir.file_name().and_then(|n| n.to_str());
            // Match primarily by exact id equality; fall back (D-05) to
            // treating a row as matching when the task's directory name is
            // contained in that row's Directory/Path cell, in case a
            // project's id column can't be relied on but the directory
            // reference still is.
            let status = completions.get(&task.id).or_else(|| {
                dir_name.and_then(|d| {
                    completions
                        .values()
                        .find(|(_, directory)| directory.contains(d))
                })
            });
            match status {
                None => {
                    task.status = QuickTaskStatus::InProgress;
                    Some(task)
                }
                Some((Some(raw), _)) if !is_passing_status(raw) => {
                    task.status = QuickTaskStatus::Failed(raw.clone());
                    Some(task)
                }
                Some(_) => None, // hidden: blank status or passing status (D-02)
            }
        })
        .collect();
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    tasks
}

/// Parse STATE.md's "Quick Tasks Completed" table into a map of task id ->
/// (raw Status if present and non-empty, Directory/Path cell text). Tolerant
/// of the heading appearing at any `#` level (D-05) and of column names/order
/// diverging from the canonical spec — columns are resolved by fuzzy header
/// name (falling back to position for the id column). A missing heading or
/// table yields an empty map; never panics on ragged/malformed rows.
fn parse_quick_completions(planning: &Path) -> HashMap<String, (Option<String>, String)> {
    let mut map = HashMap::new();
    let body = fs::read_to_string(planning.join("STATE.md")).unwrap_or_default();

    let mut lines = body.lines();
    let mut found_heading = false;
    for line in lines.by_ref() {
        let is_heading = line.trim_start().starts_with('#');
        if is_heading {
            let heading_text = line.trim_start_matches('#').trim();
            if heading_text.eq_ignore_ascii_case("Quick Tasks Completed") {
                found_heading = true;
                break;
            }
        }
    }
    if !found_heading {
        return map;
    }

    let table_rows: Vec<&str> = lines
        .by_ref()
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| l.trim_start().starts_with('|'))
        .collect();
    if table_rows.is_empty() {
        return map;
    }

    let header_cells = split_table_row(table_rows[0]);
    let id_col = header_cells
        .iter()
        .position(|c| {
            let c = c.trim().to_lowercase();
            c == "#" || c == "id"
        })
        .unwrap_or(0);
    let status_col = header_cells
        .iter()
        .position(|c| c.trim().to_lowercase().contains("status"));
    let directory_col = header_cells.iter().position(|c| {
        let c = c.trim().to_lowercase();
        c.contains("directory") || c.contains("path")
    });

    // Row 1 is the header; row 2 (if present and all-dashes) is the
    // separator — skip it defensively without assuming it's always there.
    let data_rows = table_rows.iter().skip(1).filter(|row| {
        let cells = split_table_row(row);
        !cells
            .iter()
            .all(|c| !c.trim().is_empty() && c.trim().chars().all(|ch| ch == '-' || ch == ':'))
    });

    for row in data_rows {
        let cells = split_table_row(row);
        let Some(id_cell) = cells.get(id_col) else {
            continue;
        };
        let id = id_cell.trim().to_string();
        if id.is_empty() {
            continue;
        }
        let status = status_col
            .and_then(|i| cells.get(i))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let directory = directory_col
            .and_then(|i| cells.get(i))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        map.insert(id, (status, directory));
    }

    map
}

/// Split a markdown table row on `|`, trimming each cell. Tolerates leading
/// and trailing `|` (e.g. `| a | b |`) and rows without them (`a | b`).
fn split_table_row(row: &str) -> Vec<String> {
    row.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// Minimal allowlist of Status values that count as "passing" (D-02) — this
/// deliberately does not try to interpret the failing value's meaning (D-04);
/// it only gates the hide/show decision.
fn is_passing_status(s: &str) -> bool {
    matches!(
        s.trim().to_lowercase().as_str(),
        "pass"
            | "passed"
            | "passing"
            | "success"
            | "succeeded"
            | "ok"
            | "done"
            | "complete"
            | "completed"
            | "verified"
    )
}

fn parse_quick_task(dir: &Path) -> Option<QuickTask> {
    let name = dir.file_name()?.to_str()?.to_string();
    let mut parts = name.splitn(3, '-');
    let id = match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}-{b}"),
        _ => name.clone(),
    };

    let plan_path = {
        let preferred = dir.join(format!("{id}-PLAN.md"));
        if preferred.is_file() {
            Some(preferred)
        } else {
            fs::read_dir(dir).ok().and_then(|entries| {
                entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with("-PLAN.md"))
                })
            })
        }
    };

    let mut title: Option<String> = None;
    if let Some(path) = &plan_path {
        if let Ok(body) = fs::read_to_string(path) {
            let (front, rest) = split_frontmatter(&body);
            for raw in front.lines() {
                let line = raw.trim_end();
                if line.starts_with("  ") || line.starts_with('\t') {
                    continue;
                }
                if let Some((key, value)) = split_kv(line) {
                    let value = strip_quotes(value.trim()).trim().to_string();
                    if !value.is_empty() && key.trim() == "title" {
                        title = Some(value);
                    }
                }
            }
            if title.is_none() {
                title = first_h1(rest);
            }
        }
    }
    let title = title.unwrap_or_else(|| title_from_slug(&name));

    Some(QuickTask {
        id,
        title,
        dir: dir.to_path_buf(),
        status: QuickTaskStatus::InProgress,
    })
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
            let is_step = parts
                .next()
                .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
                && parts
                    .next()
                    .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
                && parts.next().is_none();
            if !is_step {
                return None;
            }
            let checked = roadmap_plans.iter().any(|p| p.name == id && p.checked);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_phase_dir() -> PathBuf {
        PathBuf::from("sample/.planning/phases/02-coffee-acquisition")
    }

    fn sample_plans() -> Vec<Plan> {
        vec![
            Plan {
                name: "02-01".into(),
                checked: true,
            },
            Plan {
                name: "02-02".into(),
                checked: false,
            },
            Plan {
                name: "02-03".into(),
                checked: false,
            },
        ]
    }

    #[test]
    fn load_state_reads_project_title_when_state_md_absent() {
        // A freshly scaffolded project (research phase) has PROJECT.md but no
        // STATE.md yet. The banner title must still come from PROJECT.md.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("PROJECT.md"),
            "# Anthropic Cost\n\nA Rust tooling suite.\n",
        )
        .unwrap();

        let meta = load_state(dir.path());

        assert_eq!(meta.project_title, "Anthropic Cost");
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
    fn loads_pending_todos_sorted_with_title_and_fallbacks() {
        let todos = load_todos(Path::new("sample/.planning"));
        let titles: Vec<&str> = todos.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Official signed build process for pr-monitor apps",
                "Cache the decrypted secret in-process",
                "locate mcpsecret source",
            ]
        );
        assert_eq!(todos[0].area.as_deref(), Some("tooling"));
    }

    #[test]
    fn loads_untracked_quick_task_as_in_progress() {
        let tasks = load_quick_tasks(Path::new("sample/.planning"));
        let task = tasks
            .iter()
            .find(|t| t.id == "260709-aa1")
            .expect("260709-aa1 present");
        assert_eq!(task.title, "Add dark-mode toggle");
        assert_eq!(task.status, QuickTaskStatus::InProgress);
    }

    #[test]
    fn hides_completed_shows_failed_keeps_in_progress() {
        let tasks = load_quick_tasks(Path::new("sample/.planning"));

        let in_progress = tasks
            .iter()
            .find(|t| t.id == "260709-aa1")
            .expect("260709-aa1 (in progress) present");
        assert_eq!(in_progress.status, QuickTaskStatus::InProgress);

        let failed = tasks
            .iter()
            .find(|t| t.id == "260710-bb2")
            .expect("260710-bb2 (failed) present");
        assert_eq!(
            failed.status,
            QuickTaskStatus::Failed("verification failed".to_string())
        );

        assert!(
            !tasks.iter().any(|t| t.id == "260708-cc3"),
            "260708-cc3 (completed) must be hidden"
        );
    }

    #[test]
    fn returns_empty_when_no_todos_dir() {
        // The phases/ dir has no todos/ subtree.
        let todos = load_todos(Path::new("sample/.planning/phases"));
        assert!(todos.is_empty());
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
