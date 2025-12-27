use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
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

use crate::config::{EditorStyle, ThemePreset, config_path, key_match};
use crate::models::split_timestamp_line;
use app::{App, PendingEditCommand};
use chrono::{Duration, Local};
use models::{EditorMode, InputMode, Mood, VisualKind};
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

            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                handle_key_input(app, key);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn check_timers(app: &mut App) {
    handle_day_rollover(app);

    if let Some(end_time) = app.pomodoro_end
        && Local::now() >= end_time
    {
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

    if let Some(expiry) = app.pomodoro_alert_expiry
        && Local::now() >= expiry
    {
        app.pomodoro_alert_expiry = None;
        app.pomodoro_alert_message = None;
    }

    if let Some(expiry) = app.toast_expiry
        && Local::now() >= expiry
    {
        app.toast_expiry = None;
        app.toast_message = None;
    }

    if let Some(expiry) = app.visual_hint_expiry
        && Local::now() >= expiry
    {
        app.clear_visual_hint();
    }
}

fn handle_day_rollover(app: &mut App) {
    let today = Local::now().format("%Y-%m-%d").to_string();
    if today == app.active_date {
        return;
    }

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
    if !storage::is_carryover_done(&app.config.data.log_path).unwrap_or(false)
        && let Ok(tasks) =
            storage::collect_carryover_tasks(&app.config.data.log_path, &app.active_date)
    {
        for task in &tasks {
            let _ = storage::append_entry(&app.config.data.log_path, task);
        }
        carried_tasks = tasks.len();
        let _ = storage::mark_carryover_done(&app.config.data.log_path);
    }

    app.update_logs();
    if carried_tasks > 0 {
        app.toast(format!(
            "New day detected: carried over {} unfinished tasks.",
            carried_tasks
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
    if app.show_theme_popup {
        handle_theme_switcher_popup(app, key);
        return true;
    }
    if app.show_editor_style_popup {
        handle_editor_style_popup(app, key);
        return true;
    }
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
        app.transition_to(InputMode::Navigate);
    }
}

fn check_carryover(app: &mut App) {
    let already_checked = storage::is_carryover_done(&app.config.data.log_path).unwrap_or(false);
    if !already_checked {
        if let Ok(todos) =
            storage::collect_carryover_tasks(&app.config.data.log_path, &app.active_date)
        {
            if !todos.is_empty() {
                app.pending_todos = todos;
                app.show_todo_popup = true;
            } else {
                app.transition_to(InputMode::Navigate);
                let _ = storage::mark_carryover_done(&app.config.data.log_path);
            }
        } else {
            app.transition_to(InputMode::Navigate);
            let _ = storage::mark_carryover_done(&app.config.data.log_path);
        }
    } else {
        app.transition_to(InputMode::Navigate);
    }
}

fn handle_todo_popup(app: &mut App, key: event::KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        for todo in &app.pending_todos {
            let _ = storage::append_entry(&app.config.data.log_path, todo);
        }
        app.update_logs();
        app.show_todo_popup = false;
        app.transition_to(InputMode::Navigate);
        let _ = storage::mark_carryover_done(&app.config.data.log_path);
    } else if key_match(&key, &app.config.keybindings.popup.cancel) {
        app.show_todo_popup = false;
        app.transition_to(InputMode::Navigate);
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
        if let Some(i) = app.tag_list_state.selected()
            && i < app.tags.len()
        {
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
        app.show_tag_popup = false;
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.popup.cancel) {
        app.show_tag_popup = false;
        app.transition_to(InputMode::Navigate);
    }
}

fn open_theme_switcher(app: &mut App) {
    if app.show_theme_popup {
        return;
    }

    let current = app
        .config
        .ui
        .theme_preset
        .as_deref()
        .and_then(ThemePreset::from_name)
        .unwrap_or_else(ThemePreset::default);
    let selected = ThemePreset::all()
        .iter()
        .position(|preset| *preset == current)
        .unwrap_or(0);
    app.theme_list_state.select(Some(selected));
    app.theme_preview_backup = Some(app.config.theme.clone());
    app.show_theme_popup = true;
}

fn handle_theme_switcher_popup(app: &mut App, key: event::KeyEvent) {
    let presets = ThemePreset::all();
    if presets.is_empty() {
        app.show_theme_popup = false;
        return;
    }

    let selected = app.theme_list_state.selected().unwrap_or(0);
    if key_match(&key, &app.config.keybindings.popup.up) {
        let next = if selected == 0 {
            presets.len() - 1
        } else {
            selected - 1
        };
        app.theme_list_state.select(Some(next));
        app.config.theme = config::Theme::preset(presets[next]);
    } else if key_match(&key, &app.config.keybindings.popup.down) {
        let next = if selected >= presets.len() - 1 {
            0
        } else {
            selected + 1
        };
        app.theme_list_state.select(Some(next));
        app.config.theme = config::Theme::preset(presets[next]);
    } else if key_match(&key, &app.config.keybindings.popup.confirm) {
        let index = app.theme_list_state.selected().unwrap_or(0);
        let preset = presets[index];
        app.config.ui.theme_preset = Some(preset.name().to_string());
        app.config.theme = config::Theme::preset(preset);
        match app.config.save_to_path(&config_path()) {
            Ok(_) => app.toast(format!("Theme set to {}.", preset.name())),
            Err(_) => app.toast("Failed to save theme preset."),
        }
        app.theme_preview_backup = None;
        app.show_theme_popup = false;
    } else if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        if let Some(previous) = app.theme_preview_backup.take() {
            app.config.theme = previous;
        }
        app.show_theme_popup = false;
    }
}

fn open_editor_style_switcher(app: &mut App) {
    if app.show_editor_style_popup {
        return;
    }

    let current = app
        .config
        .ui
        .editor_style
        .as_deref()
        .and_then(EditorStyle::from_name)
        .unwrap_or_else(EditorStyle::default);
    let selected = EditorStyle::all()
        .iter()
        .position(|style| *style == current)
        .unwrap_or(0);
    app.editor_style_list_state.select(Some(selected));
    app.show_editor_style_popup = true;
}

fn handle_editor_style_popup(app: &mut App, key: event::KeyEvent) {
    let styles = EditorStyle::all();
    if styles.is_empty() {
        app.show_editor_style_popup = false;
        return;
    }

    let selected = app.editor_style_list_state.selected().unwrap_or(0);
    if key_match(&key, &app.config.keybindings.popup.up) {
        let next = if selected == 0 {
            styles.len() - 1
        } else {
            selected - 1
        };
        app.editor_style_list_state.select(Some(next));
    } else if key_match(&key, &app.config.keybindings.popup.down) {
        let next = if selected >= styles.len() - 1 {
            0
        } else {
            selected + 1
        };
        app.editor_style_list_state.select(Some(next));
    } else if key_match(&key, &app.config.keybindings.popup.confirm) {
        let index = app.editor_style_list_state.selected().unwrap_or(0);
        let style = styles[index];
        app.config.ui.editor_style = Some(style.name().to_string());
        match app.config.save_to_path(&config_path()) {
            Ok(_) => app.toast(format!("Editor style set to {}.", style.name())),
            Err(_) => app.toast("Failed to save editor style."),
        }
        app.show_editor_style_popup = false;
    } else if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_editor_style_popup = false;
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
    } else if key_match(&key, &app.config.keybindings.global.focus_next)
        || key_match(&key, &app.config.keybindings.global.focus_prev)
    {
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
        if let Some(i) = app.logs_state.selected()
            && i < app.logs.len()
        {
            let entry = app.logs[i].clone();
            app.start_edit_entry(&entry);
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
        if let Some(i) = app.tasks_state.selected()
            && i < app.tasks.len()
        {
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
    } else if key.code == KeyCode::Esc {
        if app.is_search_result {
            app.last_search_query = None;
            app.update_logs();
        }
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.toggle_todo)
    {
        if let Some(i) = app.logs_state.selected()
            && i < app.logs.len()
        {
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
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.toggle)
    {
        if let Some(i) = app.tasks_state.selected()
            && i < app.tasks.len()
        {
            let task = app.tasks[i].clone();
            if let Ok(completed) = storage::complete_task_chain(&app.config.data.log_path, &task)
                && task.carryover_from.is_some()
                && completed > 0
            {
                let message = if completed == 1 {
                    "Completed 1 carry-over task".to_string()
                } else {
                    format!("Completed {} carry-over tasks", completed)
                };
                app.toast(message);
            }
            app.update_logs();
        }
    } else if (app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.start_pomodoro))
        || key_match(&key, &app.config.keybindings.global.pomodoro)
    {
        open_or_toggle_pomodoro_for_selected_task(app);
    } else if key_match(&key, &app.config.keybindings.global.activity) {
        if let Ok(data) = storage::get_activity_stats(&app.config.data.log_path) {
            app.activity_data = data;
            app.show_activity_popup = true;
        }
    } else if key_match(&key, &app.config.keybindings.global.log_dir) {
        app.show_path_popup = true;
    } else if key_match(&key, &app.config.keybindings.global.theme_switcher) {
        open_theme_switcher(app);
    } else if key_match(&key, &app.config.keybindings.global.editor_style_switcher) {
        open_editor_style_switcher(app);
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
    if key_match(&key, &app.config.keybindings.composer.submit) {
        app.commit_insert_group();
        submit_composer(app);
        return;
    }

    // Simple mode: no Vim keybindings, just forward to textarea
    if !app.is_vim_mode() {
        if key_match(&key, &app.config.keybindings.composer.clear) {
            app.textarea = tui_textarea::TextArea::default();
            app.transition_to(InputMode::Editing);
            return;
        }

        if key_match(&key, &app.config.keybindings.composer.cancel) {
            cancel_composer(app);
            return;
        }

        if key_match(&key, &app.config.keybindings.composer.indent) {
            indent_or_outdent_list_line(&mut app.textarea, true);
            return;
        }

        if key_match(&key, &app.config.keybindings.composer.outdent) {
            indent_or_outdent_list_line(&mut app.textarea, false);
            return;
        }

        if key_match(&key, &app.config.keybindings.composer.newline) {
            insert_newline_with_auto_indent(&mut app.textarea);
            return;
        }

        // Forward all other keys to textarea
        if app.textarea.input(key) {
            app.composer_dirty = true;
        }
        return;
    }

    // Vim mode handling below
    if app.visual_hint_active && matches!(app.editor_mode, EditorMode::Visual(_)) {
        app.clear_visual_hint();
    }

    if matches!(app.editor_mode, EditorMode::Insert | EditorMode::Visual(_))
        && key.code == KeyCode::Esc
        && !key.modifiers.contains(KeyModifiers::CONTROL)
    {
        match app.editor_mode {
            EditorMode::Insert => exit_insert_mode(app),
            EditorMode::Visual(_) => exit_visual_mode(app),
            EditorMode::Normal => {}
        }
        return;
    }

    if key_match(&key, &app.config.keybindings.composer.clear) {
        app.commit_insert_group();
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Editing);
        return;
    }

    if key_match(&key, &app.config.keybindings.composer.cancel)
        && matches!(app.editor_mode, EditorMode::Normal)
    {
        app.commit_insert_group();
        cancel_composer(app);
        return;
    }

    match app.editor_mode {
        EditorMode::Normal => handle_editor_normal(app, key),
        EditorMode::Insert => handle_editor_insert(app, key),
        EditorMode::Visual(kind) => handle_editor_visual(app, key, kind),
    }
}

fn cancel_composer(app: &mut App) {
    if composer_has_unsaved_input(app) {
        app.show_discard_popup = true;
    } else {
        app.editing_entry = None;
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Navigate);
    }
}

fn submit_composer(app: &mut App) {
    let lines = app.textarea.lines().to_vec();
    let is_empty = lines.iter().all(|l| l.trim().is_empty());

    if let Some(editing) = app.editing_entry.take() {
        let selection_hint = (editing.file_path.clone(), editing.start_line);
        let mut new_lines: Vec<String> = Vec::new();
        if !is_empty {
            let heading_time = if editing.timestamp_prefix.is_empty() {
                Local::now().format("%H:%M:%S").to_string()
            } else if let Some((prefix, _)) = split_timestamp_line(&editing.timestamp_prefix) {
                prefix
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string()
            } else {
                Local::now().format("%H:%M:%S").to_string()
            };

            new_lines.push(format!("## [{heading_time}]"));
            new_lines.extend(lines);
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
}

fn handle_editor_insert(app: &mut App, key: event::KeyEvent) {
    if key.code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if delete_to_line_start(app) {
            app.set_yank_buffer(app.textarea.yank_text());
            app.mark_insert_modified();
            app.composer_dirty = true;
        }
        return;
    }

    if key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if delete_previous_word(app) {
            app.set_yank_buffer(app.textarea.yank_text());
            app.mark_insert_modified();
            app.composer_dirty = true;
        }
        return;
    }

    if key_match(&key, &app.config.keybindings.composer.indent) {
        let modified = if indent_or_outdent_list_line(&mut app.textarea, true) {
            true
        } else {
            app.textarea.insert_tab()
        };
        if modified {
            app.mark_insert_modified();
            app.composer_dirty = true;
        }
        return;
    }

    if key_match(&key, &app.config.keybindings.composer.outdent) {
        if indent_or_outdent_list_line(&mut app.textarea, false) {
            app.mark_insert_modified();
            app.composer_dirty = true;
        }
        return;
    }

    if key_match(&key, &app.config.keybindings.composer.newline) {
        insert_newline_with_auto_indent(&mut app.textarea);
        app.mark_insert_modified();
        app.composer_dirty = true;
        return;
    }

    if app.textarea.input(key) {
        app.mark_insert_modified();
        app.composer_dirty = true;
    }
}

fn handle_editor_normal(app: &mut App, key: event::KeyEvent) {
    if handle_pending_command(app, key) {
        return;
    }

    if let KeyCode::Char(c) = key.code
        && c.is_ascii_digit()
        && c != '0'
        && !key.modifiers.contains(KeyModifiers::CONTROL)
    {
        let digit = c.to_digit(10).unwrap_or(0) as usize;
        app.pending_count = app.pending_count.saturating_mul(10).saturating_add(digit);
        return;
    }

    if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.editor_redo() {
            app.composer_dirty = true;
            clamp_cursor_for_normal(app);
        }
        return;
    }

    if key.code == KeyCode::Char('u') && !key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.editor_undo() {
            app.composer_dirty = true;
            clamp_cursor_for_normal(app);
        }
        return;
    }

    match key.code {
        KeyCode::Char('i') => {
            enter_insert_mode(app);
        }
        KeyCode::Char('a') => {
            move_cursor_after(app);
            enter_insert_mode(app);
        }
        KeyCode::Char('o') => {
            app.begin_insert_group();
            open_line_below(app);
            app.mark_insert_modified();
            set_insert_mode(app);
        }
        KeyCode::Char('O') => {
            app.begin_insert_group();
            open_line_above(app);
            app.mark_insert_modified();
            set_insert_mode(app);
        }
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            enter_visual_mode(app, VisualKind::Block);
        }
        KeyCode::Char('v') => {
            enter_visual_mode(app, VisualKind::Char);
        }
        KeyCode::Char('V') => {
            enter_visual_mode(app, VisualKind::Line);
        }
        KeyCode::Char('h') => {
            let count = take_count_or_one(app);
            move_left(app, count);
        }
        KeyCode::Char('j') => {
            let count = take_count_or_one(app);
            move_down(app, count);
        }
        KeyCode::Char('k') => {
            let count = take_count_or_one(app);
            move_up(app, count);
        }
        KeyCode::Char('l') => {
            let count = take_count_or_one(app);
            move_right(app, count);
        }
        KeyCode::Char('w') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::NextStart, false, count);
        }
        KeyCode::Char('W') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::NextStart, true, count);
        }
        KeyCode::Char('b') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::PrevStart, false, count);
        }
        KeyCode::Char('B') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::PrevStart, true, count);
        }
        KeyCode::Char('e') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::End, false, count);
        }
        KeyCode::Char('E') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::End, true, count);
        }
        KeyCode::Char('0') => {
            app.pending_count = 0;
            move_line_start(app);
        }
        KeyCode::Char('$') => {
            let count = take_count_or_one(app);
            move_line_end_with_count(app, count);
        }
        KeyCode::Char('g') => {
            app.pending_command = Some(PendingEditCommand::GoToTop);
        }
        KeyCode::Char('G') => {
            let count = take_count(app);
            move_doc_end_with_count(app, count);
        }
        KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.pending_command = Some(PendingEditCommand::Delete);
        }
        KeyCode::Char('y') => {
            app.pending_command = Some(PendingEditCommand::Yank);
        }
        KeyCode::Char('c') => {
            app.pending_command = Some(PendingEditCommand::Change);
        }
        KeyCode::Char('x') => {
            let count = take_count_or_one(app);
            if let Some(obj) = resolve_char_object(app, count) {
                apply_operator(app, Operator::Delete, obj);
            }
        }
        KeyCode::Char('X') => {
            let count = take_count_or_one(app);
            if let Some(obj) = resolve_char_before_object(app, count) {
                apply_operator(app, Operator::Delete, obj);
            }
        }
        KeyCode::Char('p') => {
            let count = take_count_or_one(app);
            if !app.yank_buffer.is_empty() {
                app.record_undo_snapshot();
                app.textarea.set_yank_text(app.yank_buffer.clone());
                for _ in 0..count {
                    let _ = app.textarea.paste();
                }
                app.composer_dirty = true;
                clamp_cursor_for_normal(app);
            }
        }
        KeyCode::Char('P') => {
            let count = take_count_or_one(app);
            paste_before_cursor(app, count);
        }
        KeyCode::Char('D') => {
            let count = take_count_or_one(app);
            if let Some(obj) = resolve_line_end_object(app, count) {
                apply_operator(app, Operator::Delete, obj);
            }
        }
        KeyCode::Char('C') => {
            let count = take_count_or_one(app);
            if let Some(obj) = resolve_line_end_object(app, count) {
                apply_operator(app, Operator::Change, obj);
            }
        }
        KeyCode::Char('S') => {
            let count = take_count_or_one(app);
            if let Some(obj) = resolve_line_object(app, count) {
                apply_operator(app, Operator::Change, obj);
            }
        }
        KeyCode::Char('s') => {
            let count = take_count_or_one(app);
            if let Some(obj) = resolve_char_object(app, count) {
                apply_operator(app, Operator::Change, obj);
            }
        }
        KeyCode::Char('r') => {
            app.pending_command = Some(PendingEditCommand::Replace);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_half_page_down(app, count);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_half_page_up(app, count);
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_page_down(app, count);
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_page_up(app, count);
        }
        _ => {
            app.pending_count = 0;
            app.pending_command = None;
        }
    }
}

