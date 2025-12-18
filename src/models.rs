#[derive(PartialEq)]
pub enum InputMode {
    Navigate,
    Editing,
    Search,
}

#[derive(Clone, Copy, PartialEq)]
pub enum NavigateFocus {
    Timeline,
    Tasks,
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
}

#[derive(Clone)]
pub enum PomodoroTarget {
    Task {
        text: String,
        file_path: String,
        line_number: usize,
    },
}
