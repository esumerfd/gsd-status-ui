//! GSD workstream resolution (`.planning/workstreams/<name>/`).
//!
//! Mirrors gsd-core's workstream contract (`workstream.cts`,
//! `planning-workspace.cts`, `active-workstream-store.cts`,
//! `workstream-name-policy.cts`, v1.14.0): flat mode has no
//! `.planning/workstreams/` and everything lives at `.planning/` root;
//! workstream mode moves only `ROADMAP.md`, `STATE.md`, `REQUIREMENTS.md`, and
//! `phases/` into `.planning/workstreams/<name>/`.
//!
//! D2: this module never writes `.planning/active-workstream` or any other
//! pointer — it only ever reads. Switching workstreams in the TUI (see
//! `tui::app::WorkstreamDialog`) never touches disk.
//!
//! This module deliberately does NOT read GSD's session-scoped pointer under
//! `${TMPDIR}/gsd-workstream-sessions/`: a standalone TUI process computes a
//! different session key than the agent process that wrote that pointer, so
//! reading it here could never match and would be dead code.
use std::fs;
use std::path::{Path, PathBuf};

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

/// Resolve the focused workstream name. Precedence: `cli` > `env` > the
/// `.planning/active-workstream` pointer file > the first name in sorted
/// order. Returns `None` when `.planning/workstreams/` is absent or empty
/// (flat mode). An invalid or missing-directory `env`/pointer-file value
/// falls through to the next source instead of resolving; an invalid or
/// missing-directory `cli` value does NOT fall through — it resolves to
/// `None` so the caller can treat an explicit `--ws` typo as fatal.
pub(crate) fn resolve(root: &Path, cli: Option<&str>, env: Option<&str>) -> Option<String> {
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

/// Separator drawn between the active workstreams and the complete ones in
/// the `S` picker. Never selectable — it is a rendered rule, not an entry.
pub(crate) const COMPLETE_SEPARATOR: &str = "--- complete ---";

/// How far through its life a workstream is, read from its own `STATE.md`
/// frontmatter `status:`.
///
/// Mirrors gsd-core's `isCompletedInventory`
/// (`workstream-inventory-builder.cts`), which treats a status matching
/// `milestone complete` or `archived` as done. A bare `complete` is accepted
/// too, since that is what a hand-written STATE.md tends to say — but only as
/// a whole-word match, so `incomplete` and `not complete` stay `Active`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Completion {
    /// Live work. Also the verdict for a workstream with no readable
    /// STATE.md: "unknown" must never render as done, or live work hides.
    Active,
    /// Finished but still present — sorted below the separator.
    Complete,
    /// Retired. Hidden from the picker outright, and NOT revealed by the
    /// `H` show/hide-completed toggle: `H` governs completed work inside a
    /// workspace, not whether a retired workspace is offered at all.
    Archived,
}

/// Classify one workstream by its `STATE.md` frontmatter `status:`.
pub(crate) fn completion(root: &Path, name: &str) -> Completion {
    let Ok(body) = fs::read_to_string(root.join("workstreams").join(name).join("STATE.md")) else {
        return Completion::Active;
    };
    let Some(status) = body
        .lines()
        .take_while(|l| l.trim() != "---" || body.starts_with("---"))
        .find_map(|l| l.strip_prefix("status:"))
    else {
        return Completion::Active;
    };
    let status = status.trim().to_ascii_lowercase();
    // Deliberately narrower than gsd-core's `\barchived\b` / `\bmilestone
    // complete\b` regexes: a bare word match anywhere also fires on the
    // negations a hand-written STATE.md can contain ("not complete"), and
    // reading live work as done is the expensive direction to be wrong in.
    // Exact forms, plus the `milestone …` phrase gsd-core itself emits.
    if status == "archived" || status.contains("milestone archived") {
        Completion::Archived
    } else if matches!(status.as_str(), "complete" | "completed")
        || status.contains("milestone complete")
    {
        Completion::Complete
    } else {
        Completion::Active
    }
}

