//! GSD workstream resolution (`.planning/workstreams/<name>/`).
//!
//! Mirrors gsd-core's workstream contract (`workstream.cts`,
//! `planning-workspace.cts`, `active-workstream-store.cts`,
//! `workstream-name-policy.cts`, v1.14.0): flat mode has no
//! `.planning/workstreams/` and everything lives at `.planning/` root;
//! workstream mode moves only `ROADMAP.md`, `STATE.md`, `REQUIREMENTS.md`, and
//! `phases/` into `.planning/workstreams/<name>/`.
//!
//! D2: this module never writes `.planning/active-workstream`, which GSD
//! owns — it only ever reads. Switching workstreams in the TUI records the
//! choice only in gsd-status's own `.planning/gsd-status/gsd-state-<ppid>.json`
//! (see `session_state`), keyed by the launching shell's PID, and that choice
//! is read back here as the tier after `GSD_WORKSTREAM`.
//!
//! This module deliberately does NOT read GSD's session-scoped pointer under
//! `${TMPDIR}/gsd-workstream-sessions/`: a standalone TUI process computes a
//! different session key than the agent process that wrote that pointer, so
//! reading it here could never match and would be dead code.
use std::fs;
use std::path::{Path, PathBuf};

use crate::session_state::Remembered;

/// Label for the synthetic "workspace root" entry in the `S` picker.
///
/// Deliberately parenthesised: `is_valid_name` rejects `(` and `)`, so this
/// can never collide with a real `.planning/workstreams/<name>/` directory,
/// and no name-validity check has to special-case it.
pub(crate) const BASE_LABEL: &str = "(base)";

/// Does the workspace root carry workstream-shaped content of its own?
///
/// True only for a partially-migrated workspace — one where `ROADMAP.md`,
/// `STATE.md`, `REQUIREMENTS.md`, or `phases/` still sit at `.planning/`
/// alongside `workstreams/`. Those files are strictly scoped, so while a
/// workstream is focused nothing can reach them; the picker offers a base
/// entry exactly when there is something there to reach.
///
/// False for a cleanly-migrated workspace, whose root holds only shared
/// content (`todos/`, `notes/`, `research/`, `PROJECT.md`) that every
/// workstream view already federates in — a base entry there would open an
/// empty panel.
pub(crate) fn base_has_content(root: &Path) -> bool {
    ["ROADMAP.md", "STATE.md", "REQUIREMENTS.md"]
        .iter()
        .any(|f| root.join(f).is_file())
        || root.join("phases").is_dir()
}

/// Footer the picker shows when the root carries a leftover `state.json`.
pub(crate) const STALE_STATE_JSON: &str = "\u{26a0} stale .planning/state.json (no root STATE.md)";

/// True when workstreams exist and the root holds a `state.json` but none of
/// the root `STATE.md`/`ROADMAP.md`/`phases/` that would make it live (D-12).
pub(crate) fn root_state_json_is_stale(_root: &Path) -> bool {
    false
}

/// GSD's workstream name policy: `[A-Za-z0-9._-]` only, non-empty, no path
/// separators, no `..`.
pub(crate) fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Sorted, valid directory names under `.planning/workstreams/`. Empty vec
/// when that directory is absent. Skips non-directories and invalid names.
pub(crate) fn list(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join("workstreams")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|name| is_valid_name(name))
        .collect();
    names.sort();
    names
}

/// A candidate resolves only when it is format-valid AND names a workstream
/// directory that actually exists — mirroring gsd-core's
/// `resolvesToExistingWorkstream`.
fn resolves(names: &[String], candidate: &str) -> bool {
    is_valid_name(candidate) && names.iter().any(|n| n == candidate)
}

/// Resolve the focused workstream name. Precedence: `cli` > `env` > this
/// shell's remembered `S` choice (`session`, see `session_state`) > the
/// `.planning/active-workstream` pointer file > the first name in sorted
/// order. Returns `None` when `.planning/workstreams/` is absent or empty
/// (flat mode), or when `session` remembers the `(base)` view and the base
/// has content of its own. An invalid or missing-directory
/// `env`/session/pointer-file value, or a remembered base with nothing in
/// it, falls through to the next source instead of resolving; an invalid or
/// missing-directory `cli` value does NOT fall through — it resolves to
/// `None` so the caller can treat an explicit `--ws` typo as fatal.
pub(crate) fn resolve(
    root: &Path,
    cli: Option<&str>,
    env: Option<&str>,
    session: Option<&Remembered>,
) -> Option<String> {
    let names = list(root);
    if names.is_empty() {
        return None;
    }

    if let Some(name) = cli {
        return if resolves(&names, name) {
            Some(name.to_string())
        } else {
            None
        };
    }

    if let Some(name) = env {
        if resolves(&names, name) {
            return Some(name.to_string());
        }
    }

    // This shell's last `S` choice (gsd-status's own state, never GSD's).
    match session {
        Some(Remembered::Workstream(name)) if resolves(&names, name) => {
            return Some(name.clone());
        }
        Some(Remembered::Base) if base_has_content(root) => return None,
        _ => {}
    }

    // D2: read-only. A missing pointer file is not an error — it just means
    // no workstream has been switched to yet.
    let pointer = fs::read_to_string(root.join("active-workstream")).unwrap_or_default();
    let pointer = pointer.trim();
    if !pointer.is_empty() && resolves(&names, pointer) {
        return Some(pointer.to_string());
    }

    names.into_iter().next()
}

