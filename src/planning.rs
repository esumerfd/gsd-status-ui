use crate::model::{
    DocKind, Document, Other, OtherKind, Phase, Plan, QuickTask, QuickTaskStatus, Stage, StateMeta,
    Step, Todo,
};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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
    let detail_phases = parse_phase_details(&body);
    let plans_per_phase = parse_phase_plans(&body);

    let phase_dirs = scan_phase_dirs(planning);

    let mut phases = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut push_phase = |phases: &mut Vec<Phase>, id: String, title: String, checked: bool| {
        if !seen.insert(normalize_phase_id(&id)) {
            return;
        }
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
    };

    // The `## Phases` index is the primary, ordered source of phases.
    for (id, title, checked) in top_level {
        push_phase(&mut phases, id, title, checked);
    }

    // Fold in phases that appear only as a `### Phase N:` detail heading. These
    // are counted in STATE.md's total but were dropped when the `## Phases`
    // index omitted them, causing the header count and the list to diverge.
    for (id, title) in detail_phases {
        push_phase(&mut phases, id, title, false);
    }

    // Finally, phases that exist only as an on-disk directory (scaffolded but
    // not yet recorded in the roadmap at all) — titled from their slug.
    for dir in &phase_dirs {
        if let Some(raw) = phase_id_from_dir(dir) {
            let title = title_from_dir(dir);
            push_phase(&mut phases, normalize_phase_id(&raw), title, false);
        }
    }

    phases.sort_by(|a, b| {
        phase_sort_key(&a.id)
            .partial_cmp(&phase_sort_key(&b.id))
            .unwrap_or(Ordering::Equal)
    });
    phases
}

