use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
};
use std::{error::Error, io};

mod app;
mod config;
mod models;
mod storage;
mod ui;

use crate::config::key_match;
use app::App;
use chrono::{Duration, Local};
use models::{InputMode, Mood};

fn main() -> Result<(), Box<dyn Error>> {
    // 앱 초기화 및 설정 로드
    let mut app = App::new();

    // 터미널 초기화
    // 터미널 초기화
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture,)?;

    // 키보드 향상 플래그는 지원되지 않는 터미널(예: Windows Legacy Console)에서 에러를 뱉을 수 있음.
    // 에러가 발생해도 앱 실행엔 지장이 없으므로 무시함.
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 앱 실행
    let res = run_app(&mut terminal, &mut app);

    // 터미널 복구
    disable_raw_mode()?;

    // 종료 시에도 플래그 해제 시도 (실패해도 무방)
    let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        // 뽀모도로 타이머 및 알림 체크
        check_timers(app);

        terminal.draw(|f| ui::ui(f, app))?;

        // 알림 표시 중일 때는 입력을 아예 받지 않음 (강제 휴식/주목)
        if app.pomodoro_alert_expiry.is_some() {
            if event::poll(std::time::Duration::from_millis(100))? {
                let _ = event::read()?; // 이벤트 소모
            }
            continue;
        }

        if event::poll(std::time::Duration::from_millis(250))? {
            let event = event::read()?;

            if let Event::Mouse(mouse_event) = event {
                match mouse_event.kind {
                    event::MouseEventKind::ScrollUp => app.scroll_up(),
                    event::MouseEventKind::ScrollDown => app.scroll_down(),
                    _ => {}
                }
            }

            if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press {
                    handle_key_input(app, key);
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn check_timers(app: &mut App) {
    if let Some(end_time) = app.pomodoro_end {
        if Local::now() >= end_time {
            app.pomodoro_end = None; // 타이머 종료

            if let Some(models::PomodoroTarget::Task {
                text,
                file_path,
                line_number,
            }) = app.pomodoro_target.take()
            {
                let _ = storage::append_tomato_to_line(&file_path, line_number);
                app.update_logs();
                app.pomodoro_alert_message =
                    Some(format!("Pomodoro complete: 🍅 added to \"{}\".", text));
            } else {
                app.pomodoro_alert_message = Some("Pomodoro complete.".to_string());
            }

            let alert_seconds = app.config.pomodoro.alert_seconds.max(1) as i64;
            app.pomodoro_alert_expiry = Some(Local::now() + Duration::seconds(alert_seconds));
        }
    }

    if let Some(expiry) = app.pomodoro_alert_expiry {
        if Local::now() >= expiry {
            app.pomodoro_alert_expiry = None; // 알림 종료
            app.pomodoro_alert_message = None;
        }
    }
}

fn handle_key_input(app: &mut App, key: event::KeyEvent) {
    if handle_popup_events(app, key) {
        return;
    }

    match app.input_mode {
        InputMode::Navigate => handle_normal_mode(app, key),
        InputMode::Editing => handle_editing_mode(app, key),
        InputMode::Search => handle_search_mode(app, key),
    }
}

fn handle_popup_events(app: &mut App, key: event::KeyEvent) -> bool {
    if app.show_mood_popup {
        handle_mood_popup(app, key);
        return true;
    }
    if app.show_todo_popup {
        handle_todo_popup(app, key);
        return true;
    }
    if app.show_tag_popup {
        handle_tag_popup(app, key);
        return true;
    }
    if app.show_activity_popup {
        // 아무 키나 누르면 닫기
        app.show_activity_popup = false;
        return true;
    }
    if app.show_path_popup {
        handle_path_popup(app, key);
        return true;
    }
    false
}

fn handle_mood_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.up) {
        let i = match app.mood_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    Mood::all().len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        app.mood_list_state.select(Some(i));
    } else if key_match(&key, &app.config.keybindings.popup.down) {
        let i = match app.mood_list_state.selected() {
            Some(i) => {
                if i >= Mood::all().len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        app.mood_list_state.select(Some(i));
    } else if key_match(&key, &app.config.keybindings.popup.confirm) {
        if let Some(i) = app.mood_list_state.selected() {
            let mood = Mood::all()[i];
            let _ = storage::append_entry(
                &app.config.data.log_path,
                &format!("Mood: {}", mood.as_str()),
            );
            app.update_logs();
        }
        check_carryover(app);
        app.show_mood_popup = false;
    } else if key_match(&key, &app.config.keybindings.popup.cancel) {
        app.show_mood_popup = false;
        app.transition_to(InputMode::Editing);
    }
}

fn check_carryover(app: &mut App) {
    let already_checked = storage::is_carryover_done(&app.config.data.log_path).unwrap_or(false);
    if !already_checked {
        if let Ok(todos) = storage::get_last_file_pending_todos(&app.config.data.log_path) {
            if !todos.is_empty() {
                app.pending_todos = todos;
                app.show_todo_popup = true;
            } else {
                app.transition_to(InputMode::Editing);
                let _ = storage::mark_carryover_done(&app.config.data.log_path);
            }
        } else {
            app.transition_to(InputMode::Editing);
            let _ = storage::mark_carryover_done(&app.config.data.log_path);
        }
    } else {
        app.transition_to(InputMode::Editing);
    }
}

fn handle_todo_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        for todo in &app.pending_todos {
            let _ = storage::append_entry(&app.config.data.log_path, todo);
        }
        app.update_logs();
        app.show_todo_popup = false;
        app.transition_to(InputMode::Editing);
        let _ = storage::mark_carryover_done(&app.config.data.log_path);
    } else if key_match(&key, &app.config.keybindings.popup.cancel) {
        app.show_todo_popup = false;
        app.transition_to(InputMode::Editing);
        let _ = storage::mark_carryover_done(&app.config.data.log_path);
    }
}

