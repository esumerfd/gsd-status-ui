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

#[test]
fn plain_report_renders_sample_workspace() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Robot Coffee Service"), "{stdout}");
    assert!(stdout.contains("Phase 2"), "{stdout}");
    assert!(stdout.contains("executing"), "{stdout}");
    assert!(stdout.contains("1/3 phases"), "{stdout}");
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
fn plain_report_lists_pending_todos_under_next() {
    let (stdout, code) = run(&["sample"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Todos"), "{stdout}");
    let title = "Official signed build process for pr-monitor apps";
    assert!(stdout.contains(title), "{stdout}");
    let next = stdout.find("Next").expect("Next heading present");
    let todo = stdout.find(title).expect("todo title present");
    assert!(todo > next, "todos must render under the Next heading");
}
