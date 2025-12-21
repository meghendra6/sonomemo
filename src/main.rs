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
use tui_textarea::CursorMove;

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();

    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture,)?;

    // Keyboard enhancement flags may fail on unsupported terminals (e.g., Windows Legacy Console).
    // Errors are ignored as they don't affect app functionality.
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
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
        check_timers(app);

        terminal.draw(|f| ui::ui(f, app))?;

        // Block all input during pomodoro completion alert (forces break/attention)
        if app.pomodoro_alert_expiry.is_some() {
            if event::poll(std::time::Duration::from_millis(100))? {
                let _ = event::read();
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
    handle_day_rollover(app);

    if let Some(end_time) = app.pomodoro_end {
        if Local::now() >= end_time {
            app.pomodoro_end = None;
            app.pomodoro_start = None;

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
            app.pomodoro_alert_expiry = None;
            app.pomodoro_alert_message = None;
        }
    }

    if let Some(expiry) = app.toast_expiry {
        if Local::now() >= expiry {
            app.toast_expiry = None;
            app.toast_message = None;
        }
    }
}

fn handle_day_rollover(app: &mut App) {
    let today = Local::now().format("%Y-%m-%d").to_string();
    if today == app.active_date {
        return;
    }

    let prev_date = app.active_date.clone();

    // Policy: Pomodoro timers are in-memory only. On day change, running timers are reset.
    app.active_date = today;
    app.pomodoro_end = None;
    app.pomodoro_start = None;
    app.pomodoro_target = None;
    app.pomodoro_alert_expiry = None;
    app.pomodoro_alert_message = None;
    app.show_pomodoro_popup = false;
    app.pomodoro_pending_task = None;
    app.pomodoro_minutes_input.clear();

    // Day change invalidates search context in practice (different file set).
    app.is_search_result = false;
    app.last_search_query = None;

    let mut carried_tasks = 0usize;
    if !storage::is_carryover_done(&app.config.data.log_path).unwrap_or(false) {
        if let Ok(blocks) =
            storage::get_carryover_blocks_for_date(&app.config.data.log_path, &prev_date)
        {
            for block in blocks {
                carried_tasks += block.task_lines.len();
                let mut content = format!("⤴ Carryover from {}", block.from_date);
                if let Some(ctx) = block.context.as_deref() {
                    content.push_str(&format!("\n> {}", ctx));
                }
                if !block.task_lines.is_empty() {
                    content.push('\n');
                    content.push_str(&block.task_lines.join("\n"));
                }
                let _ = storage::append_entry(&app.config.data.log_path, &content);
            }
            let _ = storage::mark_carryover_done(&app.config.data.log_path);
        }
    }

    app.update_logs();
    if carried_tasks > 0 {
        app.toast(format!(
            "New day detected: carried over {} unfinished tasks from {}.",
            carried_tasks, prev_date
        ));
    } else {
        app.toast("New day detected: refreshed logs/tasks and reset pomodoro.");
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
    if app.show_help_popup {
        if key.code == KeyCode::Esc || key_match(&key, &app.config.keybindings.global.help) {
            app.show_help_popup = false;
        }
        return true;
    }

    if app.show_discard_popup {
        handle_discard_popup(app, key);
        return true;
    }
    if app.show_delete_entry_popup {
        handle_delete_entry_popup(app, key);
        return true;
    }

    if app.show_pomodoro_popup {
        handle_pomodoro_popup(app, key);
        return true;
    }

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
        // Close on any key press
        app.show_activity_popup = false;
        return true;
    }
    if app.show_path_popup {
        handle_path_popup(app, key);
        return true;
    }
    false
}

fn handle_discard_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        app.editing_entry = None;
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Navigate);
        app.show_discard_popup = false;
    } else if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_discard_popup = false;
    }
}

