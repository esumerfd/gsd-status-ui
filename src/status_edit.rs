//! Status editing: pure filesystem writes that move/relabel `.planning/`
//! items between the statuses GSD itself understands. No terminal or ratatui
//! types here — everything is path-in, path-or-error-out, so it stays
//! unit-testable the way `src/planning.rs` is.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Which sibling-directory pair a todo-shaped file belongs to: a plain todo
/// (`todos/pending` <-> `todos/completed`) or a GSD debug session
/// (`debug/` <-> `debug/resolved/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TodoKind {
    Todo,
    Debug,
}

/// What the selected row can be edited to, and the context needed to write
/// the choice back to disk.
#[derive(Debug, Clone)]
pub(crate) enum StatusTarget {
    Todo {
        path: PathBuf,
        kind: TodoKind,
    },
    /// A `## Phases` roadmap-index row, identified by its phase id. Only the
    /// three marks `parse_phase_index_line` accepts are ever offered — a
    /// phase's disk-derived stage (planned/executing/executed) can't be
    /// fabricated here.
    Phase {
        planning: PathBuf,
        phase_id: String,
    },
    /// A `Quick Tasks Completed` STATE.md table row, identified by task id.
    QuickTask {
        planning: PathBuf,
        task_id: String,
    },
    /// A note/idea/seed capture, identified by its own markdown file. Status
    /// lives in that file's frontmatter `status:` key, not in any index.
    Other {
        path: PathBuf,
    },
}

/// A choice offered in the status dialog. Which choices are offered for a
/// given [`StatusTarget`] comes from [`StatusTarget::choices`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusChoice {
    TodoPending,
    TodoComplete,
    PhaseVerified,
    PhaseOpen,
    PhaseAbandoned,
    QuickTaskInProgress,
    QuickTaskCompleted,
    QuickTaskFailed,
    OtherActive,
    OtherComplete,
}

impl StatusChoice {
    pub(crate) fn label(self) -> &'static str {
        match self {
            StatusChoice::TodoPending => "pending",
            StatusChoice::TodoComplete => "complete",
            StatusChoice::PhaseVerified => "verified",
            StatusChoice::PhaseOpen => "open (disk-inferred stage)",
            StatusChoice::PhaseAbandoned => "abandoned",
            StatusChoice::QuickTaskInProgress => "in-progress",
            StatusChoice::QuickTaskCompleted => "complete",
            StatusChoice::QuickTaskFailed => "failed",
            StatusChoice::OtherActive => "active",
            StatusChoice::OtherComplete => "complete",
        }
    }
}

impl StatusTarget {
    /// The vocabulary this target's item type actually supports, in display
    /// order. A phase's disk-derived stages are never offered here — this
    /// module only ever writes what GSD's own parsers read back.
    pub(crate) fn choices(&self) -> Vec<(StatusChoice, String)> {
        match self {
            StatusTarget::Todo { .. } => vec![
                (
                    StatusChoice::TodoPending,
                    StatusChoice::TodoPending.label().to_string(),
                ),
                (
                    StatusChoice::TodoComplete,
                    StatusChoice::TodoComplete.label().to_string(),
                ),
            ],
            StatusTarget::Phase { .. } => vec![
                (
                    StatusChoice::PhaseVerified,
                    StatusChoice::PhaseVerified.label().to_string(),
                ),
                (
                    StatusChoice::PhaseOpen,
                    StatusChoice::PhaseOpen.label().to_string(),
                ),
                (
                    StatusChoice::PhaseAbandoned,
                    StatusChoice::PhaseAbandoned.label().to_string(),
                ),
            ],
            StatusTarget::QuickTask { .. } => vec![
                (
                    StatusChoice::QuickTaskInProgress,
                    StatusChoice::QuickTaskInProgress.label().to_string(),
                ),
                (
                    StatusChoice::QuickTaskCompleted,
                    StatusChoice::QuickTaskCompleted.label().to_string(),
                ),
                (
                    StatusChoice::QuickTaskFailed,
                    StatusChoice::QuickTaskFailed.label().to_string(),
                ),
            ],
            StatusTarget::Other { .. } => vec![
                (
                    StatusChoice::OtherActive,
                    StatusChoice::OtherActive.label().to_string(),
                ),
                (
                    StatusChoice::OtherComplete,
                    StatusChoice::OtherComplete.label().to_string(),
                ),
            ],
        }
    }
}

/// Distinguish a debug session from a plain todo by whether any path
/// component is literally `debug` — true for both `.planning/debug/*.md`
/// (active) and `.planning/debug/resolved/*.md` (resolved).
pub(crate) fn detect_todo_kind(path: &Path) -> TodoKind {
    if path.components().any(|c| c.as_os_str() == "debug") {
        TodoKind::Debug
    } else {
        TodoKind::Todo
    }
}

