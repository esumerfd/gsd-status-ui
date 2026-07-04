//! Interactive tabbed shell. Owns the terminal; first tab is the status
//! panel, further tabs are leaf-rendered documents. Leaf (the doc viewer)
//! owns the unmodified keys; shell actions are Ctrl-<key> (plus Tab/BackTab
//! and digit tab-jumps, which no viewer key uses).

pub(crate) mod ansi;
pub(crate) mod app;

use crate::model::{DocKind, Phase, StateMeta};
use app::{App, Focus, OpenRequest};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use leaf_adapter::DocView;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;
use std::io;
use std::path::Path;

const STATUS_HINTS: &str = "j/k step · Enter plan · o open · Tab tab · q quit";
const DOC_HINTS: &str = "j/k scroll · C-j/k step · Tab tab · q/Esc status · C-q quit";
const DIALOG_HINTS: &str = "j/k select · Enter open · Esc cancel";

pub(crate) struct Ui {
    app: App,
    views: HashMap<(usize, DocKind), DocView>,
    report: Text<'static>,
    body_width: u16,
}

/// The colored status report as ratatui text (reuses the report's ANSI colors).
pub(crate) fn status_text(planning: &Path, state: &StateMeta, phases: &[Phase]) -> Text<'static> {
    let mut buf = Vec::new();
    crate::report::render(&mut buf, planning, state, phases, true).ok();
    ansi::ansi_to_text(&String::from_utf8_lossy(&buf))
}

impl Ui {
    pub(crate) fn new(report: Text<'static>, app: App) -> Self {
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
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q')) {
            self.app.quit = true;
            return;
        }
        if self.app.dialog().is_some() {
            self.on_dialog_key(key.code, ctrl);
            return;
        }
        if ctrl {
            self.on_shell_key(key.code);
        } else {
            self.on_unmodified_key(key.code);
        }
    }

    fn on_dialog_key(&mut self, code: KeyCode, ctrl: bool) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.app.close_dialog(),
            KeyCode::Char('o') if ctrl => self.app.close_dialog(),
            KeyCode::Char('j') | KeyCode::Down => self.app.dialog_move(1),
            KeyCode::Char('k') | KeyCode::Up => self.app.dialog_move(-1),
            KeyCode::Enter => {
                let request = self.app.dialog_select();
                self.apply(request);
            }
            _ => {}
        }
    }

    fn on_shell_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('o') => self.app.open_dialog(),
            KeyCode::Char('j') | KeyCode::Down => {
                let req = self.app.change_step(1);
                self.apply(req);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let req = self.app.change_step(-1);
                self.apply(req);
            }
            // Some terminals deliver Ctrl-h as (Ctrl-)Backspace.
            KeyCode::Char('h') | KeyCode::Backspace => self.app.focus_prev(),
            KeyCode::Char('l') => self.app.focus_next(),
            KeyCode::Char('x') => {
                if let Some(closed) = self.app.close_current() {
                    self.views.remove(&closed);
                }
            }
            _ => {}
        }
    }

    fn on_unmodified_key(&mut self, code: KeyCode) {
        // Shell aliases on keys no viewer binding uses.
        match code {
            KeyCode::Tab => {
                self.app.focus_next();
                return;
            }
            KeyCode::BackTab => {
                self.app.focus_prev();
                return;
            }
            KeyCode::Char(n @ '1'..='9') => {
                self.app.focus_slot(n as usize - '0' as usize);
                return;
            }
            _ => {}
        }
        if let Focus::Status = self.app.focus() {
            match code {
                // The back-out chain ends here: q on status exits the app.
                KeyCode::Char('q') => self.app.quit = true,
                // Browse steps without opening anything.
                KeyCode::Char('j') | KeyCode::Down => {
                    let req = self.app.change_step(1);
                    self.apply(req);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let req = self.app.change_step(-1);
                    self.apply(req);
                }
                // Enter doc mode on the selected step's plan.
                KeyCode::Enter => {
                    let req = self.app.open_doc(DocKind::Plan);
                    self.apply(req);
                }
                KeyCode::Char('o') => self.app.open_dialog(),
                _ => {}
            }
            return;
        }
        let Focus::Doc(kind) = self.app.focus() else {
            return;
        };
        // q/Esc back out of doc mode to the status panel; the tab stays open.
        if matches!(code, KeyCode::Char('q') | KeyCode::Esc) {
            self.app.focus_slot(1);
            return;
        }
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
                frame.render_widget(Paragraph::new(self.report.clone()), body);
            }
            Focus::Doc(kind) => {
                if let Some(view) = self.views.get_mut(&(self.app.current, kind)) {
                    view.render(frame, body);
                } else {
                    frame.render_widget(Paragraph::new("(no view — press C-x to close)"), body);
                }
            }
        }

        // ── open-document dialog ──
        if let Some(dialog) = self.app.dialog() {
            let name_width = dialog
                .items
                .iter()
                .map(|(_, n)| n.chars().count())
                .max()
                .unwrap_or(0)
                .max(16);
            let width = (name_width as u16 + 8).min(frame.area().width);
            let height = (dialog.items.len() as u16 + 2).min(frame.area().height);
            let popup = Rect {
                x: frame.area().x + (frame.area().width.saturating_sub(width)) / 2,
                y: frame.area().y + (frame.area().height.saturating_sub(height)) / 2,
                width,
                height,
            };
            frame.render_widget(Clear, popup);
            let lines: Vec<Line> = dialog
                .items
                .iter()
                .enumerate()
                .map(|(i, (kind, name))| {
                    let open_marker = if self.app.tabs().contains(kind) { "●" } else { " " };
                    let style = if i == dialog.selected {
                        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    Line::from(Span::styled(
                        format!(" {open_marker} {name:<name_width$} "),
                        style,
                    ))
                })
                .collect();
            frame.render_widget(
                Paragraph::new(lines).block(Block::bordered().title(" Open document ")),
                popup,
            );
        }

        // ── footer ──
        let mode = match self.app.focus() {
            Focus::Status => "[status]",
            Focus::Doc(_) => "[doc]",
        };
        let position = match self.app.current_entry() {
            Some(entry) => format!(
                "{mode} Phase {} · step {} ({}/{})",
                entry.phase_id,
                entry.step.id,
                entry.pos_in_phase + 1,
                entry.phase_steps
            ),
            None => format!("{mode} no steps"),
        };
        let right = if self.app.dialog().is_some() {
            DIALOG_HINTS.to_string()
        } else if let Some(flash) = self.app.flash.clone() {
            flash
        } else if matches!(self.app.focus(), Focus::Status) {
            STATUS_HINTS.to_string()
        } else {
            DOC_HINTS.to_string()
        };
        let footer_line = Line::from(vec![
            Span::styled(position, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled(right, Style::default().add_modifier(Modifier::DIM)),
        ]);
        frame.render_widget(Paragraph::new(footer_line), footer);
    }
}