fn handle_delete_entry_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        if let Some(entry) = app.delete_entry_target.take() {
            if storage::delete_entry_lines(&entry.file_path, entry.line_number, entry.end_line)
                .is_ok()
            {
                app.update_logs();
                app.toast("Entry deleted.");
            } else {
                app.toast("Failed to delete entry.");
            }
        } else {
            app.toast("No entry selected.");
        }
        app.show_delete_entry_popup = false;
    } else if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_delete_entry_popup = false;
        app.delete_entry_target = None;
    }
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
                    app.last_search_query = Some(query);
                    app.search_highlight_query = app.last_search_query.clone();
                    app.search_highlight_ready_at = Some(Local::now() + Duration::milliseconds(150));
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
    if key_match(&key, &app.config.keybindings.global.help) {
        app.show_help_popup = true;
    } else if key_match(&key, &app.config.keybindings.global.tags) {
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
        if let Some(i) = app.logs_state.selected() {
            if i < app.logs.len() {
                let entry = app.logs[i].clone();
                app.start_edit_entry(&entry);
            }
        }
    } else if key_match(&key, &app.config.keybindings.timeline.delete_entry) {
        if app.navigate_focus == models::NavigateFocus::Timeline {
            if let Some(i) = app.logs_state.selected() {
                if i < app.logs.len() {
                    app.delete_entry_target = Some(app.logs[i].clone());
                    app.show_delete_entry_popup = true;
                }
            } else {
                app.toast("No entry selected.");
            }
        } else {
            app.toast("Delete in Timeline.");
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
    } else if key.code == KeyCode::Esc {
        if app.is_search_result {
            app.last_search_query = None;
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
        open_or_toggle_pomodoro_for_selected_task(app);
    } else if key_match(&key, &app.config.keybindings.global.pomodoro) {
        open_or_toggle_pomodoro_for_selected_task(app);
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
        app.last_search_query = None;
        app.search_highlight_query = None;
        app.search_highlight_ready_at = None;
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.search.clear) {
        app.textarea = tui_textarea::TextArea::default();
        app.search_highlight_query = None;
        app.search_highlight_ready_at = None;
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
            app.last_search_query = Some(query.clone());
            app.search_highlight_query = Some(query.clone());
            app.search_highlight_ready_at = Some(Local::now() + Duration::milliseconds(150));
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
        if composer_has_unsaved_input(app) {
            app.show_discard_popup = true;
        } else {
            app.editing_entry = None;
            app.textarea = tui_textarea::TextArea::default();
            app.transition_to(InputMode::Navigate);
        }
    } else if key_match(&key, &app.config.keybindings.composer.clear) {
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Editing);
    } else if key_match(&key, &app.config.keybindings.composer.indent) {
        if !indent_or_outdent_list_line(&mut app.textarea, true) {
            let _ = app.textarea.insert_tab();
            app.composer_dirty = true;
        } else {
            app.composer_dirty = true;
        }
    } else if key_match(&key, &app.config.keybindings.composer.outdent) {
        if indent_or_outdent_list_line(&mut app.textarea, false) {
            app.composer_dirty = true;
        }
    } else if key_match(&key, &app.config.keybindings.composer.newline) {
        insert_newline_with_auto_indent(&mut app.textarea);
        app.composer_dirty = true;
    } else if key_match(&key, &app.config.keybindings.composer.submit) {
        let lines = app.textarea.lines().to_vec();
        let is_empty = lines.iter().all(|l| l.trim().is_empty());

        if let Some(editing) = app.editing_entry.take() {
            let selection_hint = (editing.file_path.clone(), editing.start_line);
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
            if editing.from_search {
                if let Some(query) = editing.search_query.as_deref() {
                    app.last_search_query = Some(query.to_string());
                    refresh_search_results(app, query);
                    if let Some(i) = app.logs.iter().position(|e| {
                        e.file_path == selection_hint.0 && e.line_number == selection_hint.1
                    }) {
                        app.logs_state.select(Some(i));
                    }
                } else {
                    app.last_search_query = None;
                    app.update_logs();
                }
            } else {
                app.update_logs();
            }
        } else {
            let input = lines.join("\n");
            if !input.trim().is_empty() {
                if let Err(e) = storage::append_entry(&app.config.data.log_path, &input) {
                    eprintln!("Error saving: {}", e);
                }
                app.update_logs();
            }
        }

        // Reset textarea
        app.textarea = tui_textarea::TextArea::default();
        app.composer_dirty = false;
        app.transition_to(InputMode::Navigate);
    } else {
        if app.textarea.input(key) {
            app.composer_dirty = true;
        }
    }
}

