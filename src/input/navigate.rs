use crate::{
    actions,
    app::App,
    config::key_match,
    models::{self, InputMode},
};
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.fold_toggle)
    {
        app.toggle_entry_fold();
    } else if app.navigate_focus == models::NavigateFocus::Timeline
        && key_match(&key, &app.config.keybindings.timeline.fold_cycle)
    {
        app.cycle_fold_state();
    } else if key_match(&key, &app.config.keybindings.global.help) {
        app.show_help_popup = true;
    } else if key_match(&key, &app.config.keybindings.global.tags) {
        actions::open_tag_popup(app);
    } else if key_match(&key, &app.config.keybindings.global.sync_google) {
        actions::sync_google(app);
    } else if key_match(&key, &app.config.keybindings.global.quit) {
        app.quit();
    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
        let handled = match key.code {
            KeyCode::Char('h')
                if matches!(
                    app.navigate_focus,
                    models::NavigateFocus::Agenda | models::NavigateFocus::Tasks
                ) =>
            {
                app.set_navigate_focus(models::NavigateFocus::Timeline);
                true
            }
            KeyCode::Char('j') if app.navigate_focus == models::NavigateFocus::Agenda => {
                app.set_navigate_focus(models::NavigateFocus::Tasks);
                true
            }
            KeyCode::Char('k') if app.navigate_focus == models::NavigateFocus::Tasks => {
                app.set_navigate_focus(models::NavigateFocus::Agenda);
                true
            }
            KeyCode::Char('l') if app.navigate_focus == models::NavigateFocus::Timeline => {
                let next_focus = if app.last_navigate_focus
                    == Some(models::NavigateFocus::Tasks)
                {
                    models::NavigateFocus::Tasks
                } else {
                    models::NavigateFocus::Agenda
                };
                app.set_navigate_focus(next_focus);
                true
            }
            _ => false,
        };
        if handled {
            return;
        }
    } else if key_match(&key, &app.config.keybindings.global.agenda) {
        actions::focus_agenda_panel(app);
    } else if key_match(&key, &app.config.keybindings.global.focus_next)
        || key_match(&key, &app.config.keybindings.global.focus_prev)
    {
        let next_focus = match app.navigate_focus {
            models::NavigateFocus::Timeline => models::NavigateFocus::Agenda,
            models::NavigateFocus::Agenda => models::NavigateFocus::Tasks,
            models::NavigateFocus::Tasks => models::NavigateFocus::Timeline,
        };
        app.set_navigate_focus(next_focus);
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
        && key_match(&key, &app.config.keybindings.tasks.filter_toggle)
    {
        app.cycle_task_filter();
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.filter_open)
    {
        app.set_task_filter(models::TaskFilter::Open);
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.filter_done)
    {
        app.set_task_filter(models::TaskFilter::Done);
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.filter_all)
    {
        app.set_task_filter(models::TaskFilter::All);
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
        actions::toggle_todo_in_timeline(app);
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.up)
    {
        app.agenda_move_selection(-1);
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.down)
    {
        app.agenda_move_selection(1);
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.open)
    {
        actions::open_agenda_preview(app);
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.toggle)
    {
        actions::toggle_agenda_task(app);
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.filter)
    {
        app.cycle_agenda_filter();
        app.set_agenda_selected_day(app.agenda_selected_day);
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.toggle_unscheduled)
    {
        app.toggle_agenda_unscheduled();
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.prev_day)
    {
        app.set_agenda_selected_day(app.agenda_selected_day - chrono::Duration::days(1));
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.next_day)
    {
        app.set_agenda_selected_day(app.agenda_selected_day + chrono::Duration::days(1));
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.prev_week)
    {
        app.set_agenda_selected_day(app.agenda_selected_day - chrono::Duration::days(7));
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.next_week)
    {
        app.set_agenda_selected_day(app.agenda_selected_day + chrono::Duration::days(7));
    } else if app.navigate_focus == models::NavigateFocus::Agenda
        && key_match(&key, &app.config.keybindings.agenda.today)
    {
        app.set_agenda_selected_day(Local::now().date_naive());
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.toggle)
    {
        actions::complete_task_chain(app);
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.priority_cycle)
    {
        actions::cycle_task_priority(app);
    } else if (app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.start_pomodoro))
        || key_match(&key, &app.config.keybindings.global.pomodoro)
    {
        actions::open_or_toggle_pomodoro_for_selected_task(app);
    } else if key_match(&key, &app.config.keybindings.global.activity) {
        actions::open_activity_popup(app);
    } else if key_match(&key, &app.config.keybindings.global.log_dir) {
        app.show_path_popup = true;
    } else if key_match(&key, &app.config.keybindings.global.theme_switcher) {
        actions::open_theme_switcher(app);
    } else if key_match(&key, &app.config.keybindings.global.editor_style_switcher) {
        actions::open_editor_style_switcher(app);
    }
}
