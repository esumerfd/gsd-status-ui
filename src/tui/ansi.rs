//! Minimal ANSI SGR → ratatui converter for the status panel. Handles
//! exactly the codes `crate::color` emits (reset, bold, dim, the named
//! foreground colors, and `38;5;<n>` palette colors) so the TUI status tab
//! reuses the report's colors.

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
            let codes: Vec<u8> = params.split(';').map(|c| c.parse().unwrap_or(0)).collect();
            let mut i = 0;
            while i < codes.len() {
                // "38;5;<n>" is one 256-palette foreground color spanning three
                // params, not three independent codes.
                if codes[i] == 38 && codes.get(i + 1) == Some(&5) {
                    if let Some(&n) = codes.get(i + 2) {
                        style = style.fg(Color::Indexed(n));
                    }
                    i += 3;
                    continue;
                }
                style = apply_sgr(style, codes[i]);
                i += 1;
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
        94 => style.fg(Color::LightBlue),
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
    fn bright_blue_maps_to_its_256_palette_index() {
        let s = format!("{}planned{}", color::BRIGHT_BLUE, color::RESET);
        let spans = span_texts(&ansi_to_text(&s));
        assert_eq!(spans[0].0, "planned");
        assert_eq!(spans[0].1.fg, Some(Color::Indexed(117)));
    }

    /// The three params of a `38;5;<n>` sequence are one color, not three
    /// separate codes — a naive per-param loop reads the trailing index as an
    /// SGR code and the leading `38` as an unknown one.
    #[test]
    fn extended_color_params_are_consumed_as_a_unit() {
        let s = format!("{}{}a{}", color::BRIGHT_BLUE, color::BOLD, color::RESET);
        let spans = span_texts(&ansi_to_text(&s));
        assert_eq!(spans[0].1.fg, Some(Color::Indexed(117)));
        assert!(spans[0].1.add_modifier.contains(Modifier::BOLD));
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
        let todos = crate::planning::load_todos(planning, false);
        let mut buf = Vec::new();
        crate::report::render(
            &mut buf,
            &crate::report::Report::new(planning, &state)
                .phases(&phases)
                .todos(&todos)
                .use_color(true),
        )
        .unwrap();
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
