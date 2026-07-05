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

fn search_for(view: &mut leaf_adapter::DocView, query: &str) {
    view.begin_search();
    for ch in query.chars() {
        view.push_search_draft(ch);
    }
    view.confirm_search();
}

#[test]
fn confirmed_search_jumps_to_the_first_match() {
    let body: String = (1..=50)
        .map(|i| {
            if i == 20 || i == 40 {
                format!("needle target {i}\n\n")
            } else {
                format!("line number {i}\n\n")
            }
        })
        .collect();
    let f = fixture(&body);
    let mut view = DocView::open(f.path(), 40).expect("open");

    search_for(&mut view, "needle");
    assert_eq!(view.search_match_count(), 2);
    assert_eq!(view.search_index(), 0);
    assert!(!view.is_search_mode(), "confirm leaves input mode");
    let text = rendered_text(&mut view, 40, 5);
    assert!(text.contains("needle target 20"), "not at first match:\n{text}");
}

#[test]
fn next_and_prev_match_cycle_with_wraparound() {
    let body: String = (1..=50)
        .map(|i| {
            if i == 20 || i == 40 {
                format!("needle target {i}\n\n")
            } else {
                format!("line number {i}\n\n")
            }
        })
        .collect();
    let f = fixture(&body);
    let mut view = DocView::open(f.path(), 40).expect("open");
    search_for(&mut view, "needle");

    view.next_match();
    let text = rendered_text(&mut view, 40, 5);
    assert!(text.contains("needle target 40"), "second match:\n{text}");

    view.next_match(); // wraps to the first
    let text = rendered_text(&mut view, 40, 5);
    assert!(text.contains("needle target 20"), "wraparound:\n{text}");

    view.prev_match(); // wraps back to the last
    let text = rendered_text(&mut view, 40, 5);
    assert!(text.contains("needle target 40"), "prev wraps:\n{text}");
}

#[test]
fn search_is_case_insensitive() {
    let f = fixture("# Doc\n\nThe NeEdLe hides here.\n");
    let mut view = DocView::open(f.path(), 40).expect("open");
    search_for(&mut view, "needle");
    assert_eq!(view.search_match_count(), 1);
}

#[test]
fn draft_editing_and_cancel_clear_the_search() {
    let f = fixture("# Doc\n\nneedle one.\n");
    let mut view = DocView::open(f.path(), 40).expect("open");

    view.begin_search();
    assert!(view.is_search_mode());
    view.push_search_draft('x');
    view.push_search_draft('y');
    view.pop_search_draft();
    assert_eq!(view.search_draft(), "x");
    view.cancel_search();
    assert!(!view.is_search_mode());
    assert_eq!(view.search_match_count(), 0);

    // Confirming an empty draft also resets everything.
    search_for(&mut view, "needle");
    assert_eq!(view.search_match_count(), 1);
    view.begin_search();
    view.confirm_search();
    assert_eq!(view.search_match_count(), 0);
    assert_eq!(view.search_query(), "");
}

#[test]
fn active_match_line_is_highlighted() {
    let f = fixture("# Doc\n\nplain line.\n\nneedle line.\n");
    let mut view = DocView::open(f.path(), 40).expect("open");
    search_for(&mut view, "needle");

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let highlighted = buffer
        .content()
        .iter()
        .filter(|c| c.style().bg.is_some_and(|b| b != ratatui::style::Color::Reset))
        .count();
    assert!(highlighted > 0, "active match line should get a background highlight");
}

#[test]
fn slash_starts_with_an_empty_draft_even_after_a_search() {
    let f = fixture("# Doc\n\nneedle one.\n");
    let mut view = DocView::open(f.path(), 40).expect("open");
    search_for(&mut view, "needle");
    assert_eq!(view.search_match_count(), 1);

    // A new search starts blank; the previous matches stay active
    // until the draft is confirmed or cancelled.
    view.begin_search();
    assert_eq!(view.search_draft(), "");
}

#[test]
fn is_stale_flags_a_changed_file() {
    let f = fixture("# Doc\n\noriginal body.\n");
    let view = DocView::open(f.path(), 40).expect("open");
    assert!(!view.is_stale(), "freshly opened view must not be stale");

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(f.path(), "# Doc\n\nrewritten body.\n").expect("rewrite");
    assert!(view.is_stale(), "mtime change must mark the view stale");
}

#[test]
fn reload_shows_new_content_and_keeps_scroll() {
    let body: String = (1..=50).map(|i| format!("line number {i}\n\n")).collect();
    let f = fixture(&body);
    let mut view = DocView::open(f.path(), 40).expect("open");
    for _ in 0..10 {
        view.scroll_down();
    }
    let before = rendered_text(&mut view, 40, 5);

    let changed: String = (1..=50).map(|i| format!("updated row {i}\n\n")).collect();
    std::fs::write(f.path(), &changed).expect("rewrite");
    view.reload(40).expect("reload");

    assert!(!view.is_stale(), "reload must clear staleness");
    let after = rendered_text(&mut view, 40, 5);
    assert!(after.contains("updated row"), "new content:\n{after}");
    assert_ne!(before, after);
    assert!(
        !after.contains("updated row 1 "),
        "scroll position must be preserved, not reset to top:\n{after}"
    );
}

#[test]
fn reload_reruns_the_active_search() {
    let f = fixture("# Doc\n\nneedle one.\n");
    let mut view = DocView::open(f.path(), 40).expect("open");
    search_for(&mut view, "needle");
    assert_eq!(view.search_match_count(), 1);

    std::fs::write(f.path(), "# Doc\n\nneedle one.\n\nneedle two.\n").expect("rewrite");
    view.reload(40).expect("reload");
    assert_eq!(view.search_query(), "needle", "query survives reload");
    assert_eq!(view.search_match_count(), 2, "matches re-run on new content");
}
