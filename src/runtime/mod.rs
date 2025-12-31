use crate::{actions, app::App, integrations::google, models, storage};
use chrono::{Duration, Local};
use std::sync::mpsc::TryRecvError;

pub fn tick(app: &mut App) {
    handle_day_rollover(app);
    handle_google_auth(app);

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

fn handle_google_auth(app: &mut App) {
    let result = {
        let Some(receiver) = app.google_auth_receiver.as_ref() else {
            return;
        };
        receiver.try_recv()
    };

    match result {
        Ok(google::AuthPollResult::Success) => {
            app.google_auth_receiver = None;
            app.show_google_auth_popup = false;
            app.google_auth_display = None;
            app.toast("Google auth complete. Syncing now...");
            actions::sync_google(app);
        }
        Ok(google::AuthPollResult::Error(message)) => {
            app.google_auth_receiver = None;
            app.show_google_auth_popup = false;
            app.google_auth_display = None;
            app.toast(format!("Google auth failed: {message}"));
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            app.google_auth_receiver = None;
            app.show_google_auth_popup = false;
            app.google_auth_display = None;
            app.toast("Google auth stopped.");
        }
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
