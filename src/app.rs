use crate::config::{Config, Theme};
use crate::integrations::gemini::{AiSearchOutcome, AiSearchResult};
use crate::integrations::google::{AuthDisplay, AuthPollResult};
use crate::models::{
    DatePickerField, EditorMode, EntryIdentity, FoldOverride, FoldState, InputMode, LogEntry,
    NavigateFocus, PomodoroTarget, Priority, TaskFilter, TaskItem, TaskSchedule, TimelineFilter,
    count_trailing_tomatoes, is_heading_timestamp_line, is_timestamped_line, split_timestamp_line,
    strip_timestamp_prefix,
};
use crate::storage;
use arboard::Clipboard;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, Timelike};
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::Receiver;
use tui_textarea::CursorMove;
use tui_textarea::TextArea;

pub const PLACEHOLDER_COMPOSE: &str = "Write your note here… (Shift+Enter to save, Esc to go back)";
const PLACEHOLDER_NAVIGATE: &str = "Navigate (press ? for help)…";
const PLACEHOLDER_SEARCH: &str = "Search… (prefix ? for AI)";

/// Default number of days to load initially (including today)
const INITIAL_LOAD_DAYS: i64 = 7;
/// Number of log files to load per infinite-scroll chunk.
const HISTORY_LOAD_FILE_COUNT: usize = 2;

#[derive(Clone)]
pub struct EditingEntry {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub timestamp_prefix: String, // e.g. "## [12:34:56]"
    pub from_search: bool,
    pub search_query: Option<String>,
    pub is_raw: bool,
}

#[derive(Clone)]
pub struct EditorSnapshot {
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PendingEditCommand {
    Delete,
    Yank,
    GoToTop,
    Replace,
    Change,
    ZCommand,
    DeleteInner,
    YankInner,
    ChangeInner,
}

pub struct App<'a> {
    pub input_mode: InputMode,
    pub navigate_focus: NavigateFocus,
    pub last_navigate_focus: Option<NavigateFocus>,
    pub textarea: TextArea<'a>,
    pub textarea_viewport_row: u16,
    pub textarea_viewport_col: u16,
    pub textarea_viewport_height: usize,
    pub composer_dirty: bool,
    pub editor_mode: EditorMode,
    pub visual_anchor: Option<(usize, usize)>,
    pub pending_command: Option<PendingEditCommand>,
    pub pending_count: usize,
    pub yank_buffer: String,
    pub yank_is_linewise: bool,
    pub editor_undo: Vec<EditorSnapshot>,
    pub editor_redo: Vec<EditorSnapshot>,
    pub insert_snapshot: Option<EditorSnapshot>,
    pub insert_modified: bool,
    pub visual_hint_message: Option<String>,
    pub visual_hint_expiry: Option<DateTime<Local>>,
    pub visual_hint_active: bool,
    pub active_date: String,
    pub all_logs: Vec<LogEntry>,
    pub logs: Vec<LogEntry>,
    pub logs_state: ListState,
    /// UI-space list state for Timeline (includes date separators, preserves offset across frames).
    pub timeline_ui_state: ListState,
    pub editing_entry: Option<EditingEntry>,
    pub fold_state: FoldState,
    pub fold_overrides: HashMap<EntryIdentity, FoldOverride>,
    pub all_tasks: Vec<TaskItem>,
    pub tasks: Vec<TaskItem>,
    pub tasks_state: ListState,
    pub task_filter: TaskFilter,
    pub timeline_filter: TimelineFilter,
    pub today_done_tasks: usize,
    pub today_tomatoes: usize,
    pub last_search_query: Option<String>,
    pub show_mood_popup: bool,
    pub mood_list_state: ListState,
    pub show_todo_popup: bool,
    pub pending_todos: Vec<String>,
    pub todo_list_state: ListState,
    pub show_help_popup: bool,
    pub show_tag_popup: bool,
    pub tags: Vec<(String, usize)>,
    pub tag_list_state: ListState,
    pub agenda_all_items: Vec<crate::models::AgendaItem>,
    pub agenda_items: Vec<crate::models::AgendaItem>,
    pub agenda_state: ListState,
    pub agenda_selected_day: NaiveDate,
    pub agenda_filter: TaskFilter,
    pub agenda_show_unscheduled: bool,
    pub show_date_picker_popup: bool,
    pub date_picker_field: DatePickerField,
    pub date_picker_schedule: TaskSchedule,
    pub date_picker_default_date: NaiveDate,
    pub date_picker_default_time: NaiveTime,
    pub date_picker_default_duration: u32,
    pub date_picker_input: String,
    pub date_picker_input_mode: bool,
    pub is_search_result: bool,
    pub should_quit: bool,
    pub show_exit_popup: bool,
    pub show_delete_entry_popup: bool,
    pub delete_entry_target: Option<LogEntry>,

    // Pomodoro timer state
    pub pomodoro_start: Option<DateTime<Local>>,
    pub pomodoro_end: Option<DateTime<Local>>,
    pub pomodoro_target: Option<PomodoroTarget>,
    pub show_activity_popup: bool,
    pub activity_data: HashMap<String, (usize, usize)>, // "YYYY-MM-DD" -> (line_count, tomato_count)
    pub show_path_popup: bool,
    pub show_theme_popup: bool,
    pub theme_list_state: ListState,
    pub theme_preview_backup: Option<Theme>,

    pub show_editor_style_popup: bool,
    pub editor_style_list_state: ListState,

    pub show_pomodoro_popup: bool,
    pub pomodoro_minutes_input: String,
    pub pomodoro_pending_task: Option<TaskItem>,

    pub show_memo_preview_popup: bool,
    pub memo_preview_entry: Option<LogEntry>,
    pub memo_preview_scroll: usize,
    pub show_google_auth_popup: bool,
    pub google_auth_display: Option<AuthDisplay>,
    pub google_auth_receiver: Option<Receiver<AuthPollResult>>,
    pub google_sync_receiver: Option<Receiver<crate::integrations::google::SyncOutcome>>,
    pub show_ai_response_popup: bool,
    pub ai_response: Option<AiSearchResult>,
    pub ai_response_scroll: usize,
    pub ai_search_receiver: Option<Receiver<AiSearchOutcome>>,
    pub show_ai_loading_popup: bool,
    pub ai_loading_question: Option<String>,

    // Pomodoro completion alert (blocks input until expiry)
    pub pomodoro_alert_expiry: Option<DateTime<Local>>,
    pub pomodoro_alert_message: Option<String>,

    pub toast_message: Option<String>,
    pub toast_expiry: Option<DateTime<Local>>,
    pub search_highlight_query: Option<String>,
    pub search_highlight_ready_at: Option<DateTime<Local>>,

    // History loading state for infinite scroll
    pub loaded_start_date: Option<NaiveDate>,
    pub earliest_available_date: Option<NaiveDate>,
    pub is_loading_more: bool,

    // Entry-level scroll offset for tall entries (row-based scroll within a single entry)
    pub entry_scroll_offset: usize,
    // Flag to indicate we should scroll to the bottom of the selected entry on next render
    pub entry_scroll_to_bottom: bool,
    // Cached values for the selected entry's line count and viewport height (set during render)
    pub selected_entry_line_count: usize,
    pub timeline_viewport_height: usize,

    // Configuration
    pub config: Config,
}

impl<'a> App<'a> {
    pub fn new() -> App<'a> {
        let config = Config::load();

