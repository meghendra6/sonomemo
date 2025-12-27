use crate::{
    app::App,
    config::key_match,
    editor::markdown,
    models::{EditorMode, InputMode},
    storage,
};
use chrono::{Duration, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_editing_mode(app: &mut App, key: KeyEvent) {
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
            markdown::indent_or_outdent_list_line(&mut app.textarea, true);
            return;
        }

        if key_match(&key, &app.config.keybindings.composer.outdent) {
            markdown::indent_or_outdent_list_line(&mut app.textarea, false);
            return;
        }

        if key_match(&key, &app.config.keybindings.composer.newline) {
            markdown::insert_newline_with_auto_indent(&mut app.textarea);
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
            EditorMode::Insert => crate::exit_insert_mode(app),
            EditorMode::Visual(_) => crate::exit_visual_mode(app),
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
        EditorMode::Normal => crate::handle_editor_normal(app, key),
        EditorMode::Insert => crate::handle_editor_insert(app, key),
        EditorMode::Visual(kind) => crate::handle_editor_visual(app, key, kind),
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
            } else if let Some((prefix, _)) =
                crate::models::split_timestamp_line(&editing.timestamp_prefix)
            {
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
