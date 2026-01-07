use crate::{
    app::App,
    config::{EditorStyle, ThemePreset, config_path},
    integrations::gemini,
    integrations::google,
    models::{self, Priority},
    storage,
};
use chrono::{Duration, Local};
use std::fs;

pub fn open_tag_popup(app: &mut App) {
    if let Ok(tags) = storage::get_all_tags(&app.config.data.log_path) {
        app.tags = tags;
        if !app.tags.is_empty() {
            app.tag_list_state.select(Some(0));
            app.show_tag_popup = true;
        }
    }
}

pub fn toggle_todo_in_timeline(app: &mut App) {
    if let Some(i) = app.logs_state.selected()
        && i < app.logs.len()
    {
        let entry = &app.logs[i];
        match storage::complete_entry_tasks(entry) {
            Ok(updated) => {
                if updated == 0 {
                    app.toast("No open tasks in entry.");
                    return;
                }
                app.update_logs();
                app.logs_state.select(Some(i));
            }
            Err(_) => app.toast("Failed to complete tasks."),
        }
    }
}

pub fn complete_task_chain(app: &mut App) {
    if let Some(i) = app.tasks_state.selected()
        && i < app.tasks.len()
    {
        let task = app.tasks[i].clone();
        if task.is_done {
            app.toast("Task already done.");
            return;
        }
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
}

pub fn cycle_task_priority(app: &mut App) {
    let Some(i) = app.tasks_state.selected() else {
        app.toast("No task selected.");
        return;
    };
    if i >= app.tasks.len() {
        app.toast("No task selected.");
        return;
    }

    let task = app.tasks[i].clone();
    let next = next_priority(task.priority);
    let message = match next {
        Some(Priority::High) => "Priority A",
        Some(Priority::Medium) => "Priority B",
        Some(Priority::Low) => "Priority C",
        None => "Priority cleared",
    };

    match storage::cycle_task_priority(&task.file_path, task.line_number) {
        Ok(true) => {
            app.update_logs();
            app.toast(message);
        }
        Ok(false) => app.toast("Task not found."),
        Err(_) => app.toast("Failed to update priority."),
    }
}

pub fn open_activity_popup(app: &mut App) {
    if let Ok(data) = storage::get_activity_stats(&app.config.data.log_path) {
        app.activity_data = data;
        app.show_activity_popup = true;
    }
}

pub fn sync_google(app: &mut App) {
    if app.google_auth_receiver.is_some() {
        app.show_google_auth_popup = true;
        app.toast("Google auth in progress.");
        return;
    }

    if app.google_sync_receiver.is_some() {
        app.toast("Google sync already running.");
        return;
    }

    app.toast("Starting Google sync...");
    app.google_sync_receiver = Some(google::spawn_sync(app.config.clone()));
}

pub fn open_config_in_composer(app: &mut App) {
    let path = config_path();
    if !path.exists() {
        let _ = app.config.save_to_path(&path);
    }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let lines = content.lines().map(|line| line.to_string()).collect();
    app.start_edit_raw_file(path.to_string_lossy().to_string(), lines);
}

fn next_priority(current: Option<Priority>) -> Option<Priority> {
    match current {
        None => Some(Priority::High),
        Some(Priority::High) => Some(Priority::Medium),
        Some(Priority::Medium) => Some(Priority::Low),
        Some(Priority::Low) => None,
    }
}

pub fn focus_agenda_panel(app: &mut App) {
    app.refresh_agenda();
    app.set_navigate_focus(models::NavigateFocus::Agenda);
    app.set_agenda_selected_day(app.agenda_selected_day);
}

pub fn open_agenda_preview(app: &mut App) {
    let Some(selected) = app.agenda_state.selected() else {
        app.toast("No agenda item selected.");
        return;
    };
    let Some(item) = app.agenda_items.get(selected).cloned() else {
        app.toast("No agenda item selected.");
        return;
    };

    match storage::read_entry_containing_line(&item.file_path, item.line_number) {
        Ok(Some(entry)) => {
            app.memo_preview_entry = Some(entry);
            app.memo_preview_scroll = 0;
            app.show_memo_preview_popup = true;
        }
        Ok(None) => app.toast("Memo not found."),
        Err(_) => app.toast("Failed to load memo."),
    }
}

pub fn open_task_preview(app: &mut App) {
    let Some(selected) = app.tasks_state.selected() else {
        app.toast("No task selected.");
        return;
    };
    let Some(task) = app.tasks.get(selected).cloned() else {
        app.toast("No task selected.");
        return;
    };

    match storage::read_entry_containing_line(&task.file_path, task.line_number) {
        Ok(Some(entry)) => {
            app.memo_preview_entry = Some(entry);
            app.memo_preview_scroll = 0;
            app.show_memo_preview_popup = true;
        }
        Ok(None) => app.toast("Memo not found."),
        Err(_) => app.toast("Failed to load memo."),
    }
}

pub fn toggle_agenda_task(app: &mut App) {
    let Some(selected) = app.agenda_state.selected() else {
        app.toast("No agenda item selected.");
        return;
    };
    let Some(item) = app.agenda_items.get(selected).cloned() else {
        app.toast("No agenda item selected.");
        return;
    };
    if item.kind != models::AgendaItemKind::Task {
        app.toast("Not a task.");
        return;
    }

    if storage::toggle_task_status(&item.file_path, item.line_number).is_ok() {
        app.update_logs();
    } else {
        app.toast("Failed to toggle task.");
    }
}

pub fn open_theme_switcher(app: &mut App) {
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

pub fn open_editor_style_switcher(app: &mut App) {
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

pub fn open_or_toggle_pomodoro_for_selected_task(app: &mut App) {
    app.set_navigate_focus(models::NavigateFocus::Tasks);

    let Some(i) = app.tasks_state.selected() else {
        app.toast("No task selected.");
        return;
    };
    if i >= app.tasks.len() {
        app.toast("No task selected.");
        return;
    }

    let task = app.tasks[i].clone();
    if task.is_done {
        app.toast("Cannot start pomodoro on done task.");
        return;
    }

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

pub fn submit_search(app: &mut App) {
    let query = app
        .textarea
        .lines()
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>()
        .join(" ");
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return;
    }

    if let Some(ai_question) = parse_ai_query(trimmed) {
        submit_ai_search(app, &ai_question);
        return;
    }

    app.last_search_query = Some(trimmed.to_string());
    app.search_highlight_query = Some(trimmed.to_string());
    app.search_highlight_ready_at = Some(Local::now() + Duration::milliseconds(150));
    if let Ok(results) = storage::search_entries(&app.config.data.log_path, trimmed) {
        app.logs = results;
        app.is_search_result = true;
        app.logs_state.select(Some(0));
    }
}

pub fn save_ai_answer_to_memo(app: &mut App) {
    let Some(response) = app.ai_response.as_ref() else {
        app.toast("No AI answer to save.");
        return;
    };

    let mut content = String::new();
    content.push_str("AI Answer\n");
    content.push_str("Question: ");
    content.push_str(response.question.trim());
    content.push('\n');

    if !response.keywords.is_empty() {
        content.push_str("Keywords: ");
        content.push_str(&response.keywords.join(", "));
        content.push('\n');
    }

    content.push('\n');
    let answer = response.answer.trim();
    if answer.is_empty() {
        content.push_str("Answer: (no response)\n");
    } else {
        content.push_str(answer);
        if !answer.ends_with('\n') {
            content.push('\n');
        }
    }

    if !response.entries.is_empty() {
        content.push_str("\nSources:\n");
        for (idx, entry) in response.entries.iter().enumerate() {
            let file = std::path::Path::new(&entry.file_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(entry.file_path.as_str());
            let preview = first_content_line(&entry.content);
            content.push_str(&format!(
                "- [{idx}] {file}:{line} {preview}\n",
                idx = idx + 1,
                file = file,
                line = entry.line_number + 1,
                preview = preview
            ));
        }
    }

    match storage::append_entry(&app.config.data.log_path, &content) {
        Ok(_) => {
            app.update_logs();
            app.toast("Saved AI answer to memo.");
        }
        Err(_) => app.toast("Failed to save AI answer."),
    }
}

fn first_content_line(text: &str) -> String {
    for line in text.lines() {
        let trimmed = crate::models::strip_timestamp_prefix(line).trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn parse_ai_query(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.starts_with('?') {
        let stripped = trimmed.trim_start_matches('?').trim();
        if !stripped.is_empty() {
            return Some(stripped.to_string());
        }
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("ai:") {
        let stripped = trimmed[3..].trim();
        if !stripped.is_empty() {
            return Some(stripped.to_string());
        }
    }
    if lower.starts_with("ask:") {
        let stripped = trimmed[4..].trim();
        if !stripped.is_empty() {
            return Some(stripped.to_string());
        }
    }
    None
}

fn submit_ai_search(app: &mut App, question: &str) {
    if app.ai_search_receiver.is_some() {
        app.toast("AI search already running.");
        return;
    }

    if !app.config.gemini.enabled {
        app.toast("Gemini is disabled. Enable [gemini] in config.toml.");
        return;
    }

    let api_key = app.config.gemini.api_key.trim();
    let env_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
    if api_key.is_empty() && env_key.trim().is_empty() {
        app.toast("Set gemini.api_key in config.toml or GEMINI_API_KEY.");
        return;
    }

    app.last_search_query = Some(question.to_string());
    app.search_highlight_query = None;
    app.search_highlight_ready_at = None;
    app.is_search_result = false;
    app.update_logs();
    app.show_ai_loading_popup = true;
    app.ai_loading_question = Some(question.to_string());

    let receiver = gemini::spawn_ai_search(
        app.config.gemini.clone(),
        app.config.data.log_path.clone(),
        question.to_string(),
    );
    app.ai_search_receiver = Some(receiver);
    app.toast("AI search started...");
}