        let now = Local::now();
        let today = now.date_naive();
        let active_date = today.format("%Y-%m-%d").to_string();
        let rounded_time = round_time_to_quarter(now.time());

        let mut textarea = TextArea::default();
        textarea.set_placeholder_text(PLACEHOLDER_COMPOSE);

        // Load logs from the past week (or earliest available)
        let start_date = today - Duration::days(INITIAL_LOAD_DAYS - 1);
        let earliest_available_date =
            storage::get_earliest_log_date(&config.data.log_path).unwrap_or(None);
        let effective_start = earliest_available_date
            .map(|e| e.max(start_date))
            .unwrap_or(start_date);

        let mut all_logs =
            storage::read_entries_for_date_range(&config.data.log_path, effective_start, today)
                .unwrap_or_else(|_| Vec::new());
        let fold_overrides = extract_fold_markers_from_logs(&mut all_logs);
        let timeline_filter = TimelineFilter::All;
        let logs = Vec::new();
        let logs_state = ListState::default();

        let all_tasks =
            storage::read_today_tasks(&config.data.log_path).unwrap_or_else(|_| Vec::new());
        let tasks_state = ListState::default();
        let task_filter = TaskFilter::Open;

        // Check if mood has already been logged today
        let today_logs =
            storage::read_today_entries(&config.data.log_path).unwrap_or_else(|_| Vec::new());
        let has_mood = today_logs.iter().any(|log| log.content.contains("Mood: "));
        let show_mood_popup = !has_mood;

        let mut mood_list_state = ListState::default();
        if show_mood_popup {
            mood_list_state.select(Some(0));
        }

        let mut show_todo_popup = false;
        let mut pending_todos = Vec::new();

        if !show_mood_popup {
            // Check for unfinished tasks from previous day to carry over
            let already_checked =
                storage::is_carryover_done(&config.data.log_path).unwrap_or(false);
            if !already_checked
                && let Ok(todos) =
                    storage::collect_carryover_tasks(&config.data.log_path, &active_date)
                && !todos.is_empty()
            {
                pending_todos = todos;
                show_todo_popup = true;
            }
        }

        let input_mode = InputMode::Navigate;

        // Calculate today's stats from today's logs only
        let (today_done_tasks, today_tomatoes) = compute_today_task_stats(&today_logs);

        let mut app = App {
            input_mode,
            navigate_focus: NavigateFocus::Timeline,
            last_navigate_focus: None,
            textarea,
            textarea_viewport_row: 0,
            textarea_viewport_col: 0,
            textarea_viewport_height: 0,
            composer_dirty: false,
            editor_mode: EditorMode::Normal,
            visual_anchor: None,
            pending_command: None,
            pending_count: 0,
            yank_buffer: String::new(),
            yank_is_linewise: false,
            editor_undo: Vec::new(),
            editor_redo: Vec::new(),
            insert_snapshot: None,
            insert_modified: false,
            visual_hint_message: None,
            visual_hint_expiry: None,
            visual_hint_active: false,
            active_date,
            all_logs,
            logs,
            logs_state,
            timeline_ui_state: ListState::default(),
            editing_entry: None,
            fold_state: FoldState::default(),
            fold_overrides,
            all_tasks,
            tasks: Vec::new(),
            tasks_state,
            task_filter,
            timeline_filter,
            today_done_tasks,
            today_tomatoes,
            last_search_query: None,
            show_mood_popup,
            mood_list_state,
            show_todo_popup,
            pending_todos,
            todo_list_state: ListState::default(),
            show_help_popup: false,
            show_tag_popup: false,
            tags: Vec::new(),
            tag_list_state: ListState::default(),
            agenda_all_items: Vec::new(),
            agenda_items: Vec::new(),
            agenda_state: ListState::default(),
            agenda_selected_day: today,
            agenda_filter: TaskFilter::Open,
            agenda_show_unscheduled: false,
            show_date_picker_popup: false,
            date_picker_field: DatePickerField::Scheduled,
            date_picker_schedule: TaskSchedule::default(),
            date_picker_default_date: today,
            date_picker_default_time: rounded_time,
            date_picker_default_duration: 30,
            date_picker_input: String::new(),
            date_picker_input_mode: false,
            is_search_result: false,
            should_quit: false,
            show_exit_popup: false,
            show_delete_entry_popup: false,
            delete_entry_target: None,
            pomodoro_start: None,
            pomodoro_end: None,
            pomodoro_target: None,
            show_activity_popup: false,
            activity_data: HashMap::new(),
            show_path_popup: false,
            show_theme_popup: false,
            theme_list_state: ListState::default(),
            theme_preview_backup: None,
            show_editor_style_popup: false,
            editor_style_list_state: ListState::default(),
            show_pomodoro_popup: false,
            pomodoro_minutes_input: String::new(),
            pomodoro_pending_task: None,
            show_memo_preview_popup: false,
            memo_preview_entry: None,
            memo_preview_scroll: 0,
            show_google_auth_popup: false,
            google_auth_display: None,
            google_auth_receiver: None,
            google_sync_receiver: None,
            show_ai_response_popup: false,
            ai_response: None,
            ai_response_scroll: 0,
            ai_search_receiver: None,
            show_ai_loading_popup: false,
            ai_loading_question: None,
            pomodoro_alert_expiry: None,
            pomodoro_alert_message: None,
            toast_message: None,
            toast_expiry: None,
            search_highlight_query: None,
            search_highlight_ready_at: None,
            loaded_start_date: Some(effective_start),
            earliest_available_date,
            is_loading_more: false,
            entry_scroll_offset: 0,
            entry_scroll_to_bottom: false,
            selected_entry_line_count: 0,
            timeline_viewport_height: 0,
            config,
        };

