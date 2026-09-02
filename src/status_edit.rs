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
    Todo { path: PathBuf, kind: TodoKind },
}

/// A choice offered in the status dialog. Which choices are offered for a
/// given [`StatusTarget`] comes from [`StatusTarget::choices`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusChoice {
    TodoPending,
    TodoComplete,
}

impl StatusChoice {
    pub(crate) fn label(self) -> &'static str {
        match self {
            StatusChoice::TodoPending => "pending",
            StatusChoice::TodoComplete => "complete",
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

/// Write `choice` for `target`, returning the file's new path.
pub(crate) fn apply(target: &StatusTarget, choice: StatusChoice) -> io::Result<PathBuf> {
    match target {
        StatusTarget::Todo { path, kind } => apply_todo(path, *kind, choice),
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
}
