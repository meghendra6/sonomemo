use crate::{
    app::App,
    config::{EditorStyle, ThemePreset},
    models, storage,
};
use chrono::{Duration, Local};

const AGENDA_DAYS_BACK: i64 = 7;

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

pub fn open_activity_popup(app: &mut App) {
    if let Ok(data) = storage::get_activity_stats(&app.config.data.log_path) {
        app.activity_data = data;
        app.show_activity_popup = true;
    }
}

pub fn open_agenda_popup(app: &mut App) {
    let today = Local::now().date_naive();
    let start = today - Duration::days(AGENDA_DAYS_BACK);
    let end = today;
    let items = storage::read_tasks_for_date_range(&app.config.data.log_path, start, end)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| !item.is_done)
        .collect::<Vec<_>>();

    if items.is_empty() {
        app.toast("No agenda items.");
        return;
    }

    app.agenda_items = items;
    app.agenda_state.select(Some(0));
    app.show_agenda_popup = true;
}

pub fn jump_to_agenda_item(app: &mut App) {
    let Some(selected) = app.agenda_state.selected() else {
        app.toast("No agenda item selected.");
        return;
    };
    let Some(item) = app.agenda_items.get(selected).cloned() else {
        app.toast("No agenda item selected.");
        return;
    };

    let target_date = item.date;
    if let Some(start) = app.loaded_start_date {
        if target_date < start {
            app.loaded_start_date = Some(target_date);
            app.update_logs();
        }
    } else {
        app.loaded_start_date = Some(target_date);
        app.update_logs();
    }

    let target_path = &item.file_path;
    let target_line = item.line_number;
    if let Some(index) = app.logs.iter().position(|entry| {
        entry.file_path == *target_path
            && entry.line_number <= target_line
            && target_line <= entry.end_line
    }) {
        app.logs_state.select(Some(index));
        app.navigate_focus = models::NavigateFocus::Timeline;
        app.show_agenda_popup = false;
    } else {
        app.toast("Agenda item not found.");
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
}