        app.apply_timeline_filter(true);
        app.apply_task_filter(true);
        app.refresh_agenda();
        app
    }

    pub fn start_edit_entry(&mut self, entry: &LogEntry) {
        let mut lines: Vec<String> =
            storage::read_lines_range(&entry.file_path, entry.line_number, entry.end_line)
                .unwrap_or_else(|_| entry.content.lines().map(|s| s.to_string()).collect());
        lines = strip_fold_markers_from_lines(&lines);
        if lines.is_empty() {
            return;
        }

        let first_line = lines.remove(0);
        let (timestamp_prefix, first_content) = split_timestamp_prefix(&first_line);
        if !first_content.is_empty() {
            lines.insert(0, first_content);
        } else if lines.is_empty() {
            lines.push(String::new());
        }

        self.textarea = TextArea::from(lines);
        self.editing_entry = Some(EditingEntry {
            file_path: entry.file_path.clone(),
            start_line: entry.line_number,
            end_line: entry.end_line,
            timestamp_prefix,
            from_search: self.is_search_result,
            search_query: self.last_search_query.clone(),
            is_raw: false,
        });
        self.composer_dirty = false;
        self.transition_to(InputMode::Editing);
    }

    pub fn start_edit_raw_file(&mut self, file_path: String, mut lines: Vec<String>) {
        if lines.is_empty() {
            lines.push(String::new());
        }
        let end_line = lines.len().saturating_sub(1);
        self.textarea = TextArea::from(lines);
        self.editing_entry = Some(EditingEntry {
            file_path,
            start_line: 0,
            end_line,
            timestamp_prefix: String::new(),
            from_search: false,
            search_query: None,
            is_raw: true,
        });
        self.composer_dirty = false;
        self.transition_to(InputMode::Editing);
    }

    /// Reloads logs for the currently loaded date range, updates tasks, and recalculates stats.
    pub fn update_logs(&mut self) {
        let today = Local::now().date_naive();
        let preserve_selection = self.logs_state.selected();

        // Reset entry scroll offset when logs are updated
        self.entry_scroll_offset = 0;
        self.entry_scroll_to_bottom = false;

        // Reload logs for the current date range
        if let Some(start) = self.loaded_start_date {
            if let Ok(logs) =
                storage::read_entries_for_date_range(&self.config.data.log_path, start, today)
            {
                self.all_logs = logs;
                self.is_search_result = false;
                self.search_highlight_query = None;
                self.search_highlight_ready_at = None;
                self.apply_fold_markers();
                self.apply_timeline_filter(preserve_selection.is_none());
            }
        } else {
            // Fallback to today's entries only
            if let Ok(logs) = storage::read_today_entries(&self.config.data.log_path) {
                self.all_logs = logs;
                self.is_search_result = false;
                self.search_highlight_query = None;
                self.search_highlight_ready_at = None;
                self.apply_fold_markers();
                self.apply_timeline_filter(preserve_selection.is_none());
            }
        }

        if let Ok(tasks) = storage::read_today_tasks(&self.config.data.log_path) {
            self.all_tasks = tasks;
            self.apply_task_filter(false);
        }

        self.refresh_agenda();

        if !self.fold_overrides.is_empty() {
            let mut keep = std::collections::HashSet::new();
            for entry in &self.all_logs {
                keep.insert(EntryIdentity::from(entry));
            }
            self.fold_overrides.retain(|key, _| keep.contains(key));
        }

        // Calculate stats from today's logs only
        let today_logs =
            storage::read_today_entries(&self.config.data.log_path).unwrap_or_default();
        let (done, tomatoes) = compute_today_task_stats(&today_logs);
        self.today_done_tasks = done;
        self.today_tomatoes = tomatoes;
    }

    /// Loads more historical entries when scrolling to the top.
    pub fn load_more_history(&mut self) {
        if self.is_loading_more || self.is_search_result {
            return;
        }

        let current_start = match self.loaded_start_date {
            Some(d) => d,
            None => return,
        };

        let available_dates =
            storage::get_available_log_dates(&self.config.data.log_path).unwrap_or_default();
        let earliest = match available_dates.first().copied() {
            Some(d) => d,
            None => return,
        };
        self.earliest_available_date = Some(earliest);

        if current_start <= earliest {
            self.toast("No more history to load.");
            return;
        }

        self.is_loading_more = true;
        self.toast("⏳ Loading more history...");

        let cutoff_index = available_dates.partition_point(|d| *d < current_start);
        if cutoff_index == 0 {
            self.toast("No more history to load.");
            self.is_loading_more = false;
            return;
        }

        let start_index = cutoff_index.saturating_sub(HISTORY_LOAD_FILE_COUNT);
        let dates_to_load = &available_dates[start_index..cutoff_index];
        let new_start = match dates_to_load.first().copied() {
            Some(d) => d,
            None => {
                self.is_loading_more = false;
                return;
            }
        };

        let mut older_logs = Vec::new();
        for date in dates_to_load {
            if let Ok(mut day_logs) =
                storage::read_entries_for_date_range(&self.config.data.log_path, *date, *date)
            {
                older_logs.append(&mut day_logs);
            }
        }

        // Always move the loaded range pointer forward (even if there were no entries).
        self.loaded_start_date = Some(new_start);

        if older_logs.is_empty() {
            self.toast("Loaded earlier days (no entries).");
            self.is_loading_more = false;
            return;
        }

        let older_filtered: Vec<LogEntry> = older_logs
            .iter()
            .filter(|entry| entry_matches_timeline_filter(entry, self.timeline_filter))
            .cloned()
            .collect();
        let inserted_entries = older_filtered.len();
        let inserted_separators = count_distinct_entry_dates(&older_filtered);
        let inserted_ui_items = inserted_entries + inserted_separators;

        let mut new_logs = older_logs;
        new_logs.extend(std::mem::take(&mut self.all_logs));
        self.all_logs = new_logs;
        self.apply_fold_markers();
        self.apply_timeline_filter(false);

        // Preserve viewport anchor (same UI item at the top) after prepending.
        if inserted_ui_items > 0 {
            if let Some(ui_selected) = self.timeline_ui_state.selected() {
                self.timeline_ui_state
                    .select(Some(ui_selected.saturating_add(inserted_ui_items)));
            }
            *self.timeline_ui_state.offset_mut() = self
                .timeline_ui_state
                .offset()
                .saturating_add(inserted_ui_items);
        }

        if inserted_entries == 0 {
            self.toast("Loaded earlier days (no matching entries).");
        } else {
            self.toast(format!("✓ Loaded {} more entries", inserted_entries));
        }
        self.is_loading_more = false;
    }

    pub fn toast(&mut self, message: impl Into<String>) {
        self.toast_message = Some(message.into());
        self.toast_expiry = Some(Local::now() + Duration::seconds(2));
    }

    /// Returns true if Vim-style editing is enabled
    pub fn is_vim_mode(&self) -> bool {
        self.config
            .ui
            .editor_style
            .as_deref()
            .and_then(crate::config::EditorStyle::from_name)
            .unwrap_or_else(crate::config::EditorStyle::default)
            == crate::config::EditorStyle::Vim
    }

    pub fn scroll_up(&mut self) {
        if self.logs.is_empty() || self.is_loading_more {
            return;
        }

        // If currently scrolled within a tall entry, scroll up within it first
        if self.entry_scroll_offset > 0 {
            self.entry_scroll_offset = self.entry_scroll_offset.saturating_sub(1);
            return;
        }

        let i = match self.logs_state.selected() {
            Some(i) => {
                if i == 0 {
                    // At the top - try to load more history
                    self.load_more_history();
                    // Don't change selection here - load_more_history already set it
                    return;
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.logs_state.select(Some(i));
        // Reset scroll offset when moving to a different entry
        // and position at the bottom of the new entry if it's tall
        self.entry_scroll_offset = 0;
        self.entry_scroll_to_bottom = true;
    }

    pub fn scroll_down(&mut self) {
        if self.logs.is_empty() {
            return;
        }

        // Check if we can scroll down within the current tall entry
        if self.selected_entry_line_count > self.timeline_viewport_height
            && self.timeline_viewport_height > 0
        {
            let max_offset = self
                .selected_entry_line_count
                .saturating_sub(self.timeline_viewport_height);
            if self.entry_scroll_offset < max_offset {
                self.entry_scroll_offset += 1;
                return;
            }
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
        // Reset scroll offset when moving to a different entry
        self.entry_scroll_offset = 0;
    }

    pub fn scroll_to_top(&mut self) {
        if self.logs.is_empty() {
            return;
        }
        self.logs_state.select(Some(0));
        self.entry_scroll_offset = 0;
        *self.timeline_ui_state.offset_mut() = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.logs.is_empty() {
            return;
        }
        self.logs_state.select(Some(self.logs.len() - 1));
        self.entry_scroll_offset = 0;
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

    pub fn toggle_entry_fold(&mut self) {
        let Some(i) = self.logs_state.selected() else {
            return;
        };
        let Some(entry) = self.logs.get(i) else {
            return;
        };
        let total = self.entry_display_line_count(entry);
        if total <= 1 {
            return;
        }

        let is_folded = self.entry_is_folded(entry);
        let key = EntryIdentity::from(entry);
        let override_state = if is_folded {
            FoldOverride::Expanded
        } else {
            FoldOverride::Folded
        };
        self.fold_overrides.insert(key, override_state);
        if let Err(err) = storage::update_fold_marker(
            &entry.file_path,
            entry.line_number,
            override_state,
        ) {
            eprintln!("Failed to save fold state: {}", err);
            self.toast("Failed to save fold state.");
        }
        self.entry_scroll_offset = 0;
    }

    pub fn cycle_fold_state(&mut self) {
        self.fold_state = match self.fold_state {
            FoldState::Overview => FoldState::Contents,
            FoldState::Contents => FoldState::ShowAll,
            FoldState::ShowAll => FoldState::Overview,
        };
        self.entry_scroll_offset = 0;
    }

    pub fn entry_fold_limit(&self, entry: &LogEntry) -> Option<usize> {
        let total = self.entry_display_line_count(entry);
        if total <= 1 {
            return None;
        }
        if let Some(override_state) = self.fold_overrides.get(&EntryIdentity::from(entry)) {
            return match override_state {
                FoldOverride::Folded => Some(1),
                FoldOverride::Expanded => None,
            };
        }
        match self.fold_state {
            FoldState::Overview => Some(1),
            FoldState::Contents => Some(2),
            FoldState::ShowAll => None,
        }
    }

    pub fn entry_is_folded(&self, entry: &LogEntry) -> bool {
        let total = self.entry_display_line_count(entry);
        let limit = self.entry_fold_limit(entry);
        match limit {
            Some(n) => total > n,
            None => false,
        }
    }

    pub fn entry_display_line_count(&self, entry: &LogEntry) -> usize {
        let mut count = entry.content.lines().count();
        if entry
            .content
            .lines()
            .next()
            .is_some_and(is_heading_timestamp_line)
        {
            count = count.saturating_sub(1);
        }
        count.max(1)
    }

    pub(crate) fn apply_fold_markers(&mut self) {
        let overrides = extract_fold_markers_from_logs(&mut self.all_logs);
        for (key, value) in overrides {
            self.fold_overrides.insert(key, value);
        }
    }

    pub fn apply_timeline_filter(&mut self, reset_selection: bool) {
        if self.is_search_result {
            return;
        }

        let selected_identity = if !reset_selection {
            self.logs_state
                .selected()
                .and_then(|i| self.logs.get(i))
                .map(EntryIdentity::from)
        } else {
            None
        };

        self.logs = self
            .all_logs
            .iter()
            .filter(|entry| entry_matches_timeline_filter(entry, self.timeline_filter))
            .cloned()
            .collect();

        if self.logs.is_empty() {
            self.logs_state.select(None);
        } else if let Some(identity) = selected_identity {
            if let Some(idx) = self
                .logs
                .iter()
                .position(|entry| EntryIdentity::from(entry) == identity)
            {
                self.logs_state.select(Some(idx));
            } else {
                self.logs_state.select(Some(self.logs.len() - 1));
            }
        } else if reset_selection || self.logs_state.selected().is_none() {
            self.logs_state.select(Some(self.logs.len() - 1));
        } else if let Some(i) = self.logs_state.selected() {
            self.logs_state.select(Some(i.min(self.logs.len() - 1)));
        }

        if reset_selection {
            *self.timeline_ui_state.offset_mut() = 0;
        }
    }

    pub fn set_timeline_filter(&mut self, filter: TimelineFilter) {
        if self.timeline_filter == filter {
            return;
        }
        self.timeline_filter = filter;
        self.entry_scroll_offset = 0;
        self.entry_scroll_to_bottom = false;
        *self.timeline_ui_state.offset_mut() = 0;
        self.apply_timeline_filter(false);
    }

    pub fn cycle_timeline_filter(&mut self) {
        self.timeline_filter = match self.timeline_filter {
            TimelineFilter::All => TimelineFilter::Work,
            TimelineFilter::Work => TimelineFilter::Personal,
            TimelineFilter::Personal => TimelineFilter::All,
        };
        self.entry_scroll_offset = 0;
        self.entry_scroll_to_bottom = false;
        *self.timeline_ui_state.offset_mut() = 0;
        self.apply_timeline_filter(false);
    }

    pub fn timeline_filter_label(&self) -> &'static str {
        match self.timeline_filter {
            TimelineFilter::All => "All",
            TimelineFilter::Work => "Work",
            TimelineFilter::Personal => "Personal",
        }
    }

    pub fn set_selected_entry_context(&mut self, context: TimelineFilter) {
        let Some(i) = self.logs_state.selected() else {
            self.toast("No entry selected.");
            return;
        };
        let Some(entry) = self.logs.get(i) else {
            self.toast("No entry selected.");
            return;
        };

        let mut lines =
            storage::read_lines_range(&entry.file_path, entry.line_number, entry.end_line)
                .unwrap_or_else(|_| entry.content.lines().map(|s| s.to_string()).collect());
        let changed = apply_context_tag_to_lines(&mut lines, context);
        if !changed {
            return;
        }

        if let Err(err) = storage::replace_entry_lines(
            &entry.file_path,
            entry.line_number,
            entry.end_line,
            &lines,
        ) {
            eprintln!("Failed to update context tag: {}", err);
            self.toast("Failed to update context tag.");
            return;
        }

        let message = match context {
            TimelineFilter::Work => "Context set to Work.",
            TimelineFilter::Personal => "Context set to Personal.",
            TimelineFilter::All => "Context cleared.",
        };
        self.update_logs();
        self.toast(message);
    }

    pub fn update_composer_context(&mut self, context: TimelineFilter) -> bool {
        let mut lines = self.textarea.lines().to_vec();
        let changed = apply_context_tag_to_lines(&mut lines, context);
        if !changed {
            return false;
        }

        let (row, col) = self.textarea.cursor();
        self.textarea = TextArea::from(lines);
        self.textarea.set_placeholder_text(PLACEHOLDER_COMPOSE);
        let row = row.min(self.textarea.lines().len().saturating_sub(1));
        let col = col.min(
            self.textarea
                .lines()
                .get(row)
                .map(|line| line.chars().count())
                .unwrap_or(0),
        );
        self.textarea
            .move_cursor(CursorMove::Jump(row as u16, col as u16));
        true
    }

    pub fn apply_task_filter(&mut self, reset_selection: bool) {
        self.tasks = match self.task_filter {
            TaskFilter::Open => self
                .all_tasks
                .iter()
                .filter(|task| !task.is_done)
                .cloned()
                .collect(),
            TaskFilter::Done => self
                .all_tasks
                .iter()
                .filter(|task| task.is_done)
                .cloned()
                .collect(),
            TaskFilter::All => self.all_tasks.clone(),
        };

        self.tasks.sort_by_key(|task| (task_priority_rank(task.priority), task.line_number));

        if self.tasks.is_empty() {
            self.tasks_state.select(None);
            return;
        }

        if reset_selection || self.tasks_state.selected().is_none() {
            self.tasks_state.select(Some(0));
        } else if let Some(i) = self.tasks_state.selected() {
            self.tasks_state.select(Some(i.min(self.tasks.len() - 1)));
        }
    }

    pub fn set_task_filter(&mut self, filter: TaskFilter) {
        if self.task_filter == filter {
            return;
        }
        self.task_filter = filter;
        self.apply_task_filter(true);
    }

    pub fn cycle_task_filter(&mut self) {
        self.task_filter = match self.task_filter {
            TaskFilter::Open => TaskFilter::Done,
            TaskFilter::Done => TaskFilter::All,
            TaskFilter::All => TaskFilter::Open,
        };
        self.apply_task_filter(true);
    }

    pub fn apply_agenda_filter(&mut self, reset_selection: bool) {
        let filter = self.agenda_filter;
        self.agenda_items = self
            .agenda_all_items
            .iter()
            .filter(|item| match item.kind {
                crate::models::AgendaItemKind::Note => true,
                crate::models::AgendaItemKind::Task => match filter {
                    TaskFilter::Open => !item.is_done,
                    TaskFilter::Done => item.is_done,
                    TaskFilter::All => true,
                },
            })
            .cloned()
            .collect();

        let today = Local::now().date_naive();
        self.agenda_items
            .sort_by_key(|item| agenda_sort_key(item, today));

        if self.agenda_items.is_empty() {
            self.agenda_state.select(None);
            return;
        }

        if reset_selection || self.agenda_state.selected().is_none() {
            self.agenda_state.select(Some(0));
        } else if let Some(i) = self.agenda_state.selected() {
            self.agenda_state.select(Some(i.min(self.agenda_items.len() - 1)));
        }
    }

    pub fn cycle_agenda_filter(&mut self) {
        self.agenda_filter = match self.agenda_filter {
            TaskFilter::Open => TaskFilter::Done,
            TaskFilter::Done => TaskFilter::All,
            TaskFilter::All => TaskFilter::Open,
        };
        self.apply_agenda_filter(true);
    }

    pub fn toggle_agenda_unscheduled(&mut self) {
        self.agenda_show_unscheduled = !self.agenda_show_unscheduled;
        self.set_agenda_selected_day(self.agenda_selected_day);
    }

    pub fn set_agenda_selected_day(&mut self, day: NaiveDate) {
        self.agenda_selected_day = day;
        let visible = self.agenda_visible_indices();
        if visible.is_empty() {
            self.agenda_state.select(None);
            return;
        }
        if let Some(current) = self.agenda_state.selected()
            && visible.iter().any(|idx| *idx == current)
        {
            return;
        }
        self.agenda_state.select(Some(visible[0]));
    }

    pub fn agenda_visible_indices(&self) -> Vec<usize> {
        agenda_timeline_indices(self)
    }

    pub fn agenda_filter_label(&self) -> &'static str {
        match self.agenda_filter {
            TaskFilter::Open => "Open",
            TaskFilter::Done => "Done",
            TaskFilter::All => "All",
        }
    }

    pub fn refresh_agenda(&mut self) {
        let today = Local::now().date_naive();
        let start = today - Duration::days(3650);
        let end = today + Duration::days(3650);
        let items =
            storage::read_agenda_entries(&self.config.data.log_path, start, end)
                .unwrap_or_default();
        self.agenda_all_items = items;
        self.apply_agenda_filter(true);
        self.set_agenda_selected_day(self.agenda_selected_day);
    }

    pub fn agenda_move_selection(&mut self, delta: i32) {
        let visible = self.agenda_visible_indices();
        if visible.is_empty() {
            self.agenda_state.select(None);
            return;
        }

        let current = self.agenda_state.selected();
        let pos = current
            .and_then(|idx| visible.iter().position(|i| *i == idx))
            .unwrap_or(0);
        let len = visible.len() as i32;
        let next = (pos as i32 + delta).rem_euclid(len) as usize;
        self.agenda_state.select(Some(visible[next]));
    }

    pub fn open_date_picker(&mut self) {
        let (row, _) = self.textarea.cursor();
        let line = self
            .textarea
            .lines()
            .get(row)
            .cloned()
            .unwrap_or_default();
        let (schedule, _) = crate::task_metadata::parse_task_metadata(&line);
        let now = Local::now();

        let default_duration = schedule.duration_minutes.unwrap_or(30);
        self.date_picker_schedule = schedule;
        self.date_picker_field = DatePickerField::Scheduled;
        self.date_picker_default_date = now.date_naive();
        self.date_picker_default_time = round_time_to_quarter(now.time());
        self.date_picker_default_duration = default_duration;
        self.date_picker_input.clear();
        self.date_picker_input_mode = false;
        self.show_date_picker_popup = true;
    }

    pub fn date_picker_effective_date(&self, field: DatePickerField) -> NaiveDate {
        match field {
            DatePickerField::Scheduled => self
                .date_picker_schedule
                .scheduled
                .unwrap_or(self.date_picker_default_date),
            DatePickerField::Due => self
                .date_picker_schedule
                .due
                .unwrap_or(self.date_picker_default_date),
            DatePickerField::Start => self
                .date_picker_schedule
                .start
                .unwrap_or(self.date_picker_default_date),
            DatePickerField::Time | DatePickerField::Duration => self.date_picker_default_date,
        }
    }

    pub fn date_picker_effective_time(&self) -> NaiveTime {
        self.date_picker_schedule
            .time
            .unwrap_or(self.date_picker_default_time)
    }

    pub fn date_picker_effective_duration(&self) -> u32 {
        self.date_picker_schedule
            .duration_minutes
            .unwrap_or(self.date_picker_default_duration)
    }

    pub fn set_date_picker_date(&mut self, field: DatePickerField, date: NaiveDate) {
        match field {
            DatePickerField::Scheduled => self.date_picker_schedule.scheduled = Some(date),
            DatePickerField::Due => self.date_picker_schedule.due = Some(date),
            DatePickerField::Start => self.date_picker_schedule.start = Some(date),
            DatePickerField::Time | DatePickerField::Duration => {}
        }
    }

    pub fn set_date_picker_time(&mut self, time: NaiveTime) {
        self.date_picker_schedule.time = Some(time);
    }

    pub fn set_date_picker_duration(&mut self, minutes: u32) {
        self.date_picker_schedule.duration_minutes = Some(minutes);
    }

    pub fn task_counts(&self) -> (usize, usize) {
        let mut open = 0usize;
        let mut done = 0usize;
        for task in &self.all_tasks {
            if task.is_done {
                done += 1;
            } else {
                open += 1;
            }
        }
        (open, done)
    }

    pub fn task_filter_label(&self) -> &'static str {
        match self.task_filter {
            TaskFilter::Open => "Open",
            TaskFilter::Done => "Done",
            TaskFilter::All => "All",
        }
    }

    pub fn set_navigate_focus(&mut self, focus: NavigateFocus) {
        if self.navigate_focus == focus {
            return;
        }
        self.last_navigate_focus = Some(self.navigate_focus);
        self.navigate_focus = focus;
    }

    pub fn transition_to(&mut self, mode: InputMode) {
        match mode {
            InputMode::Navigate => {
                // Clear textarea when returning from Search mode
                if self.input_mode == InputMode::Search {
                    self.textarea = TextArea::default();
                }
                self.textarea.set_placeholder_text(PLACEHOLDER_NAVIGATE);
                if !matches!(
                    self.navigate_focus,
                    NavigateFocus::Tasks | NavigateFocus::Agenda
                ) {
                    self.set_navigate_focus(NavigateFocus::Timeline);
                }
                self.composer_dirty = false;
                self.reset_editor_state();
            }
            InputMode::Editing => {
                self.textarea.set_placeholder_text(PLACEHOLDER_COMPOSE);
                self.set_navigate_focus(NavigateFocus::Timeline);
                self.textarea_viewport_row = 0;
                self.textarea_viewport_col = 0;
                self.textarea_viewport_height = 0;
                self.composer_dirty = false;
                self.reset_editor_state();
                // Return to full log view when entering Compose from search results (unless editing an entry)
                if self.is_search_result && self.editing_entry.is_none() {
                    self.update_logs();
                    self.last_search_query = None;
                }
                self.textarea.move_cursor(CursorMove::Bottom);
                self.textarea.move_cursor(CursorMove::End);
                let (row, col) = self.textarea.cursor();
                let line_len = self
                    .textarea
                    .lines()
                    .get(row)
                    .map(|line| line.chars().count())
                    .unwrap_or(0);
                if line_len > 0 && col >= line_len {
                    let new_col = line_len.saturating_sub(1);
                    self.textarea
                        .move_cursor(CursorMove::Jump(row as u16, new_col as u16));
                }
            }
            InputMode::Search => {
                self.textarea = TextArea::default();
                self.textarea.set_placeholder_text(PLACEHOLDER_SEARCH);
                self.textarea_viewport_row = 0;
                self.textarea_viewport_col = 0;
                self.textarea_viewport_height = 0;
                self.composer_dirty = false;
                self.reset_editor_state();
            }
        }
        self.input_mode = mode;
    }

    pub fn reset_editor_state(&mut self) {
        self.editor_mode = EditorMode::Normal;
        self.visual_anchor = None;
        self.pending_command = None;
        self.pending_count = 0;
        self.insert_snapshot = None;
        self.insert_modified = false;
        self.editor_undo.clear();
        self.editor_redo.clear();
        self.clear_visual_hint();
    }

    pub fn editor_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            lines: self.textarea.lines().to_vec(),
            cursor: self.textarea.cursor(),
        }
    }

    pub fn restore_editor_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.textarea = TextArea::from(snapshot.lines);
        self.textarea.set_placeholder_text(PLACEHOLDER_COMPOSE);
        self.textarea.move_cursor(CursorMove::Jump(
            snapshot.cursor.0 as u16,
            snapshot.cursor.1 as u16,
        ));
    }

    pub fn record_undo_snapshot(&mut self) {
        self.editor_undo.push(self.editor_snapshot());
        self.editor_redo.clear();
    }

    pub fn begin_insert_group(&mut self) {
        if self.insert_snapshot.is_none() {
            self.insert_snapshot = Some(self.editor_snapshot());
            self.insert_modified = false;
            self.editor_redo.clear();
        }
    }

    pub fn mark_insert_modified(&mut self) {
        self.insert_modified = true;
    }

    pub fn commit_insert_group(&mut self) {
        if !self.insert_modified {
            self.insert_snapshot = None;
            return;
        }

        if let Some(snapshot) = self.insert_snapshot.take() {
            self.editor_undo.push(snapshot);
            self.editor_redo.clear();
        }
        self.insert_modified = false;
    }

    pub fn editor_undo(&mut self) -> bool {
        let Some(snapshot) = self.editor_undo.pop() else {
            return false;
        };
        let current = self.editor_snapshot();
        self.editor_redo.push(current);
        self.restore_editor_snapshot(snapshot);
        true
    }

    pub fn editor_redo(&mut self) -> bool {
        let Some(snapshot) = self.editor_redo.pop() else {
            return false;
        };
        let current = self.editor_snapshot();
        self.editor_undo.push(current);
        self.restore_editor_snapshot(snapshot);
        true
    }

    pub fn set_yank_buffer(&mut self, text: String) {
        self.set_yank_buffer_with_kind(text, false);
    }

    pub fn set_yank_buffer_with_kind_and_clipboard(&mut self, text: String, linewise: bool) {
        self.set_yank_buffer_with_kind(text, linewise);
        copy_to_clipboard(&self.yank_buffer);
    }

    pub fn set_yank_buffer_with_kind(&mut self, text: String, linewise: bool) {
        self.yank_buffer = text;
        self.yank_is_linewise = linewise;
        self.textarea.set_yank_text(self.yank_buffer.clone());
    }