/// Write `choice` for `target`, returning the file that was written (the
/// todo/debug file itself, or ROADMAP.md / STATE.md for a phase / quick task).
pub(crate) fn apply(target: &StatusTarget, choice: StatusChoice) -> io::Result<PathBuf> {
    match target {
        StatusTarget::Todo { path, kind } => apply_todo(path, *kind, choice),
        StatusTarget::Phase { planning, phase_id } => apply_phase(planning, phase_id, choice),
        StatusTarget::QuickTask { planning, task_id } => {
            apply_quick_task(planning, task_id, choice)
        }
        StatusTarget::Other { path } => apply_other(path, choice),
    }
}

fn apply_todo(path: &Path, kind: TodoKind, choice: StatusChoice) -> io::Result<PathBuf> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no file at {}", path.display()),
        ));
    }
    match choice {
        StatusChoice::TodoComplete => complete_todo(path, kind),
        StatusChoice::TodoPending => reopen_todo(path, kind),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{other:?} is not a todo choice"),
        )),
    }
}

/// The sibling "done" directory for `path` under `kind`'s pair, and the
/// destination file path within it.
fn done_destination(path: &Path, kind: TodoKind) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent dir"))?;
    match kind {
        // pending/{file} -> completed/{file} (siblings under `todos/`).
        TodoKind::Todo => Ok(parent
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no todos dir"))?
            .join("completed")
            .join(file_name)),
        // debug/{file} -> debug/resolved/{file}; already-resolved files
        // (parent name "resolved") stay put — same destination as source.
        TodoKind::Debug => {
            if parent.file_name().and_then(|n| n.to_str()) == Some("resolved") {
                Ok(path.to_path_buf())
            } else {
                Ok(parent.join("resolved").join(file_name))
            }
        }
    }
}

/// The "not done" destination for `path` under `kind`'s pair.
fn pending_destination(path: &Path, kind: TodoKind) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent dir"))?;
    match kind {
        // completed/{file} -> pending/{file}; already-pending files stay put.
        TodoKind::Todo => {
            if parent.file_name().and_then(|n| n.to_str()) == Some("pending") {
                Ok(path.to_path_buf())
            } else {
                Ok(parent
                    .parent()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no todos dir"))?
                    .join("pending")
                    .join(file_name))
            }
        }
        // resolved/{file} -> debug/{file}; already-active files stay put.
        TodoKind::Debug => {
            if parent.file_name().and_then(|n| n.to_str()) == Some("resolved") {
                Ok(parent
                    .parent()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no debug dir"))?
                    .join(file_name))
            } else {
                Ok(path.to_path_buf())
            }
        }
    }
}

fn complete_todo(path: &Path, kind: TodoKind) -> io::Result<PathBuf> {
    let dest = done_destination(path, kind)?;
    if dest == path {
        return Ok(dest);
    }
    let body = fs::read_to_string(path)?;
    let stamped = upsert_frontmatter_key(&body, "completed", &today_string());
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, stamped)?;
    fs::remove_file(path)?;
    Ok(dest)
}

fn reopen_todo(path: &Path, kind: TodoKind) -> io::Result<PathBuf> {
    let dest = pending_destination(path, kind)?;
    if dest == path {
        return Ok(dest);
    }
    let body = fs::read_to_string(path)?;
    let reverted = remove_frontmatter_key(&body, "completed");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, reverted)?;
    fs::remove_file(path)?;
    Ok(dest)
}

// ───────────────────────────────────────────────────────────────── phase ──

/// Rewrite the `## Phases` index line for `phase_id` in ROADMAP.md, replacing
/// only its 3-character bracket mark and leaving the rest of the line — and
/// every other line in the file — byte-identical. Errors if `phase_id` has no
/// row in the index; this never silently no-ops.
fn apply_phase(planning: &Path, phase_id: &str, choice: StatusChoice) -> io::Result<PathBuf> {
    let mark = match choice {
        StatusChoice::PhaseVerified => "[x]",
        StatusChoice::PhaseOpen => "[ ]",
        StatusChoice::PhaseAbandoned => "[~]",
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{other:?} is not a phase choice"),
            ));
        }
    };

    let path = planning.join("ROADMAP.md");
    let body = fs::read_to_string(&path)?;
    let target_key = crate::planning::normalize_phase_id(phase_id);

    let mut in_phases = false;
    let mut found = false;
    let mut new_lines: Vec<String> = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## ") {
            in_phases = trimmed.eq_ignore_ascii_case("## Phases");
            new_lines.push(line.to_string());
            continue;
        }
        if !found && in_phases {
            if let Some((id, _, _)) = crate::planning::parse_phase_index_line(trimmed) {
                if crate::planning::normalize_phase_id(&id) == target_key {
                    found = true;
                    new_lines.push(replace_phase_mark(line, mark));
                    continue;
                }
            }
        }
        new_lines.push(line.to_string());
    }

    if !found {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("phase {phase_id:?} not found in ROADMAP.md's ## Phases index"),
        ));
    }

    let mut out = new_lines.join("\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&path, out)?;
    Ok(path)
}

