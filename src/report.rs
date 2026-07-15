use crate::color;
use crate::model::{Phase, QuickTask, Stage, StateMeta, Todo};
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn render(
    out: &mut impl Write,
    planning: &Path,
    state: &StateMeta,
    phases: &[Phase],
    quick_tasks: &[QuickTask],
    todos: &[Todo],
    use_color: bool,
) -> io::Result<()> {
    let c = |code: &'static str| if use_color { code } else { "" };

    let title = if state.project_title.is_empty() {
        "GSD Project".to_string()
    } else {
        state.project_title.clone()
    };

    // Top border titled by the project itself (not a generic "GSD STATUS"),
    // padded with ─ to the box's 63-column width. The title leads the banner, so
    // the separate title line below is gone.
    let top = {
        let lead = format!("╭─ {title} ");
        let fill = 63usize.saturating_sub(lead.chars().count() + 1);
        format!("{lead}{}╮", "─".repeat(fill))
    };

    writeln!(out)?;
    writeln!(
        out,
        "{bold}{cyan}{top}{reset}",
        bold = c(color::BOLD),
        cyan = c(color::CYAN),
        top = top,
        reset = c(color::RESET),
    )?;
    writeln!(out, "  path: {p}", p = short_planning(planning))?;

    let milestone = if state.milestone.is_empty() {
        "—".to_string()
    } else if state.milestone_name.is_empty() {
        state.milestone.clone()
    } else {
        format!("{} ({})", state.milestone, state.milestone_name)
    };
    let status_color = match state.status.as_str() {
        "ready_to_plan" | "planning" => color::BLUE,
        "executing" => color::YELLOW,
        "verified" | "complete" | "completed" | "shipped" => color::GREEN,
        _ => color::MAGENTA,
    };
    let status_str = if state.status.is_empty() {
        "—"
    } else {
        state.status.as_str()
    };
    writeln!(
        out,
        "  milestone: {bold}{m}{reset}    status: {sc}{s}{reset}",
        bold = c(color::BOLD),
        m = milestone,
        sc = c(status_color),
        s = status_str,
        reset = c(color::RESET),
    )?;

    let total_phases = state.total_phases.max(phases.len() as u32);
    let completed_phases = phases.iter().filter(|p| p.stage == Stage::Verified).count() as u32;
    let percent = if total_phases == 0 {
        0
    } else {
        (completed_phases * 100) / total_phases
    };

    // The phase/plan tallies live in the Roadmap and phase rows below, so the
    // banner shows only the headline percentage — no duplicated counts here.
    writeln!(
        out,
        "  progress:  {bar} {bold}{bgreen}{pct:>3}%{reset}",
        bar = progress_bar(percent, 24, use_color),
        bold = c(color::BOLD),
        bgreen = c(color::BRIGHT_GREEN),
        pct = percent,
        reset = c(color::RESET),
    )?;
    writeln!(
        out,
        "{cyan}╰─────────────────────────────────────────────────────────────╯{reset}",
        cyan = c(color::CYAN),
        reset = c(color::RESET),
    )?;
    writeln!(out)?;

    // Roadmap section — the project-level ROADMAP.md, openable from the TUI.
    // Shown above the Phases list only when a roadmap exists (phases parse from
    // it), so brand-new projects with no ROADMAP.md yet don't display it. The
    // title matches the "Phases" heading; the status line mirrors a phase row:
    // a completion bullet (green ✓ when every phase is done, else yellow ●) and
    // "Phases x/y <state>".
    if !phases.is_empty() {
        let complete = completed_phases == total_phases;
        let (icon, icon_color, state) = if complete {
            ("✓", color::GREEN, "complete")
        } else {
            ("●", color::YELLOW, "in progress")
        };
        writeln!(
            out,
            "  {bold}Roadmap{reset}",
            bold = c(color::BOLD),
            reset = c(color::RESET),
        )?;
        writeln!(
            out,
            "  {dim}{line}{reset}",
            dim = c(color::DIM),
            line = "─".repeat(63),
            reset = c(color::RESET),
        )?;
        writeln!(
            out,
            "  {ic}{icon}{reset}  {bold}Phases {cp}/{tp}{reset}  {ic}{state}{reset}",
            ic = c(icon_color),
            icon = icon,
            bold = c(color::BOLD),
            cp = completed_phases,
            tp = total_phases,
            state = state,
            reset = c(color::RESET),
        )?;
        // Separate the Roadmap section from the Phases heading with the same
        // one-line gap that sits above the Next block.
        writeln!(out)?;
    }

    // Phases section — hidden entirely when there are no phases (no roadmap
    // parsed), so a brand-new workspace shows neither Roadmap nor an empty
    // Phases heading.
    if !phases.is_empty() {
        writeln!(
            out,
            "  {bold}Phases{reset}",
            bold = c(color::BOLD),
            reset = c(color::RESET)
        )?;
        writeln!(
            out,
            "  {dim}{line}{reset}",
            dim = c(color::DIM),
            line = "─".repeat(63),
            reset = c(color::RESET),
        )?;
    }

    for ph in phases {
        let (icon, icon_color) = phase_icon(ph);
        let total = ph.plans.len();
        let done = ph.plans.iter().filter(|p| p.checked).count();
        let plan_col = if total == 0 {
            "    —    ".to_string()
        } else {
            format!("{:>2}/{:<2} plans", done, total)
        };
        let title = truncate(&ph.title, 34);
        writeln!(
            out,
            "  {ic}{icon}{reset}  {bold}Phase {id:<3}{reset} {title:<34}  {pc}  {sc}{stage}{reset}",
            ic = c(icon_color),
            icon = icon,
            bold = c(color::BOLD),
            id = ph.id,
            title = title,
            pc = plan_col,
            sc = c(ph.stage.color()),
            stage = ph.stage.label(),
            reset = c(color::RESET),
        )?;
    }

    if !phases.is_empty() {
        writeln!(out)?;
    }

    // Tasks — quick tasks (`.planning/quick/`), between Phases and Todos.
    // Rendered only when active tasks exist; unlike Todos' icon-only rows,
    // each row shows icon + title + a text label (D-06/D-07), pulled only
    // from QuickTaskStatus's own methods (no ad-hoc string matching here).
    if !quick_tasks.is_empty() {
        writeln!(
            out,
            "  {bold}Tasks{reset}",
            bold = c(color::BOLD),
            reset = c(color::RESET)
        )?;
        writeln!(
            out,
            "  {dim}{line}{reset}",
            dim = c(color::DIM),
            line = "─".repeat(63),
            reset = c(color::RESET),
        )?;
        for task in quick_tasks {
            writeln!(
                out,
                "  {sc}{icon}{reset}  {title}   {sc}{label}{reset}",
                sc = c(task.status.color()),
                icon = task.status.icon(),
                title = truncate(&task.title, 55),
                label = task.status.label(),
                reset = c(color::RESET),
            )?;
        }
        writeln!(out)?;
    }

    // Todos — its own top-level section (heading + divider), between Phases and
    // Next, styled like the other sections. Rendered only when todos exist.
    if !todos.is_empty() {
        writeln!(
            out,
            "  {bold}Todos{reset}",
            bold = c(color::BOLD),
            reset = c(color::RESET)
        )?;
        writeln!(
            out,
            "  {dim}{line}{reset}",
            dim = c(color::DIM),
            line = "─".repeat(63),
            reset = c(color::RESET),
        )?;
        for todo in todos {
            let area = match &todo.area {
                Some(a) => format!(
                    "   {dim}{a}{reset}",
                    dim = c(color::DIM),
                    a = a,
                    reset = c(color::RESET)
                ),
                None => String::new(),
            };
            // Keep the ○ bullet for every todo row (the selection highlight
            // counts rows by it); completed todos earn a trailing "done" tag.
            let done = if todo.completed {
                format!(
                    "   {g}done{reset}",
                    g = c(color::GREEN),
                    reset = c(color::RESET)
                )
            } else {
                String::new()
            };
            writeln!(
                out,
                "  {grey}○{reset}  {title}{area}{done}",
                grey = c(color::GREY),
                title = truncate(&todo.title, 55),
                area = area,
                done = done,
                reset = c(color::RESET),
            )?;
        }
        writeln!(out)?;
    }

    writeln!(
        out,
        "  {bold}Next{reset}",
        bold = c(color::BOLD),
        reset = c(color::RESET)
    )?;
    writeln!(
        out,
        "  {dim}{line}{reset}",
        dim = c(color::DIM),
        line = "─".repeat(63),
        reset = c(color::RESET),
    )?;
    if !state.next_action.is_empty() {
        for line in state.next_action.lines() {
            writeln!(out, "  {}", line)?;
        }
        writeln!(out)?;
    }

    for hint in suggest_commands(phases) {
        writeln!(
            out,
            "    {green}{cmd:<26}{reset}  {dim}{note}{reset}",
            green = c(color::GREEN),
            cmd = hint.cmd,
            dim = c(color::DIM),
            note = hint.note,
            reset = c(color::RESET),
        )?;
    }

    writeln!(out)?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

fn progress_bar(pct: u32, width: usize, use_color: bool) -> String {
    let pct = pct.min(100) as usize;
    let filled = (pct * width) / 100;
    let empty = width - filled;
    if use_color {
        format!(
            "{}{}{}{}{}{}",
            color::BOLD,
            color::BRIGHT_GREEN,
            "█".repeat(filled),
            color::GREY,
            "░".repeat(empty),
            color::RESET
        )
    } else {
        format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
    }
}

/// Compact workspace location for the banner: the directory that contains
/// `.planning` plus the `.planning` segment — e.g. "sample/.planning".
fn short_planning(p: &Path) -> String {
    let leaf = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".planning");
    match p
        .parent()
        .and_then(|par| par.file_name())
        .and_then(|n| n.to_str())
    {
        Some(parent) => format!("{parent}/{leaf}"),
        None => leaf.to_string(),
    }
}