pub fn show_visual_hint(&mut self, message: impl Into<String>) {
    self.visual_hint_message = Some(message.into());
    self.visual_hint_expiry = Some(Local::now() + Duration::seconds(2));
    self.visual_hint_active = true;
}

pub fn clear_visual_hint(&mut self) {
    self.visual_hint_message = None;
    self.visual_hint_expiry = None;
    self.visual_hint_active = false;
}
}

fn copy_to_clipboard(text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text(text.to_string());
    }
}

fn split_timestamp_prefix(line: &str) -> (String, String) {
    if let Some((prefix, rest)) = split_timestamp_line(line) {
        if is_heading_timestamp_line(line) {
            (line.trim_end().to_string(), rest.to_string())
        } else {
            (prefix.trim_end().to_string(), rest.to_string())
        }
    } else {
        ("".to_string(), line.to_string())
    }
}

/// Computes (done_count, tomato_count) for today's tasks.
/// Excludes tomatoes from carryover tasks (marked with ⟦date⟧) to ensure
/// the tomato count resets daily.
fn compute_today_task_stats(logs: &[LogEntry]) -> (usize, usize) {
    let mut done = 0usize;
    let mut tomatoes = 0usize;

    for entry in logs {
        for line in entry.content.lines() {
            let s = strip_timestamp_prefix(line).trim_start();

            // Skip carryover header lines
            if s.starts_with("⤴ Carryover from ") {
                continue;
            }

            if let Some(text) = s.strip_prefix("- [ ] ") {
                // Carryover tasks have ⟦date⟧ marker - exclude their pre-existing tomatoes
                if !is_carryover_task(text) {
                    tomatoes += count_trailing_tomatoes(text);
                }
                continue;
            }

            if let Some(text) = s
                .strip_prefix("- [x] ")
                .or_else(|| s.strip_prefix("- [X] "))
            {
                done += 1;
                // Only count tomatoes if not a carryover task
                if !is_carryover_task(text) {
                    tomatoes += count_trailing_tomatoes(text);
                }
            }
        }
    }

    (done, tomatoes)
}

