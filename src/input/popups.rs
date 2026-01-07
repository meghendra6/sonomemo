use crate::{
    app::App,
    config::{self, key_code_for_shortcuts, key_match, config_path, EditorStyle, ThemePreset},
    date_input::{parse_duration_input, parse_relative_date_input, parse_time_input},
    editor::markdown,
    input::editing,
    models::{self, DatePickerField, InputMode, Mood},
    storage,
};
use chrono::{Duration, Local, NaiveTime, Timelike};
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_popup_events(app: &mut App, key: KeyEvent) -> bool {
    if app.show_google_auth_popup {
        handle_google_auth_popup(app, key);
        return true;
    }
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
    if app.show_date_picker_popup {
        handle_date_picker_popup(app, key);
        return true;
    }
    if app.show_memo_preview_popup {
        handle_memo_preview_popup(app, key);
        return true;
    }
    if app.show_ai_response_popup {
        handle_ai_response_popup(app, key);
        return true;
    }

    if app.show_exit_popup {
        handle_exit_popup(app, key);
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

fn handle_memo_preview_popup(app: &mut App, key: KeyEvent) {
    let key_code = key_code_for_shortcuts(&key);
    if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_memo_preview_popup = false;
        app.memo_preview_entry = None;
        return;
    }

    if matches!(key_code, KeyCode::Char('e') | KeyCode::Char('E')) {
        if let Some(entry) = app.memo_preview_entry.clone() {
            app.show_memo_preview_popup = false;
            app.memo_preview_entry = None;
            app.start_edit_entry(&entry);
        }
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.up) {
        app.memo_preview_scroll = app.memo_preview_scroll.saturating_sub(1);
        return;
    }
    if key_match(&key, &app.config.keybindings.popup.down) {
        app.memo_preview_scroll = app.memo_preview_scroll.saturating_add(1);
        return;
    }

    match key_code {
        KeyCode::PageUp => {
            app.memo_preview_scroll = app.memo_preview_scroll.saturating_sub(5);
        }
        KeyCode::PageDown => {
            app.memo_preview_scroll = app.memo_preview_scroll.saturating_add(5);
        }
        _ => {}
    }
}

fn handle_ai_response_popup(app: &mut App, key: KeyEvent) {
    let key_code = key_code_for_shortcuts(&key);
    if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_ai_response_popup = false;
        app.ai_response = None;
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.up) {
        app.ai_response_scroll = app.ai_response_scroll.saturating_sub(1);
        return;
    }
    if key_match(&key, &app.config.keybindings.popup.down) {
        app.ai_response_scroll = app.ai_response_scroll.saturating_add(1);
        return;
    }

    match key_code {
        KeyCode::PageUp => {
            app.ai_response_scroll = app.ai_response_scroll.saturating_sub(5);
        }
        KeyCode::PageDown => {
            app.ai_response_scroll = app.ai_response_scroll.saturating_add(5);
        }
        _ => {}
    }
}