/// Normalize a phase id for de-duplication across sources (index, detail
/// headings, directory names): trim surrounding whitespace and leading zeros so
/// `"03"` and `"3"` collapse to the same key.
fn normalize_phase_id(id: &str) -> String {
    let trimmed = id.trim().trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

/// Numeric sort key for a phase id so integer and decimal phases order
/// naturally (1, 2, 2.1, 3). Unparseable ids sort last.
fn phase_sort_key(id: &str) -> f64 {
    id.trim().parse::<f64>().unwrap_or(f64::MAX)
}

/// The phase id encoded in a directory name, i.e. the leading alphanumeric run
/// (`"03-apply-fix"` → `"03"`). Returns None for names with no leading id.
fn phase_id_from_dir(dir: &Path) -> Option<String> {
    let name = dir.file_name().and_then(|n| n.to_str())?;
    let leading: String = name
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if leading.is_empty() {
        None
    } else {
        Some(leading)
    }
}

/// A human-readable title derived from a phase directory slug, used only when
/// the phase has no roadmap entry to name it (`"04-polish-and-ship"` →
/// `"polish and ship"`).
fn title_from_dir(dir: &Path) -> String {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.split_once('-')
        .map(|(_, rest)| rest)
        .unwrap_or(name)
        .replace('-', " ")
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

/// Phases named by a `### Phase N: Title` detail heading, in document order.
/// These headings carry the phase's real title even when the `## Phases` index
/// omits the phase entirely.
fn parse_phase_details(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("### Phase ") else {
            continue;
        };
        let (id, title) = match rest.find(':') {
            Some(c) => (
                rest[..c].trim().to_string(),
                rest[c + 1..].trim().to_string(),
            ),
            None => (rest.trim().to_string(), String::new()),
        };
        if !id.is_empty() {
            out.push((id, title));
        }
    }
    out
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

/// Load todos, sorted pending-first then by filename (date-prefixed, so
/// chronological). Pending todos come from `.planning/todos/pending/`; when
/// `show_completed` is set, resolved todos from `.planning/todos/completed/`
/// are appended (marked `completed`). Missing dirs yield an empty group.
pub(crate) fn load_todos(planning: &Path, show_completed: bool) -> Vec<Todo> {
    let base = planning.join("todos");
    let mut todos = read_todo_dir(&base.join("pending"), false);
    if show_completed {
        todos.extend(read_todo_dir(&base.join("completed"), true));
    }
    todos.sort_by(|a, b| {
        a.completed
            .cmp(&b.completed)
            .then_with(|| a.slug.cmp(&b.slug))
    });
    todos
}

fn read_todo_dir(dir: &Path, completed: bool) -> Vec<Todo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
        .filter_map(|path| parse_todo(&path, completed))
        .collect()
}

fn parse_todo(path: &Path, completed: bool) -> Option<Todo> {
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
        completed,
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
/// in-progress (D-03a); a matching row with a non-passing Status keeps the task
/// visible as `Failed(raw_status)` (D-03b/D-04); a matching row with a
/// passing/blank Status is a finished task — hidden by default (D-02), or shown
/// as `Completed` when `show_completed` is set.
pub(crate) fn load_quick_tasks(planning: &Path, show_completed: bool) -> Vec<QuickTask> {
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
                // Finished (blank or passing status): hidden unless the toggle
                // asks for completed work (D-02).
                Some(_) if show_completed => {
                    task.status = QuickTaskStatus::Completed;
                    Some(task)
                }
                Some(_) => None,
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

/// The leading alphanumeric prefix of a phase directory name, e.g. `"02"` for
/// `02-coffee-acquisition`. Used to match a phase's own docs during discovery.
#[derive(Debug, Clone)]
pub(crate) struct PhaseDocs {
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
        Self { prefix }
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

/// Sequence candidate documents into canonical tab order — the single algorithm
/// shared by phase steps and quick tasks. Each candidate is `(path, token)`,
/// where `token` is the filename with its owning prefix and `.md` extension
/// already stripped (e.g. `"RESEARCH"`, `"CONTEXT"`, `"VERIFICATION"`). A `PLAN`
/// token leads (the primary doc, index 0); the known [`DocKind`]s follow in
/// canonical order, fuzzy-matched by name so slight spelling variance keeps its
/// slot; anything unrecognized is appended after the known kinds, name-sorted.
fn sequence_documents(candidates: Vec<(PathBuf, String)>) -> Vec<Document> {
    // Rank above every canonical ORDER slot; unmatched docs sort here, after
    // the known kinds, broken by name for determinism.
    const UNMATCHED_RANK: u32 = DocKind::ORDER.len() as u32;

    let mut ranked: Vec<(u32, String, Document)> = candidates
        .into_iter()
        .map(|(path, token)| {
            let (rank, tiebreak, label) = if token.eq_ignore_ascii_case("PLAN") {
                (0, String::new(), DocKind::Plan.label().to_string())
            } else {
                match DocKind::classify(&token) {
                    Some(kind) => (
                        kind.order_index() as u32,
                        String::new(),
                        kind.label().to_string(),
                    ),
                    None => (
                        UNMATCHED_RANK,
                        token.to_ascii_uppercase(),
                        token.to_lowercase(),
                    ),
                }
            };
            (rank, tiebreak, Document { path, label })
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    ranked.into_iter().map(|(_, _, doc)| doc).collect()
}

/// Discover every document a step can open, in tab order: the step's own plan
/// first, then the phase-level docs in canonical order (fuzzy-matched by name
/// so a known kind keeps its slot), then any unmatched phase-level file sorted
/// alphabetically. `prefix` is the phase's leading token (e.g. `"02"`).
///
/// A phase step's tab set is its own `NN-MM-PLAN.md` plus the phase-level
/// `NN-<word>.md` docs — never other steps' plans/summaries, nor its own
/// summary (those have a numeric second segment). Only existing files are
/// returned, so callers can treat the result as directly openable. The ordering
/// is delegated to [`sequence_documents`], shared with quick tasks.
pub(crate) fn discover_documents(phase_dir: &Path, prefix: &str, step: &Step) -> Vec<Document> {
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();

    if step.plan_path.is_file() {
        candidates.push((step.plan_path.clone(), "PLAN".to_string()));
    }

    let phase_marker = format!("{prefix}-");
    if let Ok(entries) = fs::read_dir(phase_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            // Only this phase's docs (share the phase prefix).
            let Some(rest) = stem.strip_prefix(&phase_marker) else {
                continue;
            };
            // Skip step-scoped files (`NN-MM-…`): a numeric first segment after
            // the prefix means it belongs to a specific step, not the phase.
            let first_segment = rest.split('-').next().unwrap_or("");
            if !first_segment.is_empty() && first_segment.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let token = rest.to_string();
            candidates.push((path, token));
        }
    }

    sequence_documents(candidates)
}

/// Every markdown file at the `.planning` root, in tab order: `ROADMAP.md`
/// pinned first (so document index 0 stays the roadmap), then the rest —
/// `REQUIREMENTS.md`, `PROJECT.md`, `STATE.md`, `INGEST-CONFLICTS.md`, and any
/// future root doc — name-sorted for determinism. Discovery is generic (a glob
/// of `*.md`) rather than a hard-coded list, so new root docs appear
/// automatically. Subdirectories and non-markdown files are ignored; a missing
/// directory yields an empty `Vec`.
pub(crate) fn discover_root_documents(planning: &Path) -> Vec<Document> {
    let mut others: Vec<(String, Document)> = Vec::new();
    let mut roadmap: Option<Document> = None;
    if let Ok(entries) = fs::read_dir(planning) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            if name == "ROADMAP.md" {
                roadmap = Some(Document {
                    path,
                    label: "roadmap".into(),
                });
                continue;
            }
            let sort_key = name.to_ascii_uppercase();
            let label = stem.to_lowercase();
            others.push((sort_key, Document { path, label }));
        }
    }
    others.sort_by(|a, b| a.0.cmp(&b.0));
    roadmap
        .into_iter()
        .chain(others.into_iter().map(|(_, doc)| doc))
        .collect()
}

// ────────────────────────────────────────────────── notes / ideas / seeds ──

/// Load every note, idea, and seed markdown file under `.planning/notes/`,
/// `.planning/ideas/`, and `.planning/seeds/` into a single list for the Others
/// section. Grouped by kind in [`OtherKind::ALL`] order (Notes, Ideas, Seeds),
/// each group sorted by filename (date- or `SEED-NNN`-prefixed, so effectively
/// chronological). Missing folders contribute nothing.
pub(crate) fn load_others(planning: &Path) -> Vec<Other> {
    let mut out = Vec::new();
    for kind in OtherKind::ALL {
        let mut group = read_others_dir(&planning.join(kind.dir()), kind);
        group.sort_by(|a, b| a.slug.cmp(&b.slug));
        out.extend(group);
    }
    out
}

fn read_others_dir(dir: &Path, kind: OtherKind) -> Vec<Other> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
        .filter_map(|path| parse_other(&path, kind))
        .collect()
}

fn parse_other(path: &Path, kind: OtherKind) -> Option<Other> {
    let slug = path.file_stem()?.to_str()?.to_string();
    let body = fs::read_to_string(path).ok()?;
    let (front, rest) = split_frontmatter(&body);

    let mut title: Option<String> = None;
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

    let title = title
        .or_else(|| first_h1(rest))
        .unwrap_or_else(|| title_from_slug(&slug));
    Some(Other {
        title,
        kind,
        slug,
        path: path.to_path_buf(),
    })
}

/// Every markdown file in a quick-task directory (`.planning/quick/{id}-{slug}/`)
/// in canonical tab order via [`sequence_documents`] — the same fuzzy-classify
/// sequencing phase steps use. Task docs carry the full task-id prefix
/// (`{id}-PLAN.md`, `{id}-CONTEXT.md`, …), so it is stripped to recover the kind
/// token before classifying: `260709-aa1-CONTEXT` → `CONTEXT` → the Context
/// slot. A missing directory yields an empty `Vec`.
pub(crate) fn discover_task_documents(dir: &Path, id: &str) -> Vec<Document> {
    let marker = format!("{id}-");
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            // Strip the task-id prefix so the token is the doc kind; names that
            // don't carry it (a bare `PLAN.md`) pass through unchanged.
            let token = stem.strip_prefix(&marker).unwrap_or(stem).to_string();
            candidates.push((path, token));
        }
    }
    sequence_documents(candidates)
}

