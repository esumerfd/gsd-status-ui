//! Interactive tabbed shell. Owns the terminal; first tab is the status
//! panel, further tabs are leaf-rendered documents. Leaf (the doc viewer)
//! owns all unmodified keys; every shell action is Alt-<key>.

pub(crate) mod app;

use crate::model::{DocKind, Phase, Stage, StateMeta};
use crate::planning::{discover_steps, PhaseDocs};
use app::{App, Focus, OpenRequest};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use leaf_adapter::DocView;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::collections::HashMap;
use std::io;
use std::path::Path;

const HINTS: &str = "M-p/r/v/u/c/d open · M-j/k step · M-h/l tab · M-x close · M-q quit";

pub(crate) struct Ui {
    app: App,
    views: HashMap<(usize, DocKind), DocView>,
    report: String,
    body_width: u16,
}

impl Ui {
    pub(crate) fn new(report: String, app: App) -> Self {
        Self {
            app,
            views: HashMap::new(),
            report,
            body_width: 80,
        }
    }

    pub(crate) fn quit(&self) -> bool {
        self.app.quit
    }

    fn apply(&mut self, request: Option<OpenRequest>) {
        let Some(req) = request else { return };
        match DocView::open(&req.path, self.body_width.max(20)) {
            Ok(view) => {
                self.views.insert((req.step, req.kind), view);
            }
            Err(e) => self.app.remove_tab(req.step, req.kind, e.to_string()),
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char('c') = key.code {
                self.app.quit = true;
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            self.on_shell_key(key.code);
        } else {
            self.on_viewer_key(key.code);
        }
    }

    fn on_shell_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('p') => self.open(DocKind::Plan),
            KeyCode::Char('r') => self.open(DocKind::Research),
            KeyCode::Char('v') => self.open(DocKind::Validation),
            KeyCode::Char('u') => self.open(DocKind::Uat),
            KeyCode::Char('c') => self.open(DocKind::Context),
            KeyCode::Char('d') => self.open(DocKind::Discussion),
            KeyCode::Char('j') => {
                let req = self.app.change_step(1);
                self.apply(req);
            }
            KeyCode::Char('k') => {
                let req = self.app.change_step(-1);
                self.apply(req);
            }
            KeyCode::Char('h') => self.app.focus_prev(),
            KeyCode::Char('l') => self.app.focus_next(),
            KeyCode::Char('x') => {
                if let Some(closed) = self.app.close_current() {
                    self.views.remove(&closed);
                }
            }
            KeyCode::Char('q') => self.app.quit = true,
            KeyCode::Char(n @ '1'..='9') => {
                self.app.focus_slot(n as usize - '0' as usize);
            }
            _ => {}
        }
    }

    fn open(&mut self, kind: DocKind) {
        let request = self.app.open_doc(kind);
        self.apply(request);
    }

    fn on_viewer_key(&mut self, code: KeyCode) {
        if let Focus::Status = self.app.focus() {
            if let KeyCode::Char('q') = code {
                self.app.quit = true;
            }
            return;
        }
        let Focus::Doc(kind) = self.app.focus() else {
            return;
        };
        let Some(view) = self.views.get_mut(&(self.app.current, kind)) else {
            return;
        };
        match code {
            KeyCode::Char('j') | KeyCode::Down => view.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => view.scroll_up(),
            KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => view.page_down(),
            KeyCode::PageUp | KeyCode::Char('b') => view.page_up(),
            KeyCode::Char('g') | KeyCode::Home => view.to_top(),
            KeyCode::Char('G') | KeyCode::End => view.to_bottom(),
            _ => {}
        }
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame) {
        let [tab_bar, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        self.body_width = body.width;

        // ── tab bar ──
        let focused_slot = match self.app.focus() {
            Focus::Status => 0,
            Focus::Doc(kind) => {
                self.app.tabs().iter().position(|k| *k == kind).unwrap_or(0) + 1
            }
        };
        let mut titles: Vec<String> = vec!["Status".into()];
        for kind in self.app.tabs() {
            let title = self
                .views
                .get(&(self.app.current, *kind))
                .map(|v| v.title().to_string())
                .unwrap_or_else(|| kind.label().to_string());
            titles.push(title);
        }
        let mut spans: Vec<Span> = Vec::new();
        for (i, title) in titles.iter().enumerate() {
            let style = if i == focused_slot {
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };
            spans.push(Span::styled(format!(" {title} "), style));
            spans.push(Span::raw("│"));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), tab_bar);

        // ── body ──
        match self.app.focus() {
            Focus::Status => {
                frame.render_widget(Paragraph::new(self.report.as_str()), body);
            }
            Focus::Doc(kind) => {
                if let Some(view) = self.views.get_mut(&(self.app.current, kind)) {
                    view.render(frame, body);
                } else {
                    frame.render_widget(Paragraph::new("(no view — press M-x to close)"), body);
                }
            }
        }

        // ── footer ──
        let position = match self.app.current_step() {
            Some(step) => format!(
                "Phase {} · step {} ({}/{})",
                self.app.phase_id,
                step.id,
                self.app.current + 1,
                self.app.steps.len()
            ),
            None => format!("Phase {} · no steps", self.app.phase_id),
        };
        let right = self.app.flash.clone().unwrap_or_else(|| HINTS.to_string());
        let footer_line = Line::from(vec![
            Span::styled(position, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled(right, Style::default().add_modifier(Modifier::DIM)),
        ]);
        frame.render_widget(Paragraph::new(footer_line), footer);
    }
}

