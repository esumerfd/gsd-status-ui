//! The only crate that touches leaf types. gsd-status opens/closes a
//! document panel per tab through `DocView`; everything inside the tab
//! body (markdown parsing, styling, scrolling) lives here.

use ratatui::{layout::Rect, text::Text, widgets::Paragraph, Frame};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug)]
pub enum DocViewError {
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for DocViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocViewError::Io { path, source } => {
                write!(f, "cannot open {}: {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for DocViewError {}

/// In-document search, modeled on leaf's own SearchState: `/` collects a
/// draft, confirming runs a case-insensitive substring search over the
/// rendered lines, n/N cycle matches with wraparound.
#[derive(Default)]
struct SearchState {
    mode: bool,
    draft: String,
    query: String,
    matches: Vec<usize>,
    idx: usize,
}

pub struct DocView {
    title: String,
    path: PathBuf,
    doc: leaf::viewer::Document,
    plain_lines: Vec<String>,
    mtime: Option<SystemTime>,
    scroll: u16,
    last_viewport: u16,
    search: SearchState,
}

/// GSD planning documents mark up their sections with bare structural tags
/// (`<objective>`, `<task type="auto">`, and the like). pulldown-cmark
/// classifies a whole-line `<tag>` as an HTML block, and leaf renders HTML
/// blocks as raw literal text by design — vendor/leaf stays untouched by
/// this project, so that raw-text behavior can't be changed at the source.
/// This pass rewrites those bare tag lines into markdown headings before
/// the text ever reaches `leaf::viewer::parse`, so a planning document
/// reads as a structured outline instead of a wall of angle-bracket markup.
fn headingify_structural_tags(src: &str) -> String {
    let mut out = String::new();
    let mut stack: Vec<String> = Vec::new();
    // While `Some`, every line is inside a fenced code block and passes
    // through untouched until a matching closing fence is seen. The fence
    // marker itself is checked with 0-3 leading spaces, matching the
    // indentation rule for structural tag lines.
    let mut fence: Option<&'static str> = None;
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            out.push_str(line);
            out.push('\n');
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent < 4 {
            if trimmed.starts_with("```") {
                fence = Some("```");
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if trimmed.starts_with("~~~") {
                fence = Some("~~~");
                out.push_str(line);
                out.push('\n');
                continue;
            }
        }
        if indent >= 4 {
            // Indented code block — never a structural tag line.
            out.push_str(line);
            out.push('\n');
            continue;
        }
        match classify_tag(trimmed) {
            Some(TagShape::Close(name)) => {
                // Truncate to just below the topmost (deepest, i.e. most
                // recently pushed) occurrence of this name. A name that
                // was never opened isn't ours to convert.
                if let Some(pos) = stack.iter().rposition(|n| n == name) {
                    stack.truncate(pos);
                    ensure_blank_line(&mut out);
                } else {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            Some(TagShape::Open(name)) => {
                emit_heading(&mut out, stack.len(), name);
                stack.push(name.to_string());
            }
            Some(TagShape::SelfClosing(name)) => {
                // No push, no depth change — the tags that follow are
                // siblings of this one, not children.
                emit_heading(&mut out, stack.len(), name);
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// The three whole-line tag shapes this preprocessor recognizes. Anything
/// else (prose, an inline tag fragment, a non-tag line) classifies as
/// `None` at the call site and is passed through untouched.
enum TagShape<'a> {
    Open(&'a str),
    SelfClosing(&'a str),
    Close(&'a str),
}

/// Classify a trimmed line as an open/self-closing/close structural tag,
/// extracting the tag name with any attributes discarded. Returns `None`
/// for anything that isn't a single whole-line tag with a valid name —
/// including HTML comments (`<!--`), doctypes (`<!DOCTYPE`), and
/// processing instructions (`<?xml`), whose names begin with `!` or `?`
/// rather than an ASCII letter.
fn classify_tag(trimmed: &str) -> Option<TagShape<'_>> {
    let inner = trimmed.strip_prefix('<')?.strip_suffix('>')?;
    if let Some(name) = inner.strip_prefix('/') {
        let name = name.trim();
        return is_valid_tag_name(name).then_some(TagShape::Close(name));
    }
    if let Some(rest) = inner.strip_suffix('/') {
        let rest = rest.trim();
        let name = rest.split_whitespace().next().unwrap_or(rest);
        return is_valid_tag_name(name).then_some(TagShape::SelfClosing(name));
    }
    let name = inner.split_whitespace().next().unwrap_or(inner);
    is_valid_tag_name(name).then_some(TagShape::Open(name))
}

/// A tag name must begin with an ASCII letter and continue with ASCII
/// alphanumerics, underscore, hyphen, period, or colon.
fn is_valid_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

/// Emit a heading for `name` at the level implied by `depth` (the current
/// stack length before this tag), inserting the blank-line separator
/// first.
fn emit_heading(out: &mut String, depth: usize, name: &str) {
    let level = (depth + 1).min(6);
    ensure_blank_line(out);
    out.push_str(&"#".repeat(level));
    out.push(' ');
    out.push_str(&title_case(name));
    out.push('\n');
}

/// Emit a blank line before/after a heading, unless there's nothing
/// emitted yet or the most recently emitted line is already blank.
fn ensure_blank_line(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// Split a tag name on underscore/hyphen/period/colon, uppercase only the
/// first character of each segment (leaving the rest untouched so acronyms
/// and camelCase survive), and join with single spaces.
fn title_case(name: &str) -> String {
    name.split(['_', '-', '.', ':'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// TODO(Task 1 RED): real implementation lands in the GREEN commit.
fn strip_html_comments(src: &str) -> String {
    src.to_string()
}

// TODO(Task 1 RED): the inline-pair pass slots in here in Task 2.
fn preprocess_markdown(src: &str) -> String {
    headingify_structural_tags(&strip_html_comments(src))
}

/// Read and parse a file into rendered lines plus their searchable text.
/// The mtime is taken before the read so a write racing the read shows
/// up as stale on the next check rather than being missed.
fn load(path: &Path, width: u16) -> Result<LoadedDoc, DocViewError> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let src = std::fs::read_to_string(path).map_err(|source| DocViewError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let preprocessed = preprocess_markdown(&src);
    let mut doc = leaf::viewer::parse(&preprocessed, width as usize);
    // Drop trailing blank lines so to_bottom lands on content, not padding.
    while doc
        .lines
        .last()
        .is_some_and(|l| l.spans.iter().all(|s| s.content.trim().is_empty()))
    {
        doc.lines.pop();
    }
    let plain_lines = leaf::viewer::searchable_lines(&doc);
    Ok(LoadedDoc {
        doc,
        plain_lines,
        mtime,
    })
}

struct LoadedDoc {
    doc: leaf::viewer::Document,
    plain_lines: Vec<String>,
    mtime: Option<SystemTime>,
}

impl DocView {
    /// Read and parse a markdown file, wrapping to `width` columns.
    pub fn open(path: &Path, width: u16) -> Result<Self, DocViewError> {
        let loaded = load(path, width)?;
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(Self {
            title,
            path: path.to_path_buf(),
            doc: loaded.doc,
            plain_lines: loaded.plain_lines,
            mtime: loaded.mtime,
            scroll: 0,
            last_viewport: 10,
            search: SearchState::default(),
        })
    }

    /// True when the file's mtime has moved since the last open/reload.
    /// A file that can't be stat'ed (deleted, mid-rename) is not stale —
    /// there is nothing new to show yet.
    pub fn is_stale(&self) -> bool {
        match std::fs::metadata(&self.path).and_then(|m| m.modified()) {
            Ok(fresh) => Some(fresh) != self.mtime,
            Err(_) => false,
        }
    }

    /// Re-read the file, keeping the scroll position (clamped at the next
    /// render) and re-running the active search query on the new content
    /// without jumping the viewport.
    pub fn reload(&mut self, width: u16) -> Result<(), DocViewError> {
        let loaded = load(&self.path, width)?;
        self.doc = loaded.doc;
        self.plain_lines = loaded.plain_lines;
        self.mtime = loaded.mtime;
        if !self.search.query.is_empty() {
            self.compute_matches();
        }
        Ok(())
    }

    /// Fill `matches` for the current query and reset the active index.
    fn compute_matches(&mut self) {
        let q = self.search.query.to_lowercase();
        self.search.matches = self
            .plain_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.search.idx = 0;
    }

    /// File name, used as the tab label.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Draw the document into a tab body. The active search match line
    /// (if visible) gets leaf's search highlight.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.clamp_scroll(area.height);
        let mut lines = self.doc.lines.clone();
        if let Some(&line_idx) = self.search.matches.get(self.search.idx) {
            if !self.search.query.is_empty() {
                if let Some(line) = lines.get_mut(line_idx) {
                    *line = leaf::viewer::highlight_line(line, &self.search.query);
                }
            }
        }
        let paragraph = Paragraph::new(Text::from(lines)).scroll((self.scroll, 0));
        frame.render_widget(paragraph, area);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(self.page());
    }

    pub fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(self.page());
    }

    pub fn half_page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(self.half_page());
    }

    pub fn half_page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(self.half_page());
    }

    pub fn to_top(&mut self) {
        self.scroll = 0;
    }

    /// Scrolls past the end; the render-time clamp settles it on the last page.
    pub fn to_bottom(&mut self) {
        self.scroll = u16::MAX;
    }

    /// Enter search input mode with a blank draft. (Leaf pre-fills the
    /// last query here; we don't — q/Esc backing out to the status panel
    /// makes re-searching the same word rare, retyping cheap.)
    pub fn begin_search(&mut self) {
        self.search.mode = true;
        self.search.draft.clear();
    }

    /// Leave input mode and drop the query and matches entirely.
    pub fn cancel_search(&mut self) {
        self.search = SearchState::default();
    }

    /// Leave input mode and run the drafted query; an empty draft clears
    /// the search. Jumps to the first matching line.
    pub fn confirm_search(&mut self) {
        self.search.mode = false;
        self.search.query = std::mem::take(&mut self.search.draft);
        self.search.matches.clear();
        self.search.idx = 0;
        if self.search.query.is_empty() {
            return;
        }
        self.compute_matches();
        self.jump_to_match();
    }

    /// Run `query` as the active search without going through a draft —
    /// for a jump that arrives with the term already known (finding a
    /// requirement by ID). Leaves the same state `confirm_search` does,
    /// input mode included (off), so the caller gets the highlight, the
    /// scroll-to-first-match, and the `match n/N` footer for free. An
    /// empty `query` clears the search.
    pub fn set_search(&mut self, query: &str) {
        self.search.mode = false;
        self.search.draft.clear();
        self.search.query = query.to_string();
        self.search.matches.clear();
        self.search.idx = 0;
        if self.search.query.is_empty() {
            return;
        }
        self.compute_matches();
        self.jump_to_match();
    }

    pub fn push_search_draft(&mut self, ch: char) {
        self.search.draft.push(ch);
    }

    pub fn pop_search_draft(&mut self) {
        self.search.draft.pop();
    }

    pub fn next_match(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        self.search.idx = (self.search.idx + 1) % self.search.matches.len();
        self.jump_to_match();
    }

    pub fn prev_match(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        self.search.idx = self
            .search
            .idx
            .checked_sub(1)
            .unwrap_or(self.search.matches.len() - 1);
        self.jump_to_match();
    }

    pub fn is_search_mode(&self) -> bool {
        self.search.mode
    }

    pub fn search_draft(&self) -> &str {
        &self.search.draft
    }

    pub fn search_query(&self) -> &str {
        &self.search.query
    }

    pub fn search_match_count(&self) -> usize {
        self.search.matches.len()
    }

    /// 0-based index of the active match.
    pub fn search_index(&self) -> usize {
        self.search.idx
    }

    fn jump_to_match(&mut self) {
        if let Some(&line) = self.search.matches.get(self.search.idx) {
            self.scroll = line.min(u16::MAX as usize) as u16;
        }
    }

    fn page(&self) -> u16 {
        self.last_viewport.saturating_sub(1).max(1)
    }

    fn half_page(&self) -> u16 {
        (self.page() / 2).max(1)
    }

    fn clamp_scroll(&mut self, viewport_height: u16) {
        self.last_viewport = viewport_height;
        let max = (self.doc.lines.len() as u16).saturating_sub(viewport_height);
        self.scroll = self.scroll.min(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_depth_increases_heading_level_and_resets_on_close() {
        let input = "<foo>\na\n<bar>\nb\n<baz>\nc\n</baz>\n</bar>\n</foo>\n";
        let expected = "# Foo\na\n\n## Bar\nb\n\n### Baz\nc\n\n";
        assert_eq!(headingify_structural_tags(input), expected);
    }

    #[test]
    fn sibling_top_level_tags_both_yield_one_hash() {
        let input = "<foo>\na\n</foo>\n<bar>\nb\n</bar>\n";
        let expected = "# Foo\na\n\n# Bar\nb\n\n";
        assert_eq!(headingify_structural_tags(input), expected);
    }

    #[test]
    fn seven_levels_of_nesting_cap_heading_level_at_six() {
        let input = "<t1>\n<t2>\n<t3>\n<t4>\n<t5>\n<t6>\n<t7>\n";
        let expected = "# T1\n\n## T2\n\n### T3\n\n#### T4\n\n##### T5\n\n###### T6\n\n###### T7\n";
        let output = headingify_structural_tags(input);
        assert_eq!(output, expected);
        assert!(
            !output.contains("#######"),
            "no seven-hash heading should ever be emitted:\n{output}"
        );
    }

    #[test]
    fn tag_names_convert_to_spaced_title_case() {
        assert_eq!(
            headingify_structural_tags("<deploy_target>\n"),
            "# Deploy Target\n"
        );
        assert_eq!(
            headingify_structural_tags("<read-me-first>\n"),
            "# Read Me First\n"
        );
        assert_eq!(headingify_structural_tags("<step.one>\n"), "# Step One\n");
        assert_eq!(headingify_structural_tags("<ns:step>\n"), "# Ns Step\n");
        // A segment's non-leading characters survive untouched (acronyms/camelCase).
        assert_eq!(headingify_structural_tags("<stepUAT>\n"), "# StepUAT\n");
    }

    #[test]
    fn attributes_never_reach_the_heading_text() {
        let input = "<foo bar=\"baz qux\" other='thing'>\n";
        let output = headingify_structural_tags(input);
        assert_eq!(output, "# Foo\n");
        assert!(!output.contains("bar"));
        assert!(!output.contains("baz"));
        assert!(!output.contains("qux"));
        assert!(!output.contains("other"));
        assert!(!output.contains("thing"));
    }

    #[test]
    fn self_closing_tag_emits_heading_without_deepening_what_follows() {
        let input = "<foo/>\n<bar>\nb\n</bar>\n";
        let expected = "# Foo\n\n# Bar\nb\n\n";
        assert_eq!(headingify_structural_tags(input), expected);
    }

    #[test]
    fn self_closing_tag_with_attributes_and_space_before_slash() {
        let input = "<foo attr=\"x\" />\n";
        assert_eq!(headingify_structural_tags(input), "# Foo\n");
    }

    #[test]
    fn unmatched_close_tag_passes_through_unchanged() {
        let input = "a\n</foo>\nb\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    #[test]
    fn heading_is_preceded_by_blank_line_but_not_doubled_when_one_already_exists() {
        // The blank line is already in the source between "a" and "<bar>";
        // the preprocessor must not add a second one before the heading,
        // and the close tags below must not stack up blank lines either.
        let input = "<foo>\na\n\n<bar>\nb\n</bar>\n</foo>\n";
        let expected = "# Foo\na\n\n## Bar\nb\n\n";
        assert_eq!(headingify_structural_tags(input), expected);
    }

    #[test]
    fn tag_shaped_line_inside_a_backtick_fenced_code_block_passes_through_unchanged() {
        let input = "```\n<foo>\n```\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    #[test]
    fn tag_shaped_line_inside_a_tilde_fenced_code_block_passes_through_unchanged() {
        let input = "~~~\n<foo>\n~~~\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    #[test]
    fn tag_shaped_fragment_mid_sentence_is_left_alone() {
        let input = "See the <foo> tag for details.\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    #[test]
    fn indented_tag_shaped_line_passes_through_unchanged() {
        // Four or more leading spaces means an indented code block.
        let input = "    <foo>\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    #[test]
    fn html_comment_and_doctype_lines_pass_through_unchanged() {
        // Names beginning with '!' or '?' (comments, doctypes, processing
        // instructions) don't begin with an ASCII letter, so they're not ours.
        let input = "<!-- a comment -->\n<!DOCTYPE html>\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    #[test]
    fn document_with_no_structural_tags_round_trips_unchanged() {
        let input = "# Heading\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- item one\n- item two\n\n```\ncode here\n```\n\n> a quote\n";
        assert_eq!(headingify_structural_tags(input), input);
    }

    // -- Rule A: strip_html_comments -------------------------------------

    #[test]
    fn comment_only_line_disappears_entirely() {
        let input = "A\n<!-- c -->\nB\n";
        assert_eq!(strip_html_comments(input), "A\nB\n");
    }

    #[test]
    fn comment_spanning_several_lines_is_removed_in_full() {
        let input = "Para one.\n\n<!--\nmiddle line one\nmiddle line two\n-->\n\nPara two.\n";
        assert_eq!(strip_html_comments(input), "Para one.\n\nPara two.\n");
    }

    #[test]
    fn comment_removal_blank_line_hygiene_matches_worked_expectations() {
        // Each assertion is one row of the worked-expectations table in
        // conversion_rules, `[C]` written out as a real single-line comment.
        assert_eq!(strip_html_comments("# H\n\n<!-- c -->\n\nBody\n"), "# H\n\nBody\n");
        assert_eq!(strip_html_comments("A\n<!-- c -->\nB\n"), "A\nB\n");
        assert_eq!(strip_html_comments("A\n\n<!-- c -->\nB\n"), "A\n\nB\n");
        assert_eq!(strip_html_comments("A\n<!-- c -->\n\nB\n"), "A\n\nB\n");
        assert_eq!(
            strip_html_comments("A\n\n<!-- c -->\n<!-- c -->\n\nB\n"),
            "A\n\nB\n"
        );
        assert_eq!(strip_html_comments("<!-- c -->\n\n# H\n"), "# H\n");
    }

    #[test]
    fn trailing_comment_leaves_prose_with_no_trailing_whitespace() {
        let input = "Some prose. <!-- note -->\n";
        let output = strip_html_comments(input);
        assert_eq!(output, "Some prose.\n");
        assert!(
            !output.ends_with(" \n"),
            "trailing whitespace left behind:\n{output:?}"
        );
    }

    #[test]
    fn two_comments_on_one_line_are_both_removed() {
        let input = "A <!--one--> B <!--two--> C\n";
        let output = strip_html_comments(input);
        assert!(!output.contains("<!--") && !output.contains("-->"));
        assert!(output.contains('A') && output.contains('B') && output.contains('C'));
    }

    #[test]
    fn comment_inside_backtick_and_tilde_fenced_blocks_survives_unchanged() {
        let backtick = "```\n<!-- inside -->\n```\n";
        assert_eq!(strip_html_comments(backtick), backtick);
        let tilde = "~~~\n<!-- inside -->\n~~~\n";
        assert_eq!(strip_html_comments(tilde), tilde);
    }

    #[test]
    fn comment_line_indented_four_or_more_spaces_survives_unchanged() {
        let input = "    <!-- indented -->\n";
        assert_eq!(strip_html_comments(input), input);
    }

    #[test]
    fn unterminated_comment_restores_the_rest_of_the_document_verbatim() {
        let input = "Some text.\n<!-- opens but never closes\nmore text\n";
        assert_eq!(strip_html_comments(input), input);
    }

    #[test]
    fn document_with_no_comments_round_trips_through_strip_html_comments_unchanged() {
        let input = "# Heading\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- item one\n- item two\n\n```\ncode here\n```\n\n> a quote\n";
        assert_eq!(strip_html_comments(input), input);
    }

    // -- preprocess_markdown pipeline ordering ----------------------------

    #[test]
    fn preprocess_markdown_round_trips_a_document_with_no_comments_and_no_tags() {
        let input = "# Heading\n\nJust prose, no tags and no comments.\n";
        assert_eq!(preprocess_markdown(input), input);
    }

    #[test]
    fn preprocess_markdown_strips_comments_before_headingify_so_a_commented_tag_produces_no_heading(
    ) {
        let input = "Intro.\n\n<!--\n<objective>\n-->\n\nOutro.\n";
        let output = preprocess_markdown(input);
        assert!(
            !output.contains('#'),
            "a commented-out tag must not surface as a heading:\n{output}"
        );
        assert!(output.contains("Intro."));
        assert!(output.contains("Outro."));
    }
}