fn handle_date_picker_popup(app: &mut App, key: KeyEvent) {
    let key_code = key_code_for_shortcuts(&key);
    if app.date_picker_input_mode {
        handle_date_picker_relative_input(app, key);
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_date_picker_popup = false;
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.confirm) {
        apply_date_picker_field(app);
        app.show_date_picker_popup = false;
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.up) {
        app.date_picker_field = cycle_date_picker_field(app.date_picker_field, -1);
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.down) {
        app.date_picker_field = cycle_date_picker_field(app.date_picker_field, 1);
        return;
    }

    match key_code {
        KeyCode::Left => {
            adjust_date_picker_value(app, -1, 0);
        }
        KeyCode::Right => {
            adjust_date_picker_value(app, 1, 0);
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            adjust_date_picker_value(app, 1, 0);
        }
        KeyCode::Char('-') => {
            adjust_date_picker_value(app, -1, 0);
        }
        KeyCode::Char('[') => {
            adjust_date_picker_value(app, -7, -60);
        }
        KeyCode::Char(']') => {
            adjust_date_picker_value(app, 7, 60);
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            if is_date_picker_date_field(app.date_picker_field) {
                let today = Local::now().date_naive();
                app.set_date_picker_date(app.date_picker_field, today);
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.date_picker_input.clear();
            app.date_picker_input_mode = true;
        }
        KeyCode::Backspace | KeyCode::Delete => {
            remove_date_picker_field(app);
            app.show_date_picker_popup = false;
        }
        KeyCode::Tab => {
            app.date_picker_field = cycle_date_picker_field(app.date_picker_field, 1);
        }
        KeyCode::BackTab => {
            app.date_picker_field = cycle_date_picker_field(app.date_picker_field, -1);
        }
        _ => {}
    }
}

fn handle_date_picker_relative_input(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.date_picker_input_mode = false;
        app.date_picker_input.clear();
        return;
    }

    if key.code == KeyCode::Enter {
        let input = app.date_picker_input.trim().to_string();
        if input.is_empty() {
            app.date_picker_input_mode = false;
            return;
        }

        let field = app.date_picker_field;
        let parsed = match field {
            DatePickerField::Scheduled | DatePickerField::Due | DatePickerField::Start => {
                let base = app.date_picker_effective_date(field);
                parse_relative_date_input(&input, base).map(DatePickerValue::Date)
            }
            DatePickerField::Time => parse_time_input(&input).map(DatePickerValue::Time),
            DatePickerField::Duration => parse_duration_input(&input).map(DatePickerValue::Duration),
        };

        if let Some(value) = parsed {
            match value {
                DatePickerValue::Date(date) => app.set_date_picker_date(field, date),
                DatePickerValue::Time(time) => app.set_date_picker_time(time),
                DatePickerValue::Duration(minutes) => app.set_date_picker_duration(minutes),
            }
            app.date_picker_input.clear();
            app.date_picker_input_mode = false;
        } else {
            app.toast("Invalid relative input.");
        }
        return;
    }

    match key.code {
        KeyCode::Backspace => {
            app.date_picker_input.pop();
        }
        KeyCode::Char(c) => {
            if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                app.date_picker_input.push(c);
            }
        }
        _ => {}
    }
}

fn apply_date_picker_field(app: &mut App) {
    let field = app.date_picker_field;
    let schedule = app.date_picker_schedule.clone();
    let scheduled_value = schedule.scheduled.or_else(|| {
        (field == DatePickerField::Scheduled)
            .then(|| app.date_picker_effective_date(DatePickerField::Scheduled))
    });
    let due_value = schedule.due.or_else(|| {
        (field == DatePickerField::Due).then(|| app.date_picker_effective_date(DatePickerField::Due))
    });
    let start_value = schedule.start.or_else(|| {
        (field == DatePickerField::Start).then(|| app.date_picker_effective_date(DatePickerField::Start))
    });
    let time_value = schedule
        .time
        .or_else(|| (field == DatePickerField::Time).then(|| app.date_picker_effective_time()));
    let duration_value = schedule.duration_minutes.or_else(|| {
        (field == DatePickerField::Duration).then(|| app.date_picker_effective_duration())
    });

    let mut updated = false;
    if let Some(date) = scheduled_value {
        let value = date.format("%Y-%m-%d").to_string();
        updated |= markdown::upsert_task_metadata(
            &mut app.textarea,
            crate::task_metadata::TaskMetadataKey::Scheduled,
            &value,
        );
    }
    if let Some(date) = due_value {
        let value = date.format("%Y-%m-%d").to_string();
        updated |= markdown::upsert_task_metadata(
            &mut app.textarea,
            crate::task_metadata::TaskMetadataKey::Due,
            &value,
        );
    }
    if let Some(date) = start_value {
        let value = date.format("%Y-%m-%d").to_string();
        updated |= markdown::upsert_task_metadata(
            &mut app.textarea,
            crate::task_metadata::TaskMetadataKey::Start,
            &value,
        );
    }
    if let Some(time) = time_value {
        let value = format_time_value(time);
        updated |= markdown::upsert_task_metadata(
            &mut app.textarea,
            crate::task_metadata::TaskMetadataKey::Time,
            &value,
        );
    }
    if let Some(minutes) = duration_value {
        let value = format_duration_value(minutes);
        updated |= markdown::upsert_task_metadata(
            &mut app.textarea,
            crate::task_metadata::TaskMetadataKey::Duration,
            &value,
        );
    }

    if updated {
        app.mark_insert_modified();
        app.composer_dirty = true;
    }
}