/// Checks if a task line contains a carryover date marker (⟦YYYY-MM-DD⟧)
fn is_carryover_task(text: &str) -> bool {
    text.contains("⟦") && text.contains("⟧")
}

fn entry_matches_timeline_filter(entry: &LogEntry, filter: TimelineFilter) -> bool {
    let (has_work, has_personal) = entry_context_flags(entry);
    let is_default_personal = !has_work && !has_personal;
    match filter {
        TimelineFilter::All => true,
        TimelineFilter::Work => has_work,
        TimelineFilter::Personal => has_personal || is_default_personal,
    }
}

pub(crate) fn entry_context_kind(entry: &LogEntry) -> TimelineFilter {
    let (has_work, has_personal) = entry_context_flags(entry);
    if has_work {
        TimelineFilter::Work
    } else if has_personal {
        TimelineFilter::Personal
    } else {
        TimelineFilter::Personal
    }
}

pub(crate) fn entry_context_flags(entry: &LogEntry) -> (bool, bool) {
    let mut has_work = false;
    let mut has_personal = false;

    for line in entry.content.lines() {
        let mut chars = line.char_indices().peekable();
        while let Some((idx, ch)) = chars.next() {
            if ch != '#' {
                continue;
            }

            let prev = line[..idx].chars().last();
            let prev_ok = prev.map_or(true, |c| !is_context_tag_char(c));

            let mut token_lower = String::new();
            let mut end_idx = idx + ch.len_utf8();
            while let Some(&(next_idx, next_ch)) = chars.peek() {
                if is_context_tag_char(next_ch) {
                    token_lower.push(next_ch.to_ascii_lowercase());
                    end_idx = next_idx + next_ch.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }

            let next = line[end_idx..].chars().next();
            let next_ok = next.map_or(true, |c| !is_context_tag_char(c));
            if !(prev_ok && next_ok) {
                continue;
            }

            if token_lower == "work" {
                has_work = true;
            } else if token_lower == "personal" {
                has_personal = true;
            }

            if has_work && has_personal {
                return (true, true);
            }
        }
    }

    (has_work, has_personal)
}

fn is_context_tag_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

pub(crate) fn strip_context_tags_from_line(line: &str) -> (String, bool) {
    let mut out = String::with_capacity(line.len());
    let mut changed = false;
    let mut chars = line.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if ch != '#' {
            out.push(ch);
            continue;
        }

        let prev = line[..idx].chars().last();
        let prev_ok = prev.map_or(true, |c| !is_context_tag_char(c));

        let mut token = String::new();
        let mut token_lower = String::new();
        let mut end_idx = idx + ch.len_utf8();
        while let Some(&(next_idx, next_ch)) = chars.peek() {
            if is_context_tag_char(next_ch) {
                token.push(next_ch);
                token_lower.push(next_ch.to_ascii_lowercase());
                end_idx = next_idx + next_ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let next = line[end_idx..].chars().next();
        let next_ok = next.map_or(true, |c| !is_context_tag_char(c));
        let is_context = token_lower == "work" || token_lower == "personal";

        if prev_ok && next_ok && is_context {
            changed = true;
            if let Some(&(_, next_ch)) = chars.peek() {
                let out_has_ws =
                    out.is_empty() || out.chars().last().is_some_and(|c| c.is_whitespace());
                if next_ch.is_whitespace() && out_has_ws {
                    chars.next();
                }
            }
            continue;
        }

        out.push('#');
        out.push_str(&token);
    }

    (out, changed)
}

fn apply_context_tag_to_lines(lines: &mut Vec<String>, context: TimelineFilter) -> bool {
    let mut changed = false;
    for line in lines.iter_mut() {
        let (updated, did_change) = strip_context_tags_from_line(line);
        if did_change {
            *line = updated;
            changed = true;
        }
    }

    if context == TimelineFilter::All {
        return changed;
    }

    let tag = match context {
        TimelineFilter::Work => "#work",
        TimelineFilter::Personal => "#personal",
        TimelineFilter::All => return changed,
    };

    let mut start_idx = 0;
    if lines
        .first()
        .is_some_and(|line| is_timestamped_line(line))
    {
        start_idx = 1;
    }

    let insert_idx = lines
        .iter()
        .enumerate()
        .skip(start_idx)
        .find(|(_, line)| !line.trim().is_empty() && parse_fold_marker(line).is_none())
        .map(|(idx, _)| idx)
        .unwrap_or(lines.len());

    if insert_idx >= lines.len() {
        lines.push(tag.to_string());
        return true;
    }

    let target = &mut lines[insert_idx];
    if target.trim().is_empty() {
        *target = tag.to_string();
    } else {
        if !target.ends_with(' ') {
            target.push(' ');
        }
        target.push_str(tag);
    }

    true
}

fn extract_fold_markers_from_logs(
    logs: &mut Vec<LogEntry>,
) -> HashMap<EntryIdentity, FoldOverride> {
    let mut overrides = HashMap::new();
    for entry in logs.iter_mut() {
        let (override_state, cleaned) = strip_fold_markers(&entry.content);
        if let Some(state) = override_state {
            overrides.insert(EntryIdentity::from(&*entry), state);
        }
        entry.content = cleaned;
    }
    overrides
}

fn strip_fold_markers(content: &str) -> (Option<FoldOverride>, String) {
    if !content.contains("memolog:") {
        return (None, content.to_string());
    }
    let mut override_state = None;
    let mut lines = Vec::new();
    for line in content.lines() {
        if let Some((state, cleaned)) = parse_fold_marker(line) {
            override_state = Some(state);
            if let Some(cleaned) = cleaned {
                lines.push(cleaned);
            }
            continue;
        }
        lines.push(line.to_string());
    }
    (override_state, lines.join("\n"))
}

fn parse_fold_marker(line: &str) -> Option<(FoldOverride, Option<String>)> {
    let start = line.find("<!--")?;
    let end_rel = line[start + 4..].find("-->")?;
    let end = start + 4 + end_rel;
    let inner = line[start + 4..end].trim();
    let state = match inner {
        "memolog:expanded" => FoldOverride::Expanded,
        "memolog:folded" | "memolog:collapsed" => FoldOverride::Folded,
        _ => return None,
    };
    let after_start = end + 3;
    let before = &line[..start];
    let after = line.get(after_start..).unwrap_or("");
    let before_ends_ws = before
        .chars()
        .last()
        .map(|c| c.is_whitespace())
        .unwrap_or(true);
    let after = if before_ends_ws {
        after.trim_start_matches(|c: char| c.is_whitespace())
    } else {
        after
    };
    let mut cleaned = String::with_capacity(line.len());
    cleaned.push_str(before);
    cleaned.push_str(after);
    let cleaned = cleaned.trim_end();
    let cleaned = if cleaned.trim().is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    };
    Some((state, cleaned))
}

fn strip_fold_markers_from_lines(lines: &[String]) -> Vec<String> {
    let mut cleaned = Vec::with_capacity(lines.len());
    for line in lines {
        if let Some((_, remainder)) = parse_fold_marker(line) {
            if let Some(remainder) = remainder {
                cleaned.push(remainder);
            }
        } else {
            cleaned.push(line.clone());
        }
    }
    cleaned
}

fn count_distinct_entry_dates(entries: &[LogEntry]) -> usize {
    let mut last: Option<String> = None;
    let mut count = 0usize;
    for entry in entries {
        let date = file_date(&entry.file_path);
        let Some(date) = date else {
            continue;
        };
        if last.as_ref() != Some(&date) {
            count += 1;
            last = Some(date);
        }
    }
    count
}

fn file_date(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

fn round_time_to_quarter(time: NaiveTime) -> NaiveTime {
    let total_minutes = time.hour() as i32 * 60 + time.minute() as i32;
    let rounded = ((total_minutes + 14) / 15) * 15;
    let minutes = rounded.rem_euclid(24 * 60) as u32;
    // from_hms_opt can only fail with invalid input, but our calculations ensure valid range
    NaiveTime::from_hms_opt(minutes / 60, minutes % 60, 0)
        .unwrap_or(NaiveTime::from_hms_opt(0, 0, 0).expect("Valid time constant"))
}

fn task_priority_rank(priority: Option<Priority>) -> u8 {
    match priority {
        Some(Priority::High) => 0,
        Some(Priority::Medium) => 1,
        Some(Priority::Low) => 2,
        None => 3,
    }
}

fn agenda_sort_key(
    item: &crate::models::AgendaItem,
    today: NaiveDate,
) -> (NaiveDate, u8, u8, NaiveTime, usize) {
    let is_overdue = item.kind == crate::models::AgendaItemKind::Task
        && item.schedule.due.is_some()
        && item.schedule.due.unwrap_or(today) < today
        && !item.is_done;
    let overdue_rank = if is_overdue { 0 } else { 1 };
    let kind_rank = match item.kind {
        crate::models::AgendaItemKind::Task => 0,
        crate::models::AgendaItemKind::Note => 1,
    };
    // Use end-of-day time for items without specific time
    const END_OF_DAY: NaiveTime = match NaiveTime::from_hms_opt(23, 59, 59) {
        Some(t) => t,
        None => unreachable!(),
    };
    let time = item.time.unwrap_or(END_OF_DAY);
    (item.date, overdue_rank, kind_rank, time, item.line_number)
}

fn agenda_timeline_indices(app: &App) -> Vec<usize> {
    let day = app.agenda_selected_day;
    let mut overdue = Vec::new();
    let mut all_day = Vec::new();
    let mut timed = Vec::new();
    let mut unscheduled = Vec::new();

    for (idx, item) in app.agenda_items.iter().enumerate() {
        match item.kind {
            crate::models::AgendaItemKind::Task => {
                let is_overdue = item.schedule.due.is_some()
                    && item.schedule.due.unwrap_or(day) < day
                    && !item.is_done;
                if is_overdue {
                    overdue.push(idx);
                    continue;
                }
                if item.schedule.is_empty() {
                    if app.agenda_show_unscheduled {
                        unscheduled.push(idx);
                    }
                    continue;
                }
                if item.date != day {
                    continue;
                }
                if item.time.is_some() {
                    timed.push(idx);
                } else {
                    all_day.push(idx);
                }
            }
            crate::models::AgendaItemKind::Note => {
                if item.date != day {
                    continue;
                }
                if item.time.is_some() {
                    timed.push(idx);
                } else {
                    all_day.push(idx);
                }
            }
        }
    }

    const MIDNIGHT: NaiveTime = match NaiveTime::from_hms_opt(0, 0, 0) {
        Some(t) => t,
        None => unreachable!(),
    };
    let time_min = MIDNIGHT;
    overdue.sort_by_key(|idx| {
        let item = &app.agenda_items[*idx];
        (
            item.schedule.due.unwrap_or(day),
            item.time.unwrap_or(time_min),
            task_priority_rank(item.priority),
            item.line_number,
        )
    });
    all_day.sort_by_key(|idx| {
        let item = &app.agenda_items[*idx];
        (task_priority_rank(item.priority), item.line_number)
    });
    timed.sort_by_key(|idx| {
        let item = &app.agenda_items[*idx];
        let kind_rank = match item.kind {
            crate::models::AgendaItemKind::Task => 0,
            crate::models::AgendaItemKind::Note => 1,
        };
        (
            item.time.unwrap_or(time_min),
            kind_rank,
            task_priority_rank(item.priority),
            item.line_number,
        )
    });

    unscheduled.sort_by_key(|idx| {
        let item = &app.agenda_items[*idx];
        (task_priority_rank(item.priority), item.line_number)
    });

    let mut visible = Vec::new();
    visible.extend(overdue);
    visible.extend(all_day);
    visible.extend(timed);
    visible.extend(unscheduled);
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_app() -> App<'static> {
        let mut app = App::new();
        // Create some test log entries
        app.logs = vec![
            LogEntry {
                content: "Short entry".to_string(),
                file_path: "/test/2025-12-22.md".to_string(),
                line_number: 1,
                end_line: 1,
            },
            LogEntry {
                content: "Another short entry".to_string(),
                file_path: "/test/2025-12-22.md".to_string(),
                line_number: 2,
                end_line: 2,
            },
        ];
        app.all_logs = app.logs.clone();
        app.logs_state.select(Some(0));
        app
    }

    #[test]
    fn app_starts_in_navigate_mode() {
        let app = App::new();
        assert!(matches!(app.input_mode, InputMode::Navigate));
    }

    #[test]
    fn scroll_down_moves_to_next_entry_when_not_tall() {
        let mut app = make_test_app();
        app.logs_state.select(Some(0));
        app.selected_entry_line_count = 5;
        app.timeline_viewport_height = 10; // Entry fits in viewport

        app.scroll_down();

        assert_eq!(app.logs_state.selected(), Some(1));
        assert_eq!(app.entry_scroll_offset, 0);
    }

    #[test]
    fn scroll_down_scrolls_within_tall_entry() {
        let mut app = make_test_app();
        app.logs_state.select(Some(0));
        app.selected_entry_line_count = 30;
        app.timeline_viewport_height = 10; // Entry is taller than viewport
        app.entry_scroll_offset = 0;

        app.scroll_down();

        // Should scroll within entry, not move to next
        assert_eq!(app.logs_state.selected(), Some(0));
        assert_eq!(app.entry_scroll_offset, 1);
    }

    #[test]
    fn scroll_down_moves_to_next_after_reaching_bottom_of_tall_entry() {
        let mut app = make_test_app();
        app.logs_state.select(Some(0));
        app.selected_entry_line_count = 30;
        app.timeline_viewport_height = 10;
        app.entry_scroll_offset = 20; // At max offset (30 - 10)

        app.scroll_down();

        // Should move to next entry and reset offset
        assert_eq!(app.logs_state.selected(), Some(1));
        assert_eq!(app.entry_scroll_offset, 0);
    }

    #[test]
    fn scroll_up_scrolls_within_tall_entry() {
        let mut app = make_test_app();
        app.logs_state.select(Some(1));
        app.selected_entry_line_count = 30;
        app.timeline_viewport_height = 10;
        app.entry_scroll_offset = 5;

        app.scroll_up();

        // Should scroll within entry
        assert_eq!(app.logs_state.selected(), Some(1));
        assert_eq!(app.entry_scroll_offset, 4);
    }

    #[test]
    fn scroll_up_moves_to_previous_after_reaching_top_of_entry() {
        let mut app = make_test_app();
        app.logs_state.select(Some(1));
        app.selected_entry_line_count = 30;
        app.timeline_viewport_height = 10;
        app.entry_scroll_offset = 0; // Already at top

        app.scroll_up();

        // Should move to previous entry
        assert_eq!(app.logs_state.selected(), Some(0));
        assert!(app.entry_scroll_to_bottom); // Should position at bottom
    }

    #[test]
    fn scroll_to_top_resets_entry_scroll_offset() {
        let mut app = make_test_app();
        app.logs_state.select(Some(1));
        app.entry_scroll_offset = 10;

        app.scroll_to_top();

        assert_eq!(app.logs_state.selected(), Some(0));
        assert_eq!(app.entry_scroll_offset, 0);
    }

    #[test]
    fn scroll_to_bottom_resets_entry_scroll_offset() {
        let mut app = make_test_app();
        app.logs_state.select(Some(0));
        app.entry_scroll_offset = 10;

        app.scroll_to_bottom();

        assert_eq!(app.logs_state.selected(), Some(1));
        assert_eq!(app.entry_scroll_offset, 0);
    }
}