fn phase_icon(ph: &Phase) -> (&'static str, &'static str) {
    if ph.roadmap_checked || ph.stage == Stage::Verified {
        ("✓", color::GREEN)
    } else if matches!(ph.stage, Stage::Executing | Stage::Executed) {
        ("●", color::YELLOW)
    } else if matches!(ph.stage, Stage::Planned) {
        ("◐", color::BLUE)
    } else if matches!(ph.stage, Stage::Discussing | Stage::Discussed) {
        ("◌", color::MAGENTA)
    } else {
        ("·", color::GREY)
    }
}

struct Hint {
    cmd: String,
    note: &'static str,
}

fn suggest_commands(phases: &[Phase]) -> Vec<Hint> {
    let active = phases.iter().find(|p| p.stage != Stage::Verified);
    let mut out = Vec::new();
    match active {
        Some(p) => match p.stage {
            Stage::NotStarted => {
                out.push(Hint {
                    cmd: format!("/gsd-discuss-phase {}", p.id),
                    note: "gather context for the next phase",
                });
                out.push(Hint {
                    cmd: "/gsd-progress".into(),
                    note: "let GSD decide what to do next",
                });
            }
            Stage::Discussing => {
                out.push(Hint {
                    cmd: format!("/gsd-discuss-phase {}", p.id),
                    note: "resume the open discussion checkpoint",
                });
                out.push(Hint {
                    cmd: format!("/gsd-plan-phase {}", p.id),
                    note: "skip ahead to planning once discussion is locked",
                });
            }
            Stage::Discussed => {
                out.push(Hint {
                    cmd: format!("/gsd-plan-phase {}", p.id),
                    note: "produce PLAN.md from CONTEXT",
                });
            }
            Stage::Planned => {
                out.push(Hint {
                    cmd: format!("/gsd-execute-phase {}", p.id),
                    note: "start executing plans",
                });
            }
            Stage::Executing => {
                out.push(Hint {
                    cmd: format!("/gsd-execute-phase {}", p.id),
                    note: "continue executing remaining plans",
                });
                out.push(Hint {
                    cmd: "/gsd-progress".into(),
                    note: "show concrete next step",
                });
            }
            Stage::Executed => {
                out.push(Hint {
                    cmd: "/gsd-verify-work".into(),
                    note: "validate the implementation against UAT",
                });
                out.push(Hint {
                    cmd: "/gsd-ship".into(),
                    note: "open PR once verified",
                });
            }
            Stage::Verified => {}
        },
        None => {
            out.push(Hint {
                cmd: "/gsd-complete-milestone".into(),
                note: "all phases verified — archive milestone",
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_title_is_the_project_name() {
        let state = StateMeta {
            project_title: "Robot Coffee Service".into(),
            ..Default::default()
        };
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &state,
            &[],
            &[],
            &[],
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains("╭─ Robot Coffee Service ─"),
            "project name should title the banner border:\n{out}"
        );
        assert!(!out.contains("GSD STATUS"), "generic title dropped:\n{out}");
    }

    #[test]
    fn short_planning_shows_parent_dir_and_planning() {
        assert_eq!(
            short_planning(Path::new("sample/.planning")),
            "sample/.planning"
        );
        assert_eq!(
            short_planning(Path::new("/a/b/gsd-status-ui/work/.planning")),
            "work/.planning"
        );
        assert_eq!(short_planning(Path::new(".planning")), ".planning");
    }

    #[test]
    fn progress_bar_uses_a_bright_bold_fill_when_colored() {
        let bar = progress_bar(50, 10, true);
        assert!(bar.contains(color::BRIGHT_GREEN), "bright fill: {bar:?}");
        assert!(bar.contains(color::BOLD), "bold fill: {bar:?}");
        // No color escapes at all when color is off.
        assert_eq!(progress_bar(50, 10, false), "[#####-----]");
    }

    #[test]
    fn renders_roadmap_section_above_phases_when_phases_exist() {
        let phases = crate::planning::load_phases(Path::new("sample/.planning"));
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &phases,
            &[],
            &[],
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        let roadmap = out.find("Roadmap").expect("roadmap title present");
        let phase_list = out.find("Navigation Skeleton").expect("phase list present");
        assert!(
            roadmap < phase_list,
            "roadmap section must sit above the phase list:\n{out}"
        );
        // Status line: "Phases x/y <state>" with a not-all-complete bullet.
        assert!(out.contains("Phases 1/3"), "roadmap shows x/y:\n{out}");
        assert!(out.contains("in progress"), "roadmap shows state:\n{out}");
        assert!(out.contains("●"), "in-progress bullet:\n{out}");
    }

    #[test]
    fn roadmap_section_shows_complete_when_all_phases_verified() {
        let phases = vec![
            Phase {
                id: "1".into(),
                title: "A".into(),
                roadmap_checked: true,
                plans: vec![],
                dir: None,
                stage: Stage::Verified,
            },
            Phase {
                id: "2".into(),
                title: "B".into(),
                roadmap_checked: true,
                plans: vec![],
                dir: None,
                stage: Stage::Verified,
            },
        ];
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &phases,
            &[],
            &[],
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("Phases 2/2"), "{out}");
        assert!(out.contains("complete"), "{out}");
        assert!(!out.contains("in progress"), "{out}");
        assert!(out.contains("✓"), "complete bullet:\n{out}");
    }

    #[test]
    fn omits_roadmap_row_when_no_phases() {
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &[],
            &[],
            &[],
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            !out.contains("Roadmap"),
            "no roadmap row when there are no phases:\n{out}"
        );
        // The Phases heading and Roadmap row are hidden too, so no capitalized
        // "Phases" appears anywhere.
        assert!(
            !out.contains("Phases"),
            "no Phases heading when there are no phases:\n{out}"
        );
    }

    #[test]
    fn omits_todos_block_when_empty() {
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &[],
            &[],
            &[],
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(!out.contains("Todos"), "{out}");
    }

    #[test]
    fn renders_tasks_section_between_phases_and_todos() {
        let phases = crate::planning::load_phases(Path::new("sample/.planning"));
        let quick_tasks = vec![crate::model::QuickTask {
            id: "260709-aa1".into(),
            title: "Add dark-mode toggle".into(),
            dir: std::path::PathBuf::from("sample/.planning/quick/260709-aa1-add-dark-mode-toggle"),
            status: crate::model::QuickTaskStatus::InProgress,
        }];
        let todos = vec![Todo {
            title: "Do the thing".into(),
            area: Some("tooling".into()),
            slug: "2026-07-07-do-the-thing".into(),
            path: std::path::PathBuf::from("2026-07-07-do-the-thing.md"),
            completed: false,
        }];
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &phases,
            &quick_tasks,
            &todos,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("Tasks"), "{out}");
        assert!(out.contains("Add dark-mode toggle"), "{out}");
        assert!(out.contains("in progress"), "{out}");
        let phases_idx = out.find("Phases").expect("phases heading");
        let tasks_idx = out.find("Tasks").expect("tasks heading");
        let todos_idx = out.find("Todos").expect("todos heading");
        assert!(
            tasks_idx > phases_idx && tasks_idx < todos_idx,
            "Tasks must sit between Phases and Todos:\n{out}"
        );
    }

    #[test]
    fn omits_tasks_block_when_empty() {
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &[],
            &[],
            &[],
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(!out.contains("Tasks"), "{out}");
    }

    #[test]
    fn renders_todos_section_between_phases_and_next() {
        let todos = vec![Todo {
            title: "Do the thing".into(),
            area: Some("tooling".into()),
            slug: "2026-07-07-do-the-thing".into(),
            path: std::path::PathBuf::from("2026-07-07-do-the-thing.md"),
            completed: false,
        }];
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &[],
            &[],
            &todos,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("Todos"), "{out}");
        assert!(out.contains("○"), "{out}");
        assert!(out.contains("Do the thing"), "{out}");
        assert!(out.contains("tooling"), "{out}");
        // Its own section, above Next.
        let todos_idx = out.find("Todos").expect("todos heading");
        let next_idx = out.find("Next").expect("next heading");
        assert!(todos_idx < next_idx, "Todos must sit above Next:\n{out}");
    }
}
