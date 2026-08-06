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
    show_completed: bool,
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
        "ready_to_plan" | "planning" => color::BRIGHT_BLUE,
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
    let completed_phases = phases.iter().filter(|p| phase_settled(p)).count() as u32;
    let percent = (completed_phases * 100).checked_div(total_phases).unwrap_or(0);

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
        // An abandoned project reads as settled, not delivered: it earns the
        // full tally (nothing is still pending) but never the green ✓.
        let (icon, icon_color, state) = if state.is_abandoned() {
            ("⊘", color::GREY, "abandoned")
        } else if complete {
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

    // Docs-folder rows — one openable line each, sitting between the Roadmap and
    // Phases sections. Each names a group of `.planning` markdown files and its
    // file count (the files themselves open from the TUI). The folder list comes
    // from `discover_docs_sections`, the same discovery the TUI's nav entries and
    // `o` picker use, so a row drawn here is always navigable and vice versa —
    // `highlight_index` finds a row by its title, so a row the report doesn't
    // draw could not be selected. An empty group is already dropped by the
    // discovery, mirroring how the Tasks/Todos sections hide.
    //
    // Project (the `.planning` root — PROJECT.md, REQUIREMENTS.md, and the rest)
    // stands in for the Roadmap row, which is what reaches those files once a
    // ROADMAP.md exists, so it is included only while no phases parse; otherwise
    // a workspace mid-research has no way to open its requirements.
    let docs_sections = crate::planning::discover_docs_sections(planning, phases.is_empty());
    for section in &docs_sections {
        docs_folder_row(out, &section.title, section.documents.len(), use_color)?;
    }
    if !docs_sections.is_empty() {
        writeln!(out)?;
    }

    // Phases section — hidden entirely when there are no phases to list, so a
    // brand-new workspace shows neither Roadmap nor an empty Phases heading,
    // and a fully verified roadmap collapses to its Roadmap tally until `H`.
    // Finished phases drop out the way finished tasks and todos do; the
    // Roadmap row and progress bar above still count them.
    let visible_phases: Vec<&Phase> = phases
        .iter()
        .filter(|ph| show_completed || !phase_settled(ph))
        .collect();
    if !visible_phases.is_empty() {
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

    for ph in &visible_phases {
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

    if !visible_phases.is_empty() {
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

    // Others — notes, ideas, and seeds combined into a single section below the
    // Todos, above Next. One row per file, each tagged with its kind. Rendered
    // only when at least one of the three capture folders has files.
    let others = crate::planning::load_others(planning);
    if !others.is_empty() {
        writeln!(
            out,
            "  {bold}Others{reset}",
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
        for other in &others {
            writeln!(
                out,
                "  {grey}◇{reset}  {dim}{kind}:{reset} {title}",
                grey = c(color::GREY),
                dim = c(color::DIM),
                kind = other.kind.title(),
                title = truncate(&other.title, 55),
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

    for hint in suggest_commands(state, phases) {
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

/// One compact "folder of docs" row: a bold folder name, a dim dash fill to the
/// box width, and the file count (singular for one file). Used for every
/// discovered `.planning` subfolder, so the name can be any length — a name
/// wider than the label field eats into the dash fill instead of overrunning
/// the box.
fn docs_folder_row(
    out: &mut impl Write,
    name: &str,
    count: usize,
    use_color: bool,
) -> io::Result<()> {
    let c = |code: &'static str| if use_color { code } else { "" };
    let suffix = format!("{count} file{}", if count == 1 { "" } else { "s" });
    let label_width = 9;
    let used = name.chars().count().max(label_width) + 1 + 1 + suffix.chars().count();
    let fill = 63usize.saturating_sub(used).max(3);
    writeln!(
        out,
        "  {bold}{name:<label_width$}{reset} {dim}{dashes}{reset} {suffix}",
        bold = c(color::BOLD),
        name = name,
        dim = c(color::DIM),
        dashes = "─".repeat(fill),
        suffix = suffix,
        reset = c(color::RESET),
    )
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

/// Phases that earn a green ✓ — work actually delivered.
fn phase_verified(ph: &Phase) -> bool {
    ph.roadmap_checked || ph.stage == Stage::Verified
}

/// Settled work, for the `H` show/hide toggle: nothing here is still pending, so
/// it drops out of the list and counts toward the roadmap tally. Abandoned
/// phases settle alongside verified ones — they will never be worked again — but
/// [`phase_verified`] keeps them out of the completed count's meaning.
/// Shared with the TUI so a hidden row is also an unreachable entry.
pub(crate) fn phase_settled(ph: &Phase) -> bool {
    phase_verified(ph) || ph.stage == Stage::Abandoned
}

fn phase_icon(ph: &Phase) -> (&'static str, &'static str) {
    if ph.stage == Stage::Abandoned {
        ("⊘", color::GREY)
    } else if phase_verified(ph) {
        ("✓", color::GREEN)
    } else if matches!(ph.stage, Stage::Executing | Stage::Executed) {
        ("●", color::YELLOW)
    } else if matches!(ph.stage, Stage::Planned) {
        ("◐", color::BRIGHT_BLUE)
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

fn suggest_commands(state: &StateMeta, phases: &[Phase]) -> Vec<Hint> {
    // A shut-down project has no next step. Without this, every abandoned phase
    // still looks like pending work and the panel invites you to start phase 1.
    if state.is_abandoned() {
        return Vec::new();
    }
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
            Stage::Verified | Stage::Abandoned => {}
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
    fn banner_progress_is_zero_percent_when_there_are_no_phases() {
        // A workspace with nothing planned yet divides by a zero phase total;
        // the banner must read 0% rather than panicking.
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &[],
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains("progress:") && out.contains("  0%"),
            "no phases means 0% progress:\n{out}"
        );
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
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        let roadmap = out.find("Roadmap").expect("roadmap title present");
        let phase_list = out.find("Coffee Acquisition").expect("phase list present");
        assert!(
            roadmap < phase_list,
            "roadmap section must sit above the phase list:\n{out}"
        );
        // Status line: "Phases x/y <state>" with a not-all-complete bullet.
        assert!(out.contains("Phases 2/8"), "roadmap shows x/y:\n{out}");
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
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("Phases 2/2"), "{out}");
        assert!(out.contains("complete"), "{out}");
        assert!(!out.contains("in progress"), "{out}");
        assert!(out.contains("✓"), "complete bullet:\n{out}");
    }

    fn abandoned_workspace() -> (StateMeta, Vec<Phase>) {
        let state = StateMeta {
            status: "abandoned".into(),
            total_phases: 2,
            ..Default::default()
        };
        let phases = vec![
            Phase {
                id: "1".into(),
                title: "Own-Status Veto".into(),
                roadmap_checked: false,
                plans: vec![],
                dir: None,
                stage: Stage::Abandoned,
            },
            Phase {
                id: "2".into(),
                title: "Ancestor Chain".into(),
                roadmap_checked: false,
                plans: vec![],
                dir: None,
                stage: Stage::Abandoned,
            },
        ];
        (state, phases)
    }

    fn render_abandoned(show_completed: bool) -> String {
        let (state, phases) = abandoned_workspace();
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &state,
            &phases,
            &[],
            &[],
            show_completed,
            false,
        )
        .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn roadmap_section_reads_abandoned_for_an_abandoned_project() {
        let out = render_abandoned(false);
        assert!(
            out.contains("Phases 2/2"),
            "every phase accounted for:\n{out}"
        );
        assert!(
            out.contains("Phases 2/2  abandoned"),
            "roadmap row states abandonment:\n{out}"
        );
        assert!(!out.contains("in progress"), "not still running:\n{out}");
        assert!(
            !out.contains("2/2  complete"),
            "abandoned is not completed work:\n{out}"
        );
    }

    #[test]
    fn abandoned_project_suggests_no_next_commands() {
        let out = render_abandoned(false);
        assert!(
            !out.contains("/gsd-"),
            "a dead project invites no further work:\n{out}"
        );
    }

    #[test]
    fn abandoned_phase_rows_hide_by_default_and_read_abandoned_when_shown() {
        let hidden = render_abandoned(false);
        assert!(
            !hidden.contains("Own-Status Veto"),
            "abandoned rows hide like finished work:\n{hidden}"
        );

        let shown = render_abandoned(true);
        assert!(
            shown.contains("Own-Status Veto"),
            "revealed by show_completed:\n{shown}"
        );
        assert!(
            shown.contains("abandoned"),
            "row labelled abandoned:\n{shown}"
        );
        assert!(
            !shown.contains("not started"),
            "no longer claims pending work:\n{shown}"
        );
        assert!(
            !shown.contains('✓'),
            "no green tick for work never done:\n{shown}"
        );
    }

    #[test]
    fn hides_completed_phase_rows_until_show_completed() {
        let phases = crate::planning::load_phases(Path::new("sample/.planning"));
        let render_to_string = |show_completed| {
            let mut buf = Vec::new();
            render(
                &mut buf,
                Path::new("sample/.planning"),
                &StateMeta::default(),
                &phases,
                &[],
                &[],
                show_completed,
                false,
            )
            .unwrap();
            String::from_utf8(buf).unwrap()
        };

        let hidden = render_to_string(false);
        assert!(
            !hidden.contains("Navigation Skeleton"),
            "verified phase row hidden by default:\n{hidden}"
        );
        assert!(
            hidden.contains("Coffee Acquisition"),
            "unfinished phase rows still shown:\n{hidden}"
        );
        // The Roadmap tally counts every phase, hidden or not.
        assert!(
            hidden.contains("Phases 2/8"),
            "roadmap tally counts hidden phases:\n{hidden}"
        );

        let shown = render_to_string(true);
        assert!(
            shown.contains("Navigation Skeleton"),
            "verified phase row shown with show_completed:\n{shown}"
        );
    }

    #[test]
    fn omits_phases_heading_when_every_phase_is_completed_and_hidden() {
        let phases = vec![Phase {
            id: "1".into(),
            title: "A".into(),
            roadmap_checked: true,
            plans: vec![],
            dir: None,
            stage: Stage::Verified,
        }];
        let mut buf = Vec::new();
        render(
            &mut buf,
            Path::new("sample/.planning"),
            &StateMeta::default(),
            &phases,
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            !out.lines().any(|l| l.trim() == "Phases"),
            "empty Phases section hides its heading:\n{out}"
        );
        assert!(
            out.contains("Phases 1/1"),
            "the Roadmap row survives:\n{out}"
        );
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
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(!out.contains("Tasks"), "{out}");
    }

    fn one_phase() -> Vec<Phase> {
        vec![Phase {
            id: "1".into(),
            title: "Skeleton".into(),
            roadmap_checked: false,
            plans: vec![],
            dir: None,
            stage: Stage::NotStarted,
        }]
    }

    #[test]
    fn renders_intel_and_research_lines_between_roadmap_and_phases() {
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::create_dir_all(planning.join("intel")).unwrap();
        for name in ["ARCHITECTURE.md", "STACK.md"] {
            std::fs::write(planning.join("intel").join(name), "# i\n").unwrap();
        }
        std::fs::create_dir_all(planning.join("research")).unwrap();
        for name in ["A.md", "B.md", "C.md"] {
            std::fs::write(planning.join("research").join(name), "# r\n").unwrap();
        }

        let mut buf = Vec::new();
        render(
            &mut buf,
            planning,
            &StateMeta::default(),
            &one_phase(),
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();

        assert!(out.contains("Intel"), "intel row present:\n{out}");
        assert!(out.contains("2 files"), "intel file count:\n{out}");
        assert!(out.contains("Research"), "research row present:\n{out}");
        assert!(out.contains("3 files"), "research file count:\n{out}");

        let roadmap = out.find("Roadmap").expect("roadmap");
        let intel = out.find("Intel").expect("intel");
        let research = out.find("Research").expect("research");
        // The Phases *list* heading — the "Phases" that follows the intel rows.
        let phases_heading = out.rfind("Phases").expect("phases heading");
        assert!(
            roadmap < intel && intel < research && research < phases_heading,
            "order must be Roadmap, Intel, Research, Phases:\n{out}"
        );
    }

    #[test]
    fn intel_research_lines_use_singular_for_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::create_dir_all(planning.join("intel")).unwrap();
        std::fs::write(planning.join("intel").join("ONLY.md"), "# i\n").unwrap();

        let mut buf = Vec::new();
        render(
            &mut buf,
            planning,
            &StateMeta::default(),
            &one_phase(),
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("1 file"), "singular:\n{out}");
        assert!(!out.contains("1 files"), "no plural for one file:\n{out}");
    }

    #[test]
    fn docs_folder_row_stays_in_the_box_for_a_long_folder_name() {
        // Folder names are arbitrary now that rows are discovered, so a name
        // wider than the label field must eat into the dash fill rather than push
        // the count past the box edge. Both rows must be exactly as wide as the
        // 63-column divider the sections draw.
        for name in ["Intel", "Retrospectives"] {
            let mut buf = Vec::new();
            docs_folder_row(&mut buf, name, 2, false).unwrap();
            let row = String::from_utf8(buf).unwrap();
            let row = row.trim_end_matches('\n');
            assert_eq!(
                row.chars().count(),
                63 + 2, // the box width, plus the two-space indent
                "row must fill the box exactly, not overrun it: {row:?}"
            );
        }
    }

    #[test]
    fn renders_a_row_for_an_unowned_docs_folder() {
        // A folder no section owns — `reviews/` — gets its own count row, so a
        // file inside it has a row to be reached from.
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::create_dir_all(planning.join("reviews")).unwrap();
        std::fs::write(
            planning
                .join("reviews")
                .join("STK-EXAMPLE-pass-rate-audit.md"),
            "# audit\n",
        )
        .unwrap();

        let mut buf = Vec::new();
        render(
            &mut buf,
            planning,
            &StateMeta::default(),
            &one_phase(),
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();

        assert!(
            out.lines()
                .any(|l| l.trim_start().starts_with("Reviews") && l.contains("1 file")),
            "a Reviews row with a singular file count:\n{out}"
        );
    }

    #[test]
    fn omits_intel_and_research_when_folders_absent() {
        // A workspace with a roadmap but no intel/ or research/ folders.
        let dir = tempfile::tempdir().unwrap();
        let mut buf = Vec::new();
        render(
            &mut buf,
            dir.path(),
            &StateMeta::default(),
            &one_phase(),
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            !out.contains("Intel"),
            "no Intel row when folder absent:\n{out}"
        );
        assert!(
            !out.contains("Research"),
            "no Research row when folder absent:\n{out}"
        );
    }

    /// True when a docs row named `name` ("Project", "Intel", …) is present —
    /// matched on the row's own shape so the header box's title can't count.
    fn has_docs_row(out: &str, name: &str) -> bool {
        out.lines()
            .any(|l| l.trim_start().starts_with(name) && l.contains("file"))
    }

    #[test]
    fn renders_project_row_for_root_docs_before_a_roadmap_exists() {
        // Post-research, pre-roadmap: PROJECT.md and REQUIREMENTS.md exist but
        // no ROADMAP.md, so no phases parse and the Roadmap row is absent.
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::write(planning.join("PROJECT.md"), "# Demo\n").unwrap();
        std::fs::write(planning.join("REQUIREMENTS.md"), "# Reqs\n").unwrap();
        std::fs::create_dir_all(planning.join("research")).unwrap();
        std::fs::write(planning.join("research").join("STACK.md"), "# s\n").unwrap();

        let mut buf = Vec::new();
        render(
            &mut buf,
            planning,
            &StateMeta::default(),
            &[],
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(has_docs_row(&out, "Project"), "project row present:\n{out}");
        assert!(out.contains("2 files"), "root doc count:\n{out}");
        let project = out.find("  Project").expect("project row");
        let research = out.find("  Research").expect("research row");
        assert!(
            project < research,
            "Project row must sit above Research:\n{out}"
        );
    }

    #[test]
    fn omits_project_row_when_the_roadmap_row_carries_root_docs() {
        let dir = tempfile::tempdir().unwrap();
        let planning = dir.path();
        std::fs::write(planning.join("PROJECT.md"), "# Demo\n").unwrap();
        std::fs::write(planning.join("REQUIREMENTS.md"), "# Reqs\n").unwrap();

        let mut buf = Vec::new();
        render(
            &mut buf,
            planning,
            &StateMeta::default(),
            &one_phase(),
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("Roadmap"), "roadmap row present:\n{out}");
        assert!(
            !has_docs_row(&out, "Project"),
            "no Project row once the Roadmap row reaches root docs:\n{out}"
        );
    }

    #[test]
    fn omits_project_row_when_there_are_no_root_docs() {
        let dir = tempfile::tempdir().unwrap();
        let mut buf = Vec::new();
        render(
            &mut buf,
            dir.path(),
            &StateMeta::default(),
            &[],
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            !has_docs_row(&out, "Project"),
            "no Project row in an empty workspace:\n{out}"
        );
    }

    #[test]
    fn renders_others_section_between_todos_and_next() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("notes")).unwrap();
        std::fs::write(p.join("notes/2026-07-10-grinder.md"), "# Grinder\n").unwrap();
        std::fs::create_dir_all(p.join("ideas")).unwrap();
        std::fs::write(p.join("ideas/latte.md"), "# Latte art\n").unwrap();
        std::fs::create_dir_all(p.join("seeds")).unwrap();
        std::fs::write(p.join("seeds/SEED-001-mobile.md"), "# Mobile orders\n").unwrap();

        let todos = vec![Todo {
            title: "Do the thing".into(),
            area: None,
            slug: "2026-07-07-do-the-thing".into(),
            path: std::path::PathBuf::from("2026-07-07-do-the-thing.md"),
            completed: false,
        }];
        let mut buf = Vec::new();
        render(
            &mut buf,
            p,
            &StateMeta::default(),
            &[],
            &[],
            &todos,
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();

        assert!(out.contains("Others"), "others heading:\n{out}");
        assert!(out.contains("◇"), "others bullet:\n{out}");
        // Each row is prefixed with its capitalized type.
        for row in ["Note: Grinder", "Idea: Latte art", "Seed: Mobile orders"] {
            assert!(out.contains(row), "missing {row}:\n{out}");
        }
        let todos_idx = out.find("Todos").expect("todos heading");
        let others_idx = out.find("Others").expect("others heading");
        let next_idx = out.find("Next").expect("next heading");
        assert!(
            todos_idx < others_idx && others_idx < next_idx,
            "Others must sit between Todos and Next:\n{out}"
        );
    }

    #[test]
    fn omits_others_section_when_capture_folders_absent() {
        let dir = tempfile::tempdir().unwrap();
        let mut buf = Vec::new();
        render(
            &mut buf,
            dir.path(),
            &StateMeta::default(),
            &one_phase(),
            &[],
            &[],
            false,
            false,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            !out.contains("Others"),
            "no Others section when folders absent:\n{out}"
        );
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
