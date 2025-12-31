use crate::config::Theme;
use crate::models::Priority;
use crate::ui::color_parser::parse_color;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

/// Helper function to calculate centered popup position
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn centered_column(area: Rect, desired_width: u16) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }

    let width = desired_width.max(1).min(area.width);
    let x = area.x + area.width.saturating_sub(width) / 2;

    Rect {
        x,
        y: area.y,
        width,
        height: area.height,
    }
}

pub fn parse_markdown_spans(
    text: &str,
    theme: &Theme,
    in_code_block: bool,
    search_regex: Option<&regex::Regex>,
    search_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    let leading_len = text.len().saturating_sub(text.trim_start().len());
    if leading_len > 0 {
        spans.push(Span::raw(text[..leading_len].to_string()));
    }

    let content = text.trim_start();
    if content.is_empty() {
        return spans;
    }

    // Fenced code blocks: render as-is with a distinct style.
    if in_code_block || content.starts_with("```") {
        spans.push(Span::styled(
            content.to_string(),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ));
        return spans;
    }

    // Headings (# ...): bold + slightly brighter.
    if let Some(stripped) = heading_text(content) {
        spans.push(Span::styled(
            stripped.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        return spans;
    }

    // Carryover marker
    if content.starts_with("⤴ Carryover from ") {
        spans.push(Span::styled(
            content.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        return spans;
    }

    // TODO checkboxes at line start.
    // Keep display width comparable to the original "- [ ] " / "- [x] " prefix for cleaner wrapping.
    let (content, todo_prefix) = if let Some(stripped) = content.strip_prefix("- [ ] ") {
        let color = parse_color(&theme.todo_wip);
        spans.push(Span::styled("• [ ] ", Style::default().fg(color)));
        (stripped, true)
    } else if let Some(stripped) = content.strip_prefix("- [x] ") {
        let color = parse_color(&theme.todo_done);
        spans.push(Span::styled("• [✓] ", Style::default().fg(color)));
        (stripped, true)
    } else if let Some(stripped) = content.strip_prefix("- [X] ") {
        let color = parse_color(&theme.todo_done);
        spans.push(Span::styled("• [✓] ", Style::default().fg(color)));
        (stripped, true)
    } else {
        (content, false)
    };

    let (content, todo_prefix) = if todo_prefix {
        (content, todo_prefix)
    } else if let Some(stripped) = content.strip_prefix("- ") {
        let bullet = bullet_for_level(leading_len);
        spans.push(Span::styled(
            format!("{bullet} "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
        (stripped, false)
    } else if let Some(stripped) = content.strip_prefix("* ") {
        let bullet = bullet_for_level(leading_len);
        spans.push(Span::styled(
            format!("{bullet} "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
        (stripped, false)
    } else if let Some(stripped) = content.strip_prefix("+ ") {
        let bullet = bullet_for_level(leading_len);
        spans.push(Span::styled(
            format!("{bullet} "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
        (stripped, false)
    } else if let Some((marker, stripped)) = split_ordered_list_marker(content) {
        spans.push(Span::styled(
            marker,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" ".to_string()));
        (stripped, false)
    } else {
        (content, false)
    };

    let (content, priority_marker) = if let Some((priority, rest)) = split_priority_marker(content) {
        (rest, Some(priority))
    } else {
        (content, None)
    };

    if let Some(priority) = priority_marker {
        spans.push(Span::styled(
            format!("[#{}] ", priority.as_char()),
            priority_style(priority),
        ));
    }

    // Inline code: split on backticks and style code segments.
    let mut is_code = false;
    for segment in content.split('`') {
        if is_code {
            spans.push(Span::styled(
                segment.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.extend(parse_words(
                segment,
                theme,
                todo_prefix,
                search_regex,
                search_style,
            ));
        }
        is_code = !is_code;
    }

    spans
}

fn bullet_for_level(leading_spaces: usize) -> char {
    // Markdown list nesting is usually 2 or 4 spaces; treat 2 spaces as one level.
    let level = leading_spaces / 2;
    match level {
        0 => '•',
        1 => '◦',
        2 => '▪',
        _ => '▫',
    }
}

fn split_priority_marker(text: &str) -> Option<(Priority, &str)> {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("[#") else {
        return None;
    };
    let mut chars = rest.chars();
    let Some(letter) = chars.next() else {
        return None;
    };
    if !matches!(chars.next(), Some(']')) {
        return None;
    }
    let priority = Priority::from_char(letter)?;
    Some((priority, chars.as_str().trim_start()))
}

fn priority_style(priority: Priority) -> Style {
    match priority {
        Priority::High => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        Priority::Medium => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        Priority::Low => Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
    }
}

fn split_ordered_list_marker(line: &str) -> Option<(String, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i + 1 >= bytes.len() {
        return None;
    }

    let punct = bytes[i];
    if (punct == b'.' || punct == b')') && bytes[i + 1] == b' ' {
        // Safe because digits/punct are ASCII.
        Some((line[..i + 1].to_string(), &line[i + 2..]))
    } else {
        None
    }
}

fn heading_text(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 {
        return None;
    }
    let after = &trimmed[level..];
    if after.starts_with(' ') {
        Some(trimmed)
    } else {
        None
    }
}

fn parse_words(
    text: &str,
    theme: &Theme,
    todo_prefix: bool,
    search_regex: Option<&regex::Regex>,
    search_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // URL parsing
    static URL_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let url_regex = URL_REGEX.get_or_init(|| {
        regex::Regex::new(r"https?://[-a-zA-Z0-9+&@#/%?=~_|!:,.;]*[-a-zA-Z0-9+&@#/%=~_|]").unwrap()
    });

    for (i, word) in text.split_whitespace().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".to_string()));
        }

        if word.starts_with('⟦') && word.ends_with('⟧') {
            spans.push(Span::styled(
                word.to_string(),
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if word.starts_with('#') {
            let tag_color = parse_color(&theme.tag);
            spans.push(Span::styled(
                word.to_string(),
                Style::default().fg(tag_color).add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if word.starts_with("Mood:") {
            let mood_color = parse_color(&theme.mood);
            spans.push(Span::styled(
                "🎭 Mood:",
                Style::default()
                    .fg(mood_color)
                    .add_modifier(Modifier::ITALIC),
            ));
            continue;
        }

        if let Some(mat) = url_regex.find(word) {
            let start = mat.start();
            let end = mat.end();

            if start > 0 {
                spans.push(Span::styled(
                    word[..start].to_string(),
                    if todo_prefix {
                        Style::default().fg(Color::Reset)
                    } else {
                        Style::default()
                    },
                ));
            }

            spans.push(Span::styled(
                word[start..end].to_string(),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED),
            ));

            if end < word.len() {
                spans.push(Span::styled(
                    word[end..].to_string(),
                    if todo_prefix {
                        Style::default().fg(Color::Reset)
                    } else {
                        Style::default()
                    },
                ));
            }
            continue;
        }

        let base_style = if todo_prefix {
            Style::default().fg(Color::Reset)
        } else {
            Style::default()
        };

        if let Some(regex) = search_regex {
            spans.extend(highlight_matches(word, base_style, search_style, regex));
        } else {
            spans.push(Span::styled(word.to_string(), base_style));
        }
    }

    spans
}

fn highlight_matches(
    text: &str,
    base_style: Style,
    search_style: Style,
    regex: &regex::Regex,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut last = 0;
    for mat in regex.find_iter(text) {
        if mat.start() > last {
            spans.push(Span::styled(
                text[last..mat.start()].to_string(),
                base_style,
            ));
        }
        spans.push(Span::styled(
            text[mat.start()..mat.end()].to_string(),
            base_style.patch(search_style),
        ));
        last = mat.end();
    }
    if last < text.len() {
        spans.push(Span::styled(text[last..].to_string(), base_style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base_style));
    }
    spans
}

pub fn wrap_markdown_line(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let (prefix, rest, prefix_width) = split_markdown_prefix(text);

    // When the prefix already eats the whole line, wrapping can't help.
    if prefix_width >= width {
        return vec![format!("{prefix}{rest}")];
    }

    let available = width.saturating_sub(prefix_width).max(1);
    let wrapped = textwrap::wrap(rest, available);

    if wrapped.is_empty() {
        return vec![prefix];
    }

    let mut out = Vec::with_capacity(wrapped.len());
    for (i, part) in wrapped.iter().enumerate() {
        if i == 0 {
            out.push(format!("{prefix}{part}"));
        } else {
            out.push(format!("{}{}", " ".repeat(prefix_width), part));
        }
    }
    out
}

pub(crate) fn markdown_prefix_width(text: &str) -> usize {
    let (_prefix, _rest, width) = split_markdown_prefix(text);
    width
}

fn split_markdown_prefix(text: &str) -> (String, &str, usize) {
    let (leading, rest) = normalize_leading_whitespace(text);

    if let Some((marker, tail)) = split_ordered_list_marker(rest) {
        let mut prefix = leading;
        prefix.push_str(&marker);
        prefix.push(' ');
        let width = UnicodeWidthStr::width(prefix.as_str());
        return (prefix, tail, width);
    }

    let (marker, tail) = split_list_marker(rest);

    let mut prefix = leading;
    prefix.push_str(marker);
    let width = UnicodeWidthStr::width(prefix.as_str());
    (prefix, tail, width)
}

fn normalize_leading_whitespace(text: &str) -> (String, &str) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut spaces = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b' ' => {
                i += 1;
                spaces += 1;
            }
            b'\t' => {
                i += 1;
                spaces += 4;
            }
            _ => break,
        }
    }

    // Safe because i stops on ASCII whitespace boundaries.
    let rest = &text[i..];
    let is_list = rest.starts_with("- [ ] ")
        || rest.starts_with("- [x] ")
        || rest.starts_with("- [X] ")
        || rest.starts_with("- ")
        || rest.starts_with("* ")
        || rest.starts_with("+ ")
        || split_ordered_list_marker(rest).is_some();

    let out = if is_list {
        // Quantize indentation into list nesting levels (2 spaces per level).
        // This avoids "indentation by arbitrary spaces" and makes depth feel consistent.
        let level = spaces.div_ceil(2);
        "  ".repeat(level)
    } else {
        " ".repeat(spaces)
    };

    (out, rest)
}

fn split_list_marker(text: &str) -> (&'static str, &str) {
    if let Some(rest) = text.strip_prefix("- [ ] ") {
        return ("- [ ] ", rest);
    }
    if let Some(rest) = text.strip_prefix("- [x] ") {
        return ("- [x] ", rest);
    }
    if let Some(rest) = text.strip_prefix("- [X] ") {
        return ("- [X] ", rest);
    }

    if let Some(rest) = text.strip_prefix("- ") {
        return ("- ", rest);
    }
    if let Some(rest) = text.strip_prefix("* ") {
        return ("* ", rest);
    }
    if let Some(rest) = text.strip_prefix("+ ") {
        return ("+ ", rest);
    }

    ("", text)
}
