use crate::{
    actions,
    app::App,
    config::key_match,
    models::{self, InputMode},
};
use crossterm::event::{KeyCode, KeyEvent};

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
    } else if app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.toggle)
    {
        actions::complete_task_chain(app);
    } else if (app.navigate_focus == models::NavigateFocus::Tasks
        && key_match(&key, &app.config.keybindings.tasks.start_pomodoro))
        || key_match(&key, &app.config.keybindings.global.pomodoro)
    {
        actions::open_or_toggle_pomodoro_for_selected_task(app);
    } else if key_match(&key, &app.config.keybindings.global.activity) {
        actions::open_activity_popup(app);
    } else if key_match(&key, &app.config.keybindings.global.agenda) {
        actions::open_agenda_popup(app);
    } else if key_match(&key, &app.config.keybindings.global.log_dir) {
        app.show_path_popup = true;
    } else if key_match(&key, &app.config.keybindings.global.theme_switcher) {
        actions::open_theme_switcher(app);
    } else if key_match(&key, &app.config.keybindings.global.editor_style_switcher) {
        actions::open_editor_style_switcher(app);
    }
}
