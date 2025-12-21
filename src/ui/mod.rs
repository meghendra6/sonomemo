use chrono::Local;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::app::{App, PLACEHOLDER_COMPOSE};
use crate::models::{InputMode, NavigateFocus, is_timestamped_line};
use ratatui::style::Stylize;
use regex::Regex;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

pub mod color_parser;
pub mod components;
pub mod popups;
pub mod theme;

use components::{centered_column, parse_markdown_spans, wrap_markdown_line};
use popups::{
    render_activity_popup, render_help_popup, render_mood_popup, render_path_popup,
    render_delete_entry_popup, render_discard_popup, render_pomodoro_popup,
    render_siren_popup, render_tag_popup, render_theme_switcher_popup, render_todo_popup,
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let tokens = theme::ThemeTokens::from_theme(&app.config.theme);
    let (main_area, search_area, status_area) = match app.input_mode {
        InputMode::Editing => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(f.area());
            (chunks[0], None, chunks[1])
        }
        InputMode::Search => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(5), Constraint::Length(1)])
                .split(f.area());
            (chunks[0], Some(chunks[1]), chunks[2])
        }
        InputMode::Navigate => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(f.area());
            (chunks[0], None, chunks[1])
        }
    };

    let mut cursor_area: Option<Rect> = None;

    if app.input_mode != InputMode::Editing {
        // Split top area: 70% logs, 30% tasks
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(main_area);

        let timeline_area = top_chunks[0];
        let tasks_area = top_chunks[1];
        let timeline_inner = Block::default().borders(Borders::ALL).inner(timeline_area);

        // Timeline log view
        let list_area_width = timeline_inner.width.saturating_sub(1).max(1) as usize;
        let timestamp_width: usize = 11; // "[HH:MM:SS] "
        let blank_timestamp = " ".repeat(timestamp_width);
        let timestamp_color = tokens.content_timestamp;

        let highlight_ready = if let Some(ready_at) = app.search_highlight_ready_at {
            if Local::now() >= ready_at {
                app.search_highlight_ready_at = None;
                true
            } else {
                false
            }
        } else {
            true
        };

        let mut search_regex: Option<Regex> = None;
        if app.is_search_result && highlight_ready {
            if let Some(query) = app.search_highlight_query.as_deref() {
                let query = query.trim();
                if !query.is_empty() {
                    search_regex = Regex::new(&format!("(?i){}", regex::escape(query))).ok();
                }
            }
        }

        let search_style = Style::default()
            .bg(tokens.ui_highlight)
            .add_modifier(Modifier::BOLD);
        let visible_start = app.timeline_ui_state.offset();
        let visible_end = visible_start.saturating_add(timeline_inner.height as usize);

    // Track current date for separator rendering and maintain index mapping
    let mut last_date: Option<String> = None;
    let mut items_with_separators: Vec<ListItem> = Vec::new();
    let mut ui_to_log_index: Vec<Option<usize>> = Vec::new(); // Maps UI index to actual log index
    let mut ui_index: usize = 0;

    for (log_idx, entry) in app.logs.iter().enumerate() {
        let entry_date = file_date(&entry.file_path);

        // Insert date separator if date changed (only for non-search view)
        if !app.is_search_result {
            if let Some(ref current_date) = entry_date {
                if last_date.as_ref() != Some(current_date) {
                    let separator_line = Line::from(vec![
                        Span::styled(
                            "─".repeat(list_area_width.saturating_sub(current_date.len() + 2)),
                            Style::default().fg(tokens.ui_muted),
                        ),
                        Span::styled(
                            format!(" {} ", current_date),
                            Style::default()
                                .fg(tokens.ui_accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    items_with_separators.push(ListItem::new(separator_line));
                    ui_to_log_index.push(None); // Separator has no corresponding log entry
                    last_date = Some(current_date.clone());
                    ui_index += 1;
                }
            }
        }

        // Render the actual entry
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut in_code_block = false;

        let date_prefix = if app.is_search_result {
            file_date(&entry.file_path)
        } else {
            None
        };
        let date_width: usize = if date_prefix.is_some() { 11 } else { 0 }; // "YYYY-MM-DD "
        let blank_date = " ".repeat(date_width);

        let entry_has_timestamp = entry
            .content
            .lines()
            .next()
            .is_some_and(|l| is_timestamped_line(l));
        let content_width = if entry_has_timestamp {
            list_area_width
                .saturating_sub(date_width)
                .saturating_sub(timestamp_width)
        } else {
            list_area_width.saturating_sub(date_width)
        };

        let highlight_here = if ui_index >= visible_start && ui_index < visible_end {
            search_regex.as_ref()
        } else {
            None
        };

        for (line_idx, raw_line) in entry.content.lines().enumerate() {
            let (ts_prefix, content_line) = if entry_has_timestamp && line_idx == 0 {
                // Safe due to timestamp format: "[HH:MM:SS] "
                (&raw_line[..timestamp_width], &raw_line[timestamp_width..])
            } else {
                ("", raw_line)
            };

            let is_fence = content_line.trim_start().starts_with("```");
            let line_in_code_block = in_code_block || is_fence;

            let wrapped = wrap_markdown_line(content_line, content_width);
            for (wrap_idx, wline) in wrapped.iter().enumerate() {
                let mut spans = Vec::new();

                if date_width > 0 {
                    let date_span = if line_idx == 0 && wrap_idx == 0 {
                        let date = date_prefix.clone().unwrap_or_default();
                        Span::styled(
                            format!("{date} "),
                            Style::default()
                                .fg(tokens.ui_muted)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        Span::raw(blank_date.clone())
                    };
                    spans.push(date_span);
                }

                if entry_has_timestamp {
                    let ts_span = if line_idx == 0 && wrap_idx == 0 {
                        Span::styled(ts_prefix.to_string(), Style::default().fg(timestamp_color))
                    } else {
                        Span::raw(blank_timestamp.clone())
                    };
                    spans.push(ts_span);
                }

                spans.extend(parse_markdown_spans(
                    wline,
                    &app.config.theme,
                    line_in_code_block,
                    highlight_here,
                    search_style,
                ));
                lines.push(Line::from(spans));
            }

            if is_fence {
                in_code_block = !in_code_block;
            }
        }
        items_with_separators.push(ListItem::new(Text::from(lines)));
        ui_to_log_index.push(Some(log_idx)); // This UI item corresponds to log_idx
        ui_index += 1;
    }

    let list_items = items_with_separators;

    // Convert selected log index to UI index for rendering
    let ui_selected_index = if let Some(selected_log_idx) = app.logs_state.selected() {
        ui_to_log_index
            .iter()
            .position(|&log_idx| log_idx == Some(selected_log_idx))
    } else {
        None
    };

    let is_timeline_focused =
        app.input_mode == InputMode::Navigate && app.navigate_focus == NavigateFocus::Timeline;
    let is_tasks_focused =
        app.input_mode == InputMode::Navigate && app.navigate_focus == NavigateFocus::Tasks;

    // Collect status information (used in both search and normal mode)
    let focus_info = if let Some(selected_idx) = app.logs_state.selected() {
        if let Some(entry) = app.logs.get(selected_idx) {
            let date = file_date(&entry.file_path).unwrap_or_else(|| "N/A".to_string());
            let time_info = entry
                .content
                .lines()
                .next()
                .and_then(|line| {
                    if is_timestamped_line(line) {
                        Some(&line[1..9]) // Extract HH:MM:SS
                    } else {
                        None
                    }
                })
                .unwrap_or("--:--:--");
            format!("📅 {} {}", date, time_info)
        } else {
            "📅 N/A".to_string()
        }
    } else {
        "📅 N/A".to_string()
    };

    let task_summary = if app.tasks.is_empty() {
        "Tasks 0".to_string()
    } else {
        format!("Tasks {} ({}✓)", app.tasks.len(), app.today_done_tasks)
    };

    let stats_summary = format!(
        "{} · {} · 🍅 {}",
        focus_info, task_summary, app.today_tomatoes
    );

    let pomodoro = if let Some(end_time) = app.pomodoro_end {
        let now = Local::now();
        if now < end_time {
            let remaining = end_time - now;
            let total_secs = remaining.num_seconds();
            let mins = remaining.num_minutes();
            let secs = total_secs % 60;

            let target = match app.pomodoro_target.as_ref() {
                Some(crate::models::PomodoroTarget::Task { text, .. }) => {
                    format!(" {}", truncate(text, 20))
                }
                _ => "".to_string(),
            };

            let elapsed_ratio = if let Some(start) = app.pomodoro_start {
                let total_duration = (end_time - start).num_seconds() as f32;
                let elapsed = (now - start).num_seconds() as f32;
                (elapsed / total_duration).min(1.0)
            } else {
                0.0
            };
            let bar_width = 10;
            let filled = (elapsed_ratio * bar_width as f32) as usize;
            let empty = bar_width - filled;
            let progress_bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

            let urgency = if mins < 1 {
                "🔴"
            } else if mins < 5 {
                "🟡"
            } else {
                "🟢"
            };

            format!(
                " [{} 🍅 {:02}:{:02} {}{}]",
                urgency, mins, secs, progress_bar, target
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let summary = if app.is_search_result {
        let mut parts = Vec::new();
        parts.push(format!("{} results", app.logs.len()));
        if let Some(query) = app.last_search_query.as_deref() {
            if !query.trim().is_empty() {
                parts.push(format!("\"{}\"", query.trim()));
            }
        }
        if let Some(selected) = app.logs_state.selected() {
            if !app.logs.is_empty() {
                parts.push(format!("Sel {}/{}", selected + 1, app.logs.len()));
            }
        }
        parts.push(stats_summary.clone());
        parts.join(" · ")
    } else {
        let time = Local::now().format("%Y-%m-%d %H:%M");
        let base = format!("{} · Entries {} · {}", time, app.logs.len(), stats_summary);
        format!("{base}{pomodoro}")
    };

    let title_label = if app.is_search_result {
        "SEARCH"
    } else {
        "TIMELINE"
    };
    let timeline_title_text = format!("{title_label} — {summary}");
    let timeline_title = truncate(
        &timeline_title_text,
        timeline_area.width.saturating_sub(4) as usize,
    );
    let timeline_border_color = if is_timeline_focused {
        tokens.ui_accent
    } else {
        tokens.ui_border_default
    };
    let timeline_title_style = if is_timeline_focused {
        Style::default()
            .fg(tokens.ui_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tokens.ui_muted)
    };
    let timeline_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(timeline_border_color))
        .title(Line::from(Span::styled(timeline_title, timeline_title_style)));

    let highlight_bg = tokens.ui_selection_bg;
    let logs_highlight_style = if is_timeline_focused {
        Style::default()
            .bg(highlight_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(highlight_bg)
    };

    let logs_list = List::new(list_items)
        .block(timeline_block)
        .highlight_symbol("")
        .highlight_style(logs_highlight_style);

    // Persist list offset across frames to avoid "cursor pinned" scroll behavior.
    app.timeline_ui_state.select(ui_selected_index);
    f.render_stateful_widget(logs_list, timeline_area, &mut app.timeline_ui_state);

    // Right panel: Today's tasks
    let tasks_inner = Block::default().borders(Borders::ALL).inner(tasks_area);
    let todo_area_width = tasks_inner.width.saturating_sub(1).max(1) as usize;

    let todos: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let mut line = String::new();
            line.push_str(&"  ".repeat(task.indent));
            line.push_str("- [ ] ");
            line.push_str(&task.text);

            let is_active_pomodoro = if let (
                Some(end_time),
                Some(crate::models::PomodoroTarget::Task {
                    file_path,
                    line_number,
                    ..
                }),
            ) = (app.pomodoro_end, app.pomodoro_target.as_ref())
            {
                if *file_path == task.file_path && *line_number == task.line_number {
                    let now = Local::now();
                    if now < end_time {
                        let remaining = end_time - now;
                        let mins = remaining.num_minutes();
                        let secs = remaining.num_seconds() % 60;

                        // Urgency indicator
                        let urgency = if mins < 1 {
                            "🔴"
                        } else if mins < 5 {
                            "🟡"
                        } else {
                            "🟢"
                        };

                        // Progress bar for the task: calculate based on actual duration
                        let elapsed_ratio = if let Some(start) = app.pomodoro_start {
                            let total_duration = (end_time - start).num_seconds() as f32;
                            let elapsed = (now - start).num_seconds() as f32;
                            (elapsed / total_duration).min(1.0)
                        } else {
                            0.0
                        };
                        let bar_width = 8;
                        let filled = (elapsed_ratio * bar_width as f32) as usize;
                        let empty = bar_width - filled;
                        let progress = format!("{}{}", "▓".repeat(filled), "░".repeat(empty));

                        line.push_str(&format!(
                            " {} {:02}:{:02} {}",
                            urgency, mins, secs, progress
                        ));
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if task.tomato_count > 0 && !is_active_pomodoro {
                if task.tomato_count <= 3 {
                    line.push(' ');
                    line.push_str(&"🍅".repeat(task.tomato_count));
                } else {
                    line.push_str(&format!(" 🍅×{}", task.tomato_count));
                }
            } else if task.tomato_count > 0 && is_active_pomodoro {
                // Show tomato count after timer for active task
                line.push_str(&format!(" (🍅{})", task.tomato_count));
            }

            let wrapped = wrap_markdown_line(&line, todo_area_width);
            let lines: Vec<Line<'static>> = wrapped
                .iter()
                .map(|l| {
                    Line::from(parse_markdown_spans(
                        l,
                        &app.config.theme,
                        false,
                        None,
                        Style::default(),
                    ))
                })
                .collect();
            ListItem::new(Text::from(lines))
        })
        .collect();

    let tasks_summary = format!(
        "Open {} · Done {} · 🍅 {}",
        app.tasks.len(),
        app.today_done_tasks,
        app.today_tomatoes
    );
    let tasks_title_text = format!("TASKS — {tasks_summary}");
    let tasks_title = truncate(
        &tasks_title_text,
        tasks_area.width.saturating_sub(4) as usize,
    );
    let tasks_border_color = if is_tasks_focused {
        tokens.ui_accent
    } else {
        tokens.ui_border_default
    };
    let tasks_title_style = if is_tasks_focused {
        Style::default()
            .fg(tokens.ui_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tokens.ui_muted)
    };
    let tasks_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tasks_border_color))
        .title(Line::from(Span::styled(tasks_title, tasks_title_style)));

    let highlight_bg = tokens.ui_selection_bg;
    let todo_highlight_style = if is_tasks_focused {
        Style::default()
            .bg(highlight_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(highlight_bg)
    };

    let todo_list = List::new(todos)
        .block(tasks_block)
        .highlight_symbol("")
        .highlight_style(todo_highlight_style);
    f.render_stateful_widget(todo_list, tasks_area, &mut app.tasks_state);
    }

    match app.input_mode {
        InputMode::Editing => {
            let editor_width = app.config.editor.column_width;
            let editor_area = centered_column(main_area, editor_width);
            let input_block = Block::default().borders(Borders::NONE);
            let input_inner = input_block.inner(editor_area);
            app.textarea.set_block(input_block);
            app.textarea.set_cursor_style(Style::default().reversed());

            if input_inner.height > 0 {
                let (cursor_row, _) = app.textarea.cursor();
                let cursor_row_u16 = (cursor_row.min(u16::MAX as usize)) as u16;
                app.textarea_viewport_row = next_scroll_top(
                    app.textarea_viewport_row,
                    cursor_row_u16,
                    input_inner.height,
                );
            }

            let lines = app.textarea.lines();
            let is_empty = lines.iter().all(|line| line.trim().is_empty());
            let visible_start = app.textarea_viewport_row as usize;
            let visible_height = input_inner.height as usize;

            let mut rendered: Vec<Line<'static>> = Vec::new();
            if is_empty {
                rendered.push(Line::from(Span::styled(
                    PLACEHOLDER_COMPOSE,
                    Style::default()
                        .fg(tokens.ui_muted)
                        .add_modifier(Modifier::DIM),
                )));
            } else {
                let (cursor_row, _) = app.textarea.cursor();
                for (idx, line) in lines
                    .iter()
                    .enumerate()
                    .skip(visible_start)
                    .take(visible_height)
                {
                    let is_cursor = idx == cursor_row;
                    rendered.push(compose_render_line(line, &tokens, is_cursor));
                }
            }

            let paragraph = Paragraph::new(rendered).style(Style::default().fg(tokens.ui_fg));
            f.render_widget(paragraph, editor_area);
            cursor_area = Some(input_inner);
        }
        InputMode::Search => {
            if let Some(search_area) = search_area {
                let search_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(1)])
                    .split(search_area);

                let results_hint = if app.is_search_result {
                    format!("Results {}", app.logs.len())
                } else {
                    "Results —".to_string()
                };

                let header = Paragraph::new(Line::from(vec![
                    Span::styled(
                        "Search",
                        Style::default()
                            .fg(tokens.ui_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(results_hint, Style::default().fg(tokens.ui_muted)),
                    Span::raw("  "),
                    Span::styled(
                        "Enter: apply · Esc: cancel · Ctrl+L: clear",
                        Style::default().fg(tokens.ui_muted),
                    ),
                ]))
                .style(Style::default().fg(tokens.ui_fg));
                f.render_widget(header, search_chunks[0]);

                let input_block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .border_style(Style::default().fg(tokens.ui_border_search));
                let input_inner = input_block.inner(search_chunks[1]);
                app.textarea.set_block(input_block);
                app.textarea
                    .set_cursor_line_style(Style::default().bg(tokens.ui_cursorline_bg));
                app.textarea
                    .set_selection_style(Style::default().bg(tokens.ui_selection_bg));
                app.textarea.set_cursor_style(Style::default().reversed());
                f.render_widget(&app.textarea, search_chunks[1]);
                cursor_area = Some(input_inner);
            }
        }
        InputMode::Navigate => {}
    }

    // Manual cursor position setting (required for Korean/CJK IME support)
    if let Some(inner) = cursor_area {
        if inner.height > 0 && inner.width > 0 {
            let (cursor_row, cursor_col) = app.textarea.cursor();
            let cursor_row_u16 = (cursor_row.min(u16::MAX as usize)) as u16;
            app.textarea_viewport_row =
                next_scroll_top(app.textarea_viewport_row, cursor_row_u16, inner.height);

            if let Some(line) = app.textarea.lines().get(cursor_row) {
                let visual_col: usize = line
                    .chars()
                    .take(cursor_col)
                    .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                    .sum();

                let row_in_view = cursor_row_u16.saturating_sub(app.textarea_viewport_row);
                let row_in_view = row_in_view.min(inner.height.saturating_sub(1));

                let col_in_view = (visual_col.min(u16::MAX as usize)) as u16;
                let col_in_view = col_in_view.min(inner.width.saturating_sub(1));

                f.set_cursor_position((inner.x + col_in_view, inner.y + row_in_view));
            }
        }
    }

    render_status_bar(f, status_area, app, &tokens);

    // Render popups (order matters: later ones appear on top)
    if app.show_activity_popup {
        render_activity_popup(f, app);
    }

    if app.show_mood_popup {
        render_mood_popup(f, app);
    }

    if app.show_todo_popup {
        render_todo_popup(f, app);
    }

    if app.show_tag_popup {
        render_tag_popup(f, app);
    }

    if app.show_help_popup {
        render_help_popup(f, app);
    }
    if app.show_theme_popup {
        render_theme_switcher_popup(f, app);
    }
    if app.show_delete_entry_popup {
        render_delete_entry_popup(f);
    }
    if app.show_discard_popup {
        render_discard_popup(f, app);
    }

    if app.show_pomodoro_popup {
        render_pomodoro_popup(f, app);
    }

    if app.pomodoro_alert_expiry.is_some() {
        render_siren_popup(f, app);
    }

    if app.show_path_popup {
        render_path_popup(f, app);
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}

fn compose_render_line(
    line: &str,
    tokens: &theme::ThemeTokens,
    is_cursor: bool,
) -> Line<'static> {
    let (indent_level, indent, rest) = split_indent(line);
    let mut spans: Vec<Span<'static>> = Vec::new();

    if !indent.is_empty() {
        spans.push(Span::raw(indent.to_string()));
    }

    if let Some((bullet, tail)) = replace_list_bullet(rest, indent_level) {
        spans.push(Span::styled(
            format!("{bullet} "),
            Style::default()
                .fg(tokens.ui_accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(tail.to_string()));
    } else {
        spans.push(Span::raw(rest.to_string()));
    }

    let mut rendered = Line::from(spans);
    if is_cursor {
        rendered.style = Style::default().bg(tokens.ui_cursorline_bg);
    }
    rendered
}

fn split_indent(line: &str) -> (usize, &str, &str) {
    let mut spaces = 0usize;
    let mut split_at = 0usize;
    for (idx, ch) in line.char_indices() {
        match ch {
            ' ' => {
                spaces += 1;
                split_at = idx + ch.len_utf8();
            }
            '\t' => {
                spaces += 4;
                split_at = idx + ch.len_utf8();
            }
            _ => break,
        }
    }
    let (indent, rest) = line.split_at(split_at);
    (spaces / 2, indent, rest)
}

fn replace_list_bullet<'a>(rest: &'a str, indent_level: usize) -> Option<(char, &'a str)> {
    if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        return Some((bullet_for_level(indent_level), &rest[2..]));
    }
    None
}

fn bullet_for_level(level: usize) -> char {
    match level {
        0 => '•',
        1 => '◦',
        2 => '▪',
        _ => '▫',
    }
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App, tokens: &theme::ThemeTokens) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let mode_label = match app.input_mode {
        InputMode::Navigate => match app.navigate_focus {
            NavigateFocus::Timeline => "NAV:TL",
            NavigateFocus::Tasks => "NAV:TS",
        },
        InputMode::Editing => "[Compose]",
        InputMode::Search => "SEARCH",
    };

    let file_label = status_file_label(app);
    let dirty_mark = if app.input_mode == InputMode::Editing && app.composer_dirty {
        "*"
    } else {
        ""
    };

    let left_spans = vec![
        Span::styled(
            format!(" {mode_label} "),
            Style::default()
                .fg(tokens.ui_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{file_label}{dirty_mark}"),
            Style::default()
                .fg(tokens.ui_fg)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let mut right_plain = String::new();
    let mut right_spans = Vec::new();

    if matches!(app.input_mode, InputMode::Editing | InputMode::Search) {
        let (row, col) = app.textarea.cursor();
        let cursor_text = format!("Ln {}, Col {}", row + 1, col + 1);
        right_plain.push_str(&cursor_text);
        right_spans.push(Span::styled(
            cursor_text,
            Style::default().fg(tokens.ui_muted),
        ));
    }

    if let Some(toast) = app.toast_message.as_deref() {
        if !toast.is_empty() {
            if !right_plain.is_empty() {
                right_plain.push_str("  ");
                right_spans.push(Span::raw("  "));
            }
            right_plain.push_str(toast);
            right_spans.push(Span::styled(
                toast,
                Style::default()
                    .fg(tokens.ui_toast_info)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    let min_left_width = 10u16;
    let mut right_width = UnicodeWidthStr::width(right_plain.as_str()) as u16;
    let max_right = area.width.saturating_sub(min_left_width);
    right_width = right_width.min(max_right);

    if right_plain.is_empty() || right_width == 0 {
        let left = Paragraph::new(Line::from(left_spans))
            .style(Style::default().fg(tokens.ui_fg).bg(tokens.ui_bg));
        f.render_widget(left, area);
        return;
    }

    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(right_width)])
        .split(area);

    let left = Paragraph::new(Line::from(left_spans))
        .style(Style::default().fg(tokens.ui_fg).bg(tokens.ui_bg));
    f.render_widget(left, status_chunks[0]);

    let right = Paragraph::new(Line::from(right_spans))
        .style(Style::default().fg(tokens.ui_fg).bg(tokens.ui_bg))
        .alignment(Alignment::Right);
    f.render_widget(right, status_chunks[1]);
}

fn status_file_label(app: &App) -> String {
    if app.input_mode == InputMode::Navigate {
        let selected_path = match app.navigate_focus {
            NavigateFocus::Timeline => app
                .logs_state
                .selected()
                .and_then(|i| app.logs.get(i))
                .map(|entry| entry.file_path.as_str()),
            NavigateFocus::Tasks => app
                .tasks_state
                .selected()
                .and_then(|i| app.tasks.get(i))
                .map(|task| task.file_path.as_str()),
        };

        if let Some(path) = selected_path {
            if let Some(name) = Path::new(path).file_name().and_then(|s| s.to_str()) {
                return name.to_string();
            }
        }
    }

    if let Some(editing) = app.editing_entry.as_ref() {
        if let Some(name) = Path::new(&editing.file_path)
            .file_name()
            .and_then(|s| s.to_str())
        {
            return name.to_string();
        }
    }

    if app.is_search_result || app.input_mode == InputMode::Search {
        return "Search Results".to_string();
    }

    format!("{}.md", app.active_date)
}

fn next_scroll_top(prev_top: u16, cursor: u16, len: u16) -> u16 {
    if cursor < prev_top {
        cursor
    } else if prev_top.saturating_add(len) <= cursor {
        cursor.saturating_add(1).saturating_sub(len)
    } else {
        prev_top
    }
}

fn file_date(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::compose_render_line;
    use crate::config::Theme;
    use crate::ui::theme::ThemeTokens;

    fn line_to_string(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn renders_bullets_with_indentation_levels() {
        let tokens = ThemeTokens::from_theme(&Theme::default());

        let top = compose_render_line("* item1", &tokens, false);
        assert_eq!(line_to_string(&top), "• item1");

        let nested = compose_render_line("  * sub1", &tokens, false);
        assert_eq!(line_to_string(&nested), "  ◦ sub1");

        let deep = compose_render_line("    - sub2", &tokens, false);
        assert_eq!(line_to_string(&deep), "    ▪ sub2");
    }

    #[test]
    fn preserves_non_list_lines_verbatim() {
        let tokens = ThemeTokens::from_theme(&Theme::default());
        let line = compose_render_line("plain text", &tokens, false);
        assert_eq!(line_to_string(&line), "plain text");
    }
}
