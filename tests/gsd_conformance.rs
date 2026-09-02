//! Conformance tests: prove that the statuses this tool writes are still
//! readable by GSD's own tooling. Integration tests under `tests/` only see
//! the compiled binary (this crate has no `[lib]`), so the writes here are
//! reproduced through small filesystem helpers rather than calling
//! `status_edit::apply` directly — `src/status_edit.rs` has its own unit
//! test that runs the same oracle over a workspace mutated by the real
//! writer, using the identical support module (`gsd_conformance_support.rs`,
//! path-included by both) so the two never drift apart.
//!
//! Every test that actually shells out to the oracle prints
//! `gsd-conformance: RAN` (from `run_progress`), so a silent skip on a bare
//! CI runner (no `node`, no `~/.claude`) can never be mistaken for a pass.

#[path = "gsd_conformance_support.rs"]
mod support;

use std::path::Path;

/// Copy `sample/` into a fresh temp dir so each test mutates its own throwaway
/// workspace.
fn temp_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sample")
            .as_path(),
        dir.path(),
    );
    dir
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

/// Flip an existing `- [ ] **Phase N: Title**` roadmap-index line to
/// `- [x]`, reproducing the mark-verified edit `status_edit::apply_phase`
/// makes, without depending on it directly.
fn mark_phase_verified(planning: &Path, needle: &str) {
    let path = planning.join("ROADMAP.md");
    let body = std::fs::read_to_string(&path).unwrap();
    let from = format!("- [ ] **{needle}");
    let to = format!("- [x] **{needle}");
    assert!(
        body.contains(&from),
        "fixture must contain an unverified phase line matching {needle:?}: {body}"
    );
    std::fs::write(&path, body.replacen(&from, &to, 1)).unwrap();
}

/// Append a `complete` quick-task row after an existing row, reproducing
/// the shape `status_edit::apply_quick_task` upserts, without depending on
/// it directly.
fn upsert_quick_task_row(planning: &Path, after_row: &str, new_row: &str) {
    let path = planning.join("STATE.md");
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains(after_row),
        "fixture must contain the anchor row: {body}"
    );
    let replacement = format!("{after_row}\n{new_row}");
    std::fs::write(&path, body.replacen(after_row, &replacement, 1)).unwrap();
}

/// Move a pending todo to `todos/completed/`, stamping a `completed:` key
/// into its frontmatter — reproducing `status_edit::apply`'s todo-complete
/// edit, without depending on it directly.
fn complete_todo(planning: &Path, file_name: &str) {
    let src = planning.join("todos/pending").join(file_name);
    let dst = planning.join("todos/completed").join(file_name);
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    let body = std::fs::read_to_string(&src).unwrap();
    let stamped = if let Some(after_open) = body.strip_prefix("---\n") {
        if let Some(end) = after_open.find("\n---\n") {
            let start = body.len() - after_open.len();
            format!(
                "---\ncompleted: 2026-09-02\n{}{}",
                &body[start..start + end],
                &body[start + end..]
            )
        } else {
            format!("---\ncompleted: 2026-09-02\n---\n{body}")
        }
    } else {
        format!("---\ncompleted: 2026-09-02\n---\n{body}")
    };
    std::fs::write(&dst, stamped).unwrap();
    std::fs::remove_file(&src).unwrap();
}

/// Mark a note complete via its frontmatter `status:` key, reproducing
/// `status_edit::apply`'s note-complete edit, without depending on it
/// directly.
fn mark_note_complete(planning: &Path, rel: &str) {
    let path = planning.join(rel);
    let body = std::fs::read_to_string(&path).unwrap();
    let stamped = format!("---\nstatus: done\n---\n{body}");
    std::fs::write(&path, stamped).unwrap();
}

#[test]
fn gsd_progress_still_parses_a_workspace_after_a_phase_status_write() {
    let Some(tools) = support::oracle_available() else {
        return;
    };
    let ws = temp_workspace();
    let planning = ws.path().join(".planning");

    let before = support::run_progress(&tools, ws.path()).expect("oracle ran before the write");
    let before_numbers = support::phase_numbers(&before);

    mark_phase_verified(&planning, "Phase 2: Coffee Acquisition**");

    let after = support::run_progress(&tools, ws.path()).expect("oracle ran after the write");
    assert!(
        support::phase_scope_is_readable(&after),
        "phase_scope must stay readable after a phase status write:\n{after}"
    );
    assert_eq!(
        before_numbers,
        support::phase_numbers(&after),
        "the phase list must not shrink or reorder after a phase status write"
    );
}

#[test]
fn gsd_progress_still_parses_a_workspace_after_a_quick_task_status_write() {
    let Some(tools) = support::oracle_available() else {
        return;
    };
    let ws = temp_workspace();
    let planning = ws.path().join(".planning");

    let before = support::run_progress(&tools, ws.path()).expect("oracle ran before the write");
    let before_numbers = support::phase_numbers(&before);

    upsert_quick_task_row(
        &planning,
        "| 260708-cc3 | Tidy the README | 2026-07-08 | e4f5a6b |  | ./quick/260708-cc3-tidy-readme/ |",
        "| 260901-xyz | Test task | 2026-09-02 |  | complete | ./quick/260901-xyz-test-task/ |",
    );

    let after = support::run_progress(&tools, ws.path()).expect("oracle ran after the write");
    assert!(
        support::phase_scope_is_readable(&after),
        "STATE.md damage must not take the workspace down:\n{after}"
    );
    assert_eq!(
        before_numbers,
        support::phase_numbers(&after),
        "phase parsing must be unaffected by a quick-task STATE.md write"
    );
}

#[test]
fn gsd_todo_complete_and_our_todo_write_agree_on_the_destination() {
    let Some(tools) = support::oracle_available() else {
        return;
    };
    let gsd_ws = temp_workspace();
    let our_ws = temp_workspace();
    let file_name = "2026-07-08-cache-secret.md";

    let output = std::process::Command::new("node")
        .arg(&tools)
        .arg("todo")
        .arg("complete")
        .arg(file_name)
        .arg("--project-dir")
        .arg(gsd_ws.path())
        .output()
        .expect("run gsd-tools todo complete");
    println!("gsd-conformance: RAN");
    assert!(
        output.status.success(),
        "gsd-tools todo complete failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    complete_todo(&our_ws.path().join(".planning"), file_name);

    let gsd_dest = gsd_ws
        .path()
        .join(".planning/todos/completed")
        .join(file_name);
    let our_dest = our_ws
        .path()
        .join(".planning/todos/completed")
        .join(file_name);
    assert!(gsd_dest.exists(), "gsd-tools landed the todo in completed/");
    assert!(
        our_dest.exists(),
        "our writer landed the todo in completed/"
    );
    assert!(
        !gsd_ws
            .path()
            .join(".planning/todos/pending")
            .join(file_name)
            .exists(),
        "gsd-tools removed the pending copy"
    );
    assert!(
        !our_ws
            .path()
            .join(".planning/todos/pending")
            .join(file_name)
            .exists(),
        "our writer removed the pending copy"
    );
}

#[test]
fn a_note_status_write_leaves_the_workspace_readable() {
    let Some(tools) = support::oracle_available() else {
        return;
    };
    let ws = temp_workspace();
    let planning = ws.path().join(".planning");

    mark_note_complete(&planning, "notes/2026-07-08-grinder-timing.md");

    let after = support::run_progress(&tools, ws.path()).expect("oracle ran after the write");
    assert!(
        support::phase_scope_is_readable(&after),
        "a note status write must leave the workspace readable:\n{after}"
    );
}
