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
use std::path::{Path, PathBuf};

// RED stage (Task 1): signatures exist so the test module below compiles and
// runs, but bodies are `todo!()` until the GREEN commit wires them up and
// calls them from `main.rs`. `#[allow(dead_code)]` is temporary — GREEN
// removes it once these are reachable from the non-test build.

/// GSD's workstream name policy: `[A-Za-z0-9._-]` only, non-empty, no path
/// separators, no `..`.
#[allow(dead_code)]
pub(crate) fn is_valid_name(_name: &str) -> bool {
    todo!("Task 1 GREEN")
}

/// Sorted, valid directory names under `.planning/workstreams/`. Empty vec
/// when that directory is absent. Skips non-directories and invalid names.
#[allow(dead_code)]
pub(crate) fn list(_root: &Path) -> Vec<String> {
    todo!("Task 1 GREEN")
}

/// Resolve the focused workstream name. Precedence: `cli` > `env` > the
/// `.planning/active-workstream` pointer file > the first name in sorted
/// order. Returns `None` when `.planning/workstreams/` is absent or empty
/// (flat mode). An invalid or missing-directory `env`/pointer-file value
/// falls through to the next source instead of resolving; an invalid or
/// missing-directory `cli` value does NOT fall through — it resolves to
/// `None` so the caller can treat an explicit `--ws` typo as fatal.
#[allow(dead_code)]
pub(crate) fn resolve(_root: &Path, _cli: Option<&str>, _env: Option<&str>) -> Option<String> {
    todo!("Task 1 GREEN")
}

/// The scoped `.planning`-relative directory for `ws`: `root/workstreams/<name>`
/// when `Some`, or `root` unchanged in flat mode (`None`).
#[allow(dead_code)]
pub(crate) fn scoped_dir(_root: &Path, _ws: Option<&str>) -> PathBuf {
    todo!("Task 1 GREEN")
}

/// Recover the workspace root from a scoped directory. In flat mode (or any
/// path whose parent is not literally named `workstreams`) this is a no-op.
#[allow(dead_code)]
pub(crate) fn root_of(_scoped: &Path) -> PathBuf {
    todo!("Task 1 GREEN")
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
