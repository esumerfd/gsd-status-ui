use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GREY: &str = "\x1b[90m";
}

#[derive(Debug, Default)]
struct StateMeta {
    milestone: String,
    milestone_name: String,
    status: String,
    last_updated: String,
    total_phases: u32,
    completed_phases: u32,
    total_plans: u32,
    completed_plans: u32,
    percent: u32,
    next_action: String,
    project_title: String,
}

#[derive(Debug)]
struct Phase {
    id: String,
    title: String,
    roadmap_checked: bool,
    plans: Vec<Plan>,
    #[allow(dead_code)]
    dir: Option<PathBuf>,
    stage: Stage,
}

#[derive(Debug, Clone)]
struct Plan {
    #[allow(dead_code)]
    name: String,
    checked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    NotStarted,
    Discussing,
    Discussed,
    Planned,
    Executing,
    Executed,
    Verified,
}

impl Stage {
    fn label(self) -> &'static str {
        match self {
            Stage::NotStarted => "not started",
            Stage::Discussing => "discussing",
            Stage::Discussed => "discussed",
            Stage::Planned => "planned",
            Stage::Executing => "executing",
            Stage::Executed => "executed",
            Stage::Verified => "verified",
        }
    }
    fn color(self) -> &'static str {
        match self {
            Stage::NotStarted => color::GREY,
            Stage::Discussing | Stage::Discussed => color::MAGENTA,
            Stage::Planned => color::BLUE,
            Stage::Executing => color::YELLOW,
            Stage::Executed => color::CYAN,
            Stage::Verified => color::GREEN,
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    let start = if let Some(p) = args.first() {
        PathBuf::from(p)
    } else {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let planning = match find_planning_dir(&start) {
        Some(p) => p,
        None => {
            eprintln!(
                "gsd-status: no .planning/ directory found from {}",
                start.display()
            );
            return ExitCode::from(2);
        }
    };

    let use_color = io::stdout().is_terminal() && env::var("NO_COLOR").is_err();
    let state = load_state(&planning);
    let phases = load_phases(&planning);

    let mut out = io::stdout().lock();
    render(&mut out, &planning, &state, &phases, use_color).ok();
    ExitCode::SUCCESS
}

fn print_help() {
    println!("gsd-status — show GSD project status for a .planning/ workspace");
    println!();
    println!("Usage:");
    println!("  gsd-status [path]");
    println!();
    println!("If [path] is omitted, walks up from the current directory looking for .planning/.");
    println!("Honors NO_COLOR.");
}

fn find_planning_dir(start: &Path) -> Option<PathBuf> {
    let mut cur = start.canonicalize().ok()?;
    loop {
        let cand = cur.join(".planning");
        if cand.is_dir() {
            return Some(cand);
        }
        if !cur.pop() {
            return None;
        }
    }
}

// ───────────────────────────────────────────────────────────── STATE.md ──

fn load_state(planning: &Path) -> StateMeta {
    let mut meta = StateMeta::default();
    let path = planning.join("STATE.md");
    let body = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return meta,
    };

    let (front, rest) = split_frontmatter(&body);
    parse_frontmatter(front, &mut meta);

    meta.next_action = extract_section(rest, "Next Action");

    // Prefer PROJECT.md for the human-readable title; fall back to STATE.md's H1
    // with its leading "STATE:" / "ROADMAP:" tag stripped.
    if let Ok(p) = fs::read_to_string(planning.join("PROJECT.md")) {
        for line in p.lines() {
            if let Some(t) = line.strip_prefix("# ") {
                meta.project_title = strip_md(t).trim().to_string();
                break;
            }
        }
    }
    if meta.project_title.is_empty() {
        for line in rest.lines() {
            if let Some(t) = line.strip_prefix("# ") {
                meta.project_title = strip_md(t).trim().trim_start_matches("STATE:").trim().to_string();
                break;
            }
        }
    }
    meta
}

fn split_frontmatter(body: &str) -> (&str, &str) {
    let trimmed = body.trim_start_matches('\u{feff}');
    let rest = match trimmed.strip_prefix("---\n") {
        Some(r) => r,
        None => return ("", trimmed),
    };
    if let Some(end) = rest.find("\n---\n") {
        (&rest[..end], &rest[end + 5..])
    } else {
        ("", trimmed)
    }
}

