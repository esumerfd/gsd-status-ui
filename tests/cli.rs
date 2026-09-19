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

/// Run the binary with extra environment variables set (beyond `NO_COLOR`).
fn run_with_env(args: &[&str], env_pairs: &[(&str, &str)]) -> (String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gsd-status"));
    cmd.args(args).env("NO_COLOR", "1");
    for (k, v) in env_pairs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
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
    let (stdout, code) = run(&["sample/normal"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
    assert!(stdout.contains("Phase 2"), "{stdout}");
    assert!(stdout.contains("executing"), "{stdout}");
    // The phase count now lives only in the Roadmap row, not the banner.
    assert!(stdout.contains("Phases 2/9"), "{stdout}");
    assert!(
        !stdout.contains("phases · "),
        "banner must not duplicate the phase/plan counts:\n{stdout}"
    );
}

#[test]
fn version_flag_prints_the_package_version() {
    let (stdout, code) = run(&["--version"]);
    assert_eq!(code, 0, "--version must exit clean");
    assert_eq!(
        stdout.trim(),
        format!("gsd-status {}", env!("CARGO_PKG_VERSION")),
        "--version reports the compiled-in package version"
    );
}

#[test]
fn plain_flag_is_accepted_before_path() {
    let (stdout, code) = run(&["--plain", "sample/normal"]);
    assert_eq!(code, 0, "--plain must not be treated as a path");
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
}

#[test]
fn no_tui_alias_works() {
    let (stdout, code) = run(&["--no-tui", "sample/normal"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
}

#[test]
fn plain_report_lists_in_progress_quick_task_between_phases_and_todos() {
    let (stdout, code) = run(&["sample/normal"]);
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
    let (stdout, code) = run(&["sample/normal"]);
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
    let (stdout, code) = run(&["sample/normal"]);
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
    let (stdout, code) = run(&["sample/normal"]);
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

#[test]
fn plain_report_shows_the_project_row_for_the_pre_roadmap_sample() {
    // sample/research/ is a workspace that finished research but has no
    // ROADMAP.md yet, so the Roadmap row is absent and the Project row is what
    // reaches PROJECT.md and REQUIREMENTS.md.
    let (stdout, code) = run(&["sample/research"]);
    assert_eq!(code, 0, "{stdout}");
    let project = stdout.find("  Project").expect("Project row present");
    let research = stdout.find("  Research").expect("Research row present");
    assert!(
        project < research,
        "Project row sits above Research:\n{stdout}"
    );
    assert!(!stdout.contains("Roadmap"), "no roadmap yet:\n{stdout}");
}

#[test]
fn workstream_mode_defaults_to_the_active_workstream_pointer() {
    let (stdout, code) = run(&["--plain", "sample/workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("path: workstreams/workstreams/beta"),
        "{stdout}"
    );
    assert!(stdout.contains("Beta First Phase"), "{stdout}");
    assert!(!stdout.contains("Alpha First Phase"), "{stdout}");
}

#[test]
fn ws_flag_overrides_the_active_workstream_pointer() {
    let (stdout, code) = run(&["--ws", "alpha", "--plain", "sample/workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("path: workstreams/workstreams/alpha"),
        "{stdout}"
    );
    assert!(stdout.contains("Alpha First Phase"), "{stdout}");
    assert!(!stdout.contains("Beta First Phase"), "{stdout}");
}

#[test]
fn gsd_workstream_env_var_selects_a_workstream_with_no_flag() {
    let (stdout, code) = run_with_env(
        &["--plain", "sample/workstreams"],
        &[("GSD_WORKSTREAM", "alpha")],
    );
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("Alpha First Phase"), "{stdout}");
    assert!(!stdout.contains("Beta First Phase"), "{stdout}");
}

#[test]
fn unknown_ws_flag_exits_2_and_lists_known_workstreams() {
    let (stderr, code) = run_stderr(&["--ws", "nope", "--plain", "sample/workstreams"]);
    assert_eq!(code, 2, "stderr={stderr}");
    assert!(stderr.contains("nope"), "{stderr}");
    assert!(stderr.contains("alpha"), "{stderr}");
    assert!(stderr.contains("beta"), "{stderr}");
}

#[test]
fn ws_flag_path_traversal_is_rejected() {
    let (stderr, code) = run_stderr(&["--ws", "../../etc", "--plain", "sample/workstreams"]);
    assert_eq!(code, 2, "stderr={stderr}");
}

#[test]
fn flat_workspace_output_is_unchanged_by_workstream_support() {
    let (stdout, code) = run(&["--plain", "sample/normal"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("path: normal"), "{stdout}");
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
}

// ─────────────────────── sample/project-and-workstreams/ ───────────────────
//
// Characterization tests. They pin CURRENT gsd-status behavior on the
// partially-migrated shape — root project files (PROJECT.md, ROADMAP.md,
// phases/, research/, todos/) coexisting with workstreams that have started
// taking over planning. They do NOT assert desired behavior; the three gaps
// they document are captured, not fixed, by explicit scope decision (see the
// plan's <deferred> block).

#[test]
fn project_and_workstreams_alpha_research_shadows_the_root_research_docs() {
    // documents gap: folder shadowing is total, not a union. Alpha's own
    // research/C.md replaces the root's research/{A,B}.md entirely instead of
    // merging with them, so the Research row counts only alpha's one file.
    let (stdout, code) = run(&["--plain", "sample/project-and-workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("Research") && stdout.contains("1 file"),
        "alpha's scoped research/ must shadow the root's, counting only its own file:\n{stdout}"
    );
}

#[test]
fn project_and_workstreams_beta_scoped_todo_is_invisible_but_root_todo_is_not() {
    // documents gap: a scoped todos/ is invisible. `load_todos` is hard-rooted
    // and `todos` sits in OWNED_FOLDERS, so a todo written under
    // workstreams/beta/todos/pending/ renders nowhere, while the root todo
    // (visible from every workstream) still does.
    let (stdout, code) = run(&["--ws", "beta", "--plain", "sample/project-and-workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("Send a welcome email to new members"),
        "the root todo is hard-rooted and must still render:\n{stdout}"
    );
    assert!(
        !stdout.contains("Retry failed notification sends"),
        "a workstream-scoped todo must render nowhere: {stdout}"
    );
}

#[test]
fn project_and_workstreams_alpha_has_roadmap_split_brain() {
    // documents gap: roadmap split-brain. Alpha has no scoped ROADMAP.md, so
    // there is no Roadmap row and no Phases section even though a root
    // ROADMAP.md and a root phases/ directory both exist; the root roadmap
    // stays openable, pinned first in the Project docs row.
    let (stdout, code) = run(&["--plain", "sample/project-and-workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        !stdout.contains("Roadmap"),
        "alpha has no scoped ROADMAP.md, so no Roadmap row:\n{stdout}"
    );
    assert!(
        !stdout.contains("Phases"),
        "no scoped roadmap means no Phases section:\n{stdout}"
    );
    assert!(
        stdout.contains("Project"),
        "the root ROADMAP.md stays reachable via the Project docs row:\n{stdout}"
    );
}

#[test]
fn project_and_workstreams_beta_is_the_healthy_comparison_arm() {
    // Control arm proving tests 1-3 above are about federation rules, not a
    // broken fixture: beta has its own scoped ROADMAP.md, so its Roadmap row
    // and Phases section both render normally.
    let (stdout, code) = run(&["--ws", "beta", "--plain", "sample/project-and-workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("Roadmap"),
        "beta has its own ROADMAP.md:\n{stdout}"
    );
    assert!(
        stdout.contains("Phases"),
        "beta's own roadmap backs a Phases section:\n{stdout}"
    );
}

#[test]
fn project_and_workstreams_defaults_to_alpha_via_the_active_workstream_pointer() {
    let (stdout, code) = run(&["--plain", "sample/project-and-workstreams"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("path: project-and-workstreams/workstreams/alpha"),
        "{stdout}"
    );
}
