use chrono::Local;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::models::{InputMode, NavigateFocus};
use crate::ui::color_parser::parse_color;
use ratatui::style::Stylize;
use std::path::Path;

pub mod color_parser;
pub mod components;
pub mod popups;

use components::{parse_markdown_spans, wrap_markdown_line};
use popups::{
    render_activity_popup, render_help_popup, render_mood_popup, render_path_popup,
    render_pomodoro_popup, render_siren_popup, render_tag_popup, render_todo_popup,
};

const HELP_NAVIGATE: &str = " ?: Help  h/l: Focus  j/k: Move  Space/Enter: Toggle Task  e: Edit  i: Compose  /: Search  t: Tags  p: Pomodoro  g: Activity  o: Log Dir  Ctrl+Q: Quit ";
const HELP_COMPOSE: &str = " Enter: New line  Ctrl+S/Ctrl+D: Save  Ctrl+L: Clear  Esc: Back ";
const HELP_SEARCH: &str = " Enter: Apply  Ctrl+L: Clear  Esc: Cancel ";

pub fn ui(f: &mut Frame, app: &mut App) {
    let input_height = match app.input_mode {
        InputMode::Editing => preferred_composer_height(f.area().height),
        InputMode::Search => 5,
        InputMode::Navigate => 3,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),               // Main panels
            Constraint::Length(input_height), // Composer / Search
            Constraint::Length(1),            // Footer (Help)
        ])
        .split(f.area());

    // 상단 영역을 좌우로 분할 (로그 70%, 할 일 목록 30%)
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[0]);

    // 상단 로그 뷰
    let list_area_width = top_chunks[0].width.saturating_sub(4) as usize; // 테두리 및 여유 공간
    let timestamp_width: usize = 11; // "[HH:MM:SS] "
    let blank_timestamp = " ".repeat(timestamp_width);
    let timestamp_color = parse_color(&app.config.theme.timestamp);

    let list_items: Vec<ListItem> = app
        .logs
        .iter()
        .map(|entry| {
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
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::BOLD),
                            )
                        } else {
                            Span::raw(blank_date.clone())
                        };
                        spans.push(date_span);
                    }

                    if entry_has_timestamp {
                        let ts_span = if line_idx == 0 && wrap_idx == 0 {
                            Span::styled(
                                ts_prefix.to_string(),
                                Style::default().fg(timestamp_color),
                            )
                        } else {
                            Span::raw(blank_timestamp.clone())
                        };
                        spans.push(ts_span);
                    }

                    spans.extend(parse_markdown_spans(
                        wline,
                        &app.config.theme,
                        line_in_code_block,
                    ));
                    lines.push(Line::from(spans));
                }

                if is_fence {
                    in_code_block = !in_code_block;
                }
            }
            ListItem::new(Text::from(lines))
        })
        .collect();

    let is_timeline_focused =
        app.input_mode == InputMode::Navigate && app.navigate_focus == NavigateFocus::Timeline;
    let is_tasks_focused =
        app.input_mode == InputMode::Navigate && app.navigate_focus == NavigateFocus::Tasks;

    let focus_mark_timeline = if is_timeline_focused { "▶" } else { " " };
    let focus_mark_tasks = if is_tasks_focused { "▶" } else { " " };

    let title = if app.is_search_result {
        format!(
            " 🔍 Search Results: {} found (Esc to reset) ",
            app.logs.len()
        )
    } else {
        let time = Local::now().format("%Y-%m-%d %H:%M");
        let pomodoro = if let Some(end_time) = app.pomodoro_end {
            let now = Local::now();
            if now < end_time {
                let remaining = end_time - now;
                let target = match app.pomodoro_target.as_ref() {
                    Some(crate::models::PomodoroTarget::Task { text, .. }) => {
                        format!(" · {}", truncate(text, 24))
                    }
                    _ => "".to_string(),
                };
                format!(
                    " [🍅 {:02}:{:02}{}]",
                    remaining.num_minutes(),
                    remaining.num_seconds() % 60,
                    target
                )
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        };

        let summary = format!(
            "Entries {} · Open {} · Done {} · 🍅 {}",
            app.logs.len(),
            app.tasks.len(),
            app.today_done_tasks,
            app.today_tomatoes
        );

        format!(" {focus_mark_timeline} SONOMEMO · {time} · {summary}{pomodoro} ")
    };

    // 모드에 따른 메인 테두리 색상 결정
    let main_border_color = match app.input_mode {
        InputMode::Navigate => parse_color(&app.config.theme.border_default),
        InputMode::Editing => parse_color(&app.config.theme.border_editing),
        InputMode::Search => parse_color(&app.config.theme.border_search),
    };

    let logs_block =
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(main_border_color).add_modifier(
                if is_timeline_focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                },
            ))
            .title(title);

    let highlight_bg = parse_color(&app.config.theme.text_highlight);
    let logs_highlight_style = if is_timeline_focused {
        Style::default()
            .bg(highlight_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(highlight_bg)
    };

    let logs_list = List::new(list_items)
        .block(logs_block)
        .highlight_symbol("▶ ") // 조금 더 멋진 화살표
        .highlight_style(logs_highlight_style); // 배경색 하이라이트

    f.render_stateful_widget(logs_list, top_chunks[0], &mut app.logs_state);

    // 오른쪽 할 일 목록 뷰 (오늘의 할 일만 필터링)
    let todo_area_width = top_chunks[1].width.saturating_sub(2) as usize; // 테두리 제외

    let todos: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let mut line = String::new();
            line.push_str(&"  ".repeat(task.indent));
            line.push_str("- [ ] ");
            line.push_str(&task.text);

            if let (
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
                        line.push_str(&format!(
                            " ⏱ {:02}:{:02}",
                            remaining.num_minutes(),
                            remaining.num_seconds() % 60
                        ));
                    }
                }
            }

            if task.tomato_count > 0 {
                if task.tomato_count <= 3 {
                    line.push(' ');
                    line.push_str(&"🍅".repeat(task.tomato_count));
                } else {
                    line.push_str(&format!(" 🍅×{}", task.tomato_count));
                }
            }

            let wrapped = wrap_markdown_line(&line, todo_area_width);
            let lines: Vec<Line<'static>> = wrapped
                .iter()
                .map(|l| Line::from(parse_markdown_spans(l, &app.config.theme, false)))
                .collect();
            ListItem::new(Text::from(lines))
        })
        .collect();

    let todo_border_color = parse_color(&app.config.theme.border_todo_header);

    let todo_border_style = if is_tasks_focused {
        Style::default()
            .fg(todo_border_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(todo_border_color)
    };

    let todo_title = format!(
        " {focus_mark_tasks} Tasks · Open {} · Done {} · 🍅 {} ",
        app.tasks.len(),
        app.today_done_tasks,
        app.today_tomatoes
    );

    let todo_block = Block::default()
        .borders(Borders::ALL)
        .title(todo_title)
        .border_style(todo_border_style);

    let highlight_bg = parse_color(&app.config.theme.text_highlight);
    let todo_highlight_style = if is_tasks_focused {
        Style::default()
            .bg(highlight_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(highlight_bg)
    };

    let todo_list = List::new(todos)
        .block(todo_block)
        .highlight_symbol("▶ ")
        .highlight_style(todo_highlight_style);
    f.render_stateful_widget(todo_list, top_chunks[1], &mut app.tasks_state);

    // 하단 영역: Navigate는 Status 패널, Editing/Search는 TextArea
    match app.input_mode {
        InputMode::Navigate => {
            let border_color = parse_color(&app.config.theme.border_default);
            let focus = match app.navigate_focus {
                NavigateFocus::Timeline => "Timeline",
                NavigateFocus::Tasks => "Tasks",
            };

            let selected = match app.navigate_focus {
                NavigateFocus::Timeline => app
                    .logs_state
                    .selected()
                    .and_then(|i| app.logs.get(i))
                    .and_then(|e| e.content.lines().next())
                    .unwrap_or(""),
                NavigateFocus::Tasks => app
                    .tasks_state
                    .selected()
                    .and_then(|i| app.tasks.get(i))
                    .map(|t| t.text.as_str())
                    .unwrap_or(""),
            };

            let status = vec![
                Line::from(vec![
                    Span::styled("Focus: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(focus, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled("Selected: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(truncate(selected, 80)),
                ]),
                Line::from(Span::raw(
                    "Press ? for help. Press i to compose. Press / to search.",
                )),
            ];

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Status ")
                .border_style(Style::default().fg(border_color));
            f.render_widget(
                Paragraph::new(status)
                    .block(block)
                    .wrap(ratatui::widgets::Wrap { trim: true }),
                chunks[1],
            );
        }
        InputMode::Editing | InputMode::Search => {
            let (input_title, border_color) = match app.input_mode {
                crate::models::InputMode::Search => {
                    (" Search ", parse_color(&app.config.theme.border_search))
                }
                crate::models::InputMode::Editing => {
                    (" Composer ", parse_color(&app.config.theme.border_editing))
                }
                crate::models::InputMode::Navigate => unreachable!(),
            };

            let input_block = Block::default()
                .borders(Borders::ALL)
                .title(input_title)
                .border_style(Style::default().fg(border_color));

            app.textarea.set_block(input_block);
            app.textarea
                .set_cursor_line_style(Style::default().underline_color(Color::Reset));
            app.textarea.set_cursor_style(Style::default().reversed());
            f.render_widget(&app.textarea, chunks[1]);
        }
    }

    // 커서 위치 수동 설정 (한글 IME 지원을 위해 필수)
    if app.input_mode == crate::models::InputMode::Editing
        || app.input_mode == crate::models::InputMode::Search
    {
        let input_area = chunks[1];
        let inner = Block::default().borders(Borders::ALL).inner(input_area);

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

    // 하단 도움말 푸터 (toast 우선)
    let help_text = if let Some(msg) = app.toast_message.as_deref() {
        msg
    } else {
        match app.input_mode {
            InputMode::Navigate => HELP_NAVIGATE,
            InputMode::Editing => HELP_COMPOSE,
            InputMode::Search => HELP_SEARCH,
        }
    };
    let footer = Paragraph::new(Line::from(Span::styled(
        help_text,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::NONE));
    f.render_widget(footer, chunks[2]);

    // 팝업 렌더링 (순서 중요: 나중에 렌더링된 것이 위에 뜸)
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

fn preferred_composer_height(total_height: u16) -> u16 {
    let footer = 1;
    let min_main = 6;
    let max_input = total_height.saturating_sub(footer + min_main).max(3);
    let desired = (total_height.saturating_mul(45) / 100).max(10);
    desired.min(max_input)
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

fn is_timestamped_line(line: &str) -> bool {
    // Format: "[HH:MM:SS] " (11+ chars)
    let bytes = line.as_bytes();
    if bytes.len() < 11 {
        return false;
    }
    if bytes[0] != b'[' || bytes[9] != b']' || bytes[10] != b' ' {
        return false;
    }
    bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b':'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
        && bytes[6] == b':'
        && bytes[7].is_ascii_digit()
        && bytes[8].is_ascii_digit()
}

fn file_date(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}
