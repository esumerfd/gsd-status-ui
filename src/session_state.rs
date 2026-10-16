//! Per-shell memory of the `S`-picker workstream choice.
//!
//! gsd-status owns `.planning/gsd-status/`; GSD never reads it. Each launching
//! shell (keyed by the parent PID of the gsd-status process) gets its own
//! `gsd-state-<ppid>.json`, so several terminals in one repo can each keep a
//! different workstream across relaunches without touching GSD's global
//! `.planning/active-workstream` pointer (D2).
//!
//! The file holds a single key: `{"workstream": "<name>"}`, or
//! `{"workstream": null}` for the `(default)` view. Workstream names are
//! restricted to `[A-Za-z0-9._-]`, so no JSON escaping is ever needed and a
//! small hand-rolled reader and writer is enough.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// gsd-status's own directory under the planning root.
pub(crate) const DIR: &str = "gsd-status";
const PREFIX: &str = "gsd-state-";
const SUFFIX: &str = ".json";

/// What a shell last chose in the `S` picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Remembered {
    /// The workspace root (`(default)` in the picker).
    Base,
    /// A named workstream. Not validated here — `workstream::resolve` checks
    /// it against the names that actually exist.
    Workstream(String),
}

/// `root/gsd-status/gsd-state-<pid>.json`. `root` is always the planning
/// root (`.planning/`), never a scoped workstream dir.
pub(crate) fn state_path(root: &Path, pid: u32) -> PathBuf {
    root.join(DIR).join(format!("{PREFIX}{pid}{SUFFIX}"))
}

/// Read `pid`'s remembered choice. `None` when the file is missing or does
/// not parse; never panics on garbage.
pub(crate) fn read(root: &Path, pid: u32) -> Option<Remembered> {
    let body = fs::read_to_string(state_path(root, pid)).ok()?;
    parse(&body)
}

/// Minimal parse of the `"workstream"` key's value: `null` or a
/// double-quoted string. Anything else is `None`.
fn parse(body: &str) -> Option<Remembered> {
    let rest = body.split_once("\"workstream\"")?.1.trim_start();
    let value = rest.strip_prefix(':')?.trim_start();
    if value.starts_with("null") {
        return Some(Remembered::Base);
    }
    let (name, _) = value.strip_prefix('"')?.split_once('"')?;
    Some(Remembered::Workstream(name.to_string()))
}

/// Persist `pid`'s choice. Creates `root/gsd-status/` (and its `.gitignore`
/// entry) on first use, then writes via a temp file and rename so a reader
/// never sees a half-written file.
pub(crate) fn write(root: &Path, pid: u32, remembered: &Remembered) -> io::Result<()> {
    let dir = root.join(DIR);
    if !dir.is_dir() {
        fs::create_dir_all(&dir)?;
        ensure_gitignore(root)?;
    }
    let value = match remembered {
        Remembered::Base => "null".to_string(),
        Remembered::Workstream(name) => format!("\"{name}\""),
    };
    let path = state_path(root, pid);
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    fs::write(&tmp, format!("{{\"workstream\": {value}}}\n"))?;
    fs::rename(&tmp, &path)
}

/// Make sure `root/.gitignore` ignores `gsd-status/`. Append-only: existing
/// lines are never rewritten or reordered.
pub(crate) fn ensure_gitignore(root: &Path) -> io::Result<()> {
    let path = root.join(".gitignore");
    let existing = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let covered = existing.lines().any(|l| {
        matches!(
            l.trim(),
            "gsd-status/" | "gsd-status" | "/gsd-status/" | "/gsd-status"
        )
    });
    if covered {
        return Ok(());
    }
    let mut addition = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        addition.push('\n');
    }
    addition.push_str("gsd-status/\n");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    io::Write::write_all(&mut file, addition.as_bytes())
}

/// Delete `gsd-state-<pid>.json` files in `dir` whose pid `is_alive` says
/// is gone. `keep_pid`'s file always survives. Names that are not exactly
/// that shape, with a pid in `1..=i32::MAX`, are left alone.
pub(crate) fn prune(dir: &Path, keep_pid: u32, is_alive: impl Fn(u32) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(pid_of) else {
            continue;
        };
        if pid != keep_pid && !is_alive(pid) {
            // Another shell's gsd-status may be pruning the same file.
            fs::remove_file(entry.path()).ok();
        }
    }
}

/// The pid in an exact `gsd-state-<pid>.json` name, restricted to
/// `1..=i32::MAX`: pid 0 or a value that casts negative would make
/// `kill(2)` signal a process group.
fn pid_of(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pid: u32 = digits.parse().ok()?;
    (1..=i32::MAX as u32).contains(&pid).then_some(pid)
}

