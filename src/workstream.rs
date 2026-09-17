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

/// Recover the workspace root from a scoped directory. In flat mode (or any
/// path whose parent is not literally named `workstreams`) this is a no-op.
///
/// Not yet called from `main.rs` — Task 2 wires it into `planning.rs`'s
/// federation rule and Task 3 into the TUI's switch dialog.
#[allow(dead_code)]
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
