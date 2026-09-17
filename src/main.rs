use std::env;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

mod color;
mod model;
mod planning;
mod report;
mod status_edit;
mod tui;
mod workstream;

fn main() -> ExitCode {
    let mut path: Option<PathBuf> = None;
    let mut plain = false;
    let mut ws: Option<String> = None;
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                println!("gsd-status {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--plain" | "--no-tui" => plain = true,
            "--ws" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    ws = Some(v.clone());
                }
            }
            other if other.starts_with("--ws=") => {
                ws = Some(other["--ws=".len()..].to_string());
            }
            other => path = Some(PathBuf::from(other)),
        }
        i += 1;
    }
    let start = path.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let root = match planning::find_planning_dir(&start) {
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

    let env_ws = env::var("GSD_WORKSTREAM").ok();
    let resolved_ws = workstream::resolve(&root, ws.as_deref(), env_ws.as_deref());
    if ws.is_some() && resolved_ws.is_none() {
        let known = workstream::list(&root);
        let known = if known.is_empty() {
            "none".to_string()
        } else {
            known.join(", ")
        };
        eprintln!(
            "gsd-status: unknown workstream '{}' (known: {known}).",
            ws.unwrap_or_default()
        );
        return ExitCode::from(2);
    }
    let planning = workstream::scoped_dir(&root, resolved_ws.as_deref());

    let state = planning::load_state(&planning);
    let phases = planning::load_phases(&planning);
    let todos = planning::load_todos(&planning, false);
    let quick_tasks = planning::load_quick_tasks(&planning, false);

    let interactive = !plain && io::stdout().is_terminal();
    if interactive {
        match tui::run(&planning, &state, &phases, &quick_tasks, &todos) {
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
            &report::Report::new(&planning, &state)
                .phases(&phases)
                .quick_tasks(&quick_tasks)
                .todos(&todos)
                .use_color(use_color),
        )
        .ok();
        ExitCode::SUCCESS
    }
}

fn print_help() {
    println!("gsd-status — interactive status view for a GSD planning workspace");
    println!();
    println!("Usage:");
    println!("  gsd-status [--plain|--no-tui] [--ws <name>] [path]");
    println!("  gsd-status --version");
    println!();
    println!("If [path] is omitted, walks up from the current directory looking for .planning/.");
    println!("With a TTY it opens the tabbed TUI; otherwise (or with --plain) it prints a report.");
    println!("Honors NO_COLOR in plain mode.");
    println!();
    println!("Workstreams (.planning/workstreams/<name>/):");
    println!("  --ws <name>       focus one workstream; unknown name exits 2");
    println!("  GSD_WORKSTREAM    env var fallback when --ws is not given");
    println!(
        "  Selection order: --ws > GSD_WORKSTREAM > .planning/active-workstream > first sorted"
    );
    println!("  A flat workspace (no .planning/workstreams/) behaves exactly as before.");
    println!();
    println!("Keys (TUI) — modal: q always backs out one level (doc -> status -> exit).");
    println!("  ?         in-app help dialog listing every key by mode");
    println!(
        "  [status]  j/k browse phase/steps · Enter open plan · o open-doc dialog · s set status · q quit"
    );
    println!(
        "  [doc]     j/k/arrows scroll · d/u or PgDn/PgUp page · g/G top/bottom · q/Esc to status"
    );
    println!("            / search (type · Enter find · Esc cancel) · n/N next/prev match");
    println!("  anywhere  Ctrl-j/Ctrl-k change step · Tab / 1..9 switch tab · Ctrl-x close tab");
    println!("            R peek roadmap · H show/hide completed work · Ctrl-q / Ctrl-C quit");
    println!("  dialog    j/k select · Enter open · Esc cancel");
}