fn handle_tag_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.up) {
        let i = match app.tag_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        app.tag_list_state.select(Some(i));
    } else if key_match(&key, &app.config.keybindings.popup.down) {
        let i = match app.tag_list_state.selected() {
            Some(i) => {
                if i >= app.tags.len() - 1 {
                    app.tags.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        app.tag_list_state.select(Some(i));
    } else if key_match(&key, &app.config.keybindings.popup.confirm) {
        if let Some(i) = app.tag_list_state.selected() {
            if i < app.tags.len() {
                let query = app.tags[i].0.clone();
                if let Ok(results) = storage::search_entries(&app.config.data.log_path, &query) {
                    app.logs = results;
                    app.is_search_result = true;
                    app.logs_state.select(Some(0));
                }
            }
        }
        app.show_tag_popup = false;
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.popup.cancel) {
        app.show_tag_popup = false;
        app.transition_to(InputMode::Navigate);
    }
}

fn handle_normal_mode(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.global.tags) {
        if let Ok(tags) = storage::get_all_tags(&app.config.data.log_path) {
            app.tags = tags;
            if !app.tags.is_empty() {
                app.tag_list_state.select(Some(0));
                app.show_tag_popup = true;
            }
        }
    } else if key_match(&key, &app.config.keybindings.global.quit) {
        app.quit();
    } else if key_match(&key, &app.config.keybindings.global.focus_tasks) {
        app.navigate_focus = models::NavigateFocus::Tasks;
    } else if key_match(&key, &app.config.keybindings.global.focus_timeline) {
        app.navigate_focus = models::NavigateFocus::Timeline;
    } else if key_match(&key, &app.config.keybindings.global.focus_next) {
        app.navigate_focus = match app.navigate_focus {
            models::NavigateFocus::Timeline => models::NavigateFocus::Tasks,
            models::NavigateFocus::Tasks => models::NavigateFocus::Timeline,
        };
    } else if key_match(&key, &app.config.keybindings.global.focus_prev) {
        app.navigate_focus = match app.navigate_focus {
            models::NavigateFocus::Timeline => models::NavigateFocus::Tasks,
            models::NavigateFocus::Tasks => models::NavigateFocus::Timeline,
        };
    } else if key_match(&key, &app.config.keybindings.global.focus_composer) {
        app.transition_to(InputMode::Editing);
    } else if key_match(&key, &app.config.keybindings.global.search) {
        app.transition_to(InputMode::Search);
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.up)
    {
        app.scroll_up();
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.down)
    {
        app.scroll_down();
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.page_up)
    {
        for _ in 0..10 {
            app.scroll_up();
        }
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.page_down)
    {
        for _ in 0..10 {
            app.scroll_down();
        }
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.top)
    {
        app.scroll_to_top();
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.bottom)
    {
        app.scroll_to_bottom();
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.edit)
    {
        if !app.is_search_result {
            if let Some(i) = app.logs_state.selected() {
                if i < app.logs.len() {
                    let entry = app.logs[i].clone();
                    app.start_edit_entry(&entry);
                }
            }
        }
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.up)
    {
        app.tasks_up();
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.down)
    {
        app.tasks_down();
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.edit)
    {
        if let Some(i) = app.tasks_state.selected() {
            if i < app.tasks.len() {
                let task = app.tasks[i].clone();
                if !app.is_search_result {
                    if let Some(entry) = app.logs.iter().find(|e| {
                        e.file_path == task.file_path
                            && e.line_number <= task.line_number
                            && task.line_number <= e.end_line
                    }) {
                        let entry = entry.clone();
                        app.start_edit_entry(&entry);
                    }
                } else {
                    app.update_logs();
                    if let Some(entry) = app.logs.iter().find(|e| {
                        e.file_path == task.file_path
                            && e.line_number <= task.line_number
                            && task.line_number <= e.end_line
                    }) {
                        let entry = entry.clone();
                        app.start_edit_entry(&entry);
                    }
                }
            }
        }
    } else if key.code == KeyCode::Esc {
        if app.is_search_result {
            app.update_logs();
        }
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.toggle_todo)
    {
        if let Some(i) = app.logs_state.selected() {
            if i < app.logs.len() {
                let entry = &app.logs[i];
                if entry.content.contains("- [ ]") || entry.content.contains("- [x]") {
                    let _ = storage::toggle_todo_status(entry);
                    if app.is_search_result {
                        app.update_logs(); // TODO: Maintain search, but reloading is safer
                    } else {
                        app.update_logs();
                    }
                    app.logs_state.select(Some(i));
                }
            }
        }
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.toggle)
    {
        if let Some(i) = app.tasks_state.selected() {
            if i < app.tasks.len() {
                let task = app.tasks[i].clone();
                let _ = storage::toggle_task_status(&task.file_path, task.line_number);
                app.update_logs();
            }
        }
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.start_pomodoro)
    {
        if let Some(i) = app.tasks_state.selected() {
            if i < app.tasks.len() {
                let task = app.tasks[i].clone();
                let mins = app.config.pomodoro.work_minutes as i64;
                if mins > 0 {
                    if let Some(models::PomodoroTarget::Task {
                        file_path,
                        line_number,
                        ..
                    }) = app.pomodoro_target.as_ref()
                    {
                        if app.pomodoro_end.is_some()
                            && *file_path == task.file_path
                            && *line_number == task.line_number
                        {
                            app.pomodoro_end = None;
                            app.pomodoro_target = None;
                            return;
                        }
                    }
                    app.pomodoro_end = Some(Local::now() + Duration::minutes(mins));
                    app.pomodoro_target = Some(models::PomodoroTarget::Task {
                        text: task.text,
                        file_path: task.file_path,
                        line_number: task.line_number,
                    });
                    app.pomodoro_alert_expiry = None;
                    app.pomodoro_alert_message = None;
                }
            }
        }
    } else if key_match(&key, &app.config.keybindings.global.pomodoro) {
        app.navigate_focus = models::NavigateFocus::Tasks;
        if let Some(i) = app.tasks_state.selected() {
            if i < app.tasks.len() {
                let task = app.tasks[i].clone();
                let mins = app.config.pomodoro.work_minutes as i64;
                if mins > 0 {
                    if let Some(models::PomodoroTarget::Task {
                        file_path,
                        line_number,
                        ..
                    }) = app.pomodoro_target.as_ref()
                    {
                        if app.pomodoro_end.is_some()
                            && *file_path == task.file_path
                            && *line_number == task.line_number
                        {
                            app.pomodoro_end = None;
                            app.pomodoro_target = None;
                            return;
                        }
                    }
                    app.pomodoro_end = Some(Local::now() + Duration::minutes(mins));
                    app.pomodoro_target = Some(models::PomodoroTarget::Task {
                        text: task.text,
                        file_path: task.file_path,
                        line_number: task.line_number,
                    });
                    app.pomodoro_alert_expiry = None;
                    app.pomodoro_alert_message = None;
                }
            }
        }
    } else if key_match(&key, &app.config.keybindings.global.activity) {
        if let Ok(data) = storage::get_activity_stats(&app.config.data.log_path) {
            app.activity_data = data;
            app.show_activity_popup = true;
        }
    } else if key_match(&key, &app.config.keybindings.global.log_dir) {
        app.show_path_popup = true;
    }
}

fn handle_search_mode(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.search.cancel) {
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.search.clear) {
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Search);
    } else if key_match(&key, &app.config.keybindings.search.submit) {
        let query = app
            .textarea
            .lines()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(" ");
        if !query.trim().is_empty() {
            if let Ok(results) = storage::search_entries(&app.config.data.log_path, &query) {
                app.logs = results;
                app.is_search_result = true;
                app.logs_state.select(Some(0));
            }
        }
        app.transition_to(InputMode::Navigate);
    } else {
        app.textarea.input(key);
    }
}

