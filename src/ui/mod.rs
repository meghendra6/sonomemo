use chrono::{Local, Timelike};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, PLACEHOLDER_COMPOSE};
use crate::models::{
    AgendaItemKind, EditorMode, InputMode, NavigateFocus, VisualKind,
    is_heading_timestamp_line, is_timestamped_line, split_timestamp_line,
};
use ratatui::style::Stylize;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle as SyntectFontStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use unicode_width::UnicodeWidthStr;

pub mod color_parser;
pub mod components;
pub mod popups;
pub mod theme;

use components::{centered_column, parse_markdown_spans, wrap_markdown_line};
use popups::{
    render_activity_popup, render_date_picker_popup, render_delete_entry_popup,
    render_editor_style_popup, render_exit_popup, render_help_popup, render_memo_preview_popup,
    render_mood_popup, render_path_popup, render_pomodoro_popup, render_siren_popup,
    render_tag_popup, render_theme_switcher_popup, render_todo_popup,
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
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(5),
                    Constraint::Length(1),
                ])
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
        let right_panel = top_chunks[1];
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(right_panel);
        let agenda_area = right_chunks[0];
        let tasks_area = right_chunks[1];
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
        if app.is_search_result
            && highlight_ready
            && let Some(query) = app.search_highlight_query.as_deref()
        {
            let query = query.trim();
            if !query.is_empty() {
                search_regex = Regex::new(&format!("(?i){}", regex::escape(query))).ok();
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
        let mut entry_line_counts: Vec<usize> = Vec::new(); // Track line count for each log entry
        let mut tall_entry_lines: Option<Vec<Line<'static>>> = None; // Lines for tall selected entry
        let mut ui_index: usize = 0;
        let selected_log_idx = app.logs_state.selected();
        let viewport_height = timeline_inner.height as usize;

        for (log_idx, entry) in app.logs.iter().enumerate() {
            let entry_date = file_date(&entry.file_path);

            // Insert date separator if date changed (only for non-search view)
            if !app.is_search_result
                && let Some(ref current_date) = entry_date
                && last_date.as_ref() != Some(current_date)
            {
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

            let first_line = entry.content.lines().next();
            let entry_has_timestamp = first_line.is_some_and(is_timestamped_line);
            let heading_timestamp_prefix = first_line
                .and_then(|l| {
                    if is_heading_timestamp_line(l) {
                        split_timestamp_line(l).map(|(prefix, _)| prefix)
                    } else {
                        None
                    }
                })
                .map(|prefix| prefix.trim_end_matches(' '));
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

            let first_body_index = if heading_timestamp_prefix.is_some() {
                1
            } else {
                0
            };

            let total_display_lines = app.entry_display_line_count(entry);
            let visible_raw_limit = app.entry_fold_limit(entry).unwrap_or(total_display_lines);
            let is_folded = total_display_lines > visible_raw_limit;
            let show_marker = total_display_lines > 1;
            let fold_marker = if show_marker {
                if is_folded {
                    "▶ "
                } else {
                    "▼ "
                }
            } else {
                ""
            };
            let marker_width = if show_marker { 2 } else { 0 };
            let mut displayed_raw = 0usize;

            for (line_idx, raw_line) in entry.content.lines().enumerate() {
                if heading_timestamp_prefix.is_some() && line_idx == 0 {
                    continue;
                }
                if displayed_raw >= visible_raw_limit {
                    break;
                }

                let (ts_prefix, content_line) =
                    if entry_has_timestamp && line_idx == first_body_index {
                        if let Some(prefix) = heading_timestamp_prefix {
                            (prefix, raw_line)
                        } else if let Some((prefix, rest)) = split_timestamp_line(raw_line) {
                            (prefix, rest)
                        } else {
                            ("", raw_line)
                        }
                    } else {
                        ("", raw_line)
                    };

                let is_fence = content_line.trim_start().starts_with("```");
                let line_in_code_block = in_code_block || is_fence;

                let is_first_visible = displayed_raw == 0;
                let wrap_width = if is_first_visible {
                    content_width.saturating_sub(marker_width).max(1)
                } else {
                    content_width.max(1)
                };
                let wrapped = wrap_markdown_line(content_line, wrap_width);
                for (wrap_idx, wline) in wrapped.iter().enumerate() {
                    let mut spans = Vec::new();

                    if date_width > 0 {
                        let date_span = if line_idx == first_body_index && wrap_idx == 0 {
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
                        let ts_span = if line_idx == first_body_index && wrap_idx == 0 {
                            let mut ts_text = ts_prefix.to_string();
                            if !ts_text.is_empty() && !ts_text.ends_with(' ') {
                                ts_text.push(' ');
                            }
                            Span::styled(ts_text, Style::default().fg(timestamp_color))
                        } else {
                            Span::raw(blank_timestamp.clone())
                        };
                        spans.push(ts_span);
                    }

                    if is_first_visible && wrap_idx == 0 && show_marker {
                        spans.push(Span::styled(
                            fold_marker.to_string(),
                            Style::default().fg(tokens.ui_muted),
                        ));
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
                displayed_raw += 1;
            }

            if is_folded && !lines.is_empty() {
                if let Some(last) = lines.last_mut() {
                    last.spans.push(Span::raw(" ..."));
                }
            }

            // Track line count for this entry
            let total_lines = lines.len();
            entry_line_counts.push(total_lines);

            // For tall selected entries, we'll handle them specially below
            let is_selected = selected_log_idx == Some(log_idx);
            let is_tall = total_lines > viewport_height && viewport_height > 0;

            // Store lines for tall selected entry (for Paragraph rendering)
            if is_selected && is_tall {
                tall_entry_lines = Some(lines.clone());
            }

            items_with_separators.push(ListItem::new(Text::from(lines)));
            ui_to_log_index.push(Some(log_idx)); // This UI item corresponds to log_idx

            ui_index += 1;
        }

        // Store the line count of the selected entry for navigation logic
        let selected_entry_line_count = selected_log_idx
            .and_then(|idx| entry_line_counts.get(idx).copied())
            .unwrap_or(0);
        app.selected_entry_line_count = selected_entry_line_count;
        app.timeline_viewport_height = viewport_height;

        // Calculate whether selected entry is tall (needs special rendering)
        let selected_entry_is_tall =
            selected_entry_line_count > viewport_height && viewport_height > 0;

        // Handle scroll-to-bottom: update scroll offset and clear the flag
        if app.entry_scroll_to_bottom && selected_entry_is_tall {
            app.entry_scroll_offset = selected_entry_line_count.saturating_sub(viewport_height);
            app.entry_scroll_to_bottom = false;
        } else if app.entry_scroll_to_bottom {
            app.entry_scroll_to_bottom = false;
        }

        // Clamp scroll offset to valid range
        if selected_entry_is_tall {
            let max_offset = selected_entry_line_count.saturating_sub(viewport_height);
            app.entry_scroll_offset = app.entry_scroll_offset.min(max_offset);
        } else {
            app.entry_scroll_offset = 0;
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
        let is_agenda_focused =
            app.input_mode == InputMode::Navigate && app.navigate_focus == NavigateFocus::Agenda;
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
                    .and_then(|line| split_timestamp_line(line).map(|(prefix, _)| &prefix[1..9]))
                    .unwrap_or("--:--:--");
                format!("📅 {} {}", date, time_info)
            } else {
                "📅 N/A".to_string()
            }
        } else {
            "📅 N/A".to_string()
        };

        let (open_count, done_count) = app.task_counts();
        let task_summary = if open_count + done_count == 0 {
            "Tasks 0".to_string()
        } else {
            format!("Tasks {} ({}✓)", open_count, done_count)
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
            if let Some(query) = app.last_search_query.as_deref()
                && !query.trim().is_empty()
            {
                parts.push(format!("\"{}\"", query.trim()));
            }
            if let Some(selected) = app.logs_state.selected()
                && !app.logs.is_empty()
            {
                parts.push(format!("Sel {}/{}", selected + 1, app.logs.len()));
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

        // Add scroll indicator for tall entries
        let scroll_indicator = if selected_entry_line_count > viewport_height && viewport_height > 0
        {
            let max_offset = selected_entry_line_count.saturating_sub(viewport_height);
            let can_scroll_up = app.entry_scroll_offset > 0;
            let can_scroll_down = app.entry_scroll_offset < max_offset;
            match (can_scroll_up, can_scroll_down) {
                (true, true) => " ↕",
                (true, false) => " ↑",
                (false, true) => " ↓",
                (false, false) => "",
            }
        } else {
            ""
        };

        let timeline_title_text = format!("{title_label}{scroll_indicator} — {summary}");
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
            .title(Line::from(Span::styled(
                timeline_title,
                timeline_title_style,
            )));

        let highlight_bg = tokens.ui_selection_bg;
        let logs_highlight_style = if is_timeline_focused {
            Style::default()
                .bg(highlight_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(highlight_bg)
        };

        // When the selected entry is tall (taller than viewport), render it as a Paragraph
        // with scroll support instead of using List (which doesn't handle tall items well)
        if selected_entry_is_tall {
            if let Some(lines) = tall_entry_lines {
                // Render the tall selected entry as a Paragraph with scroll
                let selected_text = Text::from(lines);
                let scroll_offset = app.entry_scroll_offset as u16;

                let paragraph = Paragraph::new(selected_text)
                    .block(timeline_block)
                    .scroll((scroll_offset, 0))
                    .style(logs_highlight_style);

                f.render_widget(paragraph, timeline_area);
            } else {
                // Fallback to list rendering
                let logs_list = List::new(list_items)
                    .block(timeline_block)
                    .highlight_symbol("")
                    .highlight_style(logs_highlight_style);
                app.timeline_ui_state.select(ui_selected_index);
                f.render_stateful_widget(logs_list, timeline_area, &mut app.timeline_ui_state);
            }
        } else {
            // Normal list rendering for non-tall entries
            let logs_list = List::new(list_items)
                .block(timeline_block)
                .highlight_symbol("")
                .highlight_style(logs_highlight_style);
            app.timeline_ui_state.select(ui_selected_index);
            f.render_stateful_widget(logs_list, timeline_area, &mut app.timeline_ui_state);
        }

        render_agenda_panel(f, app, agenda_area, is_agenda_focused, &tokens);

        // Right panel: Today's tasks
        let tasks_inner = Block::default().borders(Borders::ALL).inner(tasks_area);
        let todo_area_width = tasks_inner.width.saturating_sub(1).max(1) as usize;

        let todos: Vec<ListItem> = app
            .tasks
            .iter()
            .map(|task| {
                let mut line = String::new();
                line.push_str(&"  ".repeat(task.indent));
                if task.is_done {
                    line.push_str("- [x] ");
                } else {
                    line.push_str("- [ ] ");
                }
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

        let (open_count, done_count) = app.task_counts();
        let tasks_summary = format!(
            "Open {} · Done {} · 🍅 {}",
            open_count, done_count, app.today_tomatoes
        );
        let filter_label = app.task_filter_label();
        let filter_summary = format!("{filter_label}: {}", app.tasks.len());
        let tasks_title_text = format!("TASKS ({filter_summary}) — {tasks_summary}");
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

            let show_line_numbers = app.config.ui.line_numbers;
            let prefix_width = compose_prefix_width(show_line_numbers) as usize;
            let content_width = (input_inner.width as usize)
                .saturating_sub(prefix_width)
                .max(1);

            let lines = app.textarea.lines();
            let is_empty = lines.iter().all(|line| line.trim().is_empty());
            let visible_height = input_inner.height as usize;
            let (cursor_row, cursor_col) = app.textarea.cursor();
            app.textarea_viewport_height = visible_height;

            // Build visual lines with wrapping and track cursor position
            let mut visual_lines: Vec<Line<'static>> = Vec::new();
            let mut cursor_visual_row: usize = 0;
            let mut cursor_visual_col: usize = 0;

            if is_empty {
                visual_lines.push(compose_placeholder_line(
                    PLACEHOLDER_COMPOSE,
                    &tokens,
                    show_line_numbers,
                    true,
                ));
            } else {
                let (code_block_info, cursor_block_id) =
                    collect_code_block_info(lines, cursor_row);
                let syntax_set = syntax_set();
                let syntax_theme = select_syntax_theme(syntax_theme_set(), &tokens);
                let code_bg = code_block_background(&tokens);
                let mut active_block_id: Option<usize> = None;
                let mut highlighter: Option<HighlightLines> = None;

                for (logical_idx, line) in lines.iter().enumerate() {
                    let line_info = &code_block_info[logical_idx];
                    if line_info.block_id != active_block_id {
                        active_block_id = line_info.block_id;
                        highlighter = None;
                        if active_block_id.is_some() {
                            let syntax = syntax_for_language(syntax_set, line_info.language.as_deref());
                            highlighter = Some(HighlightLines::new(syntax, syntax_theme));
                        }
                    }

                    let show_fence = line_info.block_id.is_some()
                        && cursor_block_id.is_some()
                        && line_info.block_id == cursor_block_id;
                    let (display_line, styled_segments) = if line_info.is_fence {
                        let display = if show_fence {
                            line.to_string()
                        } else {
                            hide_fence_marker(line)
                        };
                        let mut style = if show_fence {
                            Style::default()
                                .fg(tokens.ui_accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                                .fg(tokens.ui_muted)
                                .add_modifier(Modifier::DIM)
                        };
                        if let Some(bg) = code_bg {
                            style = style.bg(bg);
                        }
                        let segments = vec![StyledSegment {
                            text: display.clone(),
                            style,
                        }];
                        (display, Some(segments))
                    } else if line_info.block_id.is_some() {
                        let display = line.to_string();
                        let segments = if let Some(highlighter) = highlighter.as_mut() {
                            highlight_code_line(&display, highlighter, syntax_set, code_bg)
                        } else {
                            vec![StyledSegment {
                                text: display.clone(),
                                style: code_fallback_style(code_bg),
                            }]
                        };
                        (display, Some(segments))
                    } else {
                        (line.to_string(), None)
                    };

                    let is_cursor_line = logical_idx == cursor_row;
                    let selection =
                        selection_range_for_line(app, logical_idx, display_line.chars().count());
                    let wrapped = wrap_line_for_editor(&display_line, content_width);
                    let mut segment_start_col = 0usize;

                    if is_cursor_line {
                        // Calculate cursor position within wrapped lines
                        cursor_visual_row = visual_lines.len();
                        let (wrap_row, wrap_col) =
                            find_cursor_in_wrapped_lines(&wrapped, cursor_col);
                        cursor_visual_row += wrap_row;
                        cursor_visual_col = wrap_col;
                    }

                    for (wrap_idx, wline) in wrapped.iter().enumerate() {
                        let segment_len = wline.chars().count();
                        let is_first_wrap = wrap_idx == 0;
                        let is_cursor_wrap =
                            is_cursor_line && (visual_lines.len() + wrap_idx == cursor_visual_row);
                        let content_override = styled_segments.as_ref().map(|segments| {
                            slice_segments(
                                segments,
                                segment_start_col,
                                segment_start_col.saturating_add(segment_len),
                            )
                        });
                        visual_lines.push(compose_wrapped_line(
                            wline,
                            &tokens,
                            is_cursor_wrap,
                            logical_idx,
                            show_line_numbers,
                            is_first_wrap,
                            selection,
                            segment_start_col,
                            content_override,
                        ));
                        segment_start_col = segment_start_col.saturating_add(segment_len);
                    }
                }
            }

            // Update viewport to follow cursor (using visual row now)
            let cursor_visual_row_u16 = (cursor_visual_row.min(u16::MAX as usize)) as u16;
            if input_inner.height > 0 {
                app.textarea_viewport_row = next_scroll_top(
                    app.textarea_viewport_row,
                    cursor_visual_row_u16,
                    input_inner.height,
                );
            }

            // Render only visible visual lines
            let visible_start = app.textarea_viewport_row as usize;
            let rendered: Vec<Line<'static>> = visual_lines
                .into_iter()
                .skip(visible_start)
                .take(visible_height)
                .collect();

            // Store visual cursor position for later use
            app.textarea_viewport_col = cursor_visual_col as u16;

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
                        "Enter: apply · Esc: cancel",
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
    if let Some(inner) = cursor_area
        && inner.height > 0
        && inner.width > 0
    {
        let (cursor_row, cursor_col) = app.textarea.cursor();
        let prefix_width = compose_prefix_width(app.config.ui.line_numbers);
        let content_width = (inner.width as usize)
            .saturating_sub(prefix_width as usize)
            .max(1);

        // Calculate visual row considering line wrapping
        let lines = app.textarea.lines();
        let mut visual_row: usize = 0;
        let mut cursor_visual_col: usize = 0;

        for (idx, line) in lines.iter().enumerate() {
            let wrapped = wrap_line_for_editor(line, content_width);

            if idx == cursor_row {
                let (wrap_offset, wrap_col) = find_cursor_in_wrapped_lines(&wrapped, cursor_col);
                visual_row += wrap_offset;
                cursor_visual_col = wrap_col;
                break;
            }

            visual_row += wrapped.len();
        }

        let visual_row_u16 = (visual_row.min(u16::MAX as usize)) as u16;
        let row_in_view = visual_row_u16.saturating_sub(app.textarea_viewport_row);
        let row_in_view = row_in_view.min(inner.height.saturating_sub(1));

        let col_in_view = (cursor_visual_col.min(u16::MAX as usize)) as u16;
        let col_in_view = col_in_view.saturating_add(prefix_width);
        let col_in_view = col_in_view.min(inner.width.saturating_sub(1));

        f.set_cursor_position((inner.x + col_in_view, inner.y + row_in_view));
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

    if app.show_date_picker_popup {
        render_date_picker_popup(f, app);
    }

    if app.show_help_popup {
        render_help_popup(f, app);
    }
    if app.show_theme_popup {
        render_theme_switcher_popup(f, app);
    }
    if app.show_editor_style_popup {
        render_editor_style_popup(f, app);
    }
    if app.show_delete_entry_popup {
        render_delete_entry_popup(f);
    }
    if app.show_exit_popup {
        render_exit_popup(f, app);
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

    if app.show_memo_preview_popup {
        render_memo_preview_popup(f, app);
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

const LINE_NUMBER_WIDTH: usize = 3;
const LINE_MARKER: &str = "| ";

/// Wrap a logical line into multiple visual lines based on display width.
/// Returns a vector of string slices representing each visual line.
fn wrap_line_for_editor(line: &str, max_width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut result: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut current_width: usize = 0;

    for ch in line.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);

        if current_width + ch_width > max_width && !current_line.is_empty() {
            result.push(current_line);
            current_line = String::new();
            current_width = 0;
        }

        current_line.push(ch);
        current_width += ch_width;
    }

    if !current_line.is_empty() || result.is_empty() {
        result.push(current_line);
    }

    result
}

/// Find cursor position within wrapped lines.
/// Returns (visual_row_offset, visual_col) relative to the wrapped lines.
fn find_cursor_in_wrapped_lines(wrapped: &[String], cursor_col: usize) -> (usize, usize) {
    let mut chars_seen: usize = 0;

    for (idx, wline) in wrapped.iter().enumerate() {
        let line_chars = wline.chars().count();

        if cursor_col <= chars_seen + line_chars {
            // Cursor is within this wrapped line
            let col_in_line = cursor_col.saturating_sub(chars_seen);
            // Calculate visual column (display width up to cursor)
            let visual_col: usize = wline
                .chars()
                .take(col_in_line)
                .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1))
                .sum();
            return (idx, visual_col);
        }

        chars_seen += line_chars;
    }

    // Cursor is at the end
    let last_idx = wrapped.len().saturating_sub(1);
    let last_width = wrapped.last().map(|s| s.width()).unwrap_or(0);
    (last_idx, last_width)
}

/// Compose a wrapped line for rendering in editor.
#[derive(Clone, Copy)]
struct SelectionRange {
    start: usize,
    end: usize,
}

#[derive(Clone)]
struct StyledSegment {
    text: String,
    style: Style,
}

#[derive(Clone)]
struct CodeBlockLineInfo {
    block_id: Option<usize>,
    is_fence: bool,
    language: Option<String>,
}

fn collect_code_block_info(
    lines: &[String],
    cursor_row: usize,
) -> (Vec<CodeBlockLineInfo>, Option<usize>) {
    let mut info = Vec::with_capacity(lines.len());
    let mut in_code_block = false;
    let mut current_block_id: Option<usize> = None;
    let mut current_language: Option<String> = None;
    let mut next_block_id = 0usize;
    let mut cursor_block_id = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```");
        let mut line_info = CodeBlockLineInfo {
            block_id: current_block_id,
            is_fence,
            language: None,
        };

        if is_fence {
            if !in_code_block {
                next_block_id = next_block_id.saturating_add(1);
                current_block_id = Some(next_block_id);
                current_language = parse_fence_language(trimmed);
                line_info.block_id = current_block_id;
                line_info.language = current_language.clone();
                in_code_block = true;
            } else {
                line_info.block_id = current_block_id;
                line_info.language = current_language.clone();
                in_code_block = false;
                current_block_id = None;
                current_language = None;
            }
        } else if in_code_block {
            line_info.block_id = current_block_id;
            line_info.language = current_language.clone();
        }

        if idx == cursor_row {
            cursor_block_id = line_info.block_id;
        }
        info.push(line_info);
    }

    (info, cursor_block_id)
}

fn parse_fence_language(trimmed: &str) -> Option<String> {
    let rest = trimmed.trim_start_matches('`').trim();
    let candidate = rest.split_whitespace().next().unwrap_or("");
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn hide_fence_marker(line: &str) -> String {
    let trimmed = line.trim_start();
    let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
    if fence_len == 0 {
        return line.to_string();
    }

    let leading_len = line.len().saturating_sub(trimmed.len());
    let fence_start = leading_len;
    let fence_end = fence_start.saturating_add(fence_len);
    let mut out = String::with_capacity(line.len());
    out.push_str(&line[..fence_start]);
    out.extend(std::iter::repeat(' ').take(fence_len));
    out.push_str(&line[fence_end..]);
    out
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntax_theme_set() -> &'static ThemeSet {
    static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

fn select_syntax_theme<'a>(
    theme_set: &'a ThemeSet,
    tokens: &theme::ThemeTokens,
) -> &'a syntect::highlighting::Theme {
    let prefer_light = is_light_color(tokens.ui_bg).unwrap_or(false);
    let candidates = if prefer_light {
        ["InspiredGitHub", "base16-ocean.light"]
    } else {
        ["base16-ocean.dark", "Solarized (dark)"]
    };

    for name in candidates {
        if let Some(theme) = theme_set.themes.get(name) {
            return theme;
        }
    }

    theme_set
        .themes
        .values()
        .next()
        .expect("syntect theme set is empty")
}

fn syntax_for_language<'a>(
    syntax_set: &'a SyntaxSet,
    language: Option<&str>,
) -> &'a SyntaxReference {
    if let Some(lang) = language {
        let lang = lang.trim();
        if !lang.is_empty() {
            if let Some(syntax) = syntax_set.find_syntax_by_token(lang) {
                return syntax;
            }
            if let Some(syntax) = syntax_set.find_syntax_by_extension(lang) {
                return syntax;
            }
        }
    }
    syntax_set.find_syntax_plain_text()
}

fn highlight_code_line(
    line: &str,
    highlighter: &mut HighlightLines,
    syntax_set: &SyntaxSet,
    code_bg: Option<Color>,
) -> Vec<StyledSegment> {
    let ranges = match highlighter.highlight_line(line, syntax_set) {
        Ok(ranges) => ranges,
        Err(_) => {
            return vec![StyledSegment {
                text: line.to_string(),
                style: code_fallback_style(code_bg),
            }];
        }
    };

    if ranges.is_empty() {
        return vec![StyledSegment {
            text: line.to_string(),
            style: code_fallback_style(code_bg),
        }];
    }

    ranges
        .into_iter()
        .map(|(style, text)| {
            let mut out = syntect_style_to_ratatui(style);
            if let Some(bg) = code_bg {
                out = out.bg(bg);
            }
            StyledSegment {
                text: text.to_string(),
                style: out,
            }
        })
        .collect()
}

fn syntect_style_to_ratatui(style: syntect::highlighting::Style) -> Style {
    let mut out = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.contains(SyntectFontStyle::BOLD) {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(SyntectFontStyle::ITALIC) {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(SyntectFontStyle::UNDERLINE) {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    out
}

fn code_block_background(tokens: &theme::ThemeTokens) -> Option<Color> {
    match tokens.ui_bg {
        Color::Rgb(r, g, b) => {
            let lum = 0.2126 * (r as f32) + 0.7152 * (g as f32) + 0.0722 * (b as f32);
            let delta: i16 = if lum < 128.0 { 18 } else { -18 };
            Some(Color::Rgb(
                shift_channel(r, delta),
                shift_channel(g, delta),
                shift_channel(b, delta),
            ))
        }
        Color::Black => Some(Color::Rgb(20, 20, 20)),
        Color::White => Some(Color::Rgb(235, 235, 235)),
        Color::DarkGray => Some(Color::Rgb(56, 56, 56)),
        Color::Gray => Some(Color::Rgb(160, 160, 160)),
        _ => None,
    }
}

fn shift_channel(value: u8, delta: i16) -> u8 {
    let next = (value as i16).saturating_add(delta);
    next.clamp(0, 255) as u8
}

fn is_light_color(color: Color) -> Option<bool> {
    match color {
        Color::Rgb(r, g, b) => {
            let lum = 0.2126 * (r as f32) + 0.7152 * (g as f32) + 0.0722 * (b as f32);
            Some(lum >= 128.0)
        }
        Color::White | Color::Gray => Some(true),
        Color::Black | Color::DarkGray => Some(false),
        _ => None,
    }
}

fn code_fallback_style(code_bg: Option<Color>) -> Style {
    let mut style = Style::default().add_modifier(Modifier::DIM);
    if let Some(bg) = code_bg {
        style = style.bg(bg);
    }
    style
}

fn slice_segments(
    segments: &[StyledSegment],
    start: usize,
    end: usize,
) -> Vec<StyledSegment> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let end = end.max(start);

    for seg in segments {
        let seg_len = seg.text.chars().count();
        let seg_start = pos;
        let seg_end = pos + seg_len;
        if end <= seg_start {
            break;
        }
        if start >= seg_end {
            pos = seg_end;
            continue;
        }
        let local_start = start.saturating_sub(seg_start).min(seg_len);
        let local_end = end.saturating_sub(seg_start).min(seg_len);
        if local_end > local_start {
            out.push(StyledSegment {
                text: slice_by_char(&seg.text, local_start, local_end),
                style: seg.style,
            });
        }
        pos = seg_end;
    }

    out
}

// UI render helper keeps explicit parameters.
#[allow(clippy::too_many_arguments)]
fn compose_wrapped_line(
    line: &str,
    tokens: &theme::ThemeTokens,
    is_cursor: bool,
    logical_line_number: usize,
    show_line_numbers: bool,
    is_first_wrap: bool,
    selection: Option<SelectionRange>,
    wrap_start_col: usize,
    content_override: Option<Vec<StyledSegment>>,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Only show line number on first wrapped segment
    if show_line_numbers {
        if is_first_wrap {
            let label = format!(
                "{:>width$} ",
                logical_line_number + 1,
                width = LINE_NUMBER_WIDTH
            );
            spans.push(Span::styled(label, Style::default().fg(tokens.ui_muted)));
        } else {
            // Continuation line: show spaces instead of line number
            let label = format!("{:>width$} ", "", width = LINE_NUMBER_WIDTH);
            spans.push(Span::styled(label, Style::default().fg(tokens.ui_muted)));
        }
    }

    // Line marker (only on first wrap) or continuation marker
    if is_first_wrap {
        spans.push(Span::styled(
            LINE_MARKER,
            Style::default().fg(tokens.ui_muted),
        ));
    } else {
        // Use a wrap continuation indicator
        spans.push(Span::styled("↪ ", Style::default().fg(tokens.ui_muted)));
    }

    let content_segments: Vec<StyledSegment> = if let Some(segments) = content_override {
        segments
    } else {
        let mut segments = Vec::new();
        // For first wrapped line, parse indent and bullets; for continuations, just show text
        if is_first_wrap {
            let (indent_level, indent, rest) = split_indent(line);
            if !indent.is_empty() {
                segments.push(StyledSegment {
                    text: indent.to_string(),
                    style: Style::default(),
                });
            }
            if let Some((bullet, tail)) = replace_list_bullet(rest, indent_level) {
                segments.push(StyledSegment {
                    text: format!("{bullet} "),
                    style: Style::default()
                        .fg(tokens.ui_accent)
                        .add_modifier(Modifier::BOLD),
                });
                segments.push(StyledSegment {
                    text: tail.to_string(),
                    style: Style::default(),
                });
            } else {
                segments.push(StyledSegment {
                    text: rest.to_string(),
                    style: Style::default(),
                });
            }
        } else {
            segments.push(StyledSegment {
                text: line.to_string(),
                style: Style::default(),
            });
        }
        segments
    };

    let selection_spans = apply_selection_to_segments(
        content_segments,
        selection,
        wrap_start_col,
        tokens.ui_selection_bg,
    );
    spans.extend(selection_spans);

    let mut rendered = Line::from(spans);
    if is_cursor {
        rendered.style = Style::default().bg(tokens.ui_cursorline_bg);
    }
    rendered
}

fn apply_selection_to_segments(
    segments: Vec<StyledSegment>,
    selection: Option<SelectionRange>,
    wrap_start_col: usize,
    selection_bg: ratatui::style::Color,
) -> Vec<Span<'static>> {
    if segments.is_empty() {
        if selection.is_some() {
            return vec![Span::styled(" ", Style::default().bg(selection_bg))];
        }
        return Vec::new();
    }

    let Some(selection) = selection else {
        return segments
            .into_iter()
            .map(|seg| Span::styled(seg.text, seg.style))
            .collect();
    };

    let total_len: usize = segments.iter().map(|seg| seg.text.chars().count()).sum();
    if total_len == 0 {
        return vec![Span::styled(" ", Style::default().bg(selection_bg))];
    }
    let sel_start = selection.start.saturating_sub(wrap_start_col);
    let sel_end = selection.end.saturating_sub(wrap_start_col);
    let sel_start = sel_start.min(total_len);
    let sel_end = sel_end.min(total_len);

    if sel_start >= sel_end {
        return segments
            .into_iter()
            .map(|seg| Span::styled(seg.text, seg.style))
            .collect();
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut pos = 0usize;
    for seg in segments {
        let seg_len = seg.text.chars().count();
        let seg_start = pos;
        let seg_end = pos + seg_len;
        if sel_end <= seg_start || sel_start >= seg_end {
            spans.push(Span::styled(seg.text, seg.style));
        } else {
            let local_start = sel_start.saturating_sub(seg_start).min(seg_len);
            let local_end = sel_end.saturating_sub(seg_start).min(seg_len);
            if local_start > 0 {
                spans.push(Span::styled(
                    slice_by_char(&seg.text, 0, local_start),
                    seg.style,
                ));
            }
            if local_end > local_start {
                let mut selected_style = seg.style;
                selected_style = selected_style.bg(selection_bg);
                spans.push(Span::styled(
                    slice_by_char(&seg.text, local_start, local_end),
                    selected_style,
                ));
            }
            if local_end < seg_len {
                spans.push(Span::styled(
                    slice_by_char(&seg.text, local_end, seg_len),
                    seg.style,
                ));
            }
        }
        pos = seg_end;
    }
    spans
}

fn slice_by_char(s: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn compose_placeholder_line(
    placeholder: &str,
    tokens: &theme::ThemeTokens,
    show_line_numbers: bool,
    is_cursor: bool,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = compose_prefix_spans(0, tokens, show_line_numbers);
    spans.push(Span::styled(
        placeholder.to_string(),
        Style::default()
            .fg(tokens.ui_muted)
            .add_modifier(Modifier::DIM),
    ));
    let mut line = Line::from(spans);
    if is_cursor {
        line.style = Style::default().bg(tokens.ui_cursorline_bg);
    }
    line
}

fn compose_prefix_spans(
    line_number: usize,
    tokens: &theme::ThemeTokens,
    show_line_numbers: bool,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if show_line_numbers {
        let label = format!("{:>width$} ", line_number + 1, width = LINE_NUMBER_WIDTH);
        spans.push(Span::styled(label, Style::default().fg(tokens.ui_muted)));
    }
    spans.push(Span::styled(
        LINE_MARKER,
        Style::default().fg(tokens.ui_muted),
    ));
    spans
}

fn compose_prefix_width(show_line_numbers: bool) -> u16 {
    let mut width = LINE_MARKER.len() as u16;
    if show_line_numbers {
        width += (LINE_NUMBER_WIDTH + 1) as u16;
    }
    width
}

fn selection_range_for_line(app: &App, line_idx: usize, line_len: usize) -> Option<SelectionRange> {
    let EditorMode::Visual(kind) = app.editor_mode else {
        return None;
    };
    let anchor = app.visual_anchor?;
    let cursor = app.textarea.cursor();

    match kind {
        VisualKind::Char => {
            let (start, end) = ordered_positions(anchor, cursor);
            if line_idx < start.0 || line_idx > end.0 {
                return None;
            }
            let (start_col, mut end_col) = if start.0 == end.0 {
                (start.1, end.1.saturating_add(1))
            } else if line_idx == start.0 {
                (start.1, line_len)
            } else if line_idx == end.0 {
                (0, end.1.saturating_add(1))
            } else {
                (0, line_len)
            };
            if line_len == 0 {
                end_col = 0;
            }
            Some(SelectionRange {
                start: start_col.min(line_len),
                end: end_col.min(line_len),
            })
        }
        VisualKind::Line => {
            let (start, end) = ordered_positions(anchor, cursor);
            if line_idx < start.0 || line_idx > end.0 {
                return None;
            }
            Some(SelectionRange {
                start: 0,
                end: line_len,
            })
        }
        VisualKind::Block => {
            let row_start = anchor.0.min(cursor.0);
            let row_end = anchor.0.max(cursor.0);
            if line_idx < row_start || line_idx > row_end {
                return None;
            }
            let col_start = anchor.1.min(cursor.1);
            let col_end = anchor.1.max(cursor.1).saturating_add(1);
            if line_len == 0 || col_start >= line_len {
                return None;
            }
            Some(SelectionRange {
                start: col_start.min(line_len),
                end: col_end.min(line_len),
            })
        }
    }
}

fn ordered_positions(a: (usize, usize), b: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    if a <= b { (a, b) } else { (b, a) }
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

fn replace_list_bullet(rest: &str, indent_level: usize) -> Option<(char, &str)> {
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

fn render_agenda_panel(
    f: &mut Frame,
    app: &App,
    area: Rect,
    focused: bool,
    tokens: &theme::ThemeTokens,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let date_label = app.agenda_selected_day.format("%Y-%m-%d").to_string();
    let filter_label = app.agenda_filter_label();
    let unscheduled = if app.agenda_show_unscheduled {
        "Unsched: on"
    } else {
        "Unsched: off"
    };
    let title_text = format!("AGENDA {date_label} · {filter_label} · {unscheduled}");
    let title = truncate(&title_text, area.width.saturating_sub(4) as usize);

    let border_color = if focused {
        tokens.ui_accent
    } else {
        tokens.ui_border_default
    };
    let title_style = if focused {
        Style::default()
            .fg(tokens.ui_accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tokens.ui_muted)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(Span::styled(title, title_style)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let list_width = inner.width.saturating_sub(1).max(1) as usize;
    let selected = app.agenda_state.selected();
    let visible = app.agenda_visible_indices();
    let mut items: Vec<ListItem> = Vec::new();
    let mut ui_selected_index: Option<usize> = None;
    let mut ui_index = 0usize;

    let now = Local::now();
    let is_today = now.date_naive() == app.agenda_selected_day;
    let now_time = now.time();
    let cursor_time = selected
        .and_then(|idx| app.agenda_items.get(idx))
        .and_then(|item| item.time);

    let cursor_label = cursor_time
        .map(format_time)
        .unwrap_or_else(|| "--:--".to_string());
    let now_label = if is_today {
        format_time(now_time)
    } else {
        "--:--".to_string()
    };
    let header = format!(
        "Filter: {}  | Cursor: {}  | Now: {}",
        filter_label, cursor_label, now_label
    );
    items.push(ListItem::new(Line::from(Span::styled(
        truncate(&header, list_width),
        Style::default().fg(tokens.ui_muted),
    ))));
    ui_index += 1;
    items.push(ListItem::new(Line::from("")));
    ui_index += 1;

    if visible.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "No agenda items.",
            Style::default().fg(tokens.ui_muted),
        ))));
    } else {
        let mut overdue = Vec::new();
        let mut all_day = Vec::new();
        let mut timed = Vec::new();
        let mut unscheduled_items = Vec::new();

        for idx in visible {
            let item = &app.agenda_items[idx];
            let is_overdue = item.kind == AgendaItemKind::Task
                && item.schedule.due.is_some()
                && item.schedule.due.unwrap_or(app.agenda_selected_day) < app.agenda_selected_day
                && !item.is_done;
            if is_overdue {
                overdue.push(idx);
                continue;
            }
            if item.kind == AgendaItemKind::Task && item.schedule.is_empty() {
                unscheduled_items.push(idx);
                continue;
            }
            if item.date != app.agenda_selected_day {
                continue;
            }
            if item.time.is_some() {
                timed.push(idx);
            } else {
                all_day.push(idx);
            }
        }

        push_agenda_section(
            &mut items,
            &mut ui_index,
            "OVERDUE",
            &overdue,
            selected,
            &mut ui_selected_index,
            app,
            list_width,
            tokens,
        );
        push_agenda_section(
            &mut items,
            &mut ui_index,
            "ALL-DAY",
            &all_day,
            selected,
            &mut ui_selected_index,
            app,
            list_width,
            tokens,
        );

        items.push(ListItem::new(Line::from(Span::styled(
            "Time  | Timeline",
            Style::default()
                .fg(tokens.ui_accent)
                .add_modifier(Modifier::BOLD),
        ))));
        ui_index += 1;

        let slot_minutes: i32 = 30;
        let window_start_min: i32 = 6 * 60;
        let window_end_min: i32 = 22 * 60;
        let row_count =
            ((window_end_min - window_start_min) / slot_minutes).max(0) as usize + 1;

        let mut blocks = build_agenda_blocks(&timed, app, app.agenda_selected_day);
        blocks.sort_by_key(|block| (block.start_min, block.end_min, block.idx));

        let mut row_blocks: Vec<Vec<usize>> = vec![Vec::new(); row_count];
        let mut block_start_row: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();

        for (block_idx, block) in blocks.iter().enumerate() {
            let start_row = ((block.start_min - window_start_min).max(0) / slot_minutes) as usize;
            let end_row = ((block.end_min - window_start_min + slot_minutes - 1).max(0)
                / slot_minutes) as usize;
            let end_row = end_row.max(start_row + 1);
            block_start_row.insert(block.idx, start_row);

            for row in start_row..end_row {
                if row < row_count {
                    row_blocks[row].push(block_idx);
                }
            }
        }

        let now_min = now_time.hour() as i32 * 60 + now_time.minute() as i32;
        let now_row = if is_today
            && now_min >= window_start_min
            && now_min < window_end_min + slot_minutes
        {
            Some(((now_min - window_start_min) / slot_minutes) as usize)
        } else {
            None
        };

        let time_width = 5usize;
        let separator = " | ";
        let content_width = list_width.saturating_sub(time_width + separator.len()).max(1);
        for row in 0..row_count {
            let time_min = window_start_min + row as i32 * slot_minutes;
            let time_label = format!("{:02}:{:02}", time_min / 60, time_min % 60);
            let mut content = String::new();
            if let Some(block_indices) = row_blocks.get(row)
                && !block_indices.is_empty()
            {
                let starting_blocks: Vec<usize> = block_indices
                    .iter()
                    .filter(|block_idx| {
                        block_start_row.get(&blocks[**block_idx].idx) == Some(&row)
                    })
                    .copied()
                    .collect();
                let selected_block = selected.and_then(|selected_idx| {
                    starting_blocks
                        .iter()
                        .find(|block_idx| blocks[**block_idx].idx == selected_idx)
                        .copied()
                });
                let display_idx = selected_block
                    .or_else(|| starting_blocks.first().copied())
                    .unwrap_or(block_indices[0]);
                let block = &blocks[display_idx];
                let prefix = block.prefix;
                let extra = if block_indices.len() > 1 {
                    format!(" (+{})", block_indices.len() - 1)
                } else {
                    String::new()
                };
                if block_start_row.get(&block.idx) == Some(&row) {
                    content = format!("{prefix} {}{}", block.label, extra);
                    if selected == Some(block.idx) {
                        ui_selected_index = Some(ui_index);
                    }
                } else {
                    content = format!("{prefix}{}", extra);
                }
            } else if now_row == Some(row) {
                content = format!("---- NOW {} ----", now_label);
            }

            let content = truncate(&content, content_width);
            let line = format!("{time_label}{separator}{content}");
            items.push(ListItem::new(Line::from(line)));
            ui_index += 1;
        }

        if app.agenda_show_unscheduled && !unscheduled_items.is_empty() {
            items.push(ListItem::new(Line::from("")));
            ui_index += 1;
            push_agenda_section(
                &mut items,
                &mut ui_index,
                "UNSCHEDULED",
                &unscheduled_items,
                selected,
                &mut ui_selected_index,
                app,
                list_width,
                tokens,
            );
        }
    }

    let highlight_bg = tokens.ui_selection_bg;
    let highlight_style = if focused {
        Style::default()
            .bg(highlight_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(highlight_bg)
    };

    let list = List::new(items)
        .highlight_symbol("")
        .highlight_style(highlight_style);
    let mut state = ListState::default();
    state.select(ui_selected_index);
    f.render_stateful_widget(list, inner, &mut state);
}

struct AgendaBlock {
    idx: usize,
    start_min: i32,
    end_min: i32,
    label: String,
    prefix: &'static str,
}

fn build_agenda_blocks(
    timed: &[usize],
    app: &App,
    day: chrono::NaiveDate,
) -> Vec<AgendaBlock> {
    let mut blocks = Vec::new();
    for idx in timed {
        let item = &app.agenda_items[*idx];
        let Some(time) = item.time else { continue };
        let start_min = time.hour() as i32 * 60 + time.minute() as i32;
        let mut duration = item.duration_minutes.unwrap_or(30) as i32;
        if duration <= 0 {
            duration = 30;
        }
        let end_min = (start_min + duration).min(24 * 60);
        let label = format!(
            "{} {}-{}",
            agenda_item_label(item, day),
            format_time(time),
            format_time_minutes(end_min)
        );
        let prefix = match item.kind {
            AgendaItemKind::Task => "####",
            AgendaItemKind::Note => "....",
        };
        blocks.push(AgendaBlock {
            idx: *idx,
            start_min,
            end_min,
            label,
            prefix,
        });
    }
    blocks
}

fn push_agenda_section(
    items: &mut Vec<ListItem>,
    ui_index: &mut usize,
    label: &str,
    indices: &[usize],
    selected: Option<usize>,
    ui_selected_index: &mut Option<usize>,
    app: &App,
    list_width: usize,
    tokens: &theme::ThemeTokens,
) {
    if indices.is_empty() {
        return;
    }

    items.push(ListItem::new(Line::from(Span::styled(
        label.to_string(),
        Style::default()
            .fg(tokens.ui_accent)
            .add_modifier(Modifier::BOLD),
    ))));
    *ui_index += 1;

    for idx in indices {
        if selected == Some(*idx) {
            *ui_selected_index = Some(*ui_index);
        }
        let line = agenda_item_label(&app.agenda_items[*idx], app.agenda_selected_day);
        let wrapped = wrap_markdown_line(&line, list_width);
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
        items.push(ListItem::new(Text::from(lines)));
        *ui_index += 1;
    }

    items.push(ListItem::new(Line::from("")));
    *ui_index += 1;
}

fn agenda_item_label(item: &crate::models::AgendaItem, day: chrono::NaiveDate) -> String {
    let mut line = String::new();
    line.push_str(&"  ".repeat(item.indent));

    match item.kind {
        AgendaItemKind::Task => {
            if item.is_done {
                line.push_str("- [x] ");
            } else {
                line.push_str("- [ ] ");
            }
        }
        AgendaItemKind::Note => {
            line.push_str("• ");
        }
    }

    let badges = agenda_badges(item, day);
    if !badges.is_empty() {
        line.push_str(&badges);
        line.push(' ');
    }
    line.push_str(&item.text);
    if let Some(minutes) = item.duration_minutes {
        line.push_str(&format!(" ({})", format_duration(minutes)));
    }
    line
}

fn agenda_badges(item: &crate::models::AgendaItem, day: chrono::NaiveDate) -> String {
    let mut badges = Vec::new();
    if item.schedule.scheduled.is_some() {
        badges.push("[S]");
    }
    if item.schedule.due.is_some() {
        badges.push("[D]");
    }
    if item.time.is_some() {
        badges.push("[T]");
    }
    if item.kind == AgendaItemKind::Task
        && item.schedule.due.is_some()
        && item.schedule.due.unwrap_or(day) < day
        && !item.is_done
    {
        badges.push("[O]");
    }
    badges.join("")
}

fn format_time(time: chrono::NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn format_time_minutes(total_minutes: i32) -> String {
    let total = total_minutes.clamp(0, 24 * 60);
    let hours = total / 60;
    let minutes = total % 60;
    format!("{:02}:{:02}", hours, minutes)
}

fn format_duration(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 && mins > 0 {
        format!("{hours}h{mins}m")
    } else if hours > 0 {
        format!("{hours}h")
    } else {
        format!("{minutes}m")
    }
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App, tokens: &theme::ThemeTokens) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let mode_label = match app.input_mode {
        InputMode::Navigate => match app.navigate_focus {
            NavigateFocus::Timeline => "NAV:TL",
            NavigateFocus::Agenda => "NAV:AG",
            NavigateFocus::Tasks => "NAV:TS",
        },
        InputMode::Editing => match app.editor_mode {
            EditorMode::Normal => "[NORMAL]",
            EditorMode::Insert => "[INSERT]",
            EditorMode::Visual(_) => "[VISUAL]",
        },
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

    let status_message = if let Some(hint) = app.visual_hint_message.as_deref() {
        if hint.is_empty() {
            None
        } else {
            Some((hint, tokens.ui_muted))
        }
    } else if let Some(toast) = app.toast_message.as_deref()
        && !toast.is_empty()
    {
        Some((toast, tokens.ui_toast_info))
    } else {
        None
    };

    if let Some((message, color)) = status_message {
        if !right_plain.is_empty() {
            right_plain.push_str("  ");
            right_spans.push(Span::raw("  "));
        }
        right_plain.push_str(message);
        right_spans.push(Span::styled(
            message,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
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
            NavigateFocus::Agenda => app
                .agenda_state
                .selected()
                .and_then(|i| app.agenda_items.get(i))
                .map(|item| item.file_path.as_str()),
            NavigateFocus::Tasks => app
                .tasks_state
                .selected()
                .and_then(|i| app.tasks.get(i))
                .map(|task| task.file_path.as_str()),
        };

        if let Some(path) = selected_path
            && let Some(name) = Path::new(path).file_name().and_then(|s| s.to_str())
        {
            return name.to_string();
        }
    }

    if let Some(editing) = app.editing_entry.as_ref()
        && let Some(name) = Path::new(&editing.file_path)
            .file_name()
            .and_then(|s| s.to_str())
    {
        return name.to_string();
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
    use super::compose_prefix_width;
    use super::compose_wrapped_line;
    use super::collect_code_block_info;
    use super::hide_fence_marker;
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

        let top = compose_wrapped_line("* item1", &tokens, false, 0, false, true, None, 0, None);
        assert_eq!(line_to_string(&top), "| • item1");

        let nested =
            compose_wrapped_line("  * sub1", &tokens, false, 1, false, true, None, 0, None);
        assert_eq!(line_to_string(&nested), "|   ◦ sub1");

        let deep =
            compose_wrapped_line("    - sub2", &tokens, false, 2, false, true, None, 0, None);
        assert_eq!(line_to_string(&deep), "|     ▪ sub2");
    }

    #[test]
    fn preserves_non_list_lines_verbatim() {
        let tokens = ThemeTokens::from_theme(&Theme::default());
        let line =
            compose_wrapped_line("plain text", &tokens, false, 0, false, true, None, 0, None);
        assert_eq!(line_to_string(&line), "| plain text");
    }

    #[test]
    fn renders_line_numbers_in_gutter() {
        let tokens = ThemeTokens::from_theme(&Theme::default());
        let line =
            compose_wrapped_line("plain text", &tokens, false, 9, true, true, None, 0, None);
        assert_eq!(line_to_string(&line), " 10 | plain text");
    }

    #[test]
    fn prefix_width_accounts_for_line_numbers() {
        assert_eq!(compose_prefix_width(false), 2);
        assert_eq!(compose_prefix_width(true), 6);
    }

    #[test]
    fn hides_fence_marker_ticks() {
        assert_eq!(hide_fence_marker("```python"), "   python");
        assert_eq!(hide_fence_marker("  ```"), "     ");
    }

    #[test]
    fn tracks_code_block_ranges_for_cursor() {
        let lines = vec![
            "intro".to_string(),
            "```python".to_string(),
            "print('hi')".to_string(),
            "```".to_string(),
            "after".to_string(),
        ];
        let (info, cursor_block_id) = collect_code_block_info(&lines, 2);
        assert!(info[1].is_fence);
        assert!(info[2].block_id.is_some());
        assert_eq!(info[2].language.as_deref(), Some("python"));
        assert_eq!(cursor_block_id, info[2].block_id);
    }

    use super::find_cursor_in_wrapped_lines;
    use super::wrap_line_for_editor;

    #[test]
    fn wrap_line_for_editor_empty_line() {
        let wrapped = wrap_line_for_editor("", 10);
        assert_eq!(wrapped, vec![""]);
    }

    #[test]
    fn wrap_line_for_editor_short_line() {
        let wrapped = wrap_line_for_editor("hello", 10);
        assert_eq!(wrapped, vec!["hello"]);
    }

    #[test]
    fn wrap_line_for_editor_exact_width() {
        let wrapped = wrap_line_for_editor("1234567890", 10);
        assert_eq!(wrapped, vec!["1234567890"]);
    }

    #[test]
    fn wrap_line_for_editor_exceeds_width() {
        let wrapped = wrap_line_for_editor("12345678901234567890", 10);
        assert_eq!(wrapped, vec!["1234567890", "1234567890"]);
    }

    #[test]
    fn wrap_line_for_editor_cjk_characters() {
        // Each CJK character has width 2, so 5 characters = width 10
        let wrapped = wrap_line_for_editor("한글테스트", 10);
        assert_eq!(wrapped, vec!["한글테스트"]);

        // 6 CJK characters = width 12, should wrap after 5 chars (width 10)
        let wrapped = wrap_line_for_editor("한글테스트요", 10);
        assert_eq!(wrapped, vec!["한글테스트", "요"]);
    }

    #[test]
    fn find_cursor_in_wrapped_lines_single_line() {
        let wrapped = vec!["hello world".to_string()];
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 0), (0, 0));
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 5), (0, 5));
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 11), (0, 11));
    }

    #[test]
    fn find_cursor_in_wrapped_lines_multi_line() {
        // "1234567890" + "1234567890" = 20 chars wrapped at width 10
        let wrapped = vec!["1234567890".to_string(), "1234567890".to_string()];
        // Cursor at position 5 (first line)
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 5), (0, 5));
        // Cursor at position 10 is at end of first line (chars 0-9)
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 10), (0, 10));
        // Cursor at position 11 (second char of second line)
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 11), (1, 1));
        // Cursor at position 15 (6th char of second line)
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 15), (1, 5));
    }

    #[test]
    fn find_cursor_in_wrapped_lines_cjk_cursor() {
        // "한글테스트" = 5 chars, width 10; "요" = 1 char, width 2
        let wrapped = vec!["한글테스트".to_string(), "요".to_string()];
        // Cursor at char position 2 (after "한글"), visual column 4
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 2), (0, 4));
        // Cursor at char position 5 (at end of first line)
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 5), (0, 10));
        // Cursor at char position 6 (at second line "요")
        assert_eq!(find_cursor_in_wrapped_lines(&wrapped, 6), (1, 2));
    }
}
