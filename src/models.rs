#[derive(PartialEq)]
pub enum InputMode {
    Navigate,
    Editing,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    Normal,
    Insert,
    Visual(VisualKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisualKind {
    Char,
    Line,
    Block,
}

#[derive(Clone, Copy, PartialEq)]
pub enum NavigateFocus {
    Timeline,
    Tasks,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum TaskFilter {
    #[default]
    Open,
    Done,
    All,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mood {
    Happy,
    Neutral,
    Stressed,
    Focused,
    Tired,
}

impl Mood {
    pub fn all() -> Vec<Mood> {
        vec![
            Mood::Happy,
            Mood::Neutral,
            Mood::Stressed,
            Mood::Focused,
            Mood::Tired,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Mood::Happy => "😊 Happy",
            Mood::Neutral => "😐 Neutral",
            Mood::Stressed => "😫 Stressed",
            Mood::Focused => "🧐 Focused",
            Mood::Tired => "😴 Tired",
        }
    }
}

#[derive(Clone)]
pub struct LogEntry {
    pub content: String,
    pub file_path: String,
    pub line_number: usize,
    pub end_line: usize,
}

#[derive(Clone)]
pub struct TaskItem {
    pub text: String,
    pub indent: usize,
    pub tomato_count: usize,
    pub file_path: String,
    pub line_number: usize,
    pub is_done: bool,
    pub task_identity: String,
    pub carryover_from: Option<String>,
}

#[derive(Clone, PartialEq)]
pub struct TaskIdentity {
    pub file_path: String,
    pub line_number: usize,
    pub identity: String,
}

impl TaskIdentity {
    pub fn matches(&self, task: &TaskItem) -> bool {
        self.file_path == task.file_path
            && self.line_number == task.line_number
            && self.identity == task.task_identity
    }
}

impl From<&TaskItem> for TaskIdentity {
    fn from(task: &TaskItem) -> Self {
        Self {
            file_path: task.file_path.clone(),
            line_number: task.line_number,
            identity: task.task_identity.clone(),
        }
    }
}

#[derive(Clone)]
pub enum PomodoroTarget {
    Task {
        text: String,
        file_path: String,
        line_number: usize,
    },
}

/// Returns the timestamp prefix and remaining text if present.
/// Supports optional markdown heading markers like "## " before "[HH:MM:SS]".
pub fn split_timestamp_line(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut start = 0usize;

    if bytes.first() == Some(&b'#') {
        let mut i = 0usize;
        while i < bytes.len() && bytes[i] == b'#' {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b' ' {
            start = i + 1;
        }
    }

    if bytes.len() < start + 10 {
        return None;
    }
    if bytes[start] != b'[' || bytes[start + 9] != b']' {
        return None;
    }
    if !(bytes[start + 1].is_ascii_digit()
        && bytes[start + 2].is_ascii_digit()
        && bytes[start + 3] == b':'
        && bytes[start + 4].is_ascii_digit()
        && bytes[start + 5].is_ascii_digit()
        && bytes[start + 6] == b':'
        && bytes[start + 7].is_ascii_digit()
        && bytes[start + 8].is_ascii_digit())
    {
        return None;
    }

    let mut end = start + 10;
    if bytes.get(end) == Some(&b' ') {
        end += 1;
    }

    let prefix = &line[start..end];
    let rest = &line[end..];
    Some((prefix, rest))
}

/// Checks if a line starts with a timestamp in the format "[HH:MM:SS]".
pub fn is_timestamped_line(line: &str) -> bool {
    split_timestamp_line(line).is_some()
}

/// Strips the timestamp prefix (and optional heading markers) when present.
pub fn strip_timestamp_prefix(line: &str) -> &str {
    split_timestamp_line(line)
        .map(|(_, rest)| rest)
        .unwrap_or(line)
}

/// Returns true if the line starts with a heading timestamp like "## [HH:MM:SS]".
pub fn is_heading_timestamp_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#') && split_timestamp_line(line).is_some()
}

/// Counts trailing tomato emojis (🍅) in a string.
pub fn count_trailing_tomatoes(s: &str) -> usize {
    let mut count = 0;
    let mut text = s.trim_end();
    while let Some(rest) = text.strip_suffix('🍅') {
        count += 1;
        text = rest.trim_end();
    }
    count
}

/// Strips trailing tomato emojis (🍅) and returns the text without them along with the count.
pub fn strip_trailing_tomatoes(s: &str) -> (&str, usize) {
    let mut count = 0;
    let mut text = s.trim_end();
    while let Some(rest) = text.strip_suffix('🍅') {
        count += 1;
        text = rest.trim_end();
    }
    (text, count)
}

#[cfg(test)]
mod tests {
    use super::{is_heading_timestamp_line, split_timestamp_line, strip_timestamp_prefix};

    #[test]
    fn parses_heading_timestamp_line() {
        let line = "## [09:05:10]";
        let (prefix, rest) = split_timestamp_line(line).expect("timestamp");
        assert_eq!(prefix, "[09:05:10]");
        assert_eq!(rest, "");
        assert!(is_heading_timestamp_line(line));
    }

    #[test]
    fn strips_heading_timestamp_prefix() {
        let line = "## [10:12:44]";
        assert_eq!(strip_timestamp_prefix(line), "");
    }
}