fn composer_has_unsaved_input(app: &App) -> bool {
    if app.editing_entry.is_some() {
        return true;
    }
    app.textarea
        .lines()
        .iter()
        .any(|line| !line.trim().is_empty())
}

fn refresh_search_results(app: &mut App, query: &str) {
    if let Ok(results) = storage::search_entries(&app.config.data.log_path, query) {
        app.logs = results;
        app.is_search_result = true;
        app.logs_state.select(Some(0));
        app.search_highlight_query = Some(query.to_string());
        app.search_highlight_ready_at = Some(Local::now() + Duration::milliseconds(150));
    }
}

fn open_or_toggle_pomodoro_for_selected_task(app: &mut App) {
    app.navigate_focus = models::NavigateFocus::Tasks;

    let Some(i) = app.tasks_state.selected() else {
        app.toast("No task selected.");
        return;
    };
    if i >= app.tasks.len() {
        app.toast("No task selected.");
        return;
    }

    let task = app.tasks[i].clone();

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
            app.pomodoro_start = None;
            app.pomodoro_target = None;
            app.toast("Pomodoro stopped.");
            return;
        }
    }

    app.pomodoro_pending_task = Some(task);
    app.pomodoro_minutes_input = app.config.pomodoro.work_minutes.to_string();
    app.show_pomodoro_popup = true;
}

fn handle_pomodoro_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_pomodoro_popup = false;
        app.pomodoro_pending_task = None;
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.confirm) {
        let task = match app.pomodoro_pending_task.take() {
            Some(t) => t,
            None => {
                app.show_pomodoro_popup = false;
                app.toast("No task selected.");
                return;
            }
        };

        let default_mins = app.config.pomodoro.work_minutes as i64;
        let mins = app
            .pomodoro_minutes_input
            .trim()
            .parse::<i64>()
            .ok()
            .unwrap_or(default_mins)
            .clamp(1, 600);

        let now = Local::now();
        app.pomodoro_start = Some(now);
        app.pomodoro_end = Some(now + Duration::minutes(mins));
        app.pomodoro_target = Some(models::PomodoroTarget::Task {
            text: task.text.clone(),
            file_path: task.file_path.clone(),
            line_number: task.line_number,
        });
        app.pomodoro_alert_expiry = None;
        app.pomodoro_alert_message = None;
        app.show_pomodoro_popup = false;
        app.toast(format!("Pomodoro started: {}m · {}", mins, task.text));
        return;
    }

    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() => {
            app.pomodoro_minutes_input.push(c);
        }
        KeyCode::Backspace => {
            app.pomodoro_minutes_input.pop();
        }
        _ => {}
    }
}

fn insert_newline_with_auto_indent(textarea: &mut tui_textarea::TextArea) {
    let (row, _) = textarea.cursor();
    let current_line = textarea.lines().get(row).map(|s| s.as_str()).unwrap_or("");

    let prefix = list_continuation_prefix(current_line);
    textarea.insert_newline();
    if !prefix.is_empty() {
        textarea.insert_str(prefix);
    }
}

fn indent_or_outdent_list_line(textarea: &mut tui_textarea::TextArea, indent: bool) -> bool {
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

fn list_continuation_prefix(line: &str) -> String {
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

fn handle_path_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        // Try to open the log directory
        let path_to_open = if let Ok(abs_path) = std::fs::canonicalize(&app.config.data.log_path) {
            abs_path
        } else {
            // Fallback to relative path if canonicalize fails
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
