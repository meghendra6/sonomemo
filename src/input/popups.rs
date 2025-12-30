use crate::{
    app::App,
    config::{self, EditorStyle, ThemePreset, config_path, key_match},
    models::{self, InputMode, Mood},
    storage,
};
use chrono::{Duration, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_popup_events(app: &mut App, key: KeyEvent) -> bool {
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
    if app.show_quick_capture {
        handle_quick_capture_popup(app, key);
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

fn handle_quick_capture_popup(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.reset_quick_capture();
        app.show_quick_capture = false;
        return;
    }

    if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL) {
        submit_quick_capture(app, true);
        return;
    }

    if key.code == KeyCode::Enter {
        submit_quick_capture(app, false);
        return;
    }

    app.quick_capture_textarea.input(key);
}

fn submit_quick_capture(app: &mut App, keep_open: bool) {
    let text = app
        .quick_capture_textarea
        .lines()
        .join(" ")
        .trim()
        .to_string();

    if text.is_empty() {
        app.toast("Quick capture is empty.");
        if !keep_open {
            app.reset_quick_capture();
            app.show_quick_capture = false;
        }
        return;
    }

    let mut content = text;
    if let Some(tag) = app
        .config
        .capture
        .quick_capture_default_tag
        .as_deref()
        .map(str::trim)
        && !tag.is_empty()
    {
        let tag_text = if tag.starts_with('#') {
            tag.to_string()
        } else {
            format!("#{tag}")
        };
        if !content.contains(&tag_text) {
            content.push(' ');
            content.push_str(&tag_text);
        }
    }

    if storage::append_entry(&app.config.data.log_path, &content).is_ok() {
        app.update_logs();
        app.toast("Captured.");
    } else {
        app.toast("Failed to capture.");
    }

    app.reset_quick_capture();
    if !keep_open {
        app.show_quick_capture = false;
    }
}

fn handle_discard_popup(app: &mut App, key: KeyEvent) {
    if key_match(&key, &app.config.keybindings.popup.confirm) {
        app.editing_entry = None;
        app.textarea = tui_textarea::TextArea::default();
        app.transition_to(InputMode::Navigate);
        app.show_discard_popup = false;
    } else if key_match(&key, &app.config.keybindings.popup.cancel) || key.code == KeyCode::Esc {
        app.show_discard_popup = false;
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