/// Splice a new 3-character bracket mark into a `## Phases` index line,
/// leaving everything else — including leading whitespace and trailing
/// content — untouched. `line` is expected to match `- [x] **Phase N: ...**`
/// (any of the three marks) once leading whitespace is skipped.
fn replace_phase_mark(line: &str, mark: &str) -> String {
    let leading_ws = line.len() - line.trim_start().len();
    let mark_start = leading_ws + 2; // past "- "
    let mut out = String::with_capacity(line.len());
    out.push_str(&line[..mark_start]);
    out.push_str(mark);
    out.push_str(&line[mark_start + 3..]);
    out
}

// ─────────────────────────────────────────────────────────── quick task ──

/// Upsert or remove `task_id`'s row in STATE.md's `Quick Tasks Completed`
/// table. Columns are resolved by fuzzy header name via
/// `planning::quick_task_columns` (shared with the reader), so this never
/// assumes Status/Directory sit at a fixed position. Errors if the heading or
/// its table is missing; never silently no-ops on a malformed workspace.
fn apply_quick_task(planning: &Path, task_id: &str, choice: StatusChoice) -> io::Result<PathBuf> {
    let path = planning.join("STATE.md");
    let body = fs::read_to_string(&path)?;
    let lines: Vec<&str> = body.lines().collect();

    let mut heading_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let heading_text = trimmed.trim_start_matches('#').trim();
            if heading_text.eq_ignore_ascii_case("Quick Tasks Completed") {
                heading_idx = Some(i);
                break;
            }
        }
    }
    let Some(heading_idx) = heading_idx else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no 'Quick Tasks Completed' heading in STATE.md",
        ));
    };

    let mut idx = heading_idx + 1;
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    let table_start = idx;
    let mut table_end = table_start;
    while table_end < lines.len() && lines[table_end].trim_start().starts_with('|') {
        table_end += 1;
    }
    if table_start == table_end {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no table found under 'Quick Tasks Completed' heading in STATE.md",
        ));
    }

    let header_cells = crate::planning::split_table_row(lines[table_start]);
    let (id_col, status_col, directory_col) = crate::planning::quick_task_columns(&header_cells);
    let width = header_cells.len();

    let mut existing_row_idx: Option<usize> = None;
    for (i, line) in lines
        .iter()
        .enumerate()
        .take(table_end)
        .skip(table_start + 1)
    {
        let cells = crate::planning::split_table_row(line);
        let is_separator = cells
            .iter()
            .all(|c| !c.trim().is_empty() && c.trim().chars().all(|ch| ch == '-' || ch == ':'));
        if is_separator {
            continue;
        }
        if cells.get(id_col).map(|c| c.trim()) == Some(task_id) {
            existing_row_idx = Some(i);
            break;
        }
    }

    let mut out_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    match choice {
        StatusChoice::QuickTaskInProgress => {
            if let Some(row_idx) = existing_row_idx {
                out_lines.remove(row_idx);
            }
        }
        StatusChoice::QuickTaskCompleted | StatusChoice::QuickTaskFailed => {
            let status_value = choice.label().to_string();
            if let Some(row_idx) = existing_row_idx {
                let mut cells = crate::planning::split_table_row(&out_lines[row_idx]);
                if let Some(sc) = status_col {
                    if sc < cells.len() {
                        cells[sc] = status_value;
                    }
                }
                out_lines[row_idx] = render_table_row(&cells);
            } else {
                let mut cells = vec![String::new(); width];
                if id_col < width {
                    cells[id_col] = task_id.to_string();
                }
                if let Some(sc) = status_col {
                    cells[sc] = status_value;
                }
                if let Some(dc) = directory_col {
                    cells[dc] = resolve_quick_task_dir_name(planning, task_id)
                        .map(|d| format!("`{d}`"))
                        .unwrap_or_default();
                }
                if let Some(tc) = crate::planning::quick_task_description_column(&header_cells) {
                    if tc < width {
                        cells[tc] = resolve_quick_task_title(planning, task_id).unwrap_or_default();
                    }
                }
                out_lines.insert(table_end, render_table_row(&cells));
            }
        }
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{other:?} is not a quick-task choice"),
            ));
        }
    }

    let mut out = out_lines.join("\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&path, out)?;
    Ok(path)
}

/// Render a rectangular table row in the `| a | b | c |` convention every
/// fixture in this file uses (leading and trailing pipe).
fn render_table_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

/// Find `task_id`'s directory under `.planning/quick/`, by exact match or
/// `"{task_id}-"` prefix — shared by [`resolve_quick_task_dir_name`] and
/// [`resolve_quick_task_title`] so both agree on which directory a task id
/// resolves to.
fn find_quick_task_dir(planning: &Path, task_id: &str) -> Option<PathBuf> {
    let quick_dir = planning.join("quick");
    let entries = fs::read_dir(&quick_dir).ok()?;
    let prefix = format!("{task_id}-");
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name()?.to_str()?.to_string();
        if name == task_id || name.starts_with(&prefix) {
            return Some(path);
        }
    }
    None
}

