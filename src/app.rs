use crate::config::Config;
use crate::models::{InputMode, LogEntry, NavigateFocus, PomodoroTarget, TaskItem};
use crate::storage;
use chrono::{DateTime, Local};
use ratatui::widgets::ListState;
use std::collections::HashMap;
use tui_textarea::TextArea;

const PLACEHOLDER_COMPOSE: &str = "Compose…";
const PLACEHOLDER_NAVIGATE: &str = "Navigate (press ? for help)…";
const PLACEHOLDER_SEARCH: &str = "Search…";

#[derive(Clone)]
pub struct EditingEntry {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub timestamp_prefix: String, // e.g. "[12:34:56] "
}

pub struct App<'a> {
    pub input_mode: InputMode,
    pub navigate_focus: NavigateFocus,
    pub textarea: TextArea<'a>,
    pub textarea_viewport_row: u16,
    pub textarea_viewport_col: u16,
    pub logs: Vec<LogEntry>,
    pub logs_state: ListState,
    pub editing_entry: Option<EditingEntry>,
    pub tasks: Vec<TaskItem>,
    pub tasks_state: ListState,
    pub today_done_tasks: usize,
    pub today_tomatoes: usize,
    pub show_mood_popup: bool,
    pub mood_list_state: ListState,
    pub show_todo_popup: bool, // 할 일 요약 팝업
    pub pending_todos: Vec<String>,
    pub todo_list_state: ListState,
    pub show_tag_popup: bool,
    pub tags: Vec<(String, usize)>, // (태그명, 횟수)
    pub tag_list_state: ListState,
    pub is_search_result: bool,
    pub should_quit: bool,

    // 로컬 파워 기능
    pub pomodoro_end: Option<DateTime<Local>>,
    pub pomodoro_target: Option<PomodoroTarget>,
    pub show_activity_popup: bool,
    pub activity_data: HashMap<String, usize>, // "YYYY-MM-DD" -> line_count
    pub show_path_popup: bool,

    // 뽀모도로 종료 알림 (이 시간까지 알림 표시 & 입력 차단)
    // 뽀모도로 종료 알림 (이 시간까지 알림 표시 & 입력 차단)
    pub pomodoro_alert_expiry: Option<DateTime<Local>>,
    pub pomodoro_alert_message: Option<String>,

    // 설정 (안내 문구 등)
    // 설정 (안내 문구 등)
    pub config: Config,
}

impl<'a> App<'a> {
    pub fn new() -> App<'a> {
        let config = Config::load();

        let mut textarea = TextArea::default();
        textarea.set_placeholder_text(PLACEHOLDER_COMPOSE);

        let logs =
            storage::read_today_entries(&config.data.log_path).unwrap_or_else(|_| Vec::new());
        let mut logs_state = ListState::default();
        if !logs.is_empty() {
            logs_state.select(Some(logs.len() - 1));
        }

        let tasks = storage::read_today_tasks(&config.data.log_path).unwrap_or_else(|_| Vec::new());
        let mut tasks_state = ListState::default();
        if !tasks.is_empty() {
            tasks_state.select(Some(0));
        }

        // 이미 기분 로그가 있는지 확인
        let has_mood = logs.iter().any(|log| log.content.contains("Mood: "));
        let show_mood_popup = !has_mood;

        let mut mood_list_state = ListState::default();
        if show_mood_popup {
            mood_list_state.select(Some(0));
        }

        let mut show_todo_popup = false;
        let mut pending_todos = Vec::new();

        if !show_mood_popup {
            // 기분 팝업이 안 뜨는 경우(이미 기분 입력함)에도 체크할지,
            // 아니면 그냥 뜰 때만 체크할지는 정책 나름이지만, 일단 시작 시 체크
            // 단, 오늘 이미 체크했으면 다시 묻지 않음
            let already_checked =
                storage::is_carryover_done(&config.data.log_path).unwrap_or(false);
            if !already_checked {
                if let Ok(todos) = storage::get_last_file_pending_todos(&config.data.log_path) {
                    if !todos.is_empty() {
                        pending_todos = todos;
                        show_todo_popup = true;
                    }
                }
            }
        }

        let input_mode = InputMode::Editing;

        let (today_done_tasks, today_tomatoes) = compute_today_task_stats(&logs);

