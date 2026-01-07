use crate::{
    actions,
    app::App,
    config::google_token_path,
    integrations::{gemini, google},
    models,
    storage,
};
use chrono::{Duration, Local};
use std::sync::mpsc::TryRecvError;

pub fn tick(app: &mut App) {
    handle_day_rollover(app);
    handle_google_sync(app);
    handle_google_auth(app);
    handle_ai_search(app);

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

fn handle_ai_search(app: &mut App) {
    let result = {
        let Some(receiver) = app.ai_search_receiver.as_ref() else {
            return;
        };
        receiver.try_recv()
    };

    match result {
        Ok(gemini::AiSearchOutcome::Success(response)) => {
            app.ai_search_receiver = None;
            app.show_ai_loading_popup = false;
            app.ai_loading_question = None;
            app.ai_response_scroll = 0;
            app.ai_response = Some(response.clone());
            app.show_ai_response_popup = true;

            if !response.entries.is_empty() {
                app.logs = response.entries.clone();
                app.is_search_result = true;
                app.logs_state.select(Some(0));

                if let Some(keyword) = response.keywords.first() {
                    app.search_highlight_query = Some(keyword.to_string());
                    app.search_highlight_ready_at =
                        Some(Local::now() + Duration::milliseconds(150));
                } else {
                    app.search_highlight_query = None;
                    app.search_highlight_ready_at = None;
                }
            } else {
                app.is_search_result = false;
            }

            app.toast("AI search complete.");
        }
        Ok(gemini::AiSearchOutcome::Error(message)) => {
            app.ai_search_receiver = None;
            app.show_ai_loading_popup = false;
            app.ai_loading_question = None;
            app.show_ai_response_popup = false;
            app.toast(message);
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            app.ai_search_receiver = None;
            app.show_ai_loading_popup = false;
            app.ai_loading_question = None;
            app.show_ai_response_popup = false;
            app.toast("AI search stopped.");
        }
    }
}

fn handle_google_sync(app: &mut App) {
    let result = {
        let Some(receiver) = app.google_sync_receiver.as_ref() else {
            return;
        };
        receiver.try_recv()
    };

    match result {
        Ok(google::SyncOutcome::Success(report)) => {
            app.google_sync_receiver = None;
            app.update_logs();
            app.toast(format!("Google sync complete: {}", report.summary()));
        }
        Ok(google::SyncOutcome::AuthRequired(session)) => {
            app.google_sync_receiver = None;
            let token_path = google_token_path(&app.config);
            app.google_auth_display = Some(session.display.clone());
            app.show_google_auth_popup = true;
            app.google_auth_receiver = Some(google::spawn_auth_flow_poll(
                app.config.google.clone(),
                session,
                token_path,
            ));
            app.toast("Google auth required. Follow the popup instructions.");
        }
        Ok(google::SyncOutcome::Error(message)) => {
            app.google_sync_receiver = None;
            app.toast(message);
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            app.google_sync_receiver = None;
            app.toast("Google sync stopped.");
        }
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
