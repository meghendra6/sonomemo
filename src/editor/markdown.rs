use crate::models::Priority;
use tui_textarea::{CursorMove, TextArea};

pub(crate) fn insert_newline_with_auto_indent(textarea: &mut TextArea) {
    let (row, _) = textarea.cursor();
    let current_line = textarea.lines().get(row).map(|s| s.as_str()).unwrap_or("");

    let prefix = list_continuation_prefix(current_line);
    textarea.insert_newline();
    if !prefix.is_empty() {
        textarea.insert_str(prefix);
    }
}

pub(crate) fn indent_or_outdent_list_line(textarea: &mut TextArea, indent: bool) -> bool {
    let (row, col) = textarea.cursor();
    let current_line = textarea.lines().get(row).map(|s| s.as_str()).unwrap_or("");

    if !is_list_line(current_line) {
        return false;
    }

    if indent {
        textarea.move_cursor(CursorMove::Jump(row as u16, 0));
        textarea.insert_str("  ");
        textarea.move_cursor(CursorMove::Jump(row as u16, (col + 2) as u16));
        true
    } else {
        let remove = leading_outdent_chars(current_line);
        if remove == 0 {
            return true;
        }

        textarea.move_cursor(CursorMove::Jump(row as u16, 0));
        for _ in 0..remove {
            let _ = textarea.delete_next_char();
        }
        textarea.move_cursor(CursorMove::Jump(
            row as u16,
            col.saturating_sub(remove) as u16,
        ));
        true
    }
}

pub(crate) fn toggle_task_checkbox(textarea: &mut TextArea) -> bool {
    let (row, col) = textarea.cursor();
    let current_line = textarea
        .lines()
        .get(row)
        .cloned()
        .unwrap_or_default();
    let (indent, rest) = split_indent(&current_line);

    let new_line = if let Some((_marker, content)) = checkbox_marker(rest) {
        format!("{indent}- {content}")
    } else if let Some((_marker, content)) = bullet_marker(rest) {
        format!("{indent}- [ ] {content}")
    } else {
        format!("{indent}- [ ] {rest}")
    };

    if new_line == current_line {
        return false;
    }

    replace_current_line(textarea, row, &new_line);
    let indent_len = indent.chars().count();
    let new_col = adjust_cursor_for_line_edit(col, &current_line, &new_line, indent_len);
    textarea.move_cursor(CursorMove::Jump(row as u16, new_col as u16));
    true
}

pub(crate) fn cycle_task_priority(textarea: &mut TextArea) -> bool {
    let (row, col) = textarea.cursor();
    let current_line = textarea
        .lines()
        .get(row)
        .cloned()
        .unwrap_or_default();
    let (indent, rest) = split_indent(&current_line);

    let (old_prefix_len, prefix, content) = if let Some((marker, content)) = checkbox_marker(rest)
    {
        (marker.chars().count(), marker, content)
    } else if let Some((marker, content)) = bullet_marker(rest) {
        (marker.chars().count(), "- [ ] ", content)
    } else {
        (0, "- [ ] ", rest)
    };

    let (current_priority, remaining) = split_priority_marker(content);
    let next_priority = next_priority(current_priority);

    let remaining = remaining.trim_start();
    let mut new_content = String::new();
    if let Some(priority) = next_priority {
        new_content.push_str("[#");
        new_content.push(priority.as_char());
        new_content.push(']');
        if !remaining.is_empty() {
            new_content.push(' ');
        }
    }
    new_content.push_str(remaining);

    let new_line = format!("{indent}{prefix}{new_content}");
    if new_line == current_line {
        return false;
    }

    replace_current_line(textarea, row, &new_line);
    let indent_len = indent.chars().count();
    let change_start = indent_len.saturating_add(old_prefix_len);
    let new_col = adjust_cursor_for_line_edit(col, &current_line, &new_line, change_start);
    textarea.move_cursor(CursorMove::Jump(row as u16, new_col as u16));
    true
}