fn handle_editor_visual(app: &mut App, key: event::KeyEvent, kind: VisualKind) {
    if let Some(PendingEditCommand::GoToTop) = app.pending_command {
        if key.code == KeyCode::Char('g') {
            app.pending_command = None;
            let count = take_count(app);
            move_doc_start_with_count(app, count);
            return;
        }
        app.pending_command = None;
    }

    if let KeyCode::Char(c) = key.code
        && c.is_ascii_digit()
        && c != '0'
        && !key.modifiers.contains(KeyModifiers::CONTROL)
    {
        let digit = c.to_digit(10).unwrap_or(0) as usize;
        app.pending_count = app.pending_count.saturating_mul(10).saturating_add(digit);
        return;
    }

    match key.code {
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_half_page_down(app, count);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_half_page_up(app, count);
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_page_down(app, count);
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let count = take_count_or_one(app);
            move_page_up(app, count);
        }
        KeyCode::Char('h') => {
            let count = take_count_or_one(app);
            move_left(app, count);
        }
        KeyCode::Char('j') => {
            let count = take_count_or_one(app);
            move_down(app, count);
        }
        KeyCode::Char('k') => {
            let count = take_count_or_one(app);
            move_up(app, count);
        }
        KeyCode::Char('l') => {
            let count = take_count_or_one(app);
            move_right(app, count);
        }
        KeyCode::Char('w') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::NextStart, false, count);
        }
        KeyCode::Char('W') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::NextStart, true, count);
        }
        KeyCode::Char('b') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::PrevStart, false, count);
        }
        KeyCode::Char('B') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::PrevStart, true, count);
        }
        KeyCode::Char('e') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::End, false, count);
        }
        KeyCode::Char('E') => {
            let count = take_count_or_one(app);
            move_word(app, WordMotion::End, true, count);
        }
        KeyCode::Char('0') => {
            app.pending_count = 0;
            move_line_start(app);
        }
        KeyCode::Char('$') => {
            let count = take_count_or_one(app);
            move_line_end_with_count(app, count);
        }
        KeyCode::Char('g') => {
            app.pending_command = Some(PendingEditCommand::GoToTop);
        }
        KeyCode::Char('G') => {
            let count = take_count(app);
            move_doc_end_with_count(app, count);
        }
        KeyCode::Char('y') => {
            if let Some(obj) = resolve_visual_text_object(app, kind) {
                apply_operator(app, Operator::Yank, obj);
            }
            exit_visual_mode(app);
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            if let Some(obj) = resolve_visual_text_object(app, kind) {
                apply_operator(app, Operator::Delete, obj);
            }
            exit_visual_mode(app);
        }
        _ => {}
    }
}