/// Is `pid` a live process? Alive when `kill(pid, 0)` succeeds, or fails
/// with `EPERM` (it exists but belongs to someone else).
pub(crate) fn pid_alive(pid: u32) -> bool {
    // SAFETY: signal 0 performs only the existence and permission check and
    // sends nothing; the call has no memory-safety preconditions.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Prune dead shells' state files on a detached background thread so the
/// TUI never waits on it. The thread shares no state, so the process ending
/// mid-prune is harmless.
pub(crate) fn spawn_prune(root: &Path, keep_pid: u32) {
    let dir = root.join(DIR);
    std::thread::spawn(move || prune(&dir, keep_pid, pid_alive));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips_a_workstream() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, 77, &Remembered::Workstream("beta".into())).unwrap();
        let path = root.join("gsd-status/gsd-state-77.json");
        assert_eq!(state_path(root, 77), path);
        assert_eq!(
            fs::read_to_string(&path).unwrap().trim(),
            r#"{"workstream": "beta"}"#
        );
        assert_eq!(read(root, 77), Some(Remembered::Workstream("beta".into())));
    }

    #[test]
    fn base_round_trips_as_null() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, 5, &Remembered::Base).unwrap();
        assert_eq!(
            fs::read_to_string(state_path(root, 5)).unwrap().trim(),
            r#"{"workstream": null}"#
        );
        assert_eq!(read(root, 5), Some(Remembered::Base));
    }

    #[test]
    fn read_is_none_for_missing_garbage_or_another_pids_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(read(root, 1), None, "missing file");

        write(root, 2, &Remembered::Workstream("alpha".into())).unwrap();
        assert_eq!(read(root, 3), None, "another pid's file is not ours");

        for garbage in [
            "",
            "not json",
            "{",
            r#"{"workstream": 42}"#,
            r#"{"workstream": "unterminated}"#,
            r#"{"other": "alpha"}"#,
            "\u{0}\u{ff}",
        ] {
            fs::write(root.join("gsd-status/gsd-state-9.json"), garbage).unwrap();
            assert_eq!(read(root, 9), None, "garbage {garbage:?}");
        }
    }

    #[test]
    fn write_uses_a_temp_file_and_leaves_none_behind() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, 8, &Remembered::Workstream("a".into())).unwrap();
        write(root, 8, &Remembered::Workstream("b".into())).unwrap();
        let names: Vec<String> = fs::read_dir(root.join(DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["gsd-state-8.json".to_string()]);
        assert_eq!(read(root, 8), Some(Remembered::Workstream("b".into())));
    }

    #[test]
    fn first_write_creates_gitignore_with_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, 1, &Remembered::Base).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(".gitignore")).unwrap(),
            "gsd-status/\n"
        );
    }

    #[test]
    fn gitignore_entry_is_appended_preserving_existing_lines() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // No trailing newline: one must be inserted before the new line.
        fs::write(root.join(".gitignore"), "a\n# keep\nb").unwrap();
        write(root, 1, &Remembered::Base).unwrap();
        write(root, 2, &Remembered::Base).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(".gitignore")).unwrap(),
            "a\n# keep\nb\ngsd-status/\n"
        );
    }

    #[test]
    fn gitignore_is_idempotent_and_honours_equivalent_existing_entries() {
        for existing in ["gsd-status/", "gsd-status", "/gsd-status/", "/gsd-status"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let body = format!("x\n  {existing}  \n");
            fs::write(root.join(".gitignore"), &body).unwrap();
            ensure_gitignore(root).unwrap();
            ensure_gitignore(root).unwrap();
            assert_eq!(
                fs::read_to_string(root.join(".gitignore")).unwrap(),
                body,
                "{existing:?} already covers the dir"
            );
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        ensure_gitignore(root).unwrap();
        ensure_gitignore(root).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(".gitignore")).unwrap(),
            "gsd-status/\n"
        );
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn prune_removes_dead_pids_and_keeps_live_ones_and_the_current_shell() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for pid in [10, 11, 12] {
            fs::write(d.join(format!("gsd-state-{pid}.json")), "{}").unwrap();
        }
        // 10 is alive; 11 and 12 are dead, but 12 is this shell (keep_pid).
        prune(d, 12, |pid| pid == 10);
        assert_eq!(
            names_in(d),
            vec![
                "gsd-state-10.json".to_string(),
                "gsd-state-12.json".to_string()
            ]
        );
    }

    #[test]
    fn prune_ignores_names_that_are_not_exactly_a_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let too_big = format!("gsd-state-{}.json", i32::MAX as u64 + 1);
        let odd = [
            "notes.txt",
            "gsd-state-abc.json",
            "gsd-state-12.json.tmp",
            "gsd-state-.json",
            "gsd-state-0.json",
            "gsd-state--5.json",
            too_big.as_str(),
        ];
        for name in odd {
            fs::write(d.join(name), "{}").unwrap();
        }
        fs::write(d.join("gsd-state-7.json"), "{}").unwrap();
        prune(d, 1, |_| false);
        let mut expected: Vec<String> = odd.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(names_in(d), expected, "only gsd-state-7.json is pruned");
    }

    #[test]
    fn prune_of_a_missing_dir_is_quiet_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gsd-status");
        prune(&missing, 1, |_| false);
        assert!(!missing.exists());
    }

    #[test]
    fn pid_alive_is_true_for_this_process_and_false_for_a_reaped_child() {
        assert!(pid_alive(std::process::id()));
        // Best-effort: a reaped child's pid is dead unless it is reused in
        // the microseconds between wait() and the check.
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!pid_alive(pid), "reaped child {pid} reads as dead");
    }
}