/// The scoped `.planning`-relative directory for `ws`: `root/workstreams/<name>`
/// when `Some`, or `root` unchanged in flat mode (`None`).
pub(crate) fn scoped_dir(root: &Path, ws: Option<&str>) -> PathBuf {
    match ws {
        Some(name) => root.join("workstreams").join(name),
        None => root.to_path_buf(),
    }
}

/// Separator drawn between the open rows and the completed ones in
/// the `S` picker. Never selectable — it is a rendered rule, not an entry.
pub(crate) const COMPLETE_SEPARATOR: &str = "--- complete ---";

/// Open/complete verdict for a workstream (or root) directory, read from its
/// own `STATE.md` and, for a bare `complete`, from whether the milestone was
/// actually archived on disk.
///
/// Ambiguity always resolves to `Open`: reading live work as done hides it,
/// which is the expensive direction to be wrong in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Classification {
    /// Live work. `ready_to_close` is true when STATE.md says a bare
    /// `complete` but no `milestones/<milestone>-ROADMAP.md` proves the
    /// milestone was closed (gsd-core's `milestone complete` writes it).
    Open { ready_to_close: bool },
    /// Closed: `milestone complete` / `archived` status, or a bare
    /// `complete` backed by an archived `<milestone>-ROADMAP.md`.
    Complete,
}

/// First `key:` value in the leading frontmatter-ish lines of a STATE.md body.
fn frontmatter_value(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    body.lines()
        .take_while(|l| l.trim() != "---" || body.starts_with("---"))
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .map(|v| v.trim().to_string())
}

/// Classify a root or `root/workstreams/<name>` directory. Pure read: this
/// never writes anything (D2).
///
/// 1. STATE.md missing/unreadable or without `status:` -> open.
/// 2. status `archived`, or containing `milestone archived` /
///    `milestone complete` -> complete. Deliberately narrower than
///    gsd-core's word regexes so `not complete` stays open.
/// 3. status `complete`/`completed`: complete only when the STATE
///    `milestone:` is a valid name and `milestones/<milestone>-ROADMAP.md`
///    is a file; otherwise open and flagged ready to close.
/// 4. anything else -> open.
pub(crate) fn classify_dir(dir: &Path) -> Classification {
    const OPEN: Classification = Classification::Open {
        ready_to_close: false,
    };
    let Ok(body) = fs::read_to_string(dir.join("STATE.md")) else {
        return OPEN;
    };
    let Some(status) = frontmatter_value(&body, "status") else {
        return OPEN;
    };
    let status = status.to_ascii_lowercase();
    if status == "archived"
        || status.contains("milestone archived")
        || status.contains("milestone complete")
    {
        return Classification::Complete;
    }
    if matches!(status.as_str(), "complete" | "completed") {
        let archived = frontmatter_value(&body, "milestone")
            .filter(|m| m != "null" && is_valid_name(m))
            .is_some_and(|m| {
                dir.join("milestones")
                    .join(format!("{m}-ROADMAP.md"))
                    .is_file()
            });
        return if archived {
            Classification::Complete
        } else {
            Classification::Open {
                ready_to_close: true,
            }
        };
    }
    OPEN
}

/// Where one archived milestone's files live, so it can be opened read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchiveLayout {
    /// The dir the event loop's `planning` is re-scoped to.
    pub(crate) anchor: PathBuf,
    /// Banner path, relative to `.planning`.
    pub(crate) display: String,
    pub(crate) state: Option<PathBuf>,
    pub(crate) roadmap: Option<PathBuf>,
    pub(crate) requirements: Option<PathBuf>,
    pub(crate) phases_dir: Option<PathBuf>,
    pub(crate) quick_dir: Option<PathBuf>,
}

/// One completed milestone: one flat record regardless of where it was stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchivedMilestone {
    pub(crate) workstream: String,
    pub(crate) version: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) layout: ArchiveLayout,
}

/// What the `S` picker hands back on Enter.
// Rows are short-lived UI values; boxing would only complicate the shared interface.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selection {
    Live(String),
    Archive(ArchivedMilestone),
}

/// One selectable row of the `S` picker.
// Rows are short-lived UI values; boxing would only complicate the shared interface.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerRow {
    Open { label: String, ready_to_close: bool },
    Completed(ArchivedMilestone),
}

impl PickerRow {
    pub(crate) fn label(&self) -> &str {
        match self {
            PickerRow::Open { label, .. } => label,
            PickerRow::Completed(m) => &m.workstream,
        }
    }
}

/// `ws-release-french-2026-10-15[-n]` -> ("release-french", "2026-10-15").
pub(crate) fn parse_ws_archive_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("ws-")?;
    let tokens: Vec<&str> = rest.split('-').collect();
    let digits = |t: &str, n: usize| t.len() == n && t.bytes().all(|b| b.is_ascii_digit());
    // The date is the last yyyy-mm-dd, optionally followed by one `-<n>`.
    (1..tokens.len().saturating_sub(2)).rev().find_map(|i| {
        let tail = &tokens[i + 3..];
        let tail_ok = tail.is_empty() || (tail.len() == 1 && digits(tail[0], tail[0].len()));
        (tail_ok && digits(tokens[i], 4) && digits(tokens[i + 1], 2) && digits(tokens[i + 2], 2))
            .then(|| {
                (
                    tokens[..i].join("-"),
                    format!("{}-{}-{}", tokens[i], tokens[i + 1], tokens[i + 2]),
                )
            })
    })
}