fn enter_insert_mode(app: &mut App) {
    app.begin_insert_group();
    set_insert_mode(app);
}

fn set_insert_mode(app: &mut App) {
    app.editor_mode = EditorMode::Insert;
    app.pending_command = None;
    app.pending_count = 0;
    app.visual_anchor = None;
}

fn exit_insert_mode(app: &mut App) {
    app.commit_insert_group();
    app.editor_mode = EditorMode::Normal;
    app.pending_command = None;
    app.pending_count = 0;
    app.visual_anchor = None;
    clamp_cursor_for_normal(app);
}

fn enter_visual_mode(app: &mut App, kind: VisualKind) {
    app.editor_mode = EditorMode::Visual(kind);
    app.visual_anchor = Some(app.textarea.cursor());
    app.pending_command = None;
    app.pending_count = 0;
    let kind_label = match kind {
        VisualKind::Char => "CHAR",
        VisualKind::Line => "LINE",
        VisualKind::Block => "BLOCK",
    };
    app.show_visual_hint(format!(
        "VISUAL ({kind_label}): hjkl/w b e extend · y yank · d delete · Esc normal · ? help"
    ));
}

fn exit_visual_mode(app: &mut App) {
    app.editor_mode = EditorMode::Normal;
    app.visual_anchor = None;
    app.pending_command = None;
    app.pending_count = 0;
    clamp_cursor_for_normal(app);
}