        App {
            input_mode,
            navigate_focus: NavigateFocus::Timeline,
            textarea,
            textarea_viewport_row: 0,
            textarea_viewport_col: 0,
            logs,
            logs_state,
            editing_entry: None,
            tasks,
            tasks_state,
            today_done_tasks,
            today_tomatoes,
            show_mood_popup,
            mood_list_state,
            show_todo_popup,
            pending_todos,
            todo_list_state: ListState::default(),
            show_tag_popup: false,
            tags: Vec::new(),
            tag_list_state: ListState::default(),
            is_search_result: false,
            should_quit: false,
            pomodoro_end: None,
            pomodoro_target: None,
            show_activity_popup: false,
            activity_data: HashMap::new(),
            show_path_popup: false,
            pomodoro_alert_expiry: None,
            pomodoro_alert_message: None,
            config,
        }
    }

    pub fn start_edit_entry(&mut self, entry: &LogEntry) {
        let mut lines: Vec<String> = entry.content.lines().map(|s| s.to_string()).collect();
        if lines.is_empty() {
            return;
        }

        let first_line = lines.remove(0);
        let (timestamp_prefix, first_content) = split_timestamp_prefix(&first_line);
        lines.insert(0, first_content);

        self.textarea = TextArea::from(lines);
        self.editing_entry = Some(EditingEntry {
            file_path: entry.file_path.clone(),
            start_line: entry.line_number,
            end_line: entry.end_line,
            timestamp_prefix,
        });
        self.transition_to(InputMode::Editing);
    }

    pub fn update_logs(&mut self) {
        if let Ok(logs) = storage::read_today_entries(&self.config.data.log_path) {
            self.logs = logs;
            self.is_search_result = false;
            if !self.logs.is_empty() {
                self.logs_state.select(Some(self.logs.len() - 1));
            }
        }

        if let Ok(tasks) = storage::read_today_tasks(&self.config.data.log_path) {
            self.tasks = tasks;
            if self.tasks.is_empty() {
                self.tasks_state.select(None);
            } else if self.tasks_state.selected().is_none() {
                self.tasks_state.select(Some(0));
            } else if let Some(i) = self.tasks_state.selected() {
                self.tasks_state.select(Some(i.min(self.tasks.len() - 1)));
            }
        }

        let (done, tomatoes) = compute_today_task_stats(&self.logs);
        self.today_done_tasks = done;
        self.today_tomatoes = tomatoes;
    }

    pub fn scroll_up(&mut self) {
        if self.logs.is_empty() {
            return;
        }

        let i = match self.logs_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.logs_state.select(Some(i));
    }

    pub fn scroll_down(&mut self) {
        if self.logs.is_empty() {
            return;
        }

        let i = match self.logs_state.selected() {
            Some(i) => {
                if i >= self.logs.len() - 1 {
                    self.logs.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.logs_state.select(Some(i));
    }

    pub fn scroll_to_top(&mut self) {
        if self.logs.is_empty() {
            return;
        }
        self.logs_state.select(Some(0));
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.logs.is_empty() {
            return;
        }
        self.logs_state.select(Some(self.logs.len() - 1));
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn tasks_up(&mut self) {
        if self.tasks.is_empty() {
            return;
        }

        let i = match self.tasks_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.tasks_state.select(Some(i));
    }

    pub fn tasks_down(&mut self) {
        if self.tasks.is_empty() {
            return;
        }

        let i = match self.tasks_state.selected() {
            Some(i) => {
                if i >= self.tasks.len() - 1 {
                    self.tasks.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.tasks_state.select(Some(i));
    }

    pub fn transition_to(&mut self, mode: InputMode) {
        // Mode specific entry logic
        match mode {
            InputMode::Navigate => {
                // Search 모드에서 돌아올 때는 입력창 내용을(검색어) 비워야 함
                if self.input_mode == InputMode::Search {
                    self.textarea = TextArea::default();
                }
                self.textarea.set_placeholder_text(PLACEHOLDER_NAVIGATE);
                if self.navigate_focus != NavigateFocus::Tasks {
                    self.navigate_focus = NavigateFocus::Timeline;
                }
            }
            InputMode::Editing => {
                self.textarea.set_placeholder_text(PLACEHOLDER_COMPOSE);
                self.navigate_focus = NavigateFocus::Timeline;
                self.textarea_viewport_row = 0;
                self.textarea_viewport_col = 0;
                // 검색 결과 화면에서 편집으로 넘어갈 때 전체 로그로 복귀
                if self.is_search_result {
                    self.update_logs();
                }
            }
            InputMode::Search => {
                self.textarea = TextArea::default(); // 검색어 입력 위해 초기화
                self.textarea.set_placeholder_text(PLACEHOLDER_SEARCH);
                self.textarea_viewport_row = 0;
                self.textarea_viewport_col = 0;
            }
        }
        self.input_mode = mode;
    }
}

fn split_timestamp_prefix(line: &str) -> (String, String) {
    // "[HH:MM:SS] " is 11 bytes.
    let bytes = line.as_bytes();
    if bytes.len() >= 11 && bytes[0] == b'[' && bytes[9] == b']' && bytes[10] == b' ' {
        (line[..11].to_string(), line[11..].to_string())
    } else {
        ("".to_string(), line.to_string())
    }
}

fn compute_today_task_stats(logs: &[LogEntry]) -> (usize, usize) {
    let mut done = 0usize;
    let mut tomatoes = 0usize;

    for entry in logs {
        for line in entry.content.lines() {
            let mut s = line;
            if is_timestamped_line(s) {
                // Safe due to timestamp format: "[HH:MM:SS] "
                s = &s[11..];
            }
            let s = s.trim_start();

            if let Some(text) = s.strip_prefix("- [ ] ") {
                tomatoes += count_trailing_tomatoes(text);
                continue;
            }

            if let Some(text) = s
                .strip_prefix("- [x] ")
                .or_else(|| s.strip_prefix("- [X] "))
            {
                done += 1;
                tomatoes += count_trailing_tomatoes(text);
            }
        }
    }

    (done, tomatoes)
}

fn count_trailing_tomatoes(s: &str) -> usize {
    let mut count = 0usize;
    let mut text = s.trim_end();
    while let Some(rest) = text.strip_suffix('🍅') {
        count += 1;
        text = rest.trim_end();
    }
    count
}

fn is_timestamped_line(line: &str) -> bool {
    // Format: "[HH:MM:SS] " (11+ chars)
    let bytes = line.as_bytes();
    if bytes.len() < 11 {
        return false;
    }
    if bytes[0] != b'[' || bytes[9] != b']' || bytes[10] != b' ' {
        return false;
    }
    bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b':'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
        && bytes[6] == b':'
        && bytes[7].is_ascii_digit()
        && bytes[8].is_ascii_digit()
}