pub(crate) fn run(planning: &Path, state: &StateMeta, phases: &[Phase]) -> io::Result<()> {
    let mut ui = Ui::new(status_text(planning, state, phases), App::from_phases(phases));

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
        Ui::new(status_text(planning, &state, &phases), App::from_phases(&phases))
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn plain(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    /// Ctrl-o, move the selection down `moves` times, Enter.
    fn open_via_dialog(ui: &mut Ui, moves: usize) {
        ui.on_key(ctrl('o'));
        for _ in 0..moves {
            ui.on_key(plain('j'));
        }
        ui.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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
    fn status_tab_reuses_the_report_colors() {
        let mut ui = sample_ui();
        let backend = TestBackend::new(90, 24);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| ui.draw(f)).unwrap();
        let buf = term.backend().buffer().clone();
        let colored = buf
            .content()
            .iter()
            .filter(|c| c.style().fg.is_some_and(|f| f != ratatui::style::Color::Reset))
            .count();
        assert!(colored > 20, "status panel should be colored, got {colored} colored cells");
    }

    #[test]
    fn initial_screen_shows_status_tab_with_report_and_footer() {
        let mut ui = sample_ui();
        let s = screen(&mut ui);
        assert!(s.contains("Status"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "{s}");
        assert!(s.contains("Phase 2 · step 02-02 (2/3)"), "{s}");
        assert!(s.contains("j/k step · Enter plan"), "{s}");
    }

    #[test]
    fn ctrl_o_lists_the_step_documents_in_order() {
        let mut ui = sample_ui();
        ui.on_key(ctrl('o'));
        let s = screen(&mut ui);
        assert!(s.contains("Open document"), "{s}");
        for name in [
            "02-02-PLAN.md",
            "02-RESEARCH.md",
            "02-VALIDATION.md",
            "02-UAT.md",
            "02-CONTEXT.md",
            "02-DISCUSSION-LOG.md",
        ] {
            assert!(s.contains(name), "dialog missing {name}: {s}");
        }
        assert!(s.contains("Enter open"), "dialog hints: {s}");

        // Esc cancels: dialog gone, nothing opened.
        ui.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let s = screen(&mut ui);
        assert!(!s.contains("Open document"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "back on status: {s}");
    }

    #[test]
    fn dialog_enter_opens_the_step_plan_in_a_named_tab() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0); // plan is first
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "tab name missing: {s}");
        assert!(
            s.contains("Cup Handling and Fill-Level Detection"),
            "doc body missing: {s}"
        );
        assert!(!s.contains("Open document"), "dialog must close: {s}");
    }

    #[test]
    fn q_backs_out_one_level_doc_then_status_then_quit() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0); // in a doc
        ui.on_key(plain('q'));
        assert!(!ui.quit(), "first q must not quit");
        let s = screen(&mut ui);
        assert!(s.contains("Robot Coffee Service"), "q returns to status: {s}");
        assert!(s.contains("02-02-PLAN.md"), "tab stays open: {s}");
        ui.on_key(plain('q'));
        assert!(ui.quit(), "second q (on status) exits the app");
    }

    #[test]
    fn esc_leaves_a_doc_like_q() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0);
        ui.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!ui.quit());
        let s = screen(&mut ui);
        assert!(s.contains("Robot Coffee Service"), "Esc returns to status: {s}");
    }

    #[test]
    fn plain_o_opens_the_dialog_from_status() {
        let mut ui = sample_ui();
        ui.on_key(plain('o'));
        let s = screen(&mut ui);
        assert!(s.contains("Open document"), "{s}");
    }

    #[test]
    fn footer_shows_the_mode() {
        let mut ui = sample_ui();
        let s = screen(&mut ui);
        assert!(s.contains("[status]"), "{s}");
        open_via_dialog(&mut ui, 0);
        let s = screen(&mut ui);
        assert!(s.contains("[doc]"), "{s}");
        ui.on_key(plain('q'));
        let s = screen(&mut ui);
        assert!(s.contains("[status]"), "back on status: {s}");
    }

    #[test]
    fn dialog_marks_already_open_documents() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 1); // research
        ui.on_key(ctrl('o'));
        let s = screen(&mut ui);
        assert!(s.contains("● 02-RESEARCH.md"), "open marker: {s}");
    }

    #[test]
    fn dialog_on_phase_1_lists_only_existing_docs() {
        let mut ui = sample_ui();
        ui.on_key(ctrl('k'));
        ui.on_key(ctrl('k')); // phase 1, step 01-01
        ui.on_key(ctrl('o'));
        let s = screen(&mut ui);
        assert!(s.contains("01-01-PLAN.md"), "{s}");
        assert!(!s.contains("01-RESEARCH.md"), "phase 1 has no research: {s}");
    }

    #[test]
    fn end_to_end_key_sequence_maintains_per_step_tabsets() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0); // plan
        open_via_dialog(&mut ui, 1); // research
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "{s}");
        assert!(s.contains("02-RESEARCH.md"), "{s}");

        // Later step: fresh tab set, plan auto-opens.
        ui.on_key(ctrl('j'));
        let s = screen(&mut ui);
        assert!(s.contains("02-03-PLAN.md"), "{s}");
        assert!(!s.contains("02-RESEARCH.md"), "step tab sets must not mix: {s}");
        assert!(s.contains("Spill Recovery"), "{s}");

        // Open validation on this step, then go back.
        open_via_dialog(&mut ui, 2); // plan, research, validation
        let s = screen(&mut ui);
        assert!(s.contains("02-VALIDATION.md"), "{s}");

        ui.on_key(ctrl('k'));
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "{s}");
        assert!(s.contains("02-RESEARCH.md"), "{s}");
        assert!(!s.contains("02-VALIDATION.md"), "{s}");
        assert!(s.contains("(2/3)"), "{s}");
    }

    #[test]
    fn dialog_opens_uat_and_context_documents() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 3); // uat
        let s = screen(&mut ui);
        assert!(s.contains("02-UAT.md"), "{s}");
        assert!(s.contains("Morning rush order"), "{s}");

        open_via_dialog(&mut ui, 4); // context
        let s = screen(&mut ui);
        assert!(s.contains("02-CONTEXT.md"), "{s}");
        assert!(s.contains("Decisions Locked"), "{s}");
    }

    #[test]
    fn tab_and_ctrl_arrows_are_robust_aliases() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0);
        // Tab cycles focus: doc -> status.
        ui.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let s = screen(&mut ui);
        assert!(s.contains("Robot Coffee Service"), "tab should cycle to status: {s}");
        ui.on_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        let s = screen(&mut ui);
        assert!(s.contains("Cup Handling"), "backtab should return to doc: {s}");
        // Ctrl-Down/Up change step like Ctrl-j/k.
        ui.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));
        let s = screen(&mut ui);
        assert!(s.contains("02-03-PLAN.md"), "{s}");
        ui.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL));
        let s = screen(&mut ui);
        assert!(s.contains("02-02-PLAN.md"), "{s}");
    }

    #[test]
    fn unmodified_keys_scroll_the_focused_document() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0);
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
    fn ctrl_x_closes_and_falls_back_to_status() {
        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0);
        ui.on_key(ctrl('x'));
        let s = screen(&mut ui);
        assert!(!s.contains("02-02-PLAN.md"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "status body expected: {s}");
    }

    #[test]
    fn quit_keys_set_quit_flag() {
        let mut ui = sample_ui();
        ui.on_key(ctrl('q'));
        assert!(ui.quit());

        let mut ui = sample_ui();
        ui.on_key(plain('q')); // on Status tab plain q also quits
        assert!(ui.quit());

        let mut ui = sample_ui();
        open_via_dialog(&mut ui, 0);
        ui.on_key(plain('q')); // on a doc tab plain q belongs to the viewer
        assert!(!ui.quit());
        ui.on_key(ctrl('c')); // Ctrl-C always quits, even with a doc focused
        assert!(ui.quit());

        let mut ui = sample_ui();
        ui.on_key(ctrl('o'));
        ui.on_key(ctrl('q')); // quit works even while the dialog is open
        assert!(ui.quit());
    }

    #[test]
    fn status_jk_browses_steps_and_enter_opens_the_plan() {
        let mut ui = sample_ui();
        let s = screen(&mut ui);
        assert!(s.contains("Phase 2 · step 02-02 (2/3)"), "{s}");

        // Plain k browses backwards — status body stays, nothing opens.
        ui.on_key(plain('k'));
        let s = screen(&mut ui);
        assert!(s.contains("Phase 2 · step 02-01 (1/3)"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "still on status: {s}");
        assert!(!s.contains("02-01-PLAN.md"), "nothing must auto-open: {s}");

        // Across the phase boundary, still browsing.
        ui.on_key(plain('k'));
        let s = screen(&mut ui);
        assert!(s.contains("Phase 1 · step 01-01 (1/1)"), "{s}");
        assert!(s.contains("Robot Coffee Service"), "{s}");

        ui.on_key(plain('k'));
        let s = screen(&mut ui);
        assert!(s.contains("already at the first step"), "{s}");

        // Enter opens the selected step's plan (viewer mode).
        ui.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let s = screen(&mut ui);
        assert!(s.contains("01-01-PLAN.md"), "{s}");
        assert!(s.contains("Map the Office"), "phase 1 plan body: {s}");
        assert!(s.contains("C-j/k step"), "doc hints: {s}");

        // Plain j now scrolls the document, not the step.
        ui.on_key(plain('j'));
        let s = screen(&mut ui);
        assert!(s.contains("Phase 1 · step 01-01"), "step unchanged: {s}");

        // Ctrl-j still changes step from viewer mode, staying in viewer.
        ui.on_key(ctrl('j'));
        let s = screen(&mut ui);
        assert!(s.contains("Phase 2 · step 02-01 (1/3)"), "{s}");
        assert!(s.contains("02-01-PLAN.md"), "viewer stepping auto-opens plan: {s}");
        assert!(s.contains("Locate and Operate"), "{s}");
    }

}
