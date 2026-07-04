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

pub struct DocView {
    title: String,
    doc: leaf::viewer::Document,
    scroll: u16,
    last_viewport: u16,
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
        Ok(Self {
            title,
            doc,
            scroll: 0,
            last_viewport: 10,
        })
    }

    /// File name, used as the tab label.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Draw the document into a tab body.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.clamp_scroll(area.height);
        let paragraph =
            Paragraph::new(Text::from(self.doc.lines.clone())).scroll((self.scroll, 0));
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

    fn page(&self) -> u16 {
        self.last_viewport.saturating_sub(1).max(1)
    }

    fn clamp_scroll(&mut self, viewport_height: u16) {
        self.last_viewport = viewport_height;
        let max = (self.doc.lines.len() as u16).saturating_sub(viewport_height);
        self.scroll = self.scroll.min(max);
    }
}