/// `task_id`'s directory under `.planning/quick/`, for filling a new row's
/// Directory column. Returns a `.planning`-relative path (mirroring the
/// convention in existing rows), not `planning`'s own (possibly
/// differently-named) absolute path.
fn resolve_quick_task_dir_name(planning: &Path, task_id: &str) -> Option<String> {
    let path = find_quick_task_dir(planning, task_id)?;
    let name = path.file_name()?.to_str()?.to_string();
    Some(format!(".planning/quick/{name}"))
}

/// `task_id`'s human-readable title, read the same way `load_quick_tasks`
/// reads it (PLAN.md frontmatter `title:`, falling back to its first `# `
/// heading) — for backfilling a Quick Tasks table row's description column
/// when the row is being re-inserted after having been removed (D-06: a task
/// marked in-progress loses its row; re-completing it must not leave the
/// description blank forever).
fn resolve_quick_task_title(planning: &Path, task_id: &str) -> Option<String> {
    let dir = find_quick_task_dir(planning, task_id)?;
    crate::planning::parse_quick_task(&dir).map(|task| task.title)
}

// ─────────────────────────────────────────────────────────────── other ──

/// Toggle a note/idea/seed capture's `status:` frontmatter key in place.
/// Marking active removes the key (dropping the whole frontmatter block only
/// if this write is what added it); marking complete upserts it — reusing the
/// same surgery `apply_todo`'s `completed` stamp uses, so every other key and
/// the body are preserved byte-for-byte. Read back by
/// `planning::parse_other`, which treats absent/unrecognized values as active.
fn apply_other(path: &Path, choice: StatusChoice) -> io::Result<PathBuf> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no file at {}", path.display()),
        ));
    }
    let body = fs::read_to_string(path)?;
    let updated = match choice {
        StatusChoice::OtherComplete => upsert_frontmatter_key(&body, "status", "done"),
        StatusChoice::OtherActive => remove_frontmatter_key(&body, "status"),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{other:?} is not an other-capture choice"),
            ));
        }
    };
    fs::write(path, updated)?;
    Ok(path.to_path_buf())
}

// ─────────────────────────────────────────────────── frontmatter surgery ──

/// The byte range of a body's frontmatter *interior* (the text strictly
/// between the opening `---\n` and the closing `\n---\n`), or `None` when the
/// body doesn't open with a frontmatter delimiter — mirroring
/// `planning::split_frontmatter`'s "body must start with `---\n`" rule so a
/// stamp this module writes is always visible to that reader.
fn frontmatter_range(body: &str) -> Option<(usize, usize)> {
    let after_open = body.strip_prefix("---\n")?;
    let end = after_open.find("\n---\n")?;
    let start = body.len() - after_open.len();
    Some((start, start + end))
}

/// Insert or update a top-level `key: value` line inside an existing
/// frontmatter block (at the top of the block, so it reads like GSD's own
/// `completed: {date}` stamp), or open a new frontmatter block containing
/// just that key when the body has none. Never places the key above the
/// opening `---` — a stamp placed there empties the frontmatter for every
/// reader that requires the body to start with the delimiter.
fn upsert_frontmatter_key(body: &str, key: &str, value: &str) -> String {
    let Some((start, end)) = frontmatter_range(body) else {
        return format!("---\n{key}: {value}\n---\n{body}");
    };
    let interior = &body[start..end];
    let prefix = format!("{key}:");
    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in interior.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented && line.trim_start().starts_with(&prefix) {
            lines.push(format!("{key}: {value}"));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.insert(0, format!("{key}: {value}"));
    }
    let new_interior = lines.join("\n");
    format!("{}{}{}", &body[..start], new_interior, &body[end..])
}

/// Remove a top-level `key:` line from an existing frontmatter block. If
/// nothing else is left in the block, drops the whole frontmatter (both
/// delimiters) rather than leaving an empty `---\n---\n` — so a key this
/// module added is also the block it added, and reversing is a clean no-op.
fn remove_frontmatter_key(body: &str, key: &str) -> String {
    let Some((start, end)) = frontmatter_range(body) else {
        return body.to_string();
    };
    let interior = &body[start..end];
    let prefix = format!("{key}:");
    let mut lines: Vec<String> = Vec::new();
    for line in interior.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented && line.trim_start().starts_with(&prefix) {
            continue;
        }
        lines.push(line.to_string());
    }
    let new_interior = lines.join("\n");
    if new_interior.trim().is_empty() {
        let suffix = &body[end..];
        let suffix = suffix.strip_prefix("\n---\n").unwrap_or(suffix);
        return suffix.to_string();
    }
    format!("{}{}{}", &body[..start], new_interior, &body[end..])
}

// ──────────────────────────────────────────────────────────── civil date ──

