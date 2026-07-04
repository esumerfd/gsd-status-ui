//! Minimal ANSI SGR → ratatui converter for the status panel. Handles
//! exactly the codes `crate::color` emits (reset, bold, dim, and the six
//! foreground colors) so the TUI status tab reuses the report's colors.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

pub(crate) fn ansi_to_text(s: &str) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut style = Style::default();
    for raw_line in s.lines() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut buf = String::new();
        let mut chars = raw_line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\u{1b}' {
                buf.push(ch);
                continue;
            }
            // Escape sequence: expect "[<digits>(;<digits>)*m"; ignore others.
            if chars.peek() != Some(&'[') {
                continue;
            }
            chars.next();
            let mut params = String::new();
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
                params.push(c);
            }
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), style));
            }
            for code in params.split(';') {
                style = apply_sgr(style, code.parse().unwrap_or(0));
            }
        }
        if !buf.is_empty() {
            spans.push(Span::styled(buf, style));
        }
        lines.push(Line::from(spans));
    }
    Text::from(lines)
}

fn apply_sgr(style: Style, code: u8) -> Style {
    match code {
        0 => Style::default(),
        1 => style.add_modifier(Modifier::BOLD),
        2 => style.add_modifier(Modifier::DIM),
        32 => style.fg(Color::Green),
        33 => style.fg(Color::Yellow),
        34 => style.fg(Color::Blue),
        35 => style.fg(Color::Magenta),
        36 => style.fg(Color::Cyan),
        90 => style.fg(Color::DarkGray),
        _ => style,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;

    fn span_texts(text: &Text) -> Vec<(String, Style)> {
        text.lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| (s.content.to_string(), s.style))
            .collect()
    }

    #[test]
    fn plain_text_passes_through() {
        let t = ansi_to_text("hello\nworld");
        assert_eq!(t.lines.len(), 2);
        assert_eq!(span_texts(&t)[0].0, "hello");
        assert_eq!(span_texts(&t)[0].1, Style::default());
    }

    #[test]
    fn colored_segment_gets_the_matching_ratatui_style() {
        let s = format!("{}ok{} rest", color::GREEN, color::RESET);
        let spans = span_texts(&ansi_to_text(&s));
        assert_eq!(spans[0].0, "ok");
        assert_eq!(spans[0].1.fg, Some(Color::Green));
        assert_eq!(spans[1].0, " rest");
        assert_eq!(spans[1].1, Style::default());
    }

    #[test]
    fn bold_and_color_combine_until_reset() {
        let s = format!("{}{}title{}", color::BOLD, color::CYAN, color::RESET);
        let spans = span_texts(&ansi_to_text(&s));
        assert_eq!(spans[0].0, "title");
        assert_eq!(spans[0].1.fg, Some(Color::Cyan));
        assert!(spans[0].1.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn style_carries_across_lines_until_reset() {
        let s = format!("{}a\nb{}", color::YELLOW, color::RESET);
        let spans = span_texts(&ansi_to_text(&s));
        assert_eq!(spans[1].0, "b");
        assert_eq!(spans[1].1.fg, Some(Color::Yellow));
    }

    #[test]
    fn the_real_report_renders_with_colors() {
        let planning = std::path::Path::new("sample/.planning");
        let state = crate::planning::load_state(planning);
        let phases = crate::planning::load_phases(planning);
        let mut buf = Vec::new();
        crate::report::render(&mut buf, planning, &state, &phases, true).unwrap();
        let text = ansi_to_text(&String::from_utf8_lossy(&buf));
        let styled = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style.fg.is_some())
            .count();
        assert!(styled > 5, "expected many colored spans, got {styled}");
    }
}