/// Every markdown file in a single `.planning` subfolder (e.g. `intel/`,
/// `research/`), name-sorted for determinism. Each doc's label is its lowercased
/// filename stem. A missing or empty folder — and any non-markdown file or
/// nested directory within it — yields nothing, so callers can treat an empty
/// result as "hide this section".
pub(crate) fn discover_folder_documents(planning: &Path, folder: &str) -> Vec<Document> {
    let mut docs: Vec<(String, Document)> = Vec::new();
    if let Ok(entries) = fs::read_dir(planning.join(folder)) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            let sort_key = name.to_ascii_uppercase();
            let label = stem.to_lowercase();
            docs.push((sort_key, Document { path, label }));
        }
    }
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    docs.into_iter().map(|(_, doc)| doc).collect()
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
    fn load_phases_includes_phase_present_only_in_detail_section() {
        // Regression: a phase counted in STATE.md (e.g. "Phases 2/3") that lives
        // only as a `### Phase 3:` detail heading — never added to the `## Phases`
        // index — must still be listed, not silently dropped. Its empty on-disk
        // directory should surface it as NotStarted.
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::create_dir_all(planning.join("phases/03-apply-fix")).unwrap();
        std::fs::write(
            planning.join("ROADMAP.md"),
            "## Phases\n\n\
             - [x] **Phase 1: Reproduce** - diag.\n\
             - [x] **Phase 2: Fix & Verify** - fix.\n\n\
             ## Phase Details\n\n\
             ### Phase 3: Apply Confirmed Doubling Fix\n\n\
             **Goal:** apply the fix.\n",
        )
        .unwrap();

        let phases = load_phases(planning);

        let ids: Vec<&str> = phases.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["1", "2", "3"]);
        let p3 = phases
            .iter()
            .find(|p| p.id == "3")
            .expect("phase 3 present");
        assert_eq!(p3.title, "Apply Confirmed Doubling Fix");
        assert_eq!(p3.stage, Stage::NotStarted);
    }

    #[test]
    fn load_phases_includes_phase_present_only_as_directory() {
        // A scaffolded phase directory with no ROADMAP entry at all must still
        // appear, titled from its slug, rather than vanishing.
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::create_dir_all(planning.join("phases/04-polish-and-ship")).unwrap();
        std::fs::write(
            planning.join("ROADMAP.md"),
            "## Phases\n\n- [ ] **Phase 1: Reproduce** - diag.\n",
        )
        .unwrap();

        let phases = load_phases(planning);

        let ids: Vec<&str> = phases.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["1", "4"]);
        let p4 = phases
            .iter()
            .find(|p| p.id == "4")
            .expect("phase 4 present");
        assert_eq!(p4.title, "polish and ship");
        assert_eq!(p4.stage, Stage::NotStarted);
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
    fn phase_docs_derives_the_leading_prefix() {
        let docs = PhaseDocs::new(&sample_phase_dir());
        assert_eq!(docs.prefix, "02");
    }

    #[test]
    fn loads_pending_todos_sorted_with_title_and_fallbacks() {
        let todos = load_todos(Path::new("sample/.planning"), false);
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
        assert!(todos.iter().all(|t| !t.completed));
    }

    #[test]
    fn hides_completed_todos_unless_asked() {
        // Default (show_completed=false) never surfaces todos/completed/.
        let hidden = load_todos(Path::new("sample/.planning"), false);
        assert!(
            !hidden.iter().any(|t| t.completed),
            "completed todos must stay hidden by default"
        );

        // With show_completed, resolved todos append after the pending ones and
        // carry the completed marker.
        let shown = load_todos(Path::new("sample/.planning"), true);
        let completed: Vec<&str> = shown
            .iter()
            .filter(|t| t.completed)
            .map(|t| t.title.as_str())
            .collect();
        assert_eq!(completed, ["Remove debug logging from the brew loop"]);
        // Pending sort before completed.
        let first_completed = shown.iter().position(|t| t.completed).unwrap();
        assert!(shown[..first_completed].iter().all(|t| !t.completed));
    }

    #[test]
    fn loads_untracked_quick_task_as_in_progress() {
        let tasks = load_quick_tasks(Path::new("sample/.planning"), false);
        let task = tasks
            .iter()
            .find(|t| t.id == "260709-aa1")
            .expect("260709-aa1 present");
        assert_eq!(task.title, "Add dark-mode toggle");
        assert_eq!(task.status, QuickTaskStatus::InProgress);
    }

    #[test]
    fn hides_completed_shows_failed_keeps_in_progress() {
        let tasks = load_quick_tasks(Path::new("sample/.planning"), false);

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
            "260708-cc3 (completed) must be hidden by default"
        );
    }

    #[test]
    fn shows_completed_quick_task_when_asked() {
        let tasks = load_quick_tasks(Path::new("sample/.planning"), true);
        let completed = tasks
            .iter()
            .find(|t| t.id == "260708-cc3")
            .expect("260708-cc3 (completed) present when show_completed");
        assert_eq!(completed.status, QuickTaskStatus::Completed);
        // In-progress and failed tasks still show alongside it.
        assert!(tasks.iter().any(|t| t.id == "260709-aa1"));
        assert!(tasks.iter().any(|t| t.id == "260710-bb2"));
    }

    #[test]
    fn returns_empty_when_no_todos_dir() {
        // The phases/ dir has no todos/ subtree.
        let todos = load_todos(Path::new("sample/.planning/phases"), true);
        assert!(todos.is_empty());
    }

    #[test]
    fn merges_active_debug_session_into_todos_prefixed_debug() {
        let todos = load_todos(Path::new("sample/.planning"), false);
        let debug_todo = todos
            .iter()
            .find(|t| t.title == "Debug: the kiosk app crashes when checking out an empty cart")
            .expect("active debug session surfaced as a todo");
        assert!(!debug_todo.completed);
    }

    #[test]
    fn hides_resolved_debug_session_unless_asked() {
        let hidden = load_todos(Path::new("sample/.planning"), false);
        assert!(
            !hidden
                .iter()
                .any(|t| t.title.contains("receipt printer times out")),
            "resolved debug session must stay hidden by default"
        );

        let shown = load_todos(Path::new("sample/.planning"), true);
        let resolved = shown
            .iter()
            .find(|t| {
                t.title == "Debug: receipt printer times out after 30s on the first print of the day"
            })
            .expect("resolved debug session surfaced when show_completed is set");
        assert!(resolved.completed);
    }

    #[test]
    fn never_surfaces_debug_knowledge_base_as_a_row() {
        let todos = load_todos(Path::new("sample/.planning"), true);
        assert!(
            !todos
                .iter()
                .any(|t| t.title.contains("Knowledge Base") || t.slug == "knowledge-base"),
            "knowledge-base.md must never be surfaced as a debug session row"
        );
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

    #[test]
    fn discover_documents_orders_plan_first_then_canonical_kinds() {
        let dir = sample_phase_dir();
        let step = &discover_steps(&dir, &sample_plans())[0]; // 02-01
        let docs = discover_documents(&dir, "02", step);

        let labels: Vec<&str> = docs.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "plan",
                "research",
                "validation",
                "uat",
                "context",
                "discussion"
            ]
        );
        assert!(docs[0].path.ends_with("02-01-PLAN.md"));
        assert!(docs[1].path.ends_with("02-RESEARCH.md"));
    }

    #[test]
    fn discover_documents_appends_unmatched_files_at_the_end() {
        // The reported bug: 01-VERIFICATION.md is not a known kind, yet must be
        // openable — after the plan, in a trailing tab.
        let dir = PathBuf::from("sample/.planning/phases/01-navigation-skeleton");
        let step = &discover_steps(&dir, &[])[0]; // 01-01
        let docs = discover_documents(&dir, "01", step);

        let labels: Vec<&str> = docs.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["plan", "verification"]);
        assert!(docs[1].path.ends_with("01-VERIFICATION.md"));
    }

    #[test]
    fn discover_documents_excludes_sibling_step_files() {
        // A step's tab set is its own plan plus phase-level docs — never other
        // steps' PLAN/SUMMARY files, nor its own SUMMARY.
        let dir = sample_phase_dir();
        let step = &discover_steps(&dir, &sample_plans())[0]; // 02-01
        let docs = discover_documents(&dir, "02", step);

        for d in &docs {
            let name = d.path.file_name().unwrap().to_string_lossy();
            assert!(
                !name.contains("SUMMARY"),
                "SUMMARY files must be excluded: {name}"
            );
            assert!(
                !name.ends_with("02-02-PLAN.md") && !name.ends_with("02-03-PLAN.md"),
                "sibling step plans must be excluded: {name}"
            );
        }
    }

    #[test]
    fn discover_root_documents_pins_roadmap_first_then_other_root_docs_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        for name in [
            "ROADMAP.md",
            "REQUIREMENTS.md",
            "PROJECT.md",
            "STATE.md",
            "INGEST-CONFLICTS.md",
        ] {
            std::fs::write(planning.join(name), "# doc\n").unwrap();
        }
        // A non-markdown file and a subdirectory must be ignored.
        std::fs::write(planning.join("notes.txt"), "ignore me\n").unwrap();
        std::fs::create_dir_all(planning.join("phases")).unwrap();

        let docs = discover_root_documents(planning);

        let names: Vec<String> = docs
            .iter()
            .map(|d| d.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "ROADMAP.md",
                "INGEST-CONFLICTS.md",
                "PROJECT.md",
                "REQUIREMENTS.md",
                "STATE.md",
            ],
            "ROADMAP.md is pinned first; the rest follow name-sorted"
        );
        assert_eq!(docs[0].label, "roadmap");
        assert_eq!(docs[3].label, "requirements");
    }

    #[test]
    fn discover_root_documents_omits_roadmap_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::write(planning.join("PROJECT.md"), "# p\n").unwrap();
        std::fs::write(planning.join("STATE.md"), "# s\n").unwrap();

        let docs = discover_root_documents(planning);

        let names: Vec<String> = docs
            .iter()
            .map(|d| d.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["PROJECT.md", "STATE.md"]);
    }

    #[test]
    fn discover_root_documents_of_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let docs = discover_root_documents(&dir.path().join("nope"));
        assert!(docs.is_empty());
    }

    #[test]
    fn discover_folder_documents_lists_markdown_name_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::create_dir_all(planning.join("intel")).unwrap();
        for name in ["STACK.md", "ARCHITECTURE.md", "PITFALLS.md"] {
            std::fs::write(planning.join("intel").join(name), "# doc\n").unwrap();
        }
        // A non-markdown file and a nested directory must be ignored.
        std::fs::write(planning.join("intel").join("notes.txt"), "ignore\n").unwrap();
        std::fs::create_dir_all(planning.join("intel").join("sub")).unwrap();

        let docs = discover_folder_documents(planning, "intel");

        let names: Vec<String> = docs
            .iter()
            .map(|d| d.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["ARCHITECTURE.md", "PITFALLS.md", "STACK.md"]);
        assert_eq!(docs[0].label, "architecture");
    }

    #[test]
    fn discover_folder_documents_of_missing_or_empty_folder_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover_folder_documents(dir.path(), "research").is_empty());
        std::fs::create_dir_all(dir.path().join("research")).unwrap();
        assert!(discover_folder_documents(dir.path(), "research").is_empty());
    }

    #[test]
    fn load_others_groups_notes_ideas_seeds_and_derives_titles() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("notes")).unwrap();
        std::fs::write(
            p.join("notes/2026-07-10-espresso-idea.md"),
            "---\ntitle: Espresso timing\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            p.join("notes/2026-07-11-grinder.md"),
            "# Grinder calibration\n\nnotes\n",
        )
        .unwrap();
        std::fs::create_dir_all(p.join("ideas")).unwrap();
        std::fs::write(p.join("ideas/latte-art.md"), "# Latte art mode\n").unwrap();
        std::fs::create_dir_all(p.join("seeds")).unwrap();
        std::fs::write(
            p.join("seeds/SEED-001-mobile-orders.md"),
            "# Mobile orders\n",
        )
        .unwrap();

        let others = load_others(p);

        let rows: Vec<(&str, &str)> = others
            .iter()
            .map(|o| (o.kind.label(), o.title.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("note", "Espresso timing"),
                ("note", "Grinder calibration"),
                ("idea", "Latte art mode"),
                ("seed", "Mobile orders"),
            ],
            "grouped Notes -> Ideas -> Seeds, each sorted by filename, titles resolved"
        );
    }

    #[test]
    fn load_others_of_missing_folders_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_others(dir.path()).is_empty());
    }

    #[test]
    fn discover_task_documents_sequences_by_kind_and_strips_the_id_prefix() {
        // Real quick-task docs carry the full task-id prefix. They must sequence
        // through the shared algorithm (plan first, known kinds in canonical
        // order, unmatched last) with the prefix stripped from the label.
        let dir = tempfile::tempdir().unwrap();
        let task = dir.path().join("260709-aa1-add-dark-mode-toggle");
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("260709-aa1-PLAN.md"), "# plan\n").unwrap();
        std::fs::write(task.join("260709-aa1-SUMMARY.md"), "# summary\n").unwrap();
        std::fs::write(task.join("260709-aa1-CONTEXT.md"), "# context\n").unwrap();
        std::fs::write(task.join("notes.txt"), "ignore\n").unwrap();

        let docs = discover_task_documents(&task, "260709-aa1");

        let labels: Vec<&str> = docs.iter().map(|d| d.label.as_str()).collect();
        // plan (0) → context (canonical Context slot) → summary (unmatched, last).
        assert_eq!(labels, ["plan", "context", "summary"]);
        assert!(docs[0].path.ends_with("260709-aa1-PLAN.md"));
        assert_eq!(
            docs[1].label, "context",
            "id prefix stripped from the label"
        );
    }

    #[test]
    fn discover_task_documents_of_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover_task_documents(&dir.path().join("nope"), "260709-aa1").is_empty());
    }

    #[test]
    fn sequence_documents_shared_algorithm_orders_plan_kinds_then_unmatched() {
        // The one algorithm phase steps and tasks share: PLAN leads, known kinds
        // land in canonical order (fuzzy — `RESERCH` still → research), and
        // unrecognized docs trail, name-sorted.
        let docs = sequence_documents(vec![
            (PathBuf::from("z/VERIFICATION.md"), "VERIFICATION".into()),
            (PathBuf::from("z/RESERCH.md"), "RESERCH".into()),
            (PathBuf::from("z/PLAN.md"), "PLAN".into()),
            (PathBuf::from("z/ABACUS.md"), "ABACUS".into()),
        ]);
        let labels: Vec<&str> = docs.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["plan", "research", "abacus", "verification"]);
    }
}