pub(crate) fn list_continuation_prefix(line: &str) -> String {
    let (indent_level, rest) = parse_indent_level(line);
    let indent = "  ".repeat(indent_level);

    if let Some((_marker, content)) = checkbox_marker(rest) {
        if content.trim().is_empty() {
            return indent;
        }
        // Always continue checklists as unchecked by default.
        return format!("{indent}- [ ] ");
    }

    if let Some((marker, content)) = bullet_marker(rest) {
        if content.trim().is_empty() {
            return indent;
        }
        return format!("{indent}{marker}");
    }

    if let Some((next_marker, content)) = ordered_list_next_marker(rest) {
        if content.trim().is_empty() {
            return indent;
        }
        return format!("{indent}{next_marker}");
    }

    String::new()
}

fn replace_current_line(textarea: &mut TextArea, row: usize, new_line: &str) {
    textarea.move_cursor(CursorMove::Jump(row as u16, 0));
    let line_len = textarea
        .lines()
        .get(row)
        .map(|line| line.chars().count())
        .unwrap_or(0);
    if line_len > 0 {
        let _ = textarea.delete_str(line_len);
    }
    textarea.insert_str(new_line);
}

fn adjust_cursor_for_line_edit(
    col: usize,
    old_line: &str,
    new_line: &str,
    change_start: usize,
) -> usize {
    if col < change_start {
        return col;
    }
    let old_len = old_line.chars().count() as isize;
    let new_len = new_line.chars().count() as isize;
    let delta = new_len - old_len;
    let mut next = col as isize + delta;
    let min = change_start as isize;
    if next < min {
        next = min;
    }
    let max = new_len.max(0);
    next.min(max) as usize
}

fn split_indent(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    (&line[..i], &line[i..])
}

fn is_list_line(line: &str) -> bool {
    let (_, rest) = parse_indent_level(line);
    checkbox_marker(rest).is_some()
        || bullet_marker(rest).is_some()
        || ordered_list_next_marker(rest).is_some()
}

fn leading_outdent_chars(line: &str) -> usize {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return 0;
    }
    if bytes[0] == b'\t' {
        return 1;
    }
    if bytes.len() >= 2 && bytes[0] == b' ' && bytes[1] == b' ' {
        return 2;
    }
    if bytes[0] == b' ' {
        return 1;
    }
    0
}

fn parse_indent_level(line: &str) -> (usize, &str) {
    let bytes = line.as_bytes();
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
    let rest = &line[i..];
    (spaces / 2, rest)
}

fn checkbox_marker(rest: &str) -> Option<(&'static str, &str)> {
    if let Some(content) = rest.strip_prefix("- [ ] ") {
        return Some(("- [ ] ", content));
    }
    if let Some(content) = rest.strip_prefix("- [x] ") {
        return Some(("- [x] ", content));
    }
    if let Some(content) = rest.strip_prefix("- [X] ") {
        return Some(("- [X] ", content));
    }
    None
}

fn bullet_marker(rest: &str) -> Option<(&'static str, &str)> {
    if let Some(content) = rest.strip_prefix("- ") {
        return Some(("- ", content));
    }
    if let Some(content) = rest.strip_prefix("* ") {
        return Some(("* ", content));
    }
    if let Some(content) = rest.strip_prefix("+ ") {
        return Some(("+ ", content));
    }
    None
}

fn split_priority_marker(text: &str) -> (Option<Priority>, String) {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("[#") else {
        return (None, text.to_string());
    };
    let mut chars = rest.chars();
    let Some(letter) = chars.next() else {
        return (None, text.to_string());
    };
    if !matches!(chars.next(), Some(']')) {
        return (None, text.to_string());
    }
    let Some(priority) = Priority::from_char(letter) else {
        return (None, text.to_string());
    };
    (Some(priority), chars.as_str().to_string())
}

fn next_priority(current: Option<Priority>) -> Option<Priority> {
    match current {
        None => Some(Priority::High),
        Some(Priority::High) => Some(Priority::Medium),
        Some(Priority::Medium) => Some(Priority::Low),
        Some(Priority::Low) => None,
    }
}

fn ordered_list_next_marker(rest: &str) -> Option<(String, &str)> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i + 1 >= bytes.len() {
        return None;
    }

    let punct = bytes[i];
    if (punct != b'.' && punct != b')') || bytes[i + 1] != b' ' {
        return None;
    }

    let n: usize = rest[..i].parse().ok()?;
    let next = n.saturating_add(1);
    let punct_char = punct as char;
    let next_marker = format!("{}{punct_char} ", next);
    let content = &rest[i + 2..];
    Some((next_marker, content))
}
