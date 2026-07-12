use std::env;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

mod color;
mod model;
mod planning;
mod report;
mod tui;

fn main() -> ExitCode {
    let mut path: Option<PathBuf> = None;
    let mut plain = false;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "--plain" | "--no-tui" => plain = true,
            other => path = Some(PathBuf::from(other)),
        }
    }
    let start = path.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let planning = match planning::find_planning_dir(&start) {
        Some(p) => p,
        None => {
            eprintln!(
                "gsd-status: not a GSD directory (no .planning/ found from {}).\n\
                 Run /gsd-core:new-project to start.",
                start.display()
            );
            return ExitCode::from(2);
        }
    };

    let state = planning::load_state(&planning);
    let phases = planning::load_phases(&planning);
    let todos = planning::load_todos(&planning);
    let quick_tasks = planning::load_quick_tasks(&planning);

    let interactive = !plain && io::stdout().is_terminal();
    if interactive {
        match tui::run(&planning, &state, &phases, &todos) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("gsd-status: {e}");
                ExitCode::FAILURE
            }
        }
    } else {
        let use_color = io::stdout().is_terminal() && env::var("NO_COLOR").is_err();
        let mut out = io::stdout().lock();
        report::render(
            &mut out,
            &planning,
            &state,
            &phases,
            &quick_tasks,
            &todos,
            use_color,
        )
        .ok();
        ExitCode::SUCCESS
    }
}

fn print_help() {
    println!("gsd-status — interactive status view for a GSD planning workspace");
    println!();
    println!("Usage:");
    println!("  gsd-status [--plain|--no-tui] [path]");
    println!();
    println!("If [path] is omitted, walks up from the current directory looking for .planning/.");
    println!("With a TTY it opens the tabbed TUI; otherwise (or with --plain) it prints a report.");
    println!("Honors NO_COLOR in plain mode.");
    println!();
    println!("Keys (TUI) — modal: q always backs out one level (doc -> status -> exit).");
    println!("  ?         in-app help dialog listing every key by mode");
    println!("  [status]  j/k browse phase/steps · Enter open plan · o open-doc dialog · q quit");
    println!(
        "  [doc]     j/k/arrows scroll · d/u or PgDn/PgUp page · g/G top/bottom · q/Esc to status"
    );
    println!("            / search (type · Enter find · Esc cancel) · n/N next/prev match");
    println!("  anywhere  Ctrl-j/Ctrl-k change step · Tab / 1..9 switch tab · Ctrl-x close tab");
    println!("            Ctrl-q / Ctrl-C quit");
    println!("  dialog    j/k select · Enter open · Esc cancel");
}