fn handle_pending_command(app: &mut App, key: event::KeyEvent) -> bool {
    let Some(pending) = app.pending_command else {
        return false;
    };

    match pending {
        PendingEditCommand::Delete => {
            if key.code == KeyCode::Char('d') {
                let count = take_count_or_one(app);
                if let Some(obj) = resolve_line_object(app, count) {
                    apply_operator(app, Operator::Delete, obj);
                }
                app.pending_command = None;
                return true;
            }
        }
        PendingEditCommand::Yank => {
            if key.code == KeyCode::Char('y') {
                let count = take_count_or_one(app);
                if let Some(obj) = resolve_line_object(app, count) {
                    apply_operator(app, Operator::Yank, obj);
                }
                app.pending_command = None;
                return true;
            }
        }
        PendingEditCommand::Change => {
            if key.code == KeyCode::Char('c') {
                let count = take_count_or_one(app);
                if let Some(obj) = resolve_line_object(app, count) {
                    apply_operator(app, Operator::Change, obj);
                }
                app.pending_command = None;
                return true;
            }
        }
        PendingEditCommand::GoToTop => {
            if key.code == KeyCode::Char('g') {
                app.pending_command = None;
                let count = take_count(app);
                move_doc_start_with_count(app, count);
                return true;
            }
        }
        PendingEditCommand::Replace => {
            if let KeyCode::Char(c) = key.code
                && !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                app.pending_command = None;
                app.pending_count = 0;
                app.record_undo_snapshot();
                if replace_char(app, c) {
                    app.composer_dirty = true;
                }
                return true;
            }
        }
    }

    app.pending_command = None;
    app.pending_count = 0;
    false
}

fn take_count_or_one(app: &mut App) -> usize {
    let count = if app.pending_count > 0 {
        app.pending_count
    } else {
        1
    };
    app.pending_count = 0;
    count
}

fn take_count(app: &mut App) -> Option<usize> {
    if app.pending_count > 0 {
        let count = app.pending_count;
        app.pending_count = 0;
        Some(count)
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum WordMotion {
    NextStart,
    PrevStart,
    End,
}

#[derive(Clone, Copy)]
enum Operator {
    Delete,
    Yank,
    Change,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum TextObjectKind {
    Char,
    Word,
    Line,
}

#[derive(Clone, Copy)]
enum TextObject {
    Range {
        kind: TextObjectKind,
        start: (usize, usize),
        end: (usize, usize),
    },
    Block {
        start: (usize, usize),
        end: (usize, usize),
    },
}

fn move_left(app: &mut App, count: usize) {
    let (row, col) = app.textarea.cursor();
    let new_col = col.saturating_sub(count);
    app.textarea
        .move_cursor(CursorMove::Jump(row as u16, new_col as u16));
}

fn move_right(app: &mut App, count: usize) {
    let (row, col) = app.textarea.cursor();
    let line_len = current_line_len(app, row);
    if line_len == 0 {
        return;
    }
    let max_col = line_len.saturating_sub(1);
    let new_col = col.saturating_add(count).min(max_col);
    app.textarea
        .move_cursor(CursorMove::Jump(row as u16, new_col as u16));
}

fn move_up(app: &mut App, count: usize) {
    for _ in 0..count {
        app.textarea.move_cursor(CursorMove::Up);
    }
    clamp_cursor_for_normal(app);
}

fn move_down(app: &mut App, count: usize) {
    for _ in 0..count {
        app.textarea.move_cursor(CursorMove::Down);
    }
    clamp_cursor_for_normal(app);
}

fn move_word(app: &mut App, motion: WordMotion, big_word: bool, count: usize) {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return;
    }

    let mut pos = app.textarea.cursor();
    for _ in 0..count {
        let next = match motion {
            WordMotion::NextStart => next_word_start(lines, pos, big_word),
            WordMotion::PrevStart => prev_word_start(lines, pos, big_word),
            WordMotion::End => word_end(lines, pos, big_word),
        };
        pos = next;
    }

    app.textarea
        .move_cursor(CursorMove::Jump(pos.0 as u16, pos.1 as u16));
    if matches!(app.editor_mode, EditorMode::Normal | EditorMode::Visual(_)) {
        clamp_cursor_for_normal(app);
    }
}

fn apply_operator(app: &mut App, operator: Operator, object: TextObject) {
    let text = text_object_text(app, object);
    let is_linewise = object_is_line(object);
    let mut line_removed = false;
    let mut line_start_row = None;

    let modified = match operator {
        Operator::Yank => {
            app.set_yank_buffer_with_kind(text, is_linewise);
            return;
        }
        Operator::Delete => {
            app.record_undo_snapshot();
            match object {
                TextObject::Range { kind, start, end } => {
                    line_start_row = Some(start.0);
                    line_removed = line_object_removed(kind, start, end);
                    delete_range_object(app, kind, start, end)
                }
                TextObject::Block { start, end } => delete_block_object(app, start, end),
            }
        }
        Operator::Change => {
            app.begin_insert_group();
            match object {
                TextObject::Range { kind, start, end } => {
                    line_start_row = Some(start.0);
                    line_removed = line_object_removed(kind, start, end);
                    delete_range_object(app, kind, start, end)
                }
                TextObject::Block { start, end } => delete_block_object(app, start, end),
            }
        }
    };

    match operator {
        Operator::Delete => {
            app.set_yank_buffer_with_kind(text, is_linewise);
            if modified {
                app.composer_dirty = true;
            }
            if let Some(row) = line_start_row
                && object_is_line(object)
            {
                place_cursor_after_line_delete(app, row);
            } else {
                clamp_cursor_for_normal(app);
            }
        }
        Operator::Change => {
            app.set_yank_buffer_with_kind(text, is_linewise);
            if object_is_line(object) {
                if line_removed {
                    if let Some(row) = line_start_row {
                        insert_empty_line_at(app, row);
                    }
                } else if let Some(row) = line_start_row {
                    app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
                }
            }
            if modified {
                app.mark_insert_modified();
                app.composer_dirty = true;
            }
            set_insert_mode(app);
        }
        Operator::Yank => {}
    }
}

fn object_is_line(object: TextObject) -> bool {
    matches!(
        object,
        TextObject::Range {
            kind: TextObjectKind::Line,
            ..
        }
    )
}

fn line_object_removed(kind: TextObjectKind, start: (usize, usize), end: (usize, usize)) -> bool {
    kind == TextObjectKind::Line && (end.0 > start.0 || start.0 > 0)
}

fn resolve_line_object(app: &App, count: usize) -> Option<TextObject> {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return None;
    }
    let (row, _) = app.textarea.cursor();
    let start_row = row.min(lines.len().saturating_sub(1));
    let end_row_exclusive = start_row.saturating_add(count);
    let (start, end) = line_object_range(lines, start_row, end_row_exclusive);
    Some(TextObject::Range {
        kind: TextObjectKind::Line,
        start,
        end,
    })
}

fn resolve_char_object(app: &App, count: usize) -> Option<TextObject> {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return None;
    }
    let (row, col) = app.textarea.cursor();
    let len = line_len(lines, row);
    if len == 0 || col >= len {
        return None;
    }
    let end = advance_pos_by_chars(lines, (row, col), count);
    if end == (row, col) {
        return None;
    }
    Some(TextObject::Range {
        kind: TextObjectKind::Char,
        start: (row, col),
        end,
    })
}

fn resolve_line_end_object(app: &App, count: usize) -> Option<TextObject> {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return None;
    }
    let (row, col) = app.textarea.cursor();
    let target_row = row
        .saturating_add(count.saturating_sub(1))
        .min(lines.len().saturating_sub(1));
    let end_col = line_len(lines, target_row);
    if (row, col) == (target_row, end_col) {
        return None;
    }
    Some(TextObject::Range {
        kind: TextObjectKind::Char,
        start: (row, col),
        end: (target_row, end_col),
    })
}

fn resolve_char_before_object(app: &App, count: usize) -> Option<TextObject> {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return None;
    }
    let end = app.textarea.cursor();
    let mut start = end;
    let mut remaining = count;
    while remaining > 0 {
        let Some(prev) = retreat_pos_for_delete(lines, start) else {
            break;
        };
        start = prev;
        remaining -= 1;
    }
    if start == end {
        return None;
    }
    Some(TextObject::Range {
        kind: TextObjectKind::Char,
        start,
        end,
    })
}