/// `v1.0-ROADMAP.md` -> "v1.0". Excludes `archived-*` and `ws-*`.
pub(crate) fn version_of_entry(name: &str) -> Option<String> {
    const SUFFIXES: [&str; 5] = [
        "-ROADMAP.md",
        "-REQUIREMENTS.md",
        "-MILESTONE-AUDIT.md",
        "-phases",
        "-quick",
    ];
    if name.starts_with("archived-") || name.starts_with("ws-") {
        return None;
    }
    let version = SUFFIXES.iter().find_map(|s| name.strip_suffix(s))?;
    let mut chars = version.chars();
    (chars.next() == Some('v') && chars.next().is_some_and(|c| c.is_ascii_digit()))
        .then(|| version.to_string())
}

/// `archived-20261006-phases` -> "2026-10-06".
pub(crate) fn parse_archived_phases_name(name: &str) -> Option<String> {
    let d = name.strip_prefix("archived-")?.strip_suffix("-phases")?;
    (d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]))
}

/// Modification time of `path` as `YYYY-MM-DD` (UTC).
pub(crate) fn mtime_date(path: &Path) -> Option<String> {
    let secs = fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let (y, m, d) = crate::status_edit::civil_from_days((secs / 86_400) as i64);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn existing(path: PathBuf) -> Option<PathBuf> {
    path.exists().then_some(path)
}

fn non_empty(path: &Path) -> bool {
    if path.is_dir() {
        fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
    } else {
        path.is_file()
    }
}

/// Valid-named entries of `dir` (sorted), so no unchecked name is ever joined.
fn valid_entries(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| is_valid_name(n))
        .collect();
    names.sort();
    names
}

fn live_layout(dir: &Path, display: String) -> ArchiveLayout {
    ArchiveLayout {
        display,
        state: existing(dir.join("STATE.md")),
        roadmap: existing(dir.join("ROADMAP.md")),
        requirements: existing(dir.join("REQUIREMENTS.md")),
        phases_dir: existing(dir.join("phases")),
        quick_dir: existing(dir.join("quick")),
        anchor: dir.to_path_buf(),
    }
}

/// Rows from one `milestones/` directory: `v<X>-*` entries grouped into one
/// row per version, and populated `archived-<date>-phases` dirs. `display`
/// is the directory's path relative to `.planning`.
fn scan_milestones_dir(
    mdir: &Path,
    display: &str,
    workstream: &str,
    names: &[String],
    out: &mut Vec<ArchivedMilestone>,
) {
    let mut versions: Vec<String> = names.iter().filter_map(|n| version_of_entry(n)).collect();
    versions.dedup(); // names are sorted, so equal versions are adjacent
    for version in versions {
        let members: Vec<PathBuf> = names
            .iter()
            .filter(|n| version_of_entry(n).as_deref() == Some(&version))
            .map(|n| mdir.join(n))
            .collect();
        if !members.iter().any(|m| non_empty(m)) {
            continue;
        }
        let roadmap = mdir.join(format!("{version}-ROADMAP.md"));
        let date =
            mtime_date(&roadmap).or_else(|| members.iter().filter_map(|m| mtime_date(m)).max());
        out.push(ArchivedMilestone {
            workstream: workstream.to_string(),
            layout: ArchiveLayout {
                anchor: mdir.to_path_buf(),
                display: format!("{display}/{version}"),
                state: None,
                roadmap: existing(roadmap),
                requirements: existing(mdir.join(format!("{version}-REQUIREMENTS.md"))),
                phases_dir: existing(mdir.join(format!("{version}-phases"))),
                quick_dir: existing(mdir.join(format!("{version}-quick"))),
            },
            version: Some(version),
            date,
        });
    }
    for name in names {
        let Some(date) = parse_archived_phases_name(name) else {
            continue;
        };
        let dir = mdir.join(name);
        if !dir.is_dir() || !non_empty(&dir) {
            continue; // empty = `phases clear` residue, not a completion
        }
        out.push(ArchivedMilestone {
            workstream: workstream.to_string(),
            version: None,
            date: Some(date),
            layout: ArchiveLayout {
                anchor: mdir.to_path_buf(),
                display: format!("{display}/{name}"),
                state: None,
                roadmap: None,
                requirements: None,
                phases_dir: Some(dir),
                quick_dir: None,
            },
        });
    }
}

