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
