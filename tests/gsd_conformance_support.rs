//! Shared support for GSD-conformance oracle tests: resolving GSD's own
//! `gsd-tools.cjs`, shelling out to its `progress` command, and doing
//! dependency-free field checks against its JSON output (the crate carries
//! no JSON dependency, so substring/field-presence checks are what's used).
//!
//! Path-included by both `tests/gsd_conformance.rs` (drives writes through
//! filesystem helpers, since integration tests see only the compiled
//! binary) and `src/status_edit.rs`'s unit tests (drives writes through the
//! real `status_edit::apply`), so the resolution and parsing logic never
//! drifts between the two.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve `gsd-tools.cjs`'s path: the `GSD_TOOLS` env var first (an
/// explicit override always wins, even if the path doesn't exist — that's a
/// misconfigured override, not "unset"), else the default location under
/// the user's Claude config dir. Returns `None` only when nothing resolves,
/// so callers can skip cleanly rather than fail on a bare CI runner.
fn resolve_gsd_tools() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("GSD_TOOLS") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".claude/gsd-core/bin/gsd-tools.cjs");
    default.exists().then_some(default)
}

/// Whether this environment can run the oracle at all: `node` on `PATH`,
/// and `gsd-tools.cjs` resolvable. Prints a clearly worded skip line naming
/// what's missing when it can't, so a bare CI runner (no `node`, no
/// `~/.claude`) stays green instead of failing on a missing dependency this
/// test suite doesn't own.
#[allow(dead_code)]
pub(crate) fn oracle_available() -> Option<PathBuf> {
    let node_ok = Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !node_ok {
        println!("gsd-conformance: SKIP — `node --version` did not succeed");
        return None;
    }
    match resolve_gsd_tools() {
        Some(path) if path.exists() => Some(path),
        Some(path) => {
            println!(
                "gsd-conformance: SKIP — GSD_TOOLS points at {} which does not exist",
                path.display()
            );
            None
        }
        None => {
            println!(
                "gsd-conformance: SKIP — gsd-tools.cjs not found (set GSD_TOOLS or install to \
                 ~/.claude/gsd-core/bin/gsd-tools.cjs)"
            );
            None
        }
    }
}

/// Run `node <tools> progress --project-dir <dir>`, returning stdout when
/// the process exits cleanly. Always prints the `gsd-conformance: RAN`
/// marker first — reaching this call means the oracle is about to actually
/// execute, never a skip, which is what the verify command greps for.
#[allow(dead_code)]
pub(crate) fn run_progress(tools: &Path, project_dir: &Path) -> Option<String> {
    println!("gsd-conformance: RAN");
    let out = Command::new("node")
        .arg(tools)
        .arg("progress")
        .arg("--project-dir")
        .arg(project_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// True when the oracle's JSON reports a readable phase scope — i.e. its
/// `phase_scope` field is not the `"unreadable"` value GSD emits when
/// ROADMAP.md is missing or unparseable. A dependency-free substring check,
/// tolerant of the `key: value` vs `key:value` spacing `JSON.stringify`
/// happens to use.
#[allow(dead_code)]
pub(crate) fn phase_scope_is_readable(progress_json: &str) -> bool {
    !progress_json
        .lines()
        .any(|l| l.contains("\"phase_scope\"") && l.contains("\"unreadable\""))
}

/// The `"number"` field of every phase entry, in the order the oracle
/// emitted them — a dependency-free substring scan sufficient to detect a
/// write that dropped a phase from the array or reordered it.
#[allow(dead_code)]
pub(crate) fn phase_numbers(progress_json: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in progress_json.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        if let Some(rest) = trimmed.strip_prefix("\"number\":") {
            out.push(rest.trim().trim_matches('"').to_string());
        }
    }
    out
}