/// Completed milestones from every place gsd-core archives them, one flat
/// record each, newest first. Pure read; scan depth is fixed.
pub(crate) fn discover_completed(root: &Path) -> Vec<ArchivedMilestone> {
    let mut found = Vec::new();

    // (a) root `milestones/`: whole-workstream archives and root versions.
    let root_ms = root.join("milestones");
    let names = valid_entries(&root_ms);
    for name in &names {
        let Some((workstream, date)) = parse_ws_archive_name(name) else {
            continue;
        };
        let dir = root_ms.join(name);
        if !dir.is_dir() || !is_valid_name(&workstream) {
            continue;
        }
        let version = fs::read_to_string(dir.join("STATE.md"))
            .ok()
            .and_then(|b| frontmatter_value(&b, "milestone"))
            .filter(|v| v != "null" && !v.is_empty());
        let display = format!("milestones/{name}");
        scan_milestones_dir(
            &dir.join("milestones"),
            &format!("{display}/milestones"),
            &workstream,
            &valid_entries(&dir.join("milestones")),
            &mut found,
        );
        found.push(ArchivedMilestone {
            workstream,
            version,
            date: Some(date),
            layout: live_layout(&dir, display),
        });
    }
    scan_milestones_dir(&root_ms, "milestones", BASE_LABEL, &names, &mut found);

    // (b) each live workstream's own `milestones/`.
    let live = list(root);
    for name in &live {
        let mdir = root.join("workstreams").join(name).join("milestones");
        scan_milestones_dir(
            &mdir,
            &format!("workstreams/{name}/milestones"),
            name,
            &valid_entries(&mdir),
            &mut found,
        );
    }

    // (c) completed live dirs that were never archived to `milestones/`.
    let mut live_dirs: Vec<(String, PathBuf, String)> = live
        .iter()
        .map(|n| {
            (
                n.clone(),
                root.join("workstreams").join(n),
                format!("workstreams/{n}"),
            )
        })
        .collect();
    if base_has_content(root) {
        live_dirs.insert(
            0,
            (
                BASE_LABEL.to_string(),
                root.to_path_buf(),
                ".planning".into(),
            ),
        );
    }
    for (workstream, dir, display) in live_dirs {
        if classify_dir(&dir) != Classification::Complete {
            continue;
        }
        let version = fs::read_to_string(dir.join("STATE.md"))
            .ok()
            .and_then(|b| frontmatter_value(&b, "milestone"))
            .filter(|v| v != "null" && is_valid_name(v));
        if found
            .iter()
            .any(|m| m.workstream == workstream && m.version == version)
        {
            continue;
        }
        found.push(ArchivedMilestone {
            workstream,
            version,
            date: mtime_date(&dir.join("STATE.md")),
            layout: live_layout(&dir, display),
        });
    }

    found.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.workstream.cmp(&b.workstream))
            .then_with(|| b.version.cmp(&a.version))
    });
    found
}

/// The open rows of the `S` picker: the workspace root first when it carries
/// workstream-shaped content and is itself open (D-08, D-10), then the open
/// workstreams sorted by name.
pub(crate) fn open_rows(root: &Path) -> Vec<PickerRow> {
    let mut rows = Vec::new();
    if base_has_content(root) {
        if let Classification::Open { ready_to_close } = classify_dir(root) {
            rows.push(PickerRow::Open {
                label: BASE_LABEL.to_string(),
                ready_to_close,
            });
        }
    }
    rows.extend(list(root).into_iter().filter_map(|name| {
        match classify_dir(&root.join("workstreams").join(&name)) {
            Classification::Open { ready_to_close } => Some(PickerRow::Open {
                label: name,
                ready_to_close,
            }),
            Classification::Complete => None,
        }
    }));
    rows
}

/// The planning directory one `S`-picker selection re-scopes to.
///
/// [`BASE_LABEL`] means the workspace root itself; anything else is a
/// workstream name. Lives here rather than inline in the event loop so the
/// mapping is testable — the event loop owns a terminal and is not.
pub(crate) fn selection_dir(root: &Path, selection: &str) -> PathBuf {
    if selection == BASE_LABEL {
        root.to_path_buf()
    } else {
        scoped_dir(root, Some(selection))
    }
}

/// Recover the workspace root from a scoped directory. In flat mode (or any
/// path whose parent is not literally named `workstreams`) this is a no-op.
pub(crate) fn root_of(scoped: &Path) -> PathBuf {
    let leaf = |p: Option<&Path>| {
        p.and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_string)
    };
    let parent = scoped.parent();
    // A workstream literally named `milestones` is still a workstream, so the
    // `workstreams/` rule wins over the archive rules below.
    if leaf(parent).as_deref() == Some("workstreams") {
        if let Some(root) = parent.and_then(|p| p.parent()) {
            return root.to_path_buf();
        }
    }
    if let Some(parent) = parent {
        // `<dir>/milestones` itself, or an archive directly inside one.
        if leaf(Some(scoped)).as_deref() == Some("milestones") {
            return root_of(parent);
        }
        if leaf(Some(parent)).as_deref() == Some("milestones") {
            if let Some(grand) = parent.parent() {
                return root_of(grand);
            }
        }
    }
    scoped.to_path_buf()
}

/// Tempdir-built storage shapes shared by tests (not tracked sample dirs, so
/// the shared `sample/` fixtures never shift).
#[cfg(test)]
pub(crate) mod fixtures {
    use std::fs;
    use std::path::{Path, PathBuf};