fn resolve_visual_text_object(app: &App, kind: VisualKind) -> Option<TextObject> {
    let lines = app.textarea.lines();
    let anchor = app.visual_anchor?;
    let cursor = app.textarea.cursor();

    match kind {
        VisualKind::Char => {
            let (mut start, mut end) = ordered_positions(anchor, cursor);
            let start_len = line_len(lines, start.0);
            let end_len = line_len(lines, end.0);
            if start_len == 0 && end_len == 0 {
                return None;
            }
            start.1 = start.1.min(start_len);
            let end_col = end.1.saturating_add(1).min(end_len);
            end = (end.0, end_col);
            Some(TextObject::Range {
                kind: TextObjectKind::Char,
                start,
                end,
            })
        }
        VisualKind::Line => {
            let start_row = anchor.0.min(cursor.0);
            let end_row_exclusive = anchor.0.max(cursor.0).saturating_add(1);
            let (start, end) = line_object_range(lines, start_row, end_row_exclusive);
            Some(TextObject::Range {
                kind: TextObjectKind::Line,
                start,
                end,
            })
        }
        VisualKind::Block => {
            let row_start = anchor.0.min(cursor.0);
            let row_end = anchor.0.max(cursor.0);
            let col_start = anchor.1.min(cursor.1);
            let col_end = anchor.1.max(cursor.1);
            Some(TextObject::Block {
                start: (row_start, col_start),
                end: (row_end, col_end),
            })
        }
    }
}

fn text_object_text(app: &App, object: TextObject) -> String {
    let lines = app.textarea.lines();
    match object {
        TextObject::Range { start, end, .. } => extract_range_text(lines, start, end),
        TextObject::Block { start, end } => collect_block_object_text(lines, start, end),
    }
}

fn line_object_range(
    lines: &[String],
    start_row: usize,
    end_row_exclusive: usize,
) -> ((usize, usize), (usize, usize)) {
    let line_count = lines.len();
    if line_count == 0 {
        return ((0, 0), (0, 0));
    }
    let start_row = start_row.min(line_count - 1);
    if end_row_exclusive < line_count {
        ((start_row, 0), (end_row_exclusive, 0))
    } else {
        let last_row = line_count - 1;
        let end_col = line_len(lines, last_row);
        ((start_row, 0), (last_row, end_col))
    }
}

fn extract_range_text(lines: &[String], start: (usize, usize), end: (usize, usize)) -> String {
    if lines.is_empty() {
        return String::new();
    }
    if start.0 == end.0 {
        let line = lines.get(start.0).map(|s| s.as_str()).unwrap_or("");
        return slice_by_char(line, start.1, end.1);
    }

    let mut out = String::new();
    let first = lines.get(start.0).map(|s| s.as_str()).unwrap_or("");
    out.push_str(&slice_by_char(first, start.1, first.chars().count()));
    out.push('\n');
    for row in (start.0 + 1)..end.0 {
        let line = lines.get(row).map(|s| s.as_str()).unwrap_or("");
        out.push_str(line);
        out.push('\n');
    }
    let last = lines.get(end.0).map(|s| s.as_str()).unwrap_or("");
    out.push_str(&slice_by_char(last, 0, end.1));
    out
}

fn delete_range_object(
    app: &mut App,
    kind: TextObjectKind,
    start: (usize, usize),
    end: (usize, usize),
) -> bool {
    let lines = app.textarea.lines();
    let (delete_start, delete_end) =
        if kind == TextObjectKind::Line && start.0 == end.0 && start.0 > 0 {
            let prev_len = line_len(lines, start.0.saturating_sub(1));
            ((start.0 - 1, prev_len), end)
        } else {
            (start, end)
        };

    delete_range(app, delete_start, delete_end)
}

fn delete_range(app: &mut App, start: (usize, usize), end: (usize, usize)) -> bool {
    if start == end {
        return false;
    }
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    app.textarea
        .move_cursor(CursorMove::Jump(start.0 as u16, start.1 as u16));
    app.textarea.start_selection();
    app.textarea
        .move_cursor(CursorMove::Jump(end.0 as u16, end.1 as u16));
    app.textarea.cut()
}

fn place_cursor_after_line_delete(app: &mut App, deleted_row: usize) {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        app.textarea.move_cursor(CursorMove::Jump(0, 0));
        return;
    }
    let target_row = if deleted_row < lines.len() {
        deleted_row
    } else {
        lines.len().saturating_sub(1)
    };
    let col = first_non_blank_col(lines.get(target_row).map(|s| s.as_str()).unwrap_or(""));
    app.textarea
        .move_cursor(CursorMove::Jump(target_row as u16, col as u16));
}

fn first_non_blank_col(line: &str) -> usize {
    for (idx, ch) in line.chars().enumerate() {
        if !ch.is_whitespace() {
            return idx;
        }
    }
    0
}

fn insert_empty_line_at(app: &mut App, row: usize) {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        app.textarea.insert_newline();
        app.textarea.move_cursor(CursorMove::Jump(0, 0));
        return;
    }

    if row < lines.len() {
        app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
        app.textarea.insert_newline();
        app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
    } else {
        let last_row = lines.len().saturating_sub(1);
        let end_col = line_len(lines, last_row);
        app.textarea
            .move_cursor(CursorMove::Jump(last_row as u16, end_col as u16));
        app.textarea.insert_newline();
        app.textarea
            .move_cursor(CursorMove::Jump((last_row + 1) as u16, 0));
    }
}

fn ordered_positions(a: (usize, usize), b: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    if a <= b { (a, b) } else { (b, a) }
}

fn move_line_start(app: &mut App) {
    let (row, _) = app.textarea.cursor();
    app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
}

#[allow(dead_code)]
fn move_line_end(app: &mut App) {
    let (row, _) = app.textarea.cursor();
    let line_len = current_line_len(app, row);
    let new_col = if line_len == 0 {
        0
    } else {
        line_len.saturating_sub(1)
    };
    app.textarea
        .move_cursor(CursorMove::Jump(row as u16, new_col as u16));
}

fn move_line_end_with_count(app: &mut App, count: usize) {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return;
    }
    let (row, _) = app.textarea.cursor();
    let target_row = row
        .saturating_add(count.saturating_sub(1))
        .min(lines.len().saturating_sub(1));
    let col = line_end_cursor_col(lines, target_row);
    app.textarea
        .move_cursor(CursorMove::Jump(target_row as u16, col as u16));
}

fn move_doc_start(app: &mut App) {
    app.textarea.move_cursor(CursorMove::Jump(0, 0));
}

fn move_doc_end(app: &mut App) {
    let last_row = app.textarea.lines().len().saturating_sub(1);
    let line_len = current_line_len(app, last_row);
    let new_col = if line_len == 0 {
        0
    } else {
        line_len.saturating_sub(1)
    };
    app.textarea
        .move_cursor(CursorMove::Jump(last_row as u16, new_col as u16));
}

fn move_doc_start_with_count(app: &mut App, count: Option<usize>) {
    if let Some(count) = count {
        move_to_line_first_non_blank(app, count.saturating_sub(1));
    } else {
        move_doc_start(app);
    }
}

fn move_doc_end_with_count(app: &mut App, count: Option<usize>) {
    if let Some(count) = count {
        move_to_line_first_non_blank(app, count.saturating_sub(1));
    } else {
        move_doc_end(app);
    }
}

fn move_to_line_first_non_blank(app: &mut App, row: usize) {
    let lines = app.textarea.lines();
    if lines.is_empty() {
        return;
    }
    let target_row = row.min(lines.len().saturating_sub(1));
    let col = first_non_blank_col(lines.get(target_row).map(|s| s.as_str()).unwrap_or(""));
    app.textarea
        .move_cursor(CursorMove::Jump(target_row as u16, col as u16));
    clamp_cursor_for_normal(app);
}

fn move_half_page_down(app: &mut App, count: usize) {
    let step = page_step(app, true);
    move_down(app, step.saturating_mul(count));
}

fn move_half_page_up(app: &mut App, count: usize) {
    let step = page_step(app, true);
    move_up(app, step.saturating_mul(count));
}

fn move_page_down(app: &mut App, count: usize) {
    let step = page_step(app, false);
    move_down(app, step.saturating_mul(count));
}