/// Build the App for the first non-verified phase.
pub(crate) fn build_app(phases: &[Phase]) -> App {
    match phases.iter().find(|p| p.stage != Stage::Verified) {
        Some(ph) => {
            let docs = ph.dir.as_deref().map(PhaseDocs::new);
            let steps = ph
                .dir
                .as_deref()
                .map(|d| discover_steps(d, &ph.plans))
                .unwrap_or_default();
            App::new(ph.id.clone(), docs, steps)
        }
        None => App::new("—".into(), None, Vec::new()),
    }
}

pub(crate) fn run(planning: &Path, state: &StateMeta, phases: &[Phase]) -> io::Result<()> {
    let report = crate::report::render_to_string(planning, state, phases);
    let mut ui = Ui::new(report, build_app(phases));

    terminal::enable_raw_mode()?;
    execute!(io::stdout(), terminal::EnterAlternateScreen)?;
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(io::stdout());
    let mut term = ratatui::Terminal::new(backend)?;
    let result = event_loop(&mut term, &mut ui);
    restore_terminal();
    result
}

fn restore_terminal() {
    terminal::disable_raw_mode().ok();
    execute!(io::stdout(), terminal::LeaveAlternateScreen).ok();
}

fn event_loop(
    term: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    ui: &mut Ui,
) -> io::Result<()> {
    loop {
        term.draw(|frame| ui.draw(frame))?;
        if ui.quit() {
            return Ok(());
        }
        if let Event::Key(key) = event::read()? {
            ui.on_key(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn sample_ui() -> Ui {
        let planning = Path::new("sample/.planning");
        let state = crate::planning::load_state(planning);
        let phases = crate::planning::load_phases(planning);
        let report = crate::report::render_to_string(planning, &state, &phases);
        Ui::new(report, build_app(&phases))
    }

    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
    }

    fn plain(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn screen(ui: &mut Ui) -> String {
        let backend = TestBackend::new(90, 24);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| ui.draw(f)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..24 {
            for x in 0..90 {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn initial_screen_shows_status_tab_with_report_and_footer() {
        let mut ui = sample_ui();
        let s = screen(&mut ui);
        assert!(s.contains("Status"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "{s}");
        assert!(s.contains("Phase 2 · step 02-02 (2/3)"), "{s}");
        assert!(s.contains("M-j/k step"), "{s}");
    }

    #[test]
    fn alt_p_opens_the_step_plan_in_a_named_tab() {
        let mut ui = sample_ui();
        ui.on_key(alt('p'));
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "tab name missing: {s}");
        assert!(
            s.contains("Cup Handling and Fill-Level Detection"),
            "doc body missing: {s}"
        );
    }

    #[test]
    fn end_to_end_key_sequence_maintains_per_step_tabsets() {
        let mut ui = sample_ui();
        ui.on_key(alt('p'));
        ui.on_key(alt('r'));
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "{s}");
        assert!(s.contains("02-RESEARCH.md"), "{s}");

        // Later step: fresh tab set, plan auto-opens.
        ui.on_key(alt('j'));
        let s = screen(&mut ui);
        assert!(s.contains("02-03-PLAN.md"), "{s}");
        assert!(!s.contains("02-RESEARCH.md"), "step tab sets must not mix: {s}");
        assert!(s.contains("Spill Recovery"), "{s}");

        // Open validation on this step, then go back.
        ui.on_key(alt('v'));
        let s = screen(&mut ui);
        assert!(s.contains("02-VALIDATION.md"), "{s}");

        ui.on_key(alt('k'));
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "{s}");
        assert!(s.contains("02-RESEARCH.md"), "{s}");
        assert!(!s.contains("02-VALIDATION.md"), "{s}");
        assert!(s.contains("(2/3)"), "{s}");
    }

    #[test]
    fn alt_u_opens_uat_document() {
        let mut ui = sample_ui();
        ui.on_key(alt('u'));
        let s = screen(&mut ui);
        assert!(s.contains("02-UAT.md"), "{s}");
        assert!(s.contains("Morning rush order"), "{s}");
    }

    #[test]
    fn unmodified_keys_scroll_the_focused_document() {
        let mut ui = sample_ui();
        ui.on_key(alt('p'));
        let before = screen(&mut ui);
        for _ in 0..8 {
            ui.on_key(plain('j'));
        }
        let after = screen(&mut ui);
        assert_ne!(before, after, "plain j must scroll the doc");
        ui.on_key(plain('g'));
        let back = screen(&mut ui);
        assert_eq!(before, back, "g must return to top");
    }

    #[test]
    fn alt_x_closes_and_falls_back_to_status() {
        let mut ui = sample_ui();
        ui.on_key(alt('p'));
        ui.on_key(alt('x'));
        let s = screen(&mut ui);
        assert!(!s.contains("02-02-PLAN.md"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "status body expected: {s}");
    }

    #[test]
    fn quit_keys_set_quit_flag() {
        let mut ui = sample_ui();
        ui.on_key(alt('q'));
        assert!(ui.quit());

        let mut ui = sample_ui();
        ui.on_key(plain('q')); // on Status tab plain q also quits
        assert!(ui.quit());

        let mut ui = sample_ui();
        ui.on_key(alt('p'));
        ui.on_key(plain('q')); // on a doc tab plain q belongs to the viewer
        assert!(!ui.quit());
        ui.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(ui.quit());
    }

    #[test]
    fn missing_doc_flashes_in_footer() {
        let planning = Path::new("sample/.planning");
        let state = crate::planning::load_state(planning);
        let mut phases = crate::planning::load_phases(planning);
        // Force phase 1 active (it has no RESEARCH doc).
        phases[0].roadmap_checked = false;
        phases[0].stage = Stage::Executing;
        let report = crate::report::render_to_string(planning, &state, &phases);
        let mut ui = Ui::new(report, build_app(&phases));
        ui.on_key(alt('r'));
        let s = screen(&mut ui);
        assert!(s.contains("no research document"), "{s}");
    }
}
