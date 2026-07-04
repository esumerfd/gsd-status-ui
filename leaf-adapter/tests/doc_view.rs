use leaf_adapter::{DocView, DocViewError};
use ratatui::{backend::TestBackend, Terminal};
use std::io::Write;

fn fixture(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .expect("tempfile");
    f.write_all(content.as_bytes()).expect("write fixture");
    f
}

fn rendered_text(view: &mut DocView, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn open_renders_markdown_into_a_panel() {
    let f = fixture("# Plan 01-01\n\nWalking skeleton for the registry.\n");
    let mut view = DocView::open(f.path(), 40).expect("open");
    let text = rendered_text(&mut view, 40, 10);
    assert!(text.contains("Plan 01-01"), "heading missing:\n{text}");
    assert!(
        text.contains("Walking skeleton"),
        "body missing:\n{text}"
    );
}

#[test]
fn title_is_the_file_name() {
    let f = fixture("# T\n");
    let view = DocView::open(f.path(), 40).expect("open");
    let name = f.path().file_name().unwrap().to_str().unwrap().to_string();
    assert_eq!(view.title(), name);
}

#[test]
fn scrolling_changes_the_visible_region() {
    let body: String = (1..=50).map(|i| format!("line number {i}\n\n")).collect();
    let f = fixture(&body);
    let mut view = DocView::open(f.path(), 40).expect("open");
    let before = rendered_text(&mut view, 40, 5);
    assert!(before.contains("line number 1"), "top not visible:\n{before}");
    for _ in 0..20 {
        view.scroll_down();
    }
    let after = rendered_text(&mut view, 40, 5);
    assert_ne!(before, after, "scroll_down did not change viewport");
    for _ in 0..40 {
        view.scroll_up();
    }
    let back = rendered_text(&mut view, 40, 5);
    assert_eq!(before, back, "scroll_up did not return to top");
}

#[test]
fn page_and_edge_scrolling() {
    let body: String = (1..=50).map(|i| format!("line number {i}\n\n")).collect();
    let f = fixture(&body);
    let mut view = DocView::open(f.path(), 40).expect("open");
    let top = rendered_text(&mut view, 40, 5);

    view.page_down();
    let paged = rendered_text(&mut view, 40, 5);
    assert_ne!(top, paged, "page_down did not move");

    view.to_bottom();
    let bottom = rendered_text(&mut view, 40, 5);
    assert!(bottom.contains("line number 50"), "not at bottom:\n{bottom}");

    view.to_top();
    let back = rendered_text(&mut view, 40, 5);
    assert_eq!(top, back, "to_top did not return to the start");

    view.page_down();
    view.page_up();
    let again = rendered_text(&mut view, 40, 5);
    assert_eq!(top, again, "page_up did not undo page_down");
}

#[test]
fn open_missing_file_is_an_error() {
    let err = DocView::open(std::path::Path::new("/nonexistent/nope.md"), 40);
    assert!(matches!(err, Err(DocViewError::Io { .. })));
}
