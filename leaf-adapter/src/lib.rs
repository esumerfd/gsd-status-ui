//! The only crate that touches leaf types. gsd-status opens/closes a
//! document panel per tab through `DocView`; everything inside the tab
//! body (markdown parsing, styling, scrolling) lives here.

use ratatui::{layout::Rect, text::Text, widgets::Paragraph, Frame};
use std::path::Path;

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
    doc: leaf::viewer::Document,
    plain_lines: Vec<String>,
    scroll: u16,
    last_viewport: u16,
    search: SearchState,
}

impl DocView {
    /// Read and parse a markdown file, wrapping to `width` columns.
    pub fn open(path: &Path, width: u16) -> Result<Self, DocViewError> {
        let src = std::fs::read_to_string(path).map_err(|source| DocViewError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut doc = leaf::viewer::parse(&src, width as usize);
        // Drop trailing blank lines so to_bottom lands on content, not padding.
        while doc
            .lines
            .last()
            .is_some_and(|l| l.spans.iter().all(|s| s.content.trim().is_empty()))
        {
            doc.lines.pop();
        }
        let plain_lines = leaf::viewer::searchable_lines(&doc);
        Ok(Self {
            title,
            doc,
            plain_lines,
            scroll: 0,
            last_viewport: 10,
            search: SearchState::default(),
        })
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

    pub fn to_top(&mut self) {
        self.scroll = 0;
    }

    /// Scrolls past the end; the render-time clamp settles it on the last page.
    pub fn to_bottom(&mut self) {
        self.scroll = u16::MAX;
    }

    /// Enter search input mode; the draft starts from the last query.
    pub fn begin_search(&mut self) {
        self.search.mode = true;
        self.search.draft = self.search.query.clone();
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
        let q = self.search.query.to_lowercase();
        self.search.matches = self
            .plain_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains(&q))
            .map(|(i, _)| i)
            .collect();
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

    fn clamp_scroll(&mut self, viewport_height: u16) {
        self.last_viewport = viewport_height;
        let max = (self.doc.lines.len() as u16).saturating_sub(viewport_height);
        self.scroll = self.scroll.min(max);
    }
}