fn parse_frontmatter(front: &str, meta: &mut StateMeta) {
    let mut in_progress = false;
    for raw in front.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let indented = line.starts_with("  ") || line.starts_with('\t');
        if !indented {
            in_progress = false;
        }
        if let Some((key, value)) = split_kv(line) {
            let key = key.trim();
            let value = strip_quotes(value.trim());
            match (in_progress, key) {
                (_, "progress") if value.is_empty() => in_progress = true,
                (false, "milestone") => meta.milestone = value.to_string(),
                (false, "milestone_name") => meta.milestone_name = value.to_string(),
                (false, "status") => meta.status = value.to_string(),
                (false, "last_updated") => meta.last_updated = value.to_string(),
                (true, "total_phases") => meta.total_phases = value.parse().unwrap_or(0),
                (true, "completed_phases") => meta.completed_phases = value.parse().unwrap_or(0),
                (true, "total_plans") => meta.total_plans = value.parse().unwrap_or(0),
                (true, "completed_plans") => meta.completed_plans = value.parse().unwrap_or(0),
                (true, "percent") => meta.percent = value.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(':')?;
    Some((line[..idx].trim_start(), &line[idx + 1..]))
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn extract_section(body: &str, heading: &str) -> String {
    let mut collecting = false;
    let mut buf = String::new();
    for line in body.lines() {
        let is_heading = line.trim_start().starts_with('#');
        let heading_text = line.trim_start_matches('#').trim();
        if is_heading {
            if collecting {
                break;
            }
            if heading_text.eq_ignore_ascii_case(heading) {
                collecting = true;
                continue;
            }
        } else if collecting {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    buf.trim().to_string()
}

fn strip_md(s: &str) -> String {
    s.replace("**", "").replace('`', "")
}

// ─────────────────────────────────────────────────────────── ROADMAP.md ──

fn load_phases(planning: &Path) -> Vec<Phase> {
    let roadmap_path = planning.join("ROADMAP.md");
    let body = match fs::read_to_string(&roadmap_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let top_level = parse_phase_index(&body);
    let plans_per_phase = parse_phase_plans(&body);

    let phase_dirs = scan_phase_dirs(planning);

    let mut phases = Vec::new();
    for (id, title, checked) in top_level {
        let plans = plans_per_phase.get(&id).cloned().unwrap_or_default();
        let dir = phase_dirs
            .iter()
            .find(|p| dir_matches_phase(p, &id))
            .cloned();
        let stage = infer_stage(dir.as_deref(), &plans, checked);
        phases.push(Phase {
            id,
            title,
            roadmap_checked: checked,
            plans,
            dir,
            stage,
        });
    }
    phases
}

fn parse_phase_index(body: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut in_phases = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## ") {
            in_phases = trimmed.eq_ignore_ascii_case("## Phases");
            continue;
        }
        if !in_phases {
            continue;
        }
        if let Some(item) = parse_phase_index_line(trimmed) {
            out.push(item);
        }
    }
    out
}

fn parse_phase_index_line(line: &str) -> Option<(String, String, bool)> {
    let rest = line.strip_prefix("- ")?;
    let (checked, rest) = if let Some(r) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        (true, r)
    } else if let Some(r) = rest.strip_prefix("[ ] ") {
        (false, r)
    } else {
        return None;
    };
    let bold = rest.strip_prefix("**")?;
    let end = bold.find("**")?;
    let header = &bold[..end];
    let phase_part = header.strip_prefix("Phase ")?;
    let colon = phase_part.find(':')?;
    let id = phase_part[..colon].trim().to_string();
    let title = phase_part[colon + 1..].trim().to_string();
    Some((id, title, checked))
}

fn parse_phase_plans(body: &str) -> std::collections::HashMap<String, Vec<Plan>> {
    let mut map: std::collections::HashMap<String, Vec<Plan>> = std::collections::HashMap::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("### Phase ") {
            let colon = rest.find(':').unwrap_or(rest.len());
            let id = rest[..colon].trim().to_string();
            current = Some(id);
            continue;
        }
        if trimmed.starts_with("## ") {
            current = None;
            continue;
        }
        let Some(id) = current.as_ref() else { continue };
        if let Some(plan) = parse_plan_line(trimmed) {
            map.entry(id.clone()).or_default().push(plan);
        }
    }
    map
}

fn parse_plan_line(line: &str) -> Option<Plan> {
    let rest = line.strip_prefix("- ")?;
    let (checked, rest) = if let Some(r) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        (true, r)
    } else if let Some(r) = rest.strip_prefix("[ ] ") {
        (false, r)
    } else {
        return None;
    };
    if !rest.contains("PLAN.md") {
        return None;
    }
    let name = rest
        .split('—')
        .next()
        .unwrap_or(rest)
        .trim()
        .trim_end_matches(".md")
        .trim_end_matches("-PLAN")
        .to_string();
    Some(Plan { name, checked })
}

// ──────────────────────────────────────────────────────── phase scanning ──

fn scan_phase_dirs(planning: &Path) -> Vec<PathBuf> {
    let phases_root = planning.join("phases");
    let Ok(entries) = fs::read_dir(&phases_root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn dir_matches_phase(dir: &Path, phase_id: &str) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let leading: String = name
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let normalized_dir = leading.trim_start_matches('0');
    let normalized_id = phase_id.trim_start_matches('0');
    if normalized_dir.is_empty() && normalized_id.is_empty() {
        return true;
    }
    normalized_dir.eq_ignore_ascii_case(normalized_id)
}

fn infer_stage(dir: Option<&Path>, plans: &[Plan], roadmap_checked: bool) -> Stage {
    if roadmap_checked {
        return Stage::Verified;
    }
    let Some(dir) = dir else {
        return Stage::NotStarted;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Stage::NotStarted;
    };
    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();

    let has = |suffix: &str| names.iter().any(|n| n.ends_with(suffix));
    let count = |suffix: &str| names.iter().filter(|n| n.ends_with(suffix)).count();

    if has("-VERIFICATION.md") {
        return Stage::Verified;
    }
    let summaries = count("-SUMMARY.md");
    let plan_files = count("-PLAN.md");
    if plan_files > 0 && summaries >= plan_files {
        return Stage::Executed;
    }
    if plan_files > 0 && summaries > 0 {
        return Stage::Executing;
    }
    if plan_files > 0 {
        return Stage::Planned;
    }
    if !plans.is_empty() {
        return Stage::Discussed;
    }
    if has("-CONTEXT.md") {
        return Stage::Discussed;
    }
    if names.iter().any(|n| n.contains("DISCUSS-CHECKPOINT")) {
        return Stage::Discussing;
    }
    Stage::NotStarted
}

// ──────────────────────────────────────────────────────────────── render ──

fn render(
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
    let status_str = if state.status.is_empty() { "—" } else { state.status.as_str() };
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
