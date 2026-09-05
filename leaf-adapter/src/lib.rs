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
    let mut fence = CodeFence::new();
    for line in src.lines() {
        if fence.is_passthrough(line) {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let trimmed = line.trim();
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

/// Shared fenced-code-block tracker used by every preprocessor pass. A
/// line inside an open fence, or a line that is itself a fence delimiter,
/// passes through untouched either way — `is_passthrough` reports that and
/// updates the open/closed state as a side effect, so a caller need only
/// forward the line verbatim on `true` and otherwise proceed normally.
///
/// Semantics match what `headingify_structural_tags` always did: while
/// open, every line passes through and a line whose trimmed form starts
/// with the same marker closes the fence; while closed, a line indented
/// fewer than four spaces whose trimmed form starts with three backticks
/// or three tildes opens the fence and passes through; a line indented
/// four or more spaces is an indented code block and always passes
/// through, fence state untouched.
struct CodeFence {
    marker: Option<&'static str>,
}

impl CodeFence {
    fn new() -> Self {
        CodeFence { marker: None }
    }

    fn is_passthrough(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        if let Some(marker) = self.marker {
            if trimmed.starts_with(marker) {
                self.marker = None;
            }
            return true;
        }
        let indent = line.len() - line.trim_start().len();
        if indent < 4 {
            if trimmed.starts_with("```") {
                self.marker = Some("```");
                return true;
            }
            if trimmed.starts_with("~~~") {
                self.marker = Some("~~~");
                return true;
            }
        }
        if indent >= 4 {
            return true;
        }
        false
    }
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

/// The character class allowed after a tag name's first (always
/// alphabetic) character. Shared by `is_valid_tag_name` and the inline-pair
/// scanner's `take_tag_name` so the two passes can never disagree on what a
/// tag name is.
fn is_tag_name_continuation_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// A tag name must begin with an ASCII letter and continue with ASCII
/// alphanumerics, underscore, hyphen, period, or colon.
fn is_valid_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(is_tag_name_continuation_char)
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

/// GSD planning documents sometimes carry HTML/XML comments as
/// instructions-to-the-author that a reader of the rendered document never
/// wants to see. This pass removes every comment span — the four
/// characters `<`, `!`, `-`, `-` through the first following `-`, `-`, `>`
/// — before the text reaches `headingify_structural_tags` or
/// `leaf::viewer::parse`. Single-line or spanning many lines, anywhere on
/// a line, any number of spans per line: every comment in the document is
/// removed, with no keyword or convention filter.
///
/// Scope guards match `headingify_structural_tags`'s precedent exactly: a
/// comment inside a fenced code block, or on a line indented four or more
/// spaces, is left literal. Once a comment is open, comment scanning wins
/// over fence tracking — a fence marker inside an open comment is comment
/// text, not a fence.
///
/// If a comment is opened but never closed before end of input, every
/// line held since it opened is restored verbatim rather than swallowing
/// the rest of the document — a stray opening delimiter in prose must
/// never blank out real content (T-QT-02).
fn strip_html_comments(src: &str) -> String {
    let mut out = String::new();
    let mut pending_gap = false;
    let mut fence = CodeFence::new();
    let mut open: Option<OpenComment> = None;

    for line in src.lines() {
        if let Some(state) = open.as_mut() {
            // Comment scanning wins over fence tracking while a comment is
            // open: this line's only fate is "still comment" or "closes."
            state.raw_lines.push(line.to_string());
            if let Some(close_rel) = line.find("-->") {
                let after = &line[close_rel + 3..];
                let prefix = std::mem::take(&mut state.prefix);
                open = None;
                match scan_comment_spans(after) {
                    SpanScan::Closed { survivor, .. } => {
                        let full_survivor = format!("{prefix}{survivor}");
                        emit_stripped_line(&mut out, &mut pending_gap, line, true, &full_survivor);
                    }
                    SpanScan::StillOpen { prefix: new_prefix } => {
                        open = Some(OpenComment {
                            prefix: format!("{prefix}{new_prefix}"),
                            raw_lines: vec![line.to_string()],
                        });
                    }
                }
            }
            continue;
        }

        if fence.is_passthrough(line) {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        match scan_comment_spans(line) {
            SpanScan::Closed { survivor, removed } => {
                emit_stripped_line(&mut out, &mut pending_gap, line, removed, &survivor);
            }
            SpanScan::StillOpen { prefix } => {
                open = Some(OpenComment {
                    prefix,
                    raw_lines: vec![line.to_string()],
                });
            }
        }
    }

    if let Some(state) = open {
        for raw in state.raw_lines {
            out.push_str(&raw);
            out.push('\n');
        }
    }

    out
}

/// Buffered state while a multi-line comment is open: the survivor text
/// already known to precede it on the line that opened it, and every raw
/// line seen since then — restored verbatim if the comment never closes.
struct OpenComment {
    prefix: String,
    raw_lines: Vec<String>,
}

/// The result of scanning a string for zero or more comment spans, none of
/// which is open coming in.
enum SpanScan {
    /// No comment is left open at the end of the scanned text.
    Closed { survivor: String, removed: bool },
    /// The text ends inside an unterminated comment; `prefix` is
    /// everything known to survive before that final, still-open span.
    StillOpen { prefix: String },
}

/// Scan `s` left to right for comment-open/comment-close spans, removing
/// every complete one. Never backtracks and never consumes past what it
/// finds — each byte is visited at most once.
fn scan_comment_spans(s: &str) -> SpanScan {
    let mut survivor = String::new();
    let mut removed = false;
    let mut rest = s;
    loop {
        match rest.find("<!--") {
            None => {
                survivor.push_str(rest);
                return SpanScan::Closed { survivor, removed };
            }
            Some(open_pos) => {
                survivor.push_str(&rest[..open_pos]);
                let after_open = &rest[open_pos + 4..];
                match after_open.find("-->") {
                    None => {
                        return SpanScan::StillOpen { prefix: survivor };
                    }
                    Some(close_pos) => {
                        removed = true;
                        rest = &after_open[close_pos + 3..];
                    }
                }
            }
        }
    }
}

/// Apply Rule A's line-level output rule: an untouched line is emitted
/// byte-for-byte; a line that lost content is emitted trimmed of trailing
/// whitespace, or dropped entirely (raising `pending_gap`) if nothing but
/// whitespace survived. A genuinely blank source line is swallowed instead
/// of doubling a gap a dropped comment already left behind, but only while
/// that gap is still open at the tail of the output.
fn emit_stripped_line(
    out: &mut String,
    pending_gap: &mut bool,
    original_line: &str,
    removed: bool,
    survivor: &str,
) {
    if !removed {
        if original_line.trim().is_empty() {
            if *pending_gap && (out.is_empty() || out.ends_with("\n\n")) {
                return;
            }
            out.push_str(original_line);
            out.push('\n');
            *pending_gap = false;
            return;
        }
        out.push_str(original_line);
        out.push('\n');
        return;
    }

    if survivor.trim().is_empty() {
        *pending_gap = true;
        return;
    }
    out.push_str(survivor.trim_end());
    out.push('\n');
    *pending_gap = false;
}

/// A same-line pair — open tag, plain inner text, matching close tag,
/// nothing nested — renders as angle-bracketed clutter that throws the tag
/// name away, leaving a value with no indication of what it labels. This
/// pass rewrites that one shape into an emphasized tag name carrying a
/// trailing colon, then a space, then the value as plain text, generic over
/// any tag name, before the text reaches `headingify_structural_tags` or
/// `leaf::viewer::parse`.
///
/// Scope guards match Rule A's precedent exactly: a pair inside a fenced
/// code block, or on a line indented four or more spaces, is left literal.
/// The scan never backtracks and never consumes input on a failed match —
/// a failed attempt at one `<` costs exactly that one character, keeping
/// the whole pass linear even on adversarial input (T-QT-01).
fn emphasize_inline_tags(src: &str) -> String {
    let mut out = String::new();
    let mut fence = CodeFence::new();
    for line in src.lines() {
        if fence.is_passthrough(line) {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        out.push_str(&emphasize_line(line));
        out.push('\n');
    }
    out
}

/// Left-to-right scan of one line for inline tag pairs, emitting an
/// emphasized tag-name label followed by the plain value on a match, and
/// the literal `<` (only) on a failed attempt.
fn emphasize_line(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    loop {
        match rest.find('<') {
            None => {
                out.push_str(rest);
                return out;
            }
            Some(lt_pos) => {
                out.push_str(&rest[..lt_pos]);
                let after_lt = &rest[lt_pos + 1..];
                match try_match_inline_pair(after_lt) {
                    Some((name, inner, resume)) => {
                        out.push('*');
                        out.push_str(name);
                        out.push_str(":* ");
                        out.push_str(&inner);
                        rest = resume;
                    }
                    None => {
                        out.push('<');
                        rest = after_lt;
                    }
                }
            }
        }
    }
}

/// Attempt the five-part match described in conversion_rules against `s`,
/// the text immediately following an already-consumed `<`. On success,
/// returns three pieces, in order: the tag name, the trimmed inner text,
/// and the slice remaining to resume scanning from; `None`, consuming
/// nothing, on any failure.
fn try_match_inline_pair(s: &str) -> Option<(&str, String, &str)> {
    let name = take_tag_name(s)?;
    let after_name = &s[name.len()..];

    // Either `>` immediately, or whitespace then attribute text containing
    // no `<` and no `>`, then `>`.
    let after_open_tag = if let Some(rest) = after_name.strip_prefix('>') {
        rest
    } else {
        if !after_name.starts_with(|c: char| c.is_whitespace()) {
            return None;
        }
        let gt_pos = after_name.find('>')?;
        let attrs = &after_name[..gt_pos];
        if attrs.contains('<') {
            return None;
        }
        &after_name[gt_pos + 1..]
    };

    // Inner text: everything up to the next `<`. No `>`, not empty/blank.
    let next_lt = after_open_tag.find('<')?;
    let inner = &after_open_tag[..next_lt];
    if inner.contains('>') {
        return None;
    }
    let inner_trimmed = inner.trim();
    if inner_trimmed.is_empty() {
        return None;
    }

    // `</`, the SAME name exactly and case-sensitively, optional
    // whitespace, then `>`.
    let after_next_lt = &after_open_tag[next_lt + 1..];
    let after_slash = after_next_lt.strip_prefix('/')?;
    let after_close_name = after_slash.strip_prefix(name)?;
    let resume = after_close_name.trim_start().strip_prefix('>')?;

    Some((name, inner_trimmed.to_string(), resume))
}

/// Extract the maximal tag-name prefix of `s` (an ASCII letter followed by
/// `is_tag_name_continuation_char`s), reusing the same character class
/// `is_valid_tag_name` validates against so the whole-line and inline
/// passes can never disagree on what a tag name is. `None` if `s` does not
/// begin with an ASCII letter.
fn take_tag_name(s: &str) -> Option<&str> {
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let end = chars
        .find(|&(_, c)| !is_tag_name_continuation_char(c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    Some(&s[..end])
}

/// Compose the preprocessor pipeline that runs ahead of
/// `leaf::viewer::parse`. Order is load-bearing:
///
/// 1. `strip_html_comments` runs first — a comment spanning several lines
///    can contain a whole-line bare tag, and if the heading pass ran
///    first, that commented-out tag would surface as a real heading.
/// 2. `emphasize_inline_tags` runs second — a whole-line attributed
///    open+text+close tag would otherwise be classified by `classify_tag`
///    as a heading-worthy open tag, silently discarding the value. Running
///    the inline pass first turns it into ordinary prose before the
///    heading pass ever sees it.
/// 3. `headingify_structural_tags` runs last, unchanged from 260902-rej.
fn preprocess_markdown(src: &str) -> String {
    headingify_structural_tags(&emphasize_inline_tags(&strip_html_comments(src)))
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
        assert_eq!(
            strip_html_comments("# H\n\n<!-- c -->\n\nBody\n"),
            "# H\n\nBody\n"
        );
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

    // -- Rule B: emphasize_inline_tags ------------------------------------

    #[test]
    fn pair_alone_on_a_line_becomes_emphasized_name_colon_then_plain_value() {
        assert_eq!(emphasize_inline_tags("<a>value</a>\n"), "*a:* value\n");
    }

    #[test]
    fn inline_pair_core_format_pins_name_as_label_before_value() {
        assert_eq!(emphasize_inline_tags("<a>b</a>\n"), "*a:* b\n");
    }

    #[test]
    fn tag_name_is_reproduced_verbatim_never_title_cased_unlike_the_heading_rule() {
        let output = emphasize_inline_tags("<due-date>Friday</due-date>\n");
        assert_eq!(output, "*due-date:* Friday\n");
        assert!(
            !output.contains("Due Date") && !output.contains("Due-Date"),
            "inline-pair label must never be title-cased like the heading rule:\n{output}"
        );
        // The whole-line heading rule (Sections A/B territory) still title-cases
        // the very same name — the two rules disagree on casing on purpose.
        assert_eq!(headingify_structural_tags("<due-date>\n"), "# Due Date\n");
    }

    #[test]
    fn pair_embedded_mid_sentence_preserves_surrounding_prose_exactly() {
        assert_eq!(
            emphasize_inline_tags("Owner is <owner>Ed</owner> today.\n"),
            "Owner is *owner:* Ed today.\n"
        );
    }

    #[test]
    fn attributed_open_tag_yields_emphasis_with_no_attribute_leaking_through() {
        let input = "<a href=\"x\">value</a>\n";
        let output = emphasize_inline_tags(input);
        assert_eq!(output, "*a:* value\n");
        assert!(!output.contains("href"));
        assert!(!output.contains('x'));
    }

    #[test]
    fn several_tag_names_each_label_with_their_own_verbatim_name() {
        assert_eq!(emphasize_inline_tags("<a>v</a>\n"), "*a:* v\n");
        assert_eq!(emphasize_inline_tags("<b>v</b>\n"), "*b:* v\n");
        assert_eq!(emphasize_inline_tags("<em>v</em>\n"), "*em:* v\n");
        assert_eq!(emphasize_inline_tags("<owner>v</owner>\n"), "*owner:* v\n");
        assert_eq!(
            emphasize_inline_tags("<due-date>Friday</due-date>\n"),
            "*due-date:* Friday\n"
        );
    }

    #[test]
    fn inner_whitespace_is_trimmed_before_the_value_follows_the_label() {
        assert_eq!(emphasize_inline_tags("<a> value </a>\n"), "*a:* value\n");
    }

    #[test]
    fn mismatched_close_tag_name_is_left_literal() {
        let input = "<a>value</b>\n";
        assert_eq!(emphasize_inline_tags(input), input);
    }

    #[test]
    fn wrapping_pair_is_left_literal_while_its_nested_pair_converts_independently() {
        // The outer <a>'s inner-text scan hits the nested tag's `<`
        // immediately, so its inner text is empty and it never matches --
        // the wrapping pair is left literal, exactly as conversion_rules
        // requires ("a pair wrapping another pair is left literal"). The
        // nested <b>value</b>, reached moments later in the same
        // left-to-right, non-backtracking scan, is a perfectly
        // well-formed pair on its own and converts normally -- there is
        // no nesting-depth tracking in this pass to tell it apart from
        // any other independent pair on the line.
        let output = emphasize_inline_tags("<a><b>value</b></a>\n");
        assert!(
            output.contains("<a>") && output.contains("</a>"),
            "the wrapping pair must not be collapsed into emphasis:\n{output}"
        );
        assert_eq!(output, "<a>*b:* value</a>\n");
    }

    #[test]
    fn empty_pair_is_left_literal() {
        let input = "<a></a>\n";
        assert_eq!(emphasize_inline_tags(input), input);
    }

    #[test]
    fn pair_inside_backtick_and_tilde_fenced_blocks_is_left_literal() {
        let backtick = "```\n<a>value</a>\n```\n";
        assert_eq!(emphasize_inline_tags(backtick), backtick);
        let tilde = "~~~\n<a>value</a>\n~~~\n";
        assert_eq!(emphasize_inline_tags(tilde), tilde);
    }

    #[test]
    fn pair_on_an_indented_line_is_left_literal() {
        let input = "    <a>value</a>\n";
        assert_eq!(emphasize_inline_tags(input), input);
    }

    #[test]
    fn two_pairs_on_one_line_are_both_converted() {
        assert_eq!(
            emphasize_inline_tags("<a>one</a> and <b>two</b>\n"),
            "*a:* one and *b:* two\n"
        );
    }

    #[test]
    fn document_with_no_pairs_round_trips_byte_for_byte_unchanged() {
        let input = "# Heading\n\nJust prose, no inline pairs here.\n";
        assert_eq!(emphasize_inline_tags(input), input);
    }

    #[test]
    fn whole_line_bare_tag_is_not_claimed_by_this_pass() {
        // Rule 1's territory: no same-line close, so Rule B never fires,
        // and headingify_structural_tags still sees it untouched.
        let input = "<objective>\n";
        assert_eq!(emphasize_inline_tags(input), input);
    }

    // -- preprocess_markdown pipeline: Rule B vs Rule 1 collision guard --

    #[test]
    fn attributed_pair_alone_on_its_line_becomes_emphasis_not_a_heading_with_a_discarded_value() {
        let input = "<owner ref=\"x\">Ed</owner>\n";
        let output = preprocess_markdown(input);
        assert!(
            output.contains("Ed"),
            "value must survive, not be discarded by the heading pass:\n{output}"
        );
        assert!(
            !output.trim_start().starts_with('#'),
            "an attributed same-line pair must not become a heading:\n{output}"
        );
    }

    #[test]
    fn all_three_rules_apply_at_once_in_one_document() {
        let input =
            "<objective>\nOwner is <owner>Ed</owner>.\n<!-- reviewer note -->\n</objective>\n";
        let output = preprocess_markdown(input);
        assert!(
            output.contains('#'),
            "bare tag should still heading:\n{output}"
        );
        assert!(
            output.contains("Owner is *owner:* Ed."),
            "inline pair should emphasize with the name as a label:\n{output}"
        );
        assert!(
            !output.contains("reviewer note"),
            "comment should vanish:\n{output}"
        );
    }
}