/// Today's date as `YYYY-MM-DD`, derived from `SystemTime` with no date
/// dependency (the crate has none).
fn today_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert a day count since the Unix epoch (1970-01-01) into a civil
/// (year, month, day), via Howard Hinnant's `civil_from_days` algorithm.
/// Kept local instead of adding a date crate for one conversion; its
/// arithmetic is non-obvious enough to earn the round-trip test below.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Inverse of [`civil_from_days`] — test-only, so the round trip can be
/// asserted without hand-computing epoch day counts.
#[cfg(test)]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

// Path-included at the file's own top level (not nested inside `mod tests`)
// so its base directory resolves to `src/` — the real directory containing
// this file. Nesting it inside `mod tests` instead makes rustc compute a
// virtual base of `src/status_edit/tests/`, which does not exist on disk
// (`status_edit.rs` is a file, not a directory), so no relative path from
// there can ever resolve. This is the same support module
// `tests/gsd_conformance.rs` path-includes, so the oracle-resolution and
// JSON-parsing logic never drifts between the black-box and white-box sides.
#[cfg(test)]
#[path = "../tests/gsd_conformance_support.rs"]
mod gsd_conformance_support;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel: &str, content: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn civil_date_round_trips_across_a_range_of_dates() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2024, 12, 31),
            (2026, 9, 2),
            (1999, 1, 1),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(
                civil_from_days(days),
                (y, m, d),
                "round trip for {y}-{m}-{d}"
            );
        }
    }

    #[test]
    fn todo_pending_to_complete_moves_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "todos/pending/2026-01-01-x.md",
            "---\ntitle: X\n---\n\n# X\n",
        );
        let target = StatusTarget::Todo {
            path: path.clone(),
            kind: TodoKind::Todo,
        };
        let dest = apply(&target, StatusChoice::TodoComplete).expect("apply");
        assert!(!path.exists(), "pending file should be gone");
        assert_eq!(dest, tmp.path().join("todos/completed/2026-01-01-x.md"));
        assert!(dest.exists());
    }

    #[test]
    fn completing_a_todo_stamps_the_date_inside_the_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "todos/pending/2026-01-01-x.md",
            "---\ntitle: X\n---\n\n# X\n",
        );
        let target = StatusTarget::Todo {
            path,
            kind: TodoKind::Todo,
        };
        let dest = apply(&target, StatusChoice::TodoComplete).expect("apply");
        let body = fs::read_to_string(&dest).unwrap();
        assert!(
            body.starts_with("---\n"),
            "must still open with ---: {body}"
        );
        assert!(body.contains("completed: "), "missing stamp: {body}");
        let parsed = crate::planning::parse_todo(&dest, true).map(|t| t.title);
        assert_eq!(parsed.as_deref(), Some("X"));
    }

    #[test]
    fn todo_complete_to_pending_moves_it_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "todos/completed/2026-01-01-x.md",
            "completed: 2026-02-02\n---\ntitle: X\n---\n\n# X\n",
        );
        let target = StatusTarget::Todo {
            path: path.clone(),
            kind: TodoKind::Todo,
        };
        let dest = apply(&target, StatusChoice::TodoPending).expect("apply");
        assert!(!path.exists());
        assert_eq!(dest, tmp.path().join("todos/pending/2026-01-01-x.md"));
        assert!(fs::read_to_string(&dest).is_ok());
    }

    #[test]
    fn a_debug_session_uses_the_debug_resolved_pair() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), "debug/some-bug.md", "---\ntrigger: bug\n---\n");
        let target = StatusTarget::Todo {
            path: path.clone(),
            kind: TodoKind::Debug,
        };
        let dest = apply(&target, StatusChoice::TodoComplete).expect("apply");
        assert!(!path.exists());
        assert_eq!(dest, tmp.path().join("debug/resolved/some-bug.md"));
        assert!(
            !dest.starts_with(tmp.path().join("todos")),
            "must not land in todos/completed"
        );
    }

    #[test]
    fn applying_a_status_to_a_missing_file_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let target = StatusTarget::Todo {
            path: tmp.path().join("todos/pending/nope.md"),
            kind: TodoKind::Todo,
        };
        assert!(apply(&target, StatusChoice::TodoComplete).is_err());
    }

    // ──────────────────────────────────────────────────── phase writer ──

    const ROADMAP_TWO_PHASES: &str = "\
# ROADMAP: Sample

## Phases