/// The workstream rows of the `S` picker, in display order: active ones
/// sorted, then complete ones sorted. Archived workstreams are omitted.
///
/// The second element is the index the [`COMPLETE_SEPARATOR`] belongs
/// *before*, or `None` when nothing is complete. Keeping the separator out
/// of the returned list is deliberate: `selected`/`focused` then index only
/// real entries, so cursor movement and selection need no skip-the-rule
/// special case.
pub(crate) fn picker_entries(root: &Path) -> (Vec<String>, Option<usize>) {
    let mut active = Vec::new();
    let mut complete = Vec::new();
    for name in list(root) {
        match completion(root, &name) {
            Completion::Active => active.push(name),
            Completion::Complete => complete.push(name),
            Completion::Archived => {}
        }
    }
    let separator_at = (!complete.is_empty()).then_some(active.len());
    active.extend(complete);
    (active, separator_at)
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
    fn completion_classifies_from_the_workstreams_own_state_status() {
        let root = Path::new("sample/workstreams/.planning");
        assert_eq!(completion(root, "alpha"), Completion::Active);
        assert_eq!(completion(root, "gamma"), Completion::Complete);
        assert_eq!(completion(root, "zeta"), Completion::Archived);
        // No STATE.md at all must read as Active. "Unknown" can never be
        // allowed to mean "done" — that would hide live work.
        assert_eq!(completion(root, "nonexistent"), Completion::Active);
    }

    #[test]
    fn completion_does_not_mistake_incomplete_or_not_complete_for_done() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".planning");
        for (name, status) in [("a", "incomplete"), ("b", "not complete")] {
            let ws = root.join("workstreams").join(name);
            fs::create_dir_all(&ws).unwrap();
            fs::write(ws.join("STATE.md"), format!("---\nstatus: {status}\n---\n")).unwrap();
        }
        assert_eq!(completion(&root, "a"), Completion::Active);
        assert_eq!(completion(&root, "b"), Completion::Active);
    }

    #[test]
    fn picker_entries_sort_complete_to_the_bottom_and_drop_archived() {
        let root = Path::new("sample/workstreams/.planning");
        let (items, separator_at) = picker_entries(root);
        assert_eq!(
            items,
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
            "zeta is archived and must not appear at all"
        );
        assert_eq!(
            separator_at,
            Some(2),
            "the separator sits immediately before the first complete workstream"
        );
    }

    #[test]
    fn picker_entries_have_no_separator_when_nothing_is_complete() {
        let root = Path::new("sample/project-and-workstreams/.planning");
        let (items, separator_at) = picker_entries(root);
        assert_eq!(items, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(separator_at, None);
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
        assert_eq!(resolve(dir.path(), None, None), None);

        let dir2 = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir2.path().join("workstreams")).unwrap();
        assert_eq!(resolve(dir2.path(), None, None), None);
    }

    #[test]
    fn resolve_precedence_cli_env_file_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        fs::create_dir_all(root.join("workstreams/beta")).unwrap();
        fs::create_dir_all(root.join("workstreams/gamma")).unwrap();

        // No sources set: sorted-first (alpha).
        assert_eq!(resolve(root, None, None), Some("alpha".to_string()));

        // active-workstream file set: beta wins over sorted-first.
        fs::write(root.join("active-workstream"), "beta\n").unwrap();
        assert_eq!(resolve(root, None, None), Some("beta".to_string()));

        // env wins over the file.
        assert_eq!(
            resolve(root, None, Some("gamma")),
            Some("gamma".to_string())
        );

        // cli wins over env.
        assert_eq!(
            resolve(root, Some("alpha"), Some("gamma")),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn resolve_falls_through_invalid_env_and_file_values() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();

        // Invalid env value falls through to the file/sorted-first sources.
        assert_eq!(resolve(root, None, Some("nope")), Some("alpha".to_string()));

        // Invalid file value falls through to sorted-first.
        fs::write(root.join("active-workstream"), "also-nope").unwrap();
        assert_eq!(resolve(root, None, None), Some("alpha".to_string()));
    }

    #[test]
    fn resolve_invalid_cli_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("workstreams/alpha")).unwrap();
        assert_eq!(resolve(root, Some("nope"), None), None);
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
