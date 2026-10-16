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

/// One selectable row of the `S` picker. Plan 01-02 adds a completed variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerRow {
    Open { label: String, ready_to_close: bool },
}

impl PickerRow {
    pub(crate) fn label(&self) -> &str {
        match self {
            PickerRow::Open { label, .. } => label,
        }
    }
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
    let parent = scoped.parent();
    let parent_is_workstreams =
        parent.and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some("workstreams");
    if parent_is_workstreams {
        if let Some(root) = parent.and_then(|p| p.parent()) {
            return root.to_path_buf();
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
}