    pub(crate) fn write_state(dir: &Path, frontmatter: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("STATE.md"), format!("---\n{frontmatter}\n---\n")).unwrap();
    }

    /// Builds `<dir>/.planning` holding every live storage permutation and
    /// returns that root.
    pub(crate) fn live_permutations(dir: &Path) -> PathBuf {
        let root = dir.join(".planning");
        let ws = root.join("workstreams");
        // Backlog-only root: STATE.md without frontmatter, plus a todo.
        fs::create_dir_all(root.join("todos/pending")).unwrap();
        fs::write(root.join("STATE.md"), "# backlog\n").unwrap();
        fs::write(root.join("todos/pending/one.md"), "x").unwrap();

        write_state(
            &ws.join("no-milestone"),
            "milestone: null\nstatus: planning",
        );
        write_state(&ws.join("open-ms"), "milestone: v1.1\nstatus: executing");
        fs::create_dir_all(ws.join("open-ms/phases/01-x")).unwrap();

        let ready = ws.join("ready");
        write_state(&ready, "milestone: v1.5\nstatus: complete");
        fs::create_dir_all(ready.join("milestones/archived-20261001-phases")).unwrap();

        let done = ws.join("done-in-place");
        write_state(&done, "milestone: v1.6\nstatus: complete");
        fs::create_dir_all(done.join("milestones/v1.6-phases/40-x")).unwrap();
        fs::write(done.join("milestones/v1.6-ROADMAP.md"), "# v1.6").unwrap();

        write_state(&ws.join("milestone-complete"), "status: milestone complete");
        write_state(&ws.join("archived-status"), "status: archived");
        fs::create_dir_all(ws.join("no-state/phases")).unwrap();
        root
    }

    /// `live_permutations` plus every archive shape gsd-core writes.
    pub(crate) fn archive_permutations(dir: &Path) -> PathBuf {
        let root = live_permutations(dir);
        let ms = root.join("milestones");
        fs::create_dir_all(ms.join("v1.0-phases/01-x")).unwrap();
        fs::create_dir_all(ms.join("v1.0-quick")).unwrap();
        fs::write(ms.join("v1.0-ROADMAP.md"), "# v1.0").unwrap();
        fs::write(ms.join("v1.0-REQUIREMENTS.md"), "# reqs").unwrap();

        let french = ms.join("ws-release-french-2026-10-15");
        write_state(&french, "status: complete");
        fs::write(french.join("ROADMAP.md"), "# r").unwrap();
        fs::create_dir_all(french.join("phases/41-x")).unwrap();

        let old = ms.join("ws-old-2026-09-25");
        write_state(&old, "milestone: v1.1\nstatus: complete");
        fs::create_dir_all(old.join("milestones")).unwrap();
        fs::write(old.join("milestones/v1.0-ROADMAP.md"), "# v1.0").unwrap();

        // gsd-core `phases clear` residue: one empty (ready/), one populated.
        fs::create_dir_all(
            root.join("workstreams/open-ms/milestones/archived-20260901-phases/00-old"),
        )
        .unwrap();
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn selection_dir_maps_base_to_the_root_and_a_name_to_its_scoped_dir() {
        let root = Path::new("/w/.planning");
        assert_eq!(selection_dir(root, BASE_LABEL), root.to_path_buf());
        assert_eq!(
            selection_dir(root, "alpha"),
            Path::new("/w/.planning/workstreams/alpha").to_path_buf()
        );
    }

    #[test]
    fn classify_keeps_incomplete_and_not_complete_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".planning");
        for (name, status) in [("a", "incomplete"), ("b", "not complete")] {
            let ws = root.join("workstreams").join(name);
            fs::create_dir_all(&ws).unwrap();
            fs::write(ws.join("STATE.md"), format!("---\nstatus: {status}\n---\n")).unwrap();
            assert_eq!(
                classify_dir(&ws),
                Classification::Open {
                    ready_to_close: false
                },
                "{status}"
            );
        }
    }

    #[test]
    fn classify_sample_workstreams_from_their_own_state_status() {
        let root = Path::new("sample/workstreams/.planning/workstreams");
        let open = Classification::Open {
            ready_to_close: false,
        };
        assert_eq!(classify_dir(&root.join("alpha")), open);
        assert_eq!(classify_dir(&root.join("gamma")), Classification::Complete);
        assert_eq!(classify_dir(&root.join("zeta")), Classification::Complete);
        // No STATE.md at all is unknown, and unknown must read as open.
        assert_eq!(classify_dir(&root.join("nonexistent")), open);
    }

    #[test]
    fn classify_ready_to_close_when_status_is_bare_complete_and_no_archive() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        fs::create_dir_all(ws.join("milestones/archived-20261001-phases")).unwrap();
        fs::write(
            ws.join("STATE.md"),
            "---\nmilestone: v1.5\nstatus: complete\n---\n",
        )
        .unwrap();
        assert_eq!(
            classify_dir(&ws),
            Classification::Open {
                ready_to_close: true
            }
        );
    }

    const OPEN: Classification = Classification::Open {
        ready_to_close: false,
    };
    const READY: Classification = Classification::Open {
        ready_to_close: true,
    };

    fn permutations() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::live_permutations(dir.path());
        (dir, root)
    }

    #[test]
    fn classify_matrix_covers_every_live_permutation() {
        let (_d, root) = permutations();
        let c = |n: &str| classify_dir(&root.join("workstreams").join(n));
        assert_eq!(c("no-milestone"), OPEN);
        assert_eq!(c("open-ms"), OPEN);
        assert_eq!(c("no-state"), OPEN);
        assert_eq!(c("ready"), READY);
        assert_eq!(c("done-in-place"), Classification::Complete);
        assert_eq!(c("milestone-complete"), Classification::Complete);
        assert_eq!(c("archived-status"), Classification::Complete);
    }

    #[test]
    fn classify_rejects_traversal_in_milestone_field() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("a/b/ws");
        fixtures::write_state(&ws, "milestone: ../../x\nstatus: complete");
        // A file exists at the traversed path: milestones/../../x-ROADMAP.md.
        fs::create_dir_all(ws.join("milestones")).unwrap();
        fs::write(dir.path().join("a/b/x-ROADMAP.md"), "# x").unwrap();
        assert_eq!(classify_dir(&ws), READY, "never Complete via traversal");
    }

    #[test]
    fn classify_root_backlog_is_open() {
        let (_d, root) = permutations();
        assert_eq!(classify_dir(&root), OPEN);
    }

    fn labels(rows: &[PickerRow]) -> Vec<&str> {
        rows.iter().map(|r| r.label()).collect()
    }

    #[test]
    fn open_rows_list_default_first_then_open_workstreams() {
        let (_d, root) = permutations();
        assert_eq!(
            labels(&open_rows(&root)),
            [BASE_LABEL, "no-milestone", "no-state", "open-ms", "ready"]
        );
    }

    #[test]
    fn open_rows_omit_default_when_root_milestone_complete() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixtures::write_state(root, "status: milestone complete");
        fs::write(root.join("ROADMAP.md"), "# r").unwrap();
        assert!(!labels(&open_rows(root)).contains(&BASE_LABEL));
    }

    #[test]
    fn open_rows_keep_default_when_root_milestone_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixtures::write_state(root, "status: executing");
        fs::write(root.join("ROADMAP.md"), "# r").unwrap();
        assert_eq!(labels(&open_rows(root)), [BASE_LABEL]);
    }

    #[test]
    fn open_rows_omit_default_when_root_has_only_shared_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("todos")).unwrap();
        fs::write(root.join("PROJECT.md"), "# p").unwrap();
        assert!(open_rows(root).is_empty());
    }

    #[test]
    fn base_label_is_not_a_legal_workstream_name() {
        // The synthetic picker entry must be structurally unable to collide
        // with a real `.planning/workstreams/<name>/` directory.
        assert!(!is_valid_name(BASE_LABEL));
    }

    #[test]
    fn base_has_content_only_when_the_root_carries_workstream_shaped_files() {
        // Partially migrated: ROADMAP.md/STATE.md/phases/ still sit at the
        // root alongside workstreams/, and nothing else can reach them.
        assert!(base_has_content(Path::new(
            "sample/project-and-workstreams/.planning"
        )));
        // Cleanly migrated: the root holds only shared content, which every
        // workstream view already federates in — a base entry would be dead.
        assert!(!base_has_content(Path::new("sample/workstreams/.planning")));
    }

    fn stale_root(dir: &Path) -> PathBuf {
        let root = dir.join(".planning");
        fs::create_dir_all(root.join("workstreams/a")).unwrap();
        fs::write(root.join("state.json"), "{}").unwrap();
        root
    }

    #[test]
    fn root_state_json_is_stale_when_workstreams_exist_and_root_has_no_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = stale_root(dir.path());
        assert!(root_state_json_is_stale(&root));
    }

    #[test]
    fn root_state_json_is_not_stale_when_root_has_state_md() {
        for make in ["STATE.md", "ROADMAP.md", "phases"] {
            let dir = tempfile::tempdir().unwrap();
            let root = stale_root(dir.path());
            if make == "phases" {
                fs::create_dir_all(root.join(make)).unwrap();
            } else {
                fs::write(root.join(make), "# x").unwrap();
            }
            assert!(!root_state_json_is_stale(&root), "{make}");
        }
    }

    #[test]
    fn root_state_json_is_not_stale_without_workstreams_or_state_json() {
        let dir = tempfile::tempdir().unwrap();
        let flat = dir.path().join("flat/.planning");
        fs::create_dir_all(&flat).unwrap();
        fs::write(flat.join("state.json"), "{}").unwrap();
        assert!(!root_state_json_is_stale(&flat));

        let no_json = dir.path().join("nojson/.planning");
        fs::create_dir_all(no_json.join("workstreams/a")).unwrap();
        assert!(!root_state_json_is_stale(&no_json));
    }

    #[test]
    fn is_valid_name_accepts_and_rejects() {
        assert!(is_valid_name("alpha"));
        assert!(is_valid_name("a.b-c_1"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("a/b"));
        assert!(!is_valid_name(".."));
        assert!(!is_valid_name("a b"));
        assert!(!is_valid_name("a$"));
    }

    #[test]
    fn list_is_empty_when_workstreams_dir_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(list(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn list_returns_sorted_valid_directory_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/beta")).unwrap();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        fs::create_dir_all(root.join("workstreams/a b")).unwrap();
        fs::write(root.join("workstreams/notadir.md"), "x").unwrap();
        assert_eq!(list(root), vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn resolve_none_when_workstreams_absent_or_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(dir.path(), None, None, None), None);

        let dir2 = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir2.path().join("workstreams")).unwrap();
        assert_eq!(resolve(dir2.path(), None, None, None), None);
    }

    #[test]
    fn resolve_precedence_cli_env_file_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        fs::create_dir_all(root.join("workstreams/beta")).unwrap();
        fs::create_dir_all(root.join("workstreams/gamma")).unwrap();

        // No sources set: sorted-first (alpha).
        assert_eq!(resolve(root, None, None, None), Some("alpha".to_string()));

        // active-workstream file set: beta wins over sorted-first.
        fs::write(root.join("active-workstream"), "beta\n").unwrap();
        assert_eq!(resolve(root, None, None, None), Some("beta".to_string()));

        // env wins over the file.
        assert_eq!(
            resolve(root, None, Some("gamma"), None),
            Some("gamma".to_string())
        );

        // cli wins over env.
        assert_eq!(
            resolve(root, Some("alpha"), Some("gamma"), None),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn resolve_falls_through_invalid_env_and_file_values() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();

        // Invalid env value falls through to the file/sorted-first sources.
        assert_eq!(
            resolve(root, None, Some("nope"), None),
            Some("alpha".to_string())
        );

        // Invalid file value falls through to sorted-first.
        fs::write(root.join("active-workstream"), "also-nope").unwrap();
        assert_eq!(resolve(root, None, None, None), Some("alpha".to_string()));
    }

    #[test]
    fn resolve_session_sits_between_env_and_the_pointer_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        fs::create_dir_all(root.join("workstreams/beta")).unwrap();
        fs::create_dir_all(root.join("workstreams/gamma")).unwrap();
        fs::write(root.join("active-workstream"), "beta\n").unwrap();
        let gamma = Remembered::Workstream("gamma".into());

        // session beats the pointer file.
        assert_eq!(
            resolve(root, None, None, Some(&gamma)),
            Some("gamma".to_string())
        );
        // env beats session.
        assert_eq!(
            resolve(root, None, Some("alpha"), Some(&gamma)),
            Some("alpha".to_string())
        );
        // cli beats session.
        assert_eq!(
            resolve(root, Some("alpha"), None, Some(&gamma)),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn resolve_falls_through_an_invalid_or_missing_session_value() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        fs::create_dir_all(root.join("workstreams/beta")).unwrap();

        for bad in ["gone", "../beta", "a b"] {
            let s = Remembered::Workstream(bad.into());
            // No pointer file: sorted-first.
            assert_eq!(
                resolve(root, None, None, Some(&s)),
                Some("alpha".to_string()),
                "{bad:?} falls through to sorted-first"
            );
        }
        fs::write(root.join("active-workstream"), "beta").unwrap();
        let s = Remembered::Workstream("gone".into());
        assert_eq!(
            resolve(root, None, None, Some(&s)),
            Some("beta".to_string()),
            "a missing session value falls through to the pointer file"
        );
    }

    #[test]
    fn resolve_session_base_is_the_root_only_when_the_base_has_content() {
        // Partially migrated: base has content, so `null` means the root view.
        let root = Path::new("sample/project-and-workstreams/.planning");
        assert_eq!(resolve(root, None, None, Some(&Remembered::Base)), None);

        // Cleanly migrated: an empty base view would be dead, so fall through.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        assert_eq!(
            resolve(root, None, None, Some(&Remembered::Base)),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn resolve_invalid_cli_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        assert_eq!(resolve(root, Some("nope"), None, None), None);
    }

    #[test]
    fn scoped_dir_joins_workstreams_name_or_returns_root() {
        let root = Path::new("/x/.planning");
        assert_eq!(
            scoped_dir(root, Some("beta")),
            root.join("workstreams").join("beta")
        );
        assert_eq!(scoped_dir(root, None), root.to_path_buf());
    }

    #[test]
    fn root_of_strips_workstreams_segment_and_leaves_flat_paths_alone() {
        let root = Path::new("/x/.planning");
        let scoped = root.join("workstreams").join("beta");
        assert_eq!(root_of(&scoped), root.to_path_buf());
        assert_eq!(root_of(root), root.to_path_buf());

        // A path merely containing the word "workstreams" elsewhere (not as
        // the literal parent directory name) is unchanged.
        let odd = Path::new("/x/my-workstreams-tool/.planning");
        assert_eq!(root_of(odd), odd.to_path_buf());
    }

    #[test]
    fn root_of_climbs_out_of_milestones_archives() {
        assert_eq!(
            root_of(Path::new("/w/.planning/milestones/ws-x-2026-01-01")),
            PathBuf::from("/w/.planning")
        );
        assert_eq!(
            root_of(Path::new("/w/.planning/milestones")),
            PathBuf::from("/w/.planning")
        );
        assert_eq!(
            root_of(Path::new("/w/.planning/workstreams/a/milestones")),
            PathBuf::from("/w/.planning")
        );
    }

    #[test]
    fn root_of_returns_root_for_a_workstream_named_milestones() {
        assert_eq!(
            root_of(Path::new("/w/.planning/workstreams/milestones")),
            PathBuf::from("/w/.planning")
        );
    }

    #[test]
    fn parse_ws_archive_name_extracts_name_and_date() {
        assert_eq!(
            parse_ws_archive_name("ws-release-french-2026-10-15"),
            Some(("release-french".into(), "2026-10-15".into()))
        );
        assert_eq!(
            parse_ws_archive_name("ws-a-b-2026-09-25-2"),
            Some(("a-b".into(), "2026-09-25".into()))
        );
        assert_eq!(parse_ws_archive_name("ws-x"), None);
        assert_eq!(parse_ws_archive_name("v1.0-phases"), None);
    }

    #[test]
    fn discover_finds_workstream_archive_with_date_from_dir_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".planning");
        let a = root.join("milestones/ws-release-french-2026-10-15");
        fixtures::write_state(&a, "status: complete");
        fs::write(a.join("ROADMAP.md"), "# r").unwrap();
        fs::create_dir_all(a.join("phases/41-x")).unwrap();

        let found = discover_completed(&root);
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.workstream, "release-french");
        assert_eq!(m.version, None);
        assert_eq!(m.date.as_deref(), Some("2026-10-15"));
        assert_eq!(m.layout.anchor, a);
        assert_eq!(m.layout.display, "milestones/ws-release-french-2026-10-15");
    }

    fn found(root: &Path) -> Vec<ArchivedMilestone> {
        discover_completed(root)
    }

    fn row<'a>(
        rows: &'a [ArchivedMilestone],
        ws: &str,
        version: Option<&str>,
    ) -> &'a ArchivedMilestone {
        rows.iter()
            .find(|m| m.workstream == ws && m.version.as_deref() == version)
            .unwrap_or_else(|| panic!("no row {ws} {version:?} in {rows:#?}"))
    }

    #[test]
    fn version_of_entry_reads_version_prefixes() {
        assert_eq!(version_of_entry("v1.0-ROADMAP.md").as_deref(), Some("v1.0"));
        assert_eq!(
            version_of_entry("v1.0-REQUIREMENTS.md").as_deref(),
            Some("v1.0")
        );
        assert_eq!(version_of_entry("v1.0-phases").as_deref(), Some("v1.0"));
        assert_eq!(version_of_entry("v1.0-quick").as_deref(), Some("v1.0"));
        assert_eq!(
            version_of_entry("v1.6-MILESTONE-AUDIT.md").as_deref(),
            Some("v1.6")
        );
        for none in [
            "archived-20261006-phases",
            "ws-x-2026-01-01",
            "MILESTONES.md",
            "notes.md",
        ] {
            assert_eq!(version_of_entry(none), None, "{none}");
        }
    }

    #[test]
    fn parse_archived_phases_name_reads_date() {
        assert_eq!(
            parse_archived_phases_name("archived-20261006-phases").as_deref(),
            Some("2026-10-06")
        );
        assert_eq!(parse_archived_phases_name("archived-2026-phases"), None);
    }

    #[test]
    fn discover_groups_versioned_root_archives() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        // 2026-08-01 12:00 UTC = day 20666.
        let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(20_666 * 86_400 + 43_200);
        fs::File::options()
            .write(true)
            .open(root.join("milestones/v1.0-ROADMAP.md"))
            .unwrap()
            .set_modified(when)
            .unwrap();

        let rows = found(&root);
        let v1: Vec<_> = rows.iter().filter(|m| m.workstream == BASE_LABEL).collect();
        assert_eq!(v1.len(), 1, "{rows:#?}");
        let m = v1[0];
        assert_eq!(m.version.as_deref(), Some("v1.0"));
        assert_eq!(m.date.as_deref(), Some("2026-08-01"));
        assert_eq!(m.layout.display, "milestones/v1.0");
        assert!(m.layout.roadmap.is_some());
        assert!(m.layout.requirements.is_some());
        assert!(m.layout.phases_dir.is_some());
        assert!(m.layout.quick_dir.is_some());
    }

    #[test]
    fn discover_finds_workstream_versioned_archives() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        let m = row(&found(&root), "done-in-place", Some("v1.6")).clone();
        assert_eq!(
            m.layout.display,
            "workstreams/done-in-place/milestones/v1.6"
        );
    }

    #[test]
    fn discover_scans_inner_milestones_of_archived_workstreams() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        let rows = found(&root);
        let old = row(&rows, "old", Some("v1.1"));
        assert_eq!(old.date.as_deref(), Some("2026-09-25"));
        let inner = row(&rows, "old", Some("v1.0"));
        assert!(inner.layout.roadmap.is_some(), "{inner:?}");
    }

    #[test]
    fn discover_skips_empty_archived_phases_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        assert!(
            found(&root).iter().all(|m| m.workstream != "ready"),
            "empty archived-<date>-phases is not a completion"
        );
    }

    #[test]
    fn discover_lists_nonempty_archived_phases_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        let m = row(&found(&root), "open-ms", None).clone();
        assert_eq!(m.date.as_deref(), Some("2026-09-01"));
        assert!(m.layout.phases_dir.is_some());
    }

    #[test]
    fn discover_synthesizes_rows_for_completed_live_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        let rows = found(&root);
        for name in ["milestone-complete", "archived-status"] {
            let own: Vec<_> = rows.iter().filter(|m| m.workstream == name).collect();
            assert_eq!(own.len(), 1, "{name}: {rows:#?}");
            assert_eq!(own[0].layout.anchor, root.join("workstreams").join(name));
            assert_eq!(own[0].layout.display, format!("workstreams/{name}"));
        }
        assert_eq!(
            rows.iter()
                .filter(|m| m.workstream == "done-in-place")
                .count(),
            1,
            "its v1.6 archive covers it: {rows:#?}"
        );
        for open in ["open-ms", "ready", "no-milestone", "no-state"] {
            let live = root.join("workstreams").join(open);
            assert!(
                rows.iter().all(|m| m.layout.anchor != live),
                "{open} is open, never synthesized"
            );
        }
    }

    #[test]
    fn discover_orders_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = fixtures::archive_permutations(dir.path());
        let rows = found(&root);
        assert!(rows.len() >= 6, "{rows:#?}");
        for pair in rows.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            assert!(a.date >= b.date, "date desc, None last: {a:?} then {b:?}");
            if a.date == b.date {
                assert!(
                    (&a.workstream, std::cmp::Reverse(&a.version))
                        <= (&b.workstream, std::cmp::Reverse(&b.version)),
                    "ties by workstream then version: {a:?} then {b:?}"
                );
            }
        }
    }

    #[test]
    fn mtime_date_reads_utc_day() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f");
        fs::write(&f, "x").unwrap();
        let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(20_666 * 86_400 + 60);
        fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(when)
            .unwrap();
        assert_eq!(mtime_date(&f).as_deref(), Some("2026-08-01"));
        assert_eq!(mtime_date(&dir.path().join("missing")), None);
    }
}
