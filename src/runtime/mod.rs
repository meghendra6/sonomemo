use crate::{app::App, models, storage};
use chrono::{Duration, Local};

pub fn tick(app: &mut App) {
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
