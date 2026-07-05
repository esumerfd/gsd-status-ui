//! Library facade for embedding leaf's markdown rendering in another
//! ratatui application (e.g. gsd-status).
//!
//! Only the standalone modules are compiled into the lib target:
//! `markdown` (parsing → styled ratatui lines) and `theme`. The full
//! application (`app`, `render`, `runtime`, updater, CLI) remains
//! bin-only behind the `bin` feature.
//!
//! The lib compiles a subset of the crate, so items used only by the
//! bin-side modules are intentionally dead here.
#![allow(dead_code, unused_imports)]

mod markdown;
mod theme;

pub mod viewer {
    use ratatui::text::Line;
    use std::sync::OnceLock;
    use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

    /// A markdown document rendered to styled ratatui lines at a fixed width.
    pub struct Document {
        pub lines: Vec<Line<'static>>,
    }

    /// Parse markdown source into styled lines, wrapped to `width` columns.
    ///
    /// Uses leaf's default theme. Syntax/theme sets are loaded once and cached.
    pub fn parse(src: &str, width: usize) -> Document {
        static SS: OnceLock<SyntaxSet> = OnceLock::new();
        static TS: OnceLock<ThemeSet> = OnceLock::new();
        let ss = SS.get_or_init(SyntaxSet::load_defaults_newlines);
        let ts = TS.get_or_init(ThemeSet::load_defaults);
        let syntect_theme = crate::theme::current_syntect_theme(ts);
        let app_theme = crate::theme::app_theme();
        let result = crate::markdown::parse_markdown_with_width(
            src,
            ss,
            syntect_theme,
            width,
            &app_theme.markdown,
            false,
            false,
        );
        Document {
            lines: result.lines,
        }
    }

    /// Lowercased plain text per rendered line, for substring search —
    /// the same shape the app's own search runs against (`plain_lines`).
    pub fn searchable_lines(doc: &Document) -> Vec<String> {
        crate::markdown::build_searchable_lines(&doc.lines)
            .into_iter()
            .map(|line| line.to_lowercase())
            .collect()
    }

    /// Re-style a rendered line with leaf's search highlight (line
    /// background plus emphasized match text) for `query` occurrences.
    pub fn highlight_line(line: &Line<'static>, query: &str) -> Line<'static> {
        crate::markdown::highlight_line(line, &crate::theme::app_theme().markdown, query)
    }
}

#[cfg(test)]
mod lib_tests {
    use super::viewer;

    #[test]
    fn parse_renders_heading_and_body_as_styled_lines() {
        let doc = viewer::parse("# Title\n\nSome body text.\n", 80);
        assert!(!doc.lines.is_empty(), "expected rendered lines");
        let all_text: String = doc
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("Title"), "heading text missing: {all_text}");
        assert!(all_text.contains("Some body text."), "body missing");
    }

    #[test]
    fn parse_highlights_code_fences() {
        let doc = viewer::parse("```rust\nfn main() {}\n```\n", 80);
        let styled_span_count: usize = doc.lines.iter().map(|l| l.spans.len()).sum();
        assert!(styled_span_count > 1, "expected styled code spans");
    }
}