fn move_page_up(app: &mut App, count: usize) {
    let step = page_step(app, false);
    move_up(app, step.saturating_mul(count));
}

fn page_step(app: &App, half: bool) -> usize {
    let height = app.textarea_viewport_height.max(1);
    if half {
        (height / 2).max(1)
    } else {
        height.max(1)
    }
}

fn move_cursor_after(app: &mut App) {
    let (row, col) = app.textarea.cursor();
    let line_len = current_line_len(app, row);
    if line_len == 0 {
        return;
    }
    let new_col = col.saturating_add(1).min(line_len);
    app.textarea
        .move_cursor(CursorMove::Jump(row as u16, new_col as u16));
}

fn clamp_cursor_for_normal(app: &mut App) {
    let (row, col) = app.textarea.cursor();
    let line_len = current_line_len(app, row);
    let max_col = if line_len == 0 {
        0
    } else {
        line_len.saturating_sub(1)
    };
    if col > max_col {
        app.textarea
            .move_cursor(CursorMove::Jump(row as u16, max_col as u16));
    }
}

fn current_line_len(app: &App, row: usize) -> usize {
    app.textarea
        .lines()
        .get(row)
        .map(|line| line.chars().count())
        .unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordKind {
    Whitespace,
    Word,
    Punct,
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn char_kind(ch: char, big_word: bool) -> WordKind {
    if ch.is_whitespace() {
        WordKind::Whitespace
    } else if big_word || is_word_char(ch) {
        WordKind::Word
    } else {
        WordKind::Punct
    }
}

fn line_len(lines: &[String], row: usize) -> usize {
    lines.get(row).map(|line| line.chars().count()).unwrap_or(0)
}

fn line_end_cursor_col(lines: &[String], row: usize) -> usize {
    let len = line_len(lines, row);
    if len == 0 { 0 } else { len.saturating_sub(1) }
}

fn char_at(lines: &[String], row: usize, col: usize) -> Option<char> {
    lines.get(row).and_then(|line| line.chars().nth(col))
}

fn kind_at(lines: &[String], pos: (usize, usize), big_word: bool) -> WordKind {
    char_at(lines, pos.0, pos.1)
        .map(|ch| char_kind(ch, big_word))
        .unwrap_or(WordKind::Whitespace)
}

fn next_pos(lines: &[String], pos: (usize, usize)) -> Option<(usize, usize)> {
    if lines.is_empty() {
        return None;
    }
    let (row, col) = pos;
    let len = line_len(lines, row);
    if len == 0 {
        if row + 1 < lines.len() {
            return Some((row + 1, 0));
        }
        return None;
    }
    if col + 1 < len {
        Some((row, col + 1))
    } else if row + 1 < lines.len() {
        Some((row + 1, 0))
    } else {
        None
    }
}

fn prev_pos(lines: &[String], pos: (usize, usize)) -> Option<(usize, usize)> {
    if lines.is_empty() {
        return None;
    }
    let (row, col) = pos;
    let len = line_len(lines, row);
    if len == 0 {
        if row == 0 {
            return None;
        }
        let prev_len = line_len(lines, row - 1);
        let prev_col = if prev_len == 0 { 0 } else { prev_len - 1 };
        return Some((row - 1, prev_col));
    }
    if col > 0 {
        Some((row, col - 1))
    } else if row > 0 {
        let prev_len = line_len(lines, row - 1);
        let prev_col = if prev_len == 0 { 0 } else { prev_len - 1 };
        Some((row - 1, prev_col))
    } else {
        None
    }
}

fn retreat_pos_for_delete(lines: &[String], pos: (usize, usize)) -> Option<(usize, usize)> {
    if lines.is_empty() {
        return None;
    }
    let (row, col) = pos;
    if col > 0 {
        Some((row, col.saturating_sub(1)))
    } else if row > 0 {
        let prev_len = line_len(lines, row - 1);
        Some((row - 1, prev_len))
    } else {
        None
    }
}

fn next_word_start(lines: &[String], pos: (usize, usize), big_word: bool) -> (usize, usize) {
    let Some(mut cur) = next_pos(lines, pos) else {
        return pos;
    };

    let mut kind = kind_at(lines, cur, big_word);

    if kind == WordKind::Whitespace {
        while kind == WordKind::Whitespace {
            let Some(next) = next_pos(lines, cur) else {
                return cur;
            };
            cur = next;
            kind = kind_at(lines, cur, big_word);
        }
        return cur;
    }

    loop {
        let Some(next) = next_pos(lines, cur) else {
            return cur;
        };
        let next_kind = kind_at(lines, next, big_word);
        if next_kind == kind {
            cur = next;
            continue;
        }
        cur = next;
        kind = next_kind;
        break;
    }

    while kind == WordKind::Whitespace {
        let Some(next) = next_pos(lines, cur) else {
            return cur;
        };
        cur = next;
        kind = kind_at(lines, cur, big_word);
    }
    cur
}

fn prev_word_start(lines: &[String], pos: (usize, usize), big_word: bool) -> (usize, usize) {
    let Some(mut cur) = prev_pos(lines, pos) else {
        return pos;
    };

    let mut kind = kind_at(lines, cur, big_word);
    while kind == WordKind::Whitespace {
        let Some(prev) = prev_pos(lines, cur) else {
            return cur;
        };
        cur = prev;
        kind = kind_at(lines, cur, big_word);
    }

    loop {
        let Some(prev) = prev_pos(lines, cur) else {
            return cur;
        };
        let prev_kind = kind_at(lines, prev, big_word);
        if prev_kind == kind {
            cur = prev;
            continue;
        }
        return cur;
    }
}

fn word_end(lines: &[String], pos: (usize, usize), big_word: bool) -> (usize, usize) {
    let mut cur = pos;
    let mut kind = kind_at(lines, cur, big_word);

    if kind == WordKind::Whitespace {
        while kind == WordKind::Whitespace {
            let Some(next) = next_pos(lines, cur) else {
                return cur;
            };
            cur = next;
            kind = kind_at(lines, cur, big_word);
        }
    }

    loop {
        let Some(next) = next_pos(lines, cur) else {
            return cur;
        };
        let next_kind = kind_at(lines, next, big_word);
        if next_kind == kind {
            cur = next;
            continue;
        }
        return cur;
    }
}

fn open_line_below(app: &mut App) {
    let (row, _) = app.textarea.cursor();
    let line_len = current_line_len(app, row);
    app.textarea
        .move_cursor(CursorMove::Jump(row as u16, line_len as u16));
    insert_newline_with_auto_indent(&mut app.textarea);
}

fn open_line_above(app: &mut App) {
    let (row, _) = app.textarea.cursor();
    let current_line = app
        .textarea
        .lines()
        .get(row)
        .map(|s| s.as_str())
        .unwrap_or("");
    let prefix = list_continuation_prefix(current_line);
    app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
    app.textarea.insert_newline();
    app.textarea.move_cursor(CursorMove::Up);
    if !prefix.is_empty() {
        app.textarea.insert_str(prefix);
    }
}

fn delete_to_line_start(app: &mut App) -> bool {
    let (row, col) = app.textarea.cursor();
    if col == 0 {
        return false;
    }
    app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
    app.textarea.delete_str(col)
}

fn delete_previous_word(app: &mut App) -> bool {
    let (_, col) = app.textarea.cursor();
    if col == 0 {
        return false;
    }
    app.textarea.delete_word()
}

/// Paste before cursor (P command in Vim)
/// - For linewise yanks: paste above the current line
/// - For charwise yanks: paste before the cursor position
fn paste_before_cursor(app: &mut App, count: usize) {
    if app.yank_buffer.is_empty() {
        return;
    }

    app.record_undo_snapshot();

    if app.yank_is_linewise {
        // Linewise paste: insert above the current line
        let (row, _) = app.textarea.cursor();
        app.textarea.move_cursor(CursorMove::Jump(row as u16, 0));

        for _ in 0..count {
            // Insert the yanked text (which should end with newline for linewise)
            let text = if app.yank_buffer.ends_with('\n') {
                app.yank_buffer.clone()
            } else {
                format!("{}\n", app.yank_buffer)
            };
            app.textarea.insert_str(&text);
        }

        // Move cursor to the first non-blank of the first inserted line
        let col = first_non_blank_col(
            app.textarea
                .lines()
                .get(row)
                .map(|s| s.as_str())
                .unwrap_or(""),
        );
        app.textarea
            .move_cursor(CursorMove::Jump(row as u16, col as u16));
    } else {
        // Charwise paste: insert before cursor position
        app.textarea.set_yank_text(app.yank_buffer.clone());
        for _ in 0..count {
            // Move back one position before pasting (tui_textarea.paste() inserts after cursor)
            // but we want to insert before, so we insert directly
            let text = &app.yank_buffer;
            app.textarea.insert_str(text);
        }
        clamp_cursor_for_normal(app);
    }

    app.composer_dirty = true;
}

fn replace_char(app: &mut App, c: char) -> bool {
    let (row, col) = app.textarea.cursor();
    let line_len = current_line_len(app, row);
    if line_len == 0 || col >= line_len {
        return false;
    }
    if !app.textarea.delete_str(1) {
        return false;
    }
    app.set_yank_buffer(app.textarea.yank_text());
    app.textarea.insert_char(c);
    let new_col = col.min(current_line_len(app, row).saturating_sub(1));
    app.textarea
        .move_cursor(CursorMove::Jump(row as u16, new_col as u16));
    true
}

fn advance_pos_by_chars(lines: &[String], start: (usize, usize), count: usize) -> (usize, usize) {
    let mut row = start.0;
    let mut col = start.1;
    let mut remaining = count;
    let line_count = lines.len();
    if line_count == 0 {
        return start;
    }

    while remaining > 0 {
        let len = line_len(lines, row);
        if len == 0 {
            if row + 1 >= line_count {
                return (row, col);
            }
            row += 1;
            col = 0;
            remaining = remaining.saturating_sub(1);
            continue;
        }

        if col < len {
            let available = len.saturating_sub(col);
            if remaining <= available {
                col = col.saturating_add(remaining);
                break;
            }
            col = len;
            remaining = remaining.saturating_sub(available);
        }

        if remaining == 0 {
            break;
        }
        if row + 1 >= line_count {
            break;
        }
        row += 1;
        col = 0;
        remaining = remaining.saturating_sub(1);
    }

    (row, col)
}

fn collect_block_object_text(
    lines: &[String],
    start: (usize, usize),
    end: (usize, usize),
) -> String {
    let row_start = start.0.min(end.0);
    let row_end = start.0.max(end.0);
    let col_start = start.1.min(end.1);
    let col_end = start.1.max(end.1).saturating_add(1);

    let mut blocks = Vec::new();
    for row in row_start..=row_end {
        let line = lines.get(row).map(|s| s.as_str()).unwrap_or("");
        let line_len = line.chars().count();
        if line_len == 0 || col_start >= line_len {
            blocks.push(String::new());
            continue;
        }
        let end = col_end.min(line_len);
        blocks.push(slice_by_char(line, col_start, end));
    }

    blocks.join("\n")
}

fn delete_block_object(app: &mut App, start: (usize, usize), end: (usize, usize)) -> bool {
    let row_start = start.0.min(end.0);
    let row_end = start.0.max(end.0);
    let col_start = start.1.min(end.1);
    let col_end = start.1.max(end.1).saturating_add(1);

    let mut modified = false;
    for row in row_start..=row_end {
        let line = app
            .textarea
            .lines()
            .get(row)
            .map(|s| s.as_str())
            .unwrap_or("");
        let line_len = line.chars().count();
        if line_len == 0 || col_start >= line_len {
            continue;
        }
        let end = col_end.min(line_len);
        let delete_len = end.saturating_sub(col_start);
        if delete_len == 0 {
            continue;
        }
        app.textarea
            .move_cursor(CursorMove::Jump(row as u16, col_start as u16));
        if app.textarea.delete_str(delete_len) {
            modified = true;
        }
    }

    app.textarea
        .move_cursor(CursorMove::Jump(row_start as u16, col_start as u16));
    clamp_cursor_for_normal(app);
    modified
}

fn slice_by_char(s: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
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
        && app.pomodoro_end.is_some()
        && *file_path == task.file_path
        && *line_number == task.line_number
    {
        app.pomodoro_end = None;
        app.pomodoro_start = None;
        app.pomodoro_target = None;
        app.toast("Pomodoro stopped.");
        return;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_editing_app(lines: &[&str]) -> App<'static> {
        let mut app = App::new();
        app.textarea = tui_textarea::TextArea::from(lines.iter().copied());
        app.input_mode = InputMode::Editing;
        app.reset_editor_state();
        app
    }

    fn send_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        handle_editing_mode(app, KeyEvent::new(code, modifiers));
    }

    fn send_char(app: &mut App, ch: char) {
        send_key(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }

    #[test]
    fn insert_groups_undo_redo() {
        let mut app = make_editing_app(&["hello"]);
        app.textarea.move_cursor(CursorMove::End);

        send_char(&mut app, 'i');
        send_char(&mut app, 'x');
        send_char(&mut app, 'y');
        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(app.textarea.lines(), ["helloxy"]);

        send_char(&mut app, 'u');
        assert_eq!(app.textarea.lines(), ["hello"]);

        send_key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(app.textarea.lines(), ["helloxy"]);
    }

    #[test]
    fn count_moves_cursor_down() {
        let mut app = make_editing_app(&["a", "b", "c", "d"]);
        send_char(&mut app, '2');
        send_char(&mut app, 'j');
        assert_eq!(app.textarea.cursor().0, 2);
    }

    #[test]
    fn count_delete_lines() {
        let mut app = make_editing_app(&["a", "b", "c", "d"]);
        send_char(&mut app, '2');
        send_char(&mut app, 'd');
        send_char(&mut app, 'd');
        assert_eq!(app.textarea.lines(), ["c", "d"]);
    }

    #[test]
    fn delete_line_removes_newline_and_moves_cursor() {
        let mut app = make_editing_app(&["first", "  second", "third"]);
        send_char(&mut app, 'd');
        send_char(&mut app, 'd');
        assert_eq!(app.textarea.lines(), ["  second", "third"]);
        assert_eq!(app.textarea.cursor(), (0, 2));
    }

    #[test]
    fn delete_last_line_moves_to_previous() {
        let mut app = make_editing_app(&["first", "second"]);
        send_char(&mut app, 'j');
        send_char(&mut app, 'd');
        send_char(&mut app, 'd');
        assert_eq!(app.textarea.lines(), ["first"]);
        assert_eq!(app.textarea.cursor(), (0, 0));
    }

    #[test]
    fn visual_charwise_yank() {
        let mut app = make_editing_app(&["abcd"]);
        send_char(&mut app, 'v');
        send_char(&mut app, 'l');
        send_char(&mut app, 'l');
        send_char(&mut app, 'y');
        assert_eq!(app.editor_mode, EditorMode::Normal);
        assert_eq!(app.yank_buffer, "abc");
    }

    #[test]
    fn delete_char_and_paste() {
        let mut app = make_editing_app(&["abc"]);
        send_char(&mut app, 'x');
        assert_eq!(app.textarea.lines(), ["bc"]);
        send_char(&mut app, 'p');
        assert_eq!(app.textarea.lines(), ["abc"]);
    }

    #[test]
    fn visual_word_motion_w_b_e() {
        let mut app = make_editing_app(&["one two"]);
        send_char(&mut app, 'v');
        send_char(&mut app, 'w');
        assert_eq!(app.textarea.cursor(), (0, 4));
        send_char(&mut app, 'b');
        assert_eq!(app.textarea.cursor(), (0, 0));
        send_char(&mut app, 'e');
        assert_eq!(app.textarea.cursor(), (0, 2));
    }

    #[test]
    fn visual_word_motion_wbe_counts() {
        let mut app = make_editing_app(&["one two three four"]);
        send_char(&mut app, 'v');
        send_char(&mut app, '2');
        send_char(&mut app, 'w');
        assert_eq!(app.textarea.cursor(), (0, 8));
    }

    #[test]
    fn visual_word_motion_w_b_e_big_word() {
        let mut app = make_editing_app(&["foo,bar baz"]);
        send_char(&mut app, 'v');
        send_char(&mut app, 'W');
        assert_eq!(app.textarea.cursor(), (0, 8));
        send_char(&mut app, 'B');
        assert_eq!(app.textarea.cursor(), (0, 0));
        send_char(&mut app, 'E');
        assert_eq!(app.textarea.cursor(), (0, 6));
    }

    // ============ Tests for Shift+shortcut commands (D/C/S/X/P) ============

    #[test]
    fn uppercase_d_deletes_to_end_of_line() {
        // D => d$ (delete to end of line, no newline removal)
        let mut app = make_editing_app(&["hello world", "second line"]);
        app.textarea.move_cursor(CursorMove::Jump(0, 6)); // position at 'w'
        send_key(&mut app, KeyCode::Char('D'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["hello ", "second line"]);
        assert_eq!(app.yank_buffer, "world");
    }

    #[test]
    fn uppercase_c_changes_to_end_of_line() {
        // C => c$ (change to end of line, enter INSERT)
        let mut app = make_editing_app(&["hello world", "second line"]);
        app.textarea.move_cursor(CursorMove::Jump(0, 6)); // position at 'w'
        send_key(&mut app, KeyCode::Char('C'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["hello ", "second line"]);
        assert_eq!(app.editor_mode, EditorMode::Insert);
        assert_eq!(app.yank_buffer, "world");
    }

    #[test]
    fn uppercase_s_changes_entire_line() {
        // S => cc (change entire line, remove line incl newline, insert empty line)
        let mut app = make_editing_app(&["first", "second", "third"]);
        send_char(&mut app, 'j'); // move to "second"
        send_key(&mut app, KeyCode::Char('S'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["first", "", "third"]);
        assert_eq!(app.editor_mode, EditorMode::Insert);
        assert_eq!(app.yank_buffer, "second\n");
    }

    #[test]
    fn uppercase_x_deletes_char_before_cursor() {
        // X => dh (delete char before cursor)
        let mut app = make_editing_app(&["abcd"]);
        app.textarea.move_cursor(CursorMove::Jump(0, 2)); // position at 'c'
        send_key(&mut app, KeyCode::Char('X'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["acd"]);
        assert_eq!(app.yank_buffer, "b");
    }

    #[test]
    fn uppercase_x_at_start_of_line_does_nothing() {
        // X at column 0 should not delete anything
        let mut app = make_editing_app(&["abcd"]);
        app.textarea.move_cursor(CursorMove::Jump(0, 0)); // position at 'a'
        send_key(&mut app, KeyCode::Char('X'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["abcd"]);
    }

    #[test]
    fn uppercase_p_pastes_before_cursor_charwise() {
        // P for charwise yank pastes before cursor
        let mut app = make_editing_app(&["abcd"]);
        send_char(&mut app, 'x'); // delete 'a', yank_buffer = "a"
        assert_eq!(app.textarea.lines(), ["bcd"]);
        send_key(&mut app, KeyCode::Char('P'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["abcd"]);
    }

    #[test]
    fn uppercase_p_pastes_above_line_linewise() {
        // P for linewise yank pastes above current line
        let mut app = make_editing_app(&["first", "second", "third"]);
        send_char(&mut app, 'd');
        send_char(&mut app, 'd'); // delete "first", linewise yank
        assert_eq!(app.textarea.lines(), ["second", "third"]);
        assert!(app.yank_is_linewise);
        send_char(&mut app, 'j'); // move to "third"
        send_key(&mut app, KeyCode::Char('P'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.lines(), ["second", "first", "third"]);
    }

    // ============ Tests for Visual mode extended motions ============

    #[test]
    fn visual_zero_moves_to_line_start() {
        let mut app = make_editing_app(&["hello world"]);
        app.textarea.move_cursor(CursorMove::Jump(0, 6)); // position at 'w'
        send_char(&mut app, 'v');
        send_char(&mut app, '0');
        assert_eq!(app.textarea.cursor(), (0, 0));
        // Anchor should still be at original position
        assert_eq!(app.visual_anchor, Some((0, 6)));
    }

    #[test]
    fn visual_dollar_moves_to_line_end() {
        let mut app = make_editing_app(&["hello world"]);
        send_char(&mut app, 'v');
        send_char(&mut app, '$');
        // Should be at last character (0-indexed, 10 for 'd' in "hello world")
        assert_eq!(app.textarea.cursor(), (0, 10));
        assert_eq!(app.visual_anchor, Some((0, 0)));
    }

    #[test]
    fn visual_gg_moves_to_document_start() {
        let mut app = make_editing_app(&["first", "second", "third"]);
        send_char(&mut app, 'j');
        send_char(&mut app, 'j'); // move to "third"
        send_char(&mut app, 'v');
        send_char(&mut app, 'g');
        send_char(&mut app, 'g');
        assert_eq!(app.textarea.cursor(), (0, 0));
        assert_eq!(app.visual_anchor, Some((2, 0)));
    }

    #[test]
    fn visual_uppercase_g_moves_to_document_end() {
        let mut app = make_editing_app(&["first", "second", "third"]);
        send_char(&mut app, 'v');
        send_key(&mut app, KeyCode::Char('G'), KeyModifiers::SHIFT);
        // Should move to last line, last column
        assert_eq!(app.textarea.cursor().0, 2); // row 2
        assert_eq!(app.visual_anchor, Some((0, 0)));
    }

    #[test]
    fn visual_ctrl_d_half_page_down() {
        let mut app = make_editing_app(&[
            "line1", "line2", "line3", "line4", "line5", "line6", "line7", "line8", "line9",
            "line10",
        ]);
        app.textarea_viewport_height = 4; // simulate viewport
        send_char(&mut app, 'v');
        send_key(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
        // Half page = 2 lines down from line 0
        assert_eq!(app.textarea.cursor().0, 2);
        assert_eq!(app.visual_anchor, Some((0, 0)));
    }

    #[test]
    fn visual_ctrl_b_full_page_up() {
        let mut app = make_editing_app(&[
            "line1", "line2", "line3", "line4", "line5", "line6", "line7", "line8", "line9",
            "line10",
        ]);
        app.textarea_viewport_height = 4; // simulate viewport
        // Move to end first
        send_key(&mut app, KeyCode::Char('G'), KeyModifiers::SHIFT);
        send_char(&mut app, 'v');
        let start_row = app.textarea.cursor().0;
        send_key(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
        // Full page = 4 lines up
        assert_eq!(app.textarea.cursor().0, start_row.saturating_sub(4));
    }

    #[test]
    fn visual_count_with_uppercase_g() {
        // 3G should go to line 3 (1-indexed, so row 2)
        let mut app = make_editing_app(&["line1", "line2", "line3", "line4", "line5"]);
        send_char(&mut app, 'v');
        send_char(&mut app, '3');
        send_key(&mut app, KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(app.textarea.cursor().0, 2); // line 3, 0-indexed
    }

    #[test]
    fn visual_count_with_ctrl_d() {
        let mut app = make_editing_app(&[
            "line1", "line2", "line3", "line4", "line5", "line6", "line7", "line8", "line9",
            "line10",
        ]);
        app.textarea_viewport_height = 2; // half page = 1
        send_char(&mut app, 'v');
        send_char(&mut app, '2');
        send_key(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
        // 2 * half page (1) = 2 lines down
        assert_eq!(app.textarea.cursor().0, 2);
    }
}
