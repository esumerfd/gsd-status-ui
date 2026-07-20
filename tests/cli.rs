use std::process::Command;

fn run(args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_gsd-status"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Run the binary and capture (stderr, exit code) — for error-path assertions.
fn run_stderr(args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_gsd-status"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run binary");
    (
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn no_planning_directory_prints_actionable_error() {
    // A directory with no .planning/ in it or any ancestor.
    let tmp = std::env::temp_dir().join(format!("gsd-status-no-planning-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create temp dir");

    let (stderr, code) = run_stderr(&[tmp.to_str().unwrap()]);
    assert_eq!(code, 2, "missing .planning/ exits 2; stderr={stderr}");
    assert!(
        stderr.contains("not a GSD directory"),
        "error should name the situation: {stderr}"
    );
    assert!(
        stderr.contains("/gsd-core:new-project"),
        "error should point to the fix: {stderr}"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn plain_report_renders_sample_workspace() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
    assert!(stdout.contains("Phase 2"), "{stdout}");
    assert!(stdout.contains("executing"), "{stdout}");
    // The phase count now lives only in the Roadmap row, not the banner.
    assert!(stdout.contains("Phases 1/3"), "{stdout}");
    assert!(
        !stdout.contains("phases · "),
        "banner must not duplicate the phase/plan counts:\n{stdout}"
    );
}

#[test]
fn plain_flag_is_accepted_before_path() {
    let (stdout, code) = run(&["--plain", "sample"]);
    assert_eq!(code, 0, "--plain must not be treated as a path");
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
}

#[test]
fn no_tui_alias_works() {
    let (stdout, code) = run(&["--no-tui", "sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
}

#[test]
fn plain_report_lists_in_progress_quick_task_between_phases_and_todos() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Tasks"), "{stdout}");
    assert!(stdout.contains("Add dark-mode toggle"), "{stdout}");
    assert!(stdout.contains("in progress"), "{stdout}");
    let phases = stdout.find("Phases").expect("Phases heading present");
    let tasks = stdout.find("Tasks").expect("Tasks heading present");
    let todos = stdout.find("Todos").expect("Todos heading present");
    assert!(
        tasks > phases && tasks < todos,
        "Tasks section must render between Phases and Todos:\n{stdout}"
    );
}

#[test]
fn plain_report_shows_failed_status_raw_and_hides_completed() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Fix export crash"), "{stdout}");
    assert!(stdout.contains("verification failed"), "{stdout}");
    assert!(stdout.contains("✗"), "{stdout}");
    assert!(
        !stdout.contains("Tidy the README"),
        "completed task must be hidden: {stdout}"
    );
}

#[test]
fn plain_report_lists_pending_todos_between_phases_and_next() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Todos"), "{stdout}");
    let title = "Official signed build process for pr-monitor apps";
    assert!(stdout.contains(title), "{stdout}");
    let todos = stdout.find("Todos").expect("Todos heading present");
    let next = stdout.find("Next").expect("Next heading present");
    let todo_title = stdout.find(title).expect("todo title present");
    // Todos is its own section above Next; the title sits within it.
    assert!(todos < next, "Todos section must render above Next");
    assert!(
        todo_title > todos && todo_title < next,
        "todo title must render inside the Todos section (above Next)"
    );
}

#[test]
fn plain_report_lists_active_debug_session_prefixed_debug_in_todos() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    // The full trigger is 60 chars, past report.rs's 55-char todo-row
    // truncation, so only a prefix survives in the rendered row.
    assert!(
        stdout.contains("Debug: the kiosk app crashes when checking out an empt"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("receipt printer times out"),
        "resolved debug session must stay hidden by default: {stdout}"
    );
    let todos = stdout.find("Todos").expect("Todos heading present");
    let next = stdout.find("Next").expect("Next heading present");
    let debug_row = stdout
        .find("Debug: the kiosk app crashes")
        .expect("debug row present");
    assert!(
        debug_row > todos && debug_row < next,
        "debug row must render inside the Todos section (above Next)"
    );
}