- [ ] **Phase 1: Navigation Skeleton**
- [ ] **Phase 2: Coffee Acquisition**
";

    fn phase_target(planning: &Path, phase_id: &str) -> StatusTarget {
        StatusTarget::Phase {
            planning: planning.to_path_buf(),
            phase_id: phase_id.to_string(),
        }
    }

    #[test]
    fn setting_a_phase_verified_checks_its_roadmap_box() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "ROADMAP.md", ROADMAP_TWO_PHASES);
        let target = phase_target(tmp.path(), "2");
        apply(&target, StatusChoice::PhaseVerified).expect("apply");
        let phases = crate::planning::load_phases(tmp.path());
        let phase2 = phases.iter().find(|p| p.id == "2").expect("phase 2");
        assert_eq!(phase2.stage, crate::model::Stage::Verified);
    }

    #[test]
    fn setting_a_phase_abandoned_uses_the_tilde_mark() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "ROADMAP.md", ROADMAP_TWO_PHASES);
        let target = phase_target(tmp.path(), "2");
        apply(&target, StatusChoice::PhaseAbandoned).expect("apply");
        let body = fs::read_to_string(tmp.path().join("ROADMAP.md")).unwrap();
        assert!(
            body.contains("- [~] **Phase 2: Coffee Acquisition**"),
            "expected tilde mark: {body}"
        );
        let phases = crate::planning::load_phases(tmp.path());
        let phase2 = phases.iter().find(|p| p.id == "2").expect("phase 2");
        assert_eq!(phase2.stage, crate::model::Stage::Abandoned);
    }

    #[test]
    fn reopening_a_phase_returns_it_to_the_disk_inferred_stage() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "ROADMAP.md",
            "\
# ROADMAP: Sample

## Phases

- [x] **Phase 1: Navigation Skeleton**
",
        );
        // A phase dir with a plan and a matching summary: infer_stage would
        // call this Executed once the roadmap checkbox stops overriding it.
        write(tmp.path(), "phases/01-nav/01-01-PLAN.md", "plan");
        write(tmp.path(), "phases/01-nav/01-01-SUMMARY.md", "summary");

        let target = phase_target(tmp.path(), "1");
        apply(&target, StatusChoice::PhaseOpen).expect("apply");
        let phases = crate::planning::load_phases(tmp.path());
        let phase1 = phases.iter().find(|p| p.id == "1").expect("phase 1");
        assert_eq!(phase1.stage, crate::model::Stage::Executed);
    }

    #[test]
    fn writing_one_phase_leaves_every_other_roadmap_line_byte_identical() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "ROADMAP.md", ROADMAP_TWO_PHASES);
        let target = phase_target(tmp.path(), "2");
        apply(&target, StatusChoice::PhaseVerified).expect("apply");
        let body = fs::read_to_string(tmp.path().join("ROADMAP.md")).unwrap();
        let before: Vec<&str> = ROADMAP_TWO_PHASES.lines().collect();
        let after: Vec<&str> = body.lines().collect();
        assert_eq!(before.len(), after.len(), "line count must not change");
        for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
            if i == 5 {
                // the Phase 2 line: only this one may change.
                assert_ne!(b, a, "phase 2's line should have changed");
                continue;
            }
            assert_eq!(b, a, "line {i} changed unexpectedly: {b:?} -> {a:?}");
        }
    }

    #[test]
    fn a_phase_absent_from_the_roadmap_index_is_an_error_not_a_silent_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "ROADMAP.md", ROADMAP_TWO_PHASES);
        let target = phase_target(tmp.path(), "99");
        assert!(apply(&target, StatusChoice::PhaseVerified).is_err());
    }

    // ────────────────────────────────────────────────── quick task writer ──

    const STATE_SIX_COLUMN: &str = "\
# STATE

## Quick Tasks Completed

| # | Description | Date | Commit | Status | Directory |
|---|---|---|---|---|---|
| 260101-abc | An older task | 2026-01-01 | abc123 | complete | `.planning/quick/260101-abc-an-older-task` |
";

    const STATE_FOUR_COLUMN: &str = "\
# STATE

## Quick Tasks Completed