fn remove_date_picker_field(app: &mut App) {
    let key = match app.date_picker_field {
        DatePickerField::Scheduled => crate::task_metadata::TaskMetadataKey::Scheduled,
        DatePickerField::Due => crate::task_metadata::TaskMetadataKey::Due,
        DatePickerField::Start => crate::task_metadata::TaskMetadataKey::Start,
        DatePickerField::Time => crate::task_metadata::TaskMetadataKey::Time,
        DatePickerField::Duration => crate::task_metadata::TaskMetadataKey::Duration,
    };

    let updated = markdown::remove_task_metadata(&mut app.textarea, key);
    if updated {
        app.mark_insert_modified();
        app.composer_dirty = true;
    }
}

fn adjust_date_picker_value(app: &mut App, delta_days: i64, delta_minutes: i32) {
    match app.date_picker_field {
        DatePickerField::Scheduled | DatePickerField::Due | DatePickerField::Start => {
            let base = app.date_picker_effective_date(app.date_picker_field);
            let next = base + Duration::days(delta_days);
            app.set_date_picker_date(app.date_picker_field, next);
        }
        DatePickerField::Time => {
            let time = app.date_picker_effective_time();
            let next = add_minutes_wrapping(time, if delta_minutes == 0 { delta_days * 15 } else { delta_minutes as i64 });
            app.set_date_picker_time(next);
        }
        DatePickerField::Duration => {
            let current = app.date_picker_effective_duration() as i64;
            let delta = if delta_minutes == 0 { delta_days * 15 } else { delta_minutes as i64 };
            let next = (current + delta).clamp(15, 24 * 60);
            app.set_date_picker_duration(next as u32);
        }
    }
}

fn add_minutes_wrapping(time: NaiveTime, delta_minutes: i64) -> NaiveTime {
    let total = time.hour() as i64 * 60 + time.minute() as i64 + delta_minutes;
    let minutes = total.rem_euclid(24 * 60) as u32;
    NaiveTime::from_hms_opt(minutes / 60, minutes % 60, 0)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap())
}

fn is_date_picker_date_field(field: DatePickerField) -> bool {
    matches!(
        field,
        DatePickerField::Scheduled | DatePickerField::Due | DatePickerField::Start
    )
}

fn cycle_date_picker_field(field: DatePickerField, delta: i32) -> DatePickerField {
    let fields = [
        DatePickerField::Scheduled,
        DatePickerField::Due,
        DatePickerField::Start,
        DatePickerField::Time,
        DatePickerField::Duration,
    ];
    let index = fields
        .iter()
        .position(|f| *f == field)
        .unwrap_or(0) as i32;
    let len = fields.len() as i32;
    let next = (index + delta).rem_euclid(len) as usize;
    fields[next]
}

fn format_time_value(time: NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn format_duration_value(minutes: u32) -> String {
    if minutes >= 60 {
        let hours = minutes / 60;
        let mins = minutes % 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h{mins}m")
        }
    } else {
        format!("{minutes}m")
    }
}

enum DatePickerValue {
    Date(chrono::NaiveDate),
    Time(NaiveTime),
    Duration(u32),
}

fn handle_exit_popup(app: &mut App, key: KeyEvent) {
    let key_code = key_code_for_shortcuts(&key);
    match key_code {
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.show_exit_popup = false;
            app.commit_insert_group();
            editing::submit_composer(app);
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            app.show_exit_popup = false;
            editing::discard_composer(app);
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.show_exit_popup = false;
        }
        _ => {}
    }
}

fn handle_delete_entry_popup(app: &mut App, key: KeyEvent) {
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

fn handle_mood_popup(app: &mut App, key: KeyEvent) {
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

fn handle_todo_popup(app: &mut App, key: KeyEvent) {
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

fn handle_tag_popup(app: &mut App, key: KeyEvent) {
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

fn handle_theme_switcher_popup(app: &mut App, key: KeyEvent) {
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

fn handle_editor_style_popup(app: &mut App, key: KeyEvent) {
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

fn handle_pomodoro_popup(app: &mut App, key: KeyEvent) {
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

fn handle_path_popup(app: &mut App, key: KeyEvent) {
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

fn handle_google_auth_popup(app: &mut App, key: KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        if let Some(display) = app.google_auth_display.as_ref() {
            if let Err(e) = open::that(&display.local_url) {
                eprintln!("Failed to open browser: {}", e);
            }
        }
        return;
    }

    if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_google_auth_popup = false;
        return;
    }
}
