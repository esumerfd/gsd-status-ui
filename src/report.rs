use crate::color;
use crate::model::{Phase, Stage, StateMeta};
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn render(
    out: &mut impl Write,
    planning: &Path,
    state: &StateMeta,
    phases: &[Phase],
    use_color: bool,
) -> io::Result<()> {
    let c = |code: &'static str| if use_color { code } else { "" };

    let title = if state.project_title.is_empty() {
        "GSD Project".to_string()
    } else {
        state.project_title.clone()
    };

    writeln!(out)?;
    writeln!(
        out,
        "{bold}{cyan}╭─ GSD STATUS ────────────────────────────────────────────────╮{reset}",
        bold = c(color::BOLD),
        cyan = c(color::CYAN),
        reset = c(color::RESET),
    )?;
    writeln!(
        out,
        "  {bold}{title}{reset}",
        bold = c(color::BOLD),
        title = title,
        reset = c(color::RESET),
    )?;
    writeln!(
        out,
        "  {dim}{p}{reset}",
        dim = c(color::DIM),
        p = planning.display(),
        reset = c(color::RESET),
    )?;

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
    let total_plans: u32 = phases
        .iter()
        .map(|p| p.plans.len() as u32)
        .sum::<u32>()
        .max(state.total_plans);
    let completed_plans: u32 = phases
        .iter()
        .map(|p| p.plans.iter().filter(|pl| pl.checked).count() as u32)
        .sum();
    let percent = if total_phases == 0 {
        0
    } else {
        (completed_phases * 100) / total_phases
    };

    writeln!(
        out,
        "  progress:  {bar} {bold}{pct:>3}%{reset}  {dim}({cp}/{tp} phases · {cpl}/{tpl} plans){reset}",
        bar = progress_bar(percent, 24, use_color),
        bold = c(color::BOLD),
        pct = percent,
        cp = completed_phases,
        tp = total_phases,
        cpl = completed_plans,
        tpl = total_plans,
        dim = c(color::DIM),
        reset = c(color::RESET),
    )?;
    writeln!(
        out,
        "{cyan}╰─────────────────────────────────────────────────────────────╯{reset}",
        cyan = c(color::CYAN),
        reset = c(color::RESET),
    )?;
    writeln!(out)?;

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

    writeln!(out)?;
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

/// The plain report as a String (no color) — used by the TUI status panel.
pub(crate) fn render_to_string(planning: &Path, state: &StateMeta, phases: &[Phase]) -> String {
    let mut buf = Vec::new();
    render(&mut buf, planning, state, phases, false).ok();
    String::from_utf8_lossy(&buf).into_owned()
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
            "{}{}{}{}{}",
            color::GREEN,
            "█".repeat(filled),
            color::GREY,
            "░".repeat(empty),
            color::RESET
        )
    } else {
        format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
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
    out.push(Hint {
        cmd: "/gsd-help".into(),
        note: "list all GSD commands",
    });
    out
}