| id | task | status | directory |
|---|---|---|---|
| 260101-abc | An older task | complete | `.planning/quick/260101-abc-an-older-task` |
";

    fn quick_task_target(planning: &Path, task_id: &str) -> StatusTarget {
        StatusTarget::QuickTask {
            planning: planning.to_path_buf(),
            task_id: task_id.to_string(),
        }
    }

    fn setup_quick_task_workspace(state_body: &str, task_id: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "STATE.md", state_body);
        write(
            tmp.path(),
            &format!("quick/{task_id}-brand-new-task/{task_id}-PLAN.md"),
            "# New task\n",
        );
        tmp
    }

    #[test]
    fn marking_a_task_complete_adds_a_row_whose_status_is_passing() {
        for state_body in [STATE_SIX_COLUMN, STATE_FOUR_COLUMN] {
            let tmp = setup_quick_task_workspace(state_body, "260902-new");
            let target = quick_task_target(tmp.path(), "260902-new");
            apply(&target, StatusChoice::QuickTaskCompleted).expect("apply");
            let tasks = crate::planning::load_quick_tasks(tmp.path(), true);
            let task = tasks
                .iter()
                .find(|t| t.id == "260902-new")
                .expect("new task");
            assert_eq!(task.status, crate::model::QuickTaskStatus::Completed);
        }
    }

    #[test]
    fn marking_a_task_failed_writes_a_non_passing_status() {
        for state_body in [STATE_SIX_COLUMN, STATE_FOUR_COLUMN] {
            let tmp = setup_quick_task_workspace(state_body, "260902-new");
            let target = quick_task_target(tmp.path(), "260902-new");
            apply(&target, StatusChoice::QuickTaskFailed).expect("apply");
            let tasks = crate::planning::load_quick_tasks(tmp.path(), false);
            let task = tasks
                .iter()
                .find(|t| t.id == "260902-new")
                .expect("new task");
            match &task.status {
                crate::model::QuickTaskStatus::Failed(raw) => {
                    assert!(!raw.trim().is_empty());
                }
                other => panic!("expected Failed, got {other:?}"),
            }
        }
    }

    #[test]
    fn marking_a_task_in_progress_removes_its_row() {
        for state_body in [STATE_SIX_COLUMN, STATE_FOUR_COLUMN] {
            let tmp = tempfile::tempdir().unwrap();
            write(tmp.path(), "STATE.md", state_body);
            write(
                tmp.path(),
                "quick/260101-abc-an-older-task/260101-abc-PLAN.md",
                "# Old task\n",
            );
            let target = quick_task_target(tmp.path(), "260101-abc");
            apply(&target, StatusChoice::QuickTaskInProgress).expect("apply");
            let tasks = crate::planning::load_quick_tasks(tmp.path(), false);
            let task = tasks
                .iter()
                .find(|t| t.id == "260101-abc")
                .expect("the task still lists (in progress, unfiltered)");
            assert_eq!(task.status, crate::model::QuickTaskStatus::InProgress);
        }
    }

    #[test]
    fn the_status_cell_lands_in_the_column_named_by_the_header() {
        // The 6-column sample table: Status is column 4, Directory is column 5.
        // A positional writer (assuming Status is last) corrupts the Directory
        // cell; this test only passes when columns are resolved by header name.
        let tmp = setup_quick_task_workspace(STATE_SIX_COLUMN, "260902-new");
        let target = quick_task_target(tmp.path(), "260902-new");
        apply(&target, StatusChoice::QuickTaskCompleted).expect("apply");
        let body = fs::read_to_string(tmp.path().join("STATE.md")).unwrap();
        let new_row = body
            .lines()
            .find(|l| l.contains("260902-new"))
            .expect("new row");
        let cells = crate::planning::split_table_row(new_row);
        assert_eq!(
            cells[4].trim(),
            "complete",
            "status in Status column: {cells:?}"
        );
        assert!(
            cells[5].contains("quick"),
            "directory column still holds a directory: {cells:?}"
        );
    }

    #[test]
    fn an_existing_row_is_updated_rather_than_duplicated() {
        for state_body in [STATE_SIX_COLUMN, STATE_FOUR_COLUMN] {
            let tmp = tempfile::tempdir().unwrap();
            write(tmp.path(), "STATE.md", state_body);
            write(
                tmp.path(),
                "quick/260101-abc-an-older-task/260101-abc-PLAN.md",
                "# Old task\n",
            );
            let target = quick_task_target(tmp.path(), "260101-abc");
            apply(&target, StatusChoice::QuickTaskFailed).expect("apply");
            let body = fs::read_to_string(tmp.path().join("STATE.md")).unwrap();
            // The fixture's Directory cell embeds the task id as a substring
            // of the directory name, so a correctly-updated single row always
            // shows the id twice (id cell + directory cell) — duplicating
            // the row would show it four times.
            let occurrences = body.matches("260101-abc").count();
            assert_eq!(
                occurrences, 2,
                "row updated in place, not duplicated: {body}"
            );
        }
    }

    #[test]
    fn recompleting_after_in_progress_restores_the_task_description() {
        // Reproduces a real data-loss report: marking a completed task
        // in-progress removes its row (by design — the table only lists
        // completed work); marking it complete again must re-insert a row
        // whose description is resolved from the task's own PLAN.md title,
        // not left permanently blank.
        for state_body in [STATE_SIX_COLUMN, STATE_FOUR_COLUMN] {
            let tmp = tempfile::tempdir().unwrap();
            write(tmp.path(), "STATE.md", state_body);
            write(
                tmp.path(),
                "quick/260101-abc-an-older-task/260101-abc-PLAN.md",
                "# An older task\n",
            );
            let target = quick_task_target(tmp.path(), "260101-abc");
            apply(&target, StatusChoice::QuickTaskInProgress).expect("apply in-progress");
            apply(&target, StatusChoice::QuickTaskCompleted).expect("apply complete");

            let body = fs::read_to_string(tmp.path().join("STATE.md")).unwrap();
            let row = body
                .lines()
                .find(|l| l.contains("260101-abc"))
                .expect("row still present after re-completing");
            assert!(
                row.contains("An older task"),
                "description should be restored from the task's PLAN.md title, got: {row}"
            );
        }
    }

    #[test]
    fn marking_a_note_complete_writes_the_status_key_into_its_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "notes/2026-07-10-espresso.md",
            "---\ntitle: Espresso timing\n---\nbody\n",
        );
        let target = StatusTarget::Other { path: path.clone() };

        apply(&target, StatusChoice::OtherComplete).expect("apply");

        let written = fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("status: done"),
            "status key written:\n{written}"
        );
        let other = crate::planning::load_others(tmp.path(), true)
            .into_iter()
            .find(|o| o.path == path)
            .expect("note still parses");
        assert!(other.completed, "parse_other reports it completed");
    }

    #[test]
    fn marking_a_note_active_removes_the_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "notes/2026-07-10-espresso.md",
            "---\ntitle: Espresso timing\nstatus: done\n---\nbody\n",
        );
        let target = StatusTarget::Other { path: path.clone() };

        apply(&target, StatusChoice::OtherActive).expect("apply");

        let written = fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("status:"),
            "status key removed:\n{written}"
        );
        let others = crate::planning::load_others(tmp.path(), false);
        assert_eq!(
            others.len(),
            1,
            "shows again with the toggle off:\n{others:?}"
        );
    }

    #[test]
    fn a_note_with_no_frontmatter_block_gains_one() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "notes/grinder.md",
            "# Grinder calibration\n\nnotes\n",
        );
        let target = StatusTarget::Other { path: path.clone() };

        apply(&target, StatusChoice::OtherComplete).expect("apply");

        let written = fs::read_to_string(&path).unwrap();
        assert!(
            written.starts_with("---\n"),
            "a frontmatter block was added:\n{written}"
        );
        assert!(written.contains("status: done"));
        let other = crate::planning::load_others(tmp.path(), true)
            .into_iter()
            .find(|o| o.path == path)
            .expect("its title still parses");
        assert_eq!(other.title, "Grinder calibration");
    }

    #[test]
    fn marking_a_note_preserves_its_other_frontmatter_keys_and_its_body() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "notes/2026-07-10-espresso.md",
            "---\ntitle: Espresso timing\ntags: coffee, brewing\n---\n\n# Espresso timing\n\nSome body text.\n",
        );
        let target = StatusTarget::Other { path: path.clone() };

        apply(&target, StatusChoice::OtherComplete).expect("apply");
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("title: Espresso timing"), "{written}");
        assert!(written.contains("tags: coffee, brewing"), "{written}");
        assert!(written.contains("# Espresso timing"), "{written}");
        assert!(written.contains("Some body text."), "{written}");

        apply(&target, StatusChoice::OtherActive).expect("apply");
        let reverted = fs::read_to_string(&path).unwrap();
        assert!(!reverted.contains("status:"), "{reverted}");
        assert!(reverted.contains("title: Espresso timing"), "{reverted}");
        assert!(reverted.contains("tags: coffee, brewing"), "{reverted}");
        assert!(reverted.contains("# Espresso timing"), "{reverted}");
        assert!(reverted.contains("Some body text."), "{reverted}");
    }

    fn copy_sample(dst: &Path) {
        fn copy_dir(src: &Path, dst: &Path) {
            fs::create_dir_all(dst).unwrap();
            for entry in fs::read_dir(src).unwrap() {
                let entry = entry.unwrap();
                let from = entry.path();
                let to = dst.join(entry.file_name());
                if from.is_dir() {
                    copy_dir(&from, &to);
                } else {
                    fs::copy(&from, &to).unwrap();
                }
            }
        }
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("sample");
        copy_dir(&sample, dst);
    }

    #[test]
    fn our_writer_leaves_a_workspace_gsd_tools_can_still_parse() {
        let Some(tools) = gsd_conformance_support::oracle_available() else {
            return;
        };
        let ws = tempfile::tempdir().unwrap();
        copy_sample(ws.path());
        let planning = ws.path().join(".planning");

        let before = gsd_conformance_support::run_progress(&tools, ws.path())
            .expect("oracle ran before the write");
        let before_numbers = gsd_conformance_support::phase_numbers(&before);

        let note_path = planning.join("notes/2026-07-08-grinder-timing.md");
        let target = StatusTarget::Other {
            path: note_path.clone(),
        };
        apply(&target, StatusChoice::OtherComplete).expect("apply the real writer");

        let after = gsd_conformance_support::run_progress(&tools, ws.path())
            .expect("oracle ran after the write");
        assert!(
            gsd_conformance_support::phase_scope_is_readable(&after),
            "a real status_edit::apply write must leave the workspace readable:\n{after}"
        );
        assert_eq!(
            before_numbers,
            gsd_conformance_support::phase_numbers(&after),
            "phase parsing must be unaffected by our writer"
        );

        let others = crate::planning::load_others(&planning, true);
        let note = others
            .iter()
            .find(|o| o.path == note_path)
            .expect("the note still parses");
        assert!(note.completed, "and our own parser agrees it's completed");
    }
}