fn handle_editing_mode(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.composer.cancel) {
        app.editing_entry = None;
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.composer.clear) {
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Editing);
    } else if key_match(&key, &app.config.keybindings.composer.newline) {
        app.textarea.insert_newline();
    } else if key_match(&key, &app.config.keybindings.composer.submit) {
        let lines = app.textarea.lines().to_vec();
        let is_empty = lines.iter().all(|l| l.trim().is_empty());

        if let Some(editing) = app.editing_entry.take() {
            let mut new_lines: Vec<String> = Vec::new();
            if !is_empty {
                let timestamp_prefix = if editing.timestamp_prefix.is_empty() {
                    format!("[{}] ", Local::now().format("%H:%M:%S"))
                } else {
                    editing.timestamp_prefix
                };

                let mut it = lines.into_iter();
                let first = it.next().unwrap_or_default();
                new_lines.push(format!("{timestamp_prefix}{first}"));
                new_lines.extend(it);
            }

            if let Err(e) = storage::replace_entry_lines(
                &editing.file_path,
                editing.start_line,
                editing.end_line,
                &new_lines,
            ) {
                eprintln!("Error updating entry: {}", e);
            }
            app.update_logs();
        } else {
            let input = lines.join("\n");
            if !input.trim().is_empty() {
                if let Err(e) = storage::append_entry(&app.config.data.log_path, &input) {
                    eprintln!("Error saving: {}", e);
                }
                app.update_logs();
            }
        }

        // 텍스트 영역 초기화
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Editing);
    } else {
        app.textarea.input(key);
    }
}

fn handle_path_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        // 폴더 열기
        // 절대 경로 변환 시도
        let path_to_open = if let Ok(abs_path) = std::fs::canonicalize(&app.config.data.log_path) {
            abs_path
        } else {
            // 실패 시 상대 경로라도 시도
            std::path::PathBuf::from(&app.config.data.log_path)
        };

        if let Err(e) = open::that(path_to_open) {
            eprintln!("Failed to open folder: {}", e);
        }

        app.show_path_popup = false;
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.popup.cancel) {
        app.show_path_popup = false;
        app.transition_to(InputMode::Navigate);
    }
}
