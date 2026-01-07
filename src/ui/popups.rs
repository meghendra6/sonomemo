use super::components::{
    centered_rect, markdown_prefix_width, parse_markdown_spans, wrap_markdown_line,
};
use crate::app::App;
use crate::config::{EditorStyle, ThemePreset};
use crate::models::{DatePickerField, EditorMode, InputMode, Mood, VisualKind};
use crate::ui::color_parser::parse_color;
use crate::ui::theme::ThemeTokens;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use syntect::easy::HighlightLines;

pub fn render_siren_popup(f: &mut Frame, app: &App) {
    let block = Block::default().borders(Borders::ALL).style(
        Style::default()
            .fg(Color::Red)
            .bg(Color::Black)
            .add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK),
    );

    let area = centered_rect(80, 60, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let message = app
        .pomodoro_alert_message
        .as_deref()
        .unwrap_or("Pomodoro complete.");

    let siren_art = vec![
        "         _______  TIME'S UP!  _______",
        "        /       \\            /       \\",
        "       |  (o)  |   🚨🚨🚨   |  (o)  |",
        "        \\_______/            \\_______/",
        "",
        message,
        "",
        "Take a break. Stretch. Drink water.",
    ];

    let text_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(2)
        .split(area)[0];

    let mut art_spans = Vec::new();
    for line in siren_art {
        art_spans.push(ListItem::new(Line::from(Span::styled(
            line,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))));
    }

    f.render_widget(List::new(art_spans), text_area);
}

pub fn render_activity_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .title(" 🌱 Activity Graph (Last 2 Weeks) ")
        .borders(Borders::ALL);
    let area = centered_rect(70, 50, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let today = Local::now().date_naive();
    let mut items = Vec::new();

    // Header row
    items.push(ListItem::new(Line::from(vec![Span::styled(
        "Date        Logs  🍅   Activity                    Pomodoros",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )])));
    items.push(ListItem::new(Line::from("")));

    for i in 0..14 {
        let date = today - chrono::Duration::days(i);
        let date_str = date.format("%Y-%m-%d").to_string();
        let (line_count, tomato_count) =
            app.activity_data.get(&date_str).cloned().unwrap_or((0, 0));

        // Activity bar (based on log count)
        let bar_len = line_count.min(20);
        let bar: String = "■".repeat(bar_len);

        let activity_color = if line_count == 0 {
            Color::DarkGray
        } else if line_count < 5 {
            Color::Green
        } else if line_count < 15 {
            Color::LightGreen
        } else {
            Color::Yellow
        };

        // Pomodoro bar (🍅 count)
        let tomato_bar_len = tomato_count.min(10);
        let tomato_bar: String = "🍅".repeat(tomato_bar_len);
        let tomato_extra = if tomato_count > 10 {
            format!("+{}", tomato_count - 10)
        } else {
            String::new()
        };

        items.push(ListItem::new(Line::from(vec![
            Span::raw(format!("{} ", date_str)),
            Span::styled(
                format!("{:3}", line_count),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:2}", tomato_count),
                Style::default().fg(Color::Red),
            ),
            Span::raw("   "),
            Span::styled(format!("{:<20}", bar), Style::default().fg(activity_color)),
            Span::raw(" "),
            Span::raw(tomato_bar),
            Span::styled(tomato_extra, Style::default().fg(Color::Red)),
        ])));
    }

    let inner_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(2)
        .split(area)[0];

    f.render_widget(List::new(items), inner_area);
}

pub fn render_memo_preview_popup(f: &mut Frame, app: &App) {
    let Some(entry) = app.memo_preview_entry.as_ref() else {
        return;
    };

    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" Memo Preview ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(90, 80, f.area());
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let content_area = sections[0];
    let footer_area = sections[1];

    let width = content_area.width.saturating_sub(2).max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let theme_preset = super::resolve_theme_preset(&app.config);
    let syntax_set = super::syntax_set();
    let syntax_theme = super::select_syntax_theme(super::syntax_theme_set(), &tokens, theme_preset);
    let code_bg = super::code_block_background(&tokens);
    let fence_style = super::code_fallback_style(code_bg).fg(tokens.ui_muted);
    let mut in_code_block = false;
    let mut code_highlighter: Option<HighlightLines> = None;

    for raw_line in entry.content.lines() {
        let trimmed = raw_line.trim_start();
        let is_fence = trimmed.starts_with("```");
        let opening_fence = is_fence && !in_code_block;
        let closing_fence = is_fence && in_code_block;
        if opening_fence {
            let language = super::parse_fence_language(trimmed);
            let syntax = super::syntax_for_language(syntax_set, language.as_deref());
            code_highlighter = Some(HighlightLines::new(syntax, syntax_theme));
        }

        let line_in_code_block = in_code_block || is_fence;
        let wrapped = wrap_markdown_line(raw_line, width);
        let code_segments = if line_in_code_block {
            if is_fence {
                Some(vec![super::StyledSegment {
                    text: raw_line.to_string(),
                    style: fence_style,
                }])
            } else if let Some(highlighter) = code_highlighter.as_mut() {
                Some(super::highlight_code_line(
                    raw_line,
                    highlighter,
                    syntax_set,
                    code_bg,
                ))
            } else {
                Some(vec![super::StyledSegment {
                    text: raw_line.to_string(),
                    style: super::code_fallback_style(code_bg),
                }])
            }
        } else {
            None
        };
        let prefix_width = if code_segments.is_some() {
            markdown_prefix_width(raw_line)
        } else {
            0
        };
        let mut segment_start_col = 0usize;
        for (wrap_idx, line) in wrapped.iter().enumerate() {
            if let Some(segments) = code_segments.as_ref() {
                let segment_len = line.chars().count();
                let (code_spans, consumed_len) = super::code_spans_for_wrapped_line(
                    segments,
                    wrap_idx,
                    segment_start_col,
                    segment_len,
                    prefix_width,
                    code_bg,
                );
                lines.push(Line::from(code_spans));
                segment_start_col = segment_start_col.saturating_add(consumed_len);
            } else {
                lines.push(Line::from(parse_markdown_spans(
                    line,
                    &app.config.theme,
                    line_in_code_block,
                    None,
                    Style::default(),
                )));
            }
        }

        if closing_fence {
            in_code_block = false;
            code_highlighter = None;
        } else if opening_fence {
            in_code_block = true;
        }
    }

    let max_scroll = lines.len().saturating_sub(content_area.height as usize);
    let scroll = app.memo_preview_scroll.min(max_scroll);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    f.render_widget(paragraph, content_area);

    let footer = Paragraph::new("Esc close · E edit · J/K scroll")
        .style(Style::default().fg(tokens.ui_muted));
    f.render_widget(footer, footer_area);
}

pub fn render_ai_response_popup(f: &mut Frame, app: &App) {
    let Some(response) = app.ai_response.as_ref() else {
        return;
    };

    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" AI Answer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(90, 80, f.area());
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let content_area = sections[0];
    let footer_area = sections[1];

    let mut body = String::new();
    body.push_str("Question: ");
    body.push_str(response.question.trim());
    body.push('\n');
    if !response.keywords.is_empty() {
        body.push_str("Keywords: ");
        body.push_str(&response.keywords.join(", "));
        body.push_str("\n\n");
    } else {
        body.push('\n');
    }
    let answer = response.answer.trim();
    if answer.is_empty() {
        body.push_str("Answer: (no response)");
    } else {
        body.push_str(answer);
    }

    if !response.entries.is_empty() {
        body.push_str("\n\nSources:\n");
        for (idx, entry) in response.entries.iter().enumerate() {
            let file = std::path::Path::new(&entry.file_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(entry.file_path.as_str());
            let preview = first_content_line(&entry.content);
            body.push_str(&format!(
                "- [{idx}] {file}:{line} {preview}\n",
                idx = idx + 1,
                file = file,
                line = entry.line_number + 1,
                preview = preview
            ));
        }
    }

    let width = content_area.width.saturating_sub(2).max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let theme_preset = super::resolve_theme_preset(&app.config);
    let syntax_set = super::syntax_set();
    let syntax_theme = super::select_syntax_theme(super::syntax_theme_set(), &tokens, theme_preset);
    let code_bg = super::code_block_background(&tokens);
    let fence_style = super::code_fallback_style(code_bg).fg(tokens.ui_muted);
    let mut in_code_block = false;
    let mut code_highlighter: Option<HighlightLines> = None;

    for raw_line in body.lines() {
        let trimmed = raw_line.trim_start();
        let is_fence = trimmed.starts_with("```");
        let opening_fence = is_fence && !in_code_block;
        let closing_fence = is_fence && in_code_block;
        if opening_fence {
            let language = super::parse_fence_language(trimmed);
            let syntax = super::syntax_for_language(syntax_set, language.as_deref());
            code_highlighter = Some(HighlightLines::new(syntax, syntax_theme));
        }

        let line_in_code_block = in_code_block || is_fence;
        let wrapped = wrap_markdown_line(raw_line, width);
        let code_segments = if line_in_code_block {
            if is_fence {
                Some(vec![super::StyledSegment {
                    text: raw_line.to_string(),
                    style: fence_style,
                }])
            } else if let Some(highlighter) = code_highlighter.as_mut() {
                Some(super::highlight_code_line(
                    raw_line,
                    highlighter,
                    syntax_set,
                    code_bg,
                ))
            } else {
                Some(vec![super::StyledSegment {
                    text: raw_line.to_string(),
                    style: super::code_fallback_style(code_bg),
                }])
            }
        } else {
            None
        };
        let prefix_width = if code_segments.is_some() {
            markdown_prefix_width(raw_line)
        } else {
            0
        };
        let mut segment_start_col = 0usize;
        for (wrap_idx, line) in wrapped.iter().enumerate() {
            if let Some(segments) = code_segments.as_ref() {
                let segment_len = line.chars().count();
                let (code_spans, consumed_len) = super::code_spans_for_wrapped_line(
                    segments,
                    wrap_idx,
                    segment_start_col,
                    segment_len,
                    prefix_width,
                    code_bg,
                );
                lines.push(Line::from(code_spans));
                segment_start_col = segment_start_col.saturating_add(consumed_len);
            } else {
                lines.push(Line::from(parse_markdown_spans(
                    line,
                    &app.config.theme,
                    line_in_code_block,
                    None,
                    Style::default(),
                )));
            }
        }

        if closing_fence {
            in_code_block = false;
            code_highlighter = None;
        } else if opening_fence {
            in_code_block = true;
        }
    }

    let max_scroll = lines.len().saturating_sub(content_area.height as usize);
    let scroll = app.ai_response_scroll.min(max_scroll);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    f.render_widget(paragraph, content_area);

    let footer = Paragraph::new("Esc close · J/K scroll · S save")
        .style(Style::default().fg(tokens.ui_muted));
    f.render_widget(footer, footer_area);
}

pub fn render_ai_loading_popup(f: &mut Frame, app: &App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" AI Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(70, 30, f.area());
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let spinner = loading_spinner();
    let title = Line::from(vec![
        Span::styled(spinner, Style::default().fg(tokens.ui_accent)),
        Span::raw(" "),
        Span::styled(
            "Analyzing question and searching memos...",
            Style::default().fg(tokens.ui_fg),
        ),
    ]);
    f.render_widget(Paragraph::new(title), chunks[0]);

    let question = app
        .ai_loading_question
        .as_deref()
        .unwrap_or("Preparing request...");
    let content = Paragraph::new(question)
        .style(Style::default().fg(tokens.ui_muted))
        .wrap(Wrap { trim: true });
    f.render_widget(content, chunks[1]);

    let footer = Paragraph::new("Esc hide")
        .style(Style::default().fg(tokens.ui_muted));
    f.render_widget(footer, chunks[2]);
}

fn loading_spinner() -> &'static str {
    const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
    let idx = (Local::now().timestamp_subsec_millis() / 250) as usize % FRAMES.len();
    FRAMES[idx]
}

fn first_content_line(text: &str) -> String {
    for line in text.lines() {
        let trimmed = crate::models::strip_timestamp_prefix(line).trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

pub fn render_date_picker_popup(f: &mut Frame, app: &App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" 📅 Date/Time Picker ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(80, 60, f.area());
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(inner);

    let body = sections[0];
    let footer = sections[1];
    let input_line = sections[2];

    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(1)])
        .split(body);

    render_date_picker_fields(f, app, body_cols[0], &tokens);
    render_date_picker_detail(f, app, body_cols[1], &tokens);

    let footer_text = "Enter apply | Esc cancel | +/- day | [/] week | T today | R relative";
    let footer_style = Style::default().fg(tokens.ui_muted);
    f.render_widget(Paragraph::new(footer_text).style(footer_style), footer);

    let input_text = if app.date_picker_input_mode {
        format!("Relative: {}", app.date_picker_input)
    } else {
        "Relative: (press R)".to_string()
    };
    f.render_widget(
        Paragraph::new(input_text).style(Style::default().fg(tokens.ui_accent)),
        input_line,
    );
}

fn render_date_picker_fields(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    tokens: &ThemeTokens,
) {
    let fields = [
        DatePickerField::Scheduled,
        DatePickerField::Due,
        DatePickerField::Start,
        DatePickerField::Time,
        DatePickerField::Duration,
    ];
    let selected = fields
        .iter()
        .position(|f| *f == app.date_picker_field)
        .unwrap_or(0);

    let mut items: Vec<ListItem> = Vec::new();
    for field in fields {
        let label = date_picker_field_label(field);
        let value = date_picker_field_value(app, field);
        let line = Line::from(vec![
            Span::styled(
                format!("{:<10}", label),
                Style::default().fg(tokens.ui_accent),
            ),
            Span::styled(value, Style::default().fg(tokens.ui_fg)),
        ]);
        items.push(ListItem::new(line));
    }

    let highlight = Style::default()
        .bg(tokens.ui_selection_bg)
        .add_modifier(Modifier::BOLD);
    let list = List::new(items)
        .highlight_style(highlight)
        .highlight_symbol(" ");
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_date_picker_detail(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    tokens: &ThemeTokens,
) {
    match app.date_picker_field {
        DatePickerField::Scheduled | DatePickerField::Due | DatePickerField::Start => {
            render_date_picker_calendar(f, app, area, tokens);
        }
        DatePickerField::Time => render_date_picker_time(f, app, area, tokens),
        DatePickerField::Duration => render_date_picker_duration(f, app, area, tokens),
    }
}

fn render_date_picker_calendar(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    tokens: &ThemeTokens,
) {
    let selected = app.date_picker_effective_date(app.date_picker_field);
    let today = Local::now().date_naive();
    let header = selected.format("%B %Y").to_string();

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(tokens.ui_accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("Mo Tu We Th Fr Sa Su"));

    let month_start =
        NaiveDate::from_ymd_opt(selected.year(), selected.month(), 1).unwrap_or(selected);
    let first_weekday = month_start.weekday().num_days_from_monday() as usize;
    let days_in_month = last_day_of_month(selected.year(), selected.month());

    let mut day = 1u32;
    for row in 0..6 {
        let mut spans: Vec<Span> = Vec::new();
        for col in 0..7 {
            let index = row * 7 + col;
            if index < first_weekday || day > days_in_month {
                spans.push(Span::raw("   "));
                continue;
            }

            let date =
                NaiveDate::from_ymd_opt(selected.year(), selected.month(), day).unwrap_or(selected);
            let mut style = Style::default().fg(tokens.ui_fg);
            if date == today {
                style = style.fg(tokens.ui_accent);
            }
            if date == selected {
                style = style
                    .fg(tokens.ui_bg)
                    .bg(tokens.ui_accent)
                    .add_modifier(Modifier::BOLD);
            }

            spans.push(Span::styled(format!("{:>2} ", day), style));
            day += 1;
        }
        lines.push(Line::from(spans));
        if day > days_in_month {
            break;
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_date_picker_time(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    tokens: &ThemeTokens,
) {
    let selected = app.date_picker_effective_time();
    let header = format!("Time: {}", format_time(selected));

    let mut spans: Vec<Span> = Vec::new();
    for offset in -3..=3 {
        let time = add_minutes_wrapping(selected, offset * 15);
        let mut style = Style::default().fg(tokens.ui_fg);
        if offset == 0 {
            style = style
                .bg(tokens.ui_selection_bg)
                .add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(format!("{:>5}", format_time(time)), style));
        spans.push(Span::raw(" "));
    }

    let lines = vec![
        Line::from(Span::styled(
            header,
            Style::default()
                .fg(tokens.ui_accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(spans),
        Line::from(""),
        Line::from("Use left/right or +/- to adjust by 15m, [/] for 60m."),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn render_date_picker_duration(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    tokens: &ThemeTokens,
) {
    let selected = app.date_picker_effective_duration();
    let header = format!("Duration: {}", format_duration(selected));

    let presets = [15u32, 30, 45, 60, 90, 120];
    let mut spans: Vec<Span> = Vec::new();
    for preset in presets {
        let mut style = Style::default().fg(tokens.ui_fg);
        if preset == selected {
            style = style
                .bg(tokens.ui_selection_bg)
                .add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(
            format!("{:>4}", format_duration(preset)),
            style,
        ));
        spans.push(Span::raw(" "));
    }

    let lines = vec![
        Line::from(Span::styled(
            header,
            Style::default()
                .fg(tokens.ui_accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(spans),
        Line::from(""),
        Line::from("Use left/right or +/- to adjust by 15m, [/] for 60m."),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn date_picker_field_label(field: DatePickerField) -> &'static str {
    match field {
        DatePickerField::Scheduled => "Scheduled",
        DatePickerField::Due => "Due",
        DatePickerField::Start => "Start",
        DatePickerField::Time => "Time",
        DatePickerField::Duration => "Duration",
    }
}

fn date_picker_field_value(app: &App, field: DatePickerField) -> String {
    match field {
        DatePickerField::Scheduled => app
            .date_picker_schedule
            .scheduled
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "--".to_string()),
        DatePickerField::Due => app
            .date_picker_schedule
            .due
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "--".to_string()),
        DatePickerField::Start => app
            .date_picker_schedule
            .start
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "--".to_string()),
        DatePickerField::Time => app
            .date_picker_schedule
            .time
            .map(format_time)
            .unwrap_or_else(|| "--".to_string()),
        DatePickerField::Duration => app
            .date_picker_schedule
            .duration_minutes
            .map(format_duration)
            .unwrap_or_else(|| "--".to_string()),
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year, month, 1).unwrap());
    let last = first_next - Duration::days(1);
    last.day()
}

fn add_minutes_wrapping(time: NaiveTime, delta: i32) -> NaiveTime {
    let total = time.hour() as i32 * 60 + time.minute() as i32 + delta;
    let minutes = total.rem_euclid(24 * 60) as u32;
    NaiveTime::from_hms_opt(minutes / 60, minutes % 60, 0)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap())
}

fn format_time(time: NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn format_duration(minutes: u32) -> String {
    if minutes >= 60 {
        let hours = minutes / 60;
        let mins = minutes % 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h{mins}m")
        }
    } else {
        format!("{minutes}m")
    }
}

pub fn render_mood_popup(f: &mut Frame, app: &mut App) {
    let block = Block::default()
        .title(" Mood Check-in ")
        .borders(Borders::ALL);
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let moods = Mood::all();
    let items: Vec<ListItem> = moods.iter().map(|m| ListItem::new(m.as_str())).collect();

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(1)
        .split(area);

    let list = List::new(items)
        .highlight_symbol(">> ")
        .highlight_style(Style::default().fg(Color::Yellow));

    f.render_stateful_widget(list, popup_layout[0], &mut app.mood_list_state);
}

pub fn render_todo_popup(f: &mut Frame, app: &mut App) {
    let title = format!(
        " Carry over {} unfinished tasks from the last session? (Y/n) ",
        app.pending_todos.len()
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::LightRed));
    let area = centered_rect(70, 40, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = app
        .pending_todos
        .iter()
        .map(|t| ListItem::new(format!("• {}", t)))
        .collect();

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(1)
        .split(area);

    let list = List::new(items).highlight_symbol(">> ");

    f.render_stateful_widget(list, popup_layout[0], &mut app.todo_list_state);
}

pub fn render_tag_popup(f: &mut Frame, app: &mut App) {
    let selection = app
        .tag_list_state
        .selected()
        .map(|i| format!("{}/{}", i + 1, app.tags.len()))
        .unwrap_or_else(|| "0/0".to_string());
    let title = format!(" Tags {selection} · Enter: filter · Esc: close ");
    let block = Block::default().title(title).borders(Borders::ALL);
    let area = centered_rect(50, 60, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let tag_color = parse_color(&app.config.theme.tag);
    let items: Vec<ListItem> = app
        .tags
        .iter()
        .map(|(tag, count)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    tag.clone(),
                    Style::default().fg(tag_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" ({})", count)),
            ]))
        })
        .collect();

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(1)
        .split(area);

    let highlight_bg = parse_color(&app.config.theme.text_highlight);
    let list = List::new(items).highlight_symbol("").highlight_style(
        Style::default()
            .bg(highlight_bg)
            .add_modifier(Modifier::BOLD),
    );

    f.render_stateful_widget(list, popup_layout[0], &mut app.tag_list_state);
}

pub fn render_path_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .title(" 📂 Log Directory Path ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    let area = centered_rect(70, 20, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    // Try to get absolute path
    let path_str = if let Ok(abs_path) = std::fs::canonicalize(&app.config.data.log_path) {
        abs_path.to_string_lossy().to_string()
    } else {
        // Fallback to configured path if canonicalize fails (e.g., path doesn't exist yet)
        let mut p = std::env::current_dir().unwrap_or_default();
        p.push(&app.config.data.log_path);
        p.to_string_lossy().to_string()
    };

    let text_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .margin(2)
        .split(area);

    let path_text = Paragraph::new(path_str)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .wrap(ratatui::widgets::Wrap { trim: true });

    let help_text = Paragraph::new("[Enter] Open Folder    [Esc] Close")
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(path_text, text_area[0]);
    f.render_widget(help_text, text_area[1]);
}

pub fn render_google_auth_popup(f: &mut Frame, app: &App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" Google Sync ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(70, 40, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let text_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(2)
        .split(area);

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Connect your Google account",
        Style::default()
            .fg(tokens.ui_accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if let Some(display) = app.google_auth_display.as_ref() {
        let remaining = display.expires_at.signed_duration_since(Local::now());
        let remaining_seconds = remaining.num_seconds().max(0);
        let remaining_text = if remaining_seconds > 0 {
            format!(
                "Expires in {:02}m {:02}s",
                remaining_seconds / 60,
                remaining_seconds % 60
            )
        } else {
            "Authorization expired. Press Esc and try again.".to_string()
        };

        lines.push(Line::from(Span::raw(
            "Open this local URL in your browser:",
        )));
        lines.push(Line::from(Span::styled(
            display.local_url.as_str(),
            Style::default()
                .fg(tokens.ui_accent)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "This will redirect to Google and back to {}",
                display.listen_addr
            ),
            Style::default().fg(tokens.ui_muted),
        )));
        lines.push(Line::from(Span::styled(
            remaining_text,
            Style::default().fg(tokens.ui_muted),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Starting Google authorization…",
            Style::default().fg(tokens.ui_muted),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Enter] Open browser    [Esc] Close",
        Style::default().fg(tokens.ui_muted),
    )));

    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true });
    f.render_widget(paragraph, text_area[0]);
}

pub fn render_help_popup(f: &mut Frame, app: &App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    if app.input_mode == InputMode::Editing
        && let EditorMode::Visual(kind) = app.editor_mode
    {
        render_visual_help_popup(f, app, kind, &tokens);
        return;
    }

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .margin(2)
        .split(area);

    let content_area = inner_area[0];
    let column_count = help_column_count(content_area.width);
    let mut sections = help_sections(app, false);
    let mut block_gap = 1;
    let (mut columns, mut overflow) =
        layout_help_sections(&sections, column_count, content_area.height, block_gap);
    if overflow {
        sections = help_sections(app, true);
        block_gap = 0;
        let layout = layout_help_sections(&sections, column_count, content_area.height, block_gap);
        columns = layout.0;
        overflow = layout.1;
    }

    let column_areas = if column_count == 1 {
        vec![content_area]
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(help_column_constraints(column_count))
            .split(content_area)
            .to_vec()
    };

    for (idx, area) in column_areas.iter().enumerate() {
        if let Some(section_list) = columns.get(idx) {
            render_help_blocks_column(f, *area, section_list, &tokens, block_gap);
        }
    }
    let footer_text = if overflow {
        "Some sections hidden (widen window) · Esc / ?: close"
    } else {
        "Esc / ?: close"
    };
    let muted_style = Style::default().fg(tokens.ui_muted);
    f.render_widget(
        Paragraph::new(footer_text).style(muted_style),
        inner_area[1],
    );
}

#[derive(Clone)]
struct HelpSection {
    title: String,
    entries: Vec<(String, String)>,
    show_header: bool,
}

fn join_key_groups_with_sep(groups: &[String], sep: &str) -> String {
    let mut filtered = Vec::new();
    for value in groups {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "-" {
            continue;
        }
        filtered.push(trimmed.to_string());
    }
    if filtered.is_empty() {
        "-".to_string()
    } else {
        filtered.join(sep)
    }
}

fn help_sections(app: &App, compact: bool) -> Vec<HelpSection> {
    let kb = &app.config.keybindings;
    let show_header = !compact;

    let timeline_filter_keys = join_key_groups_with_sep(
        &[
            fmt_keys(&kb.timeline.filter_work),
            fmt_keys(&kb.timeline.filter_personal),
            fmt_keys(&kb.timeline.filter_all),
        ],
        " / ",
    );
    let timeline_context_keys = join_key_groups_with_sep(
        &[
            fmt_keys(&kb.timeline.context_work),
            fmt_keys(&kb.timeline.context_personal),
            fmt_keys(&kb.timeline.context_clear),
        ],
        " / ",
    );
    let tasks_filter_keys = join_key_groups_with_sep(
        &[
            fmt_keys(&kb.tasks.filter_open),
            fmt_keys(&kb.tasks.filter_done),
            fmt_keys(&kb.tasks.filter_all),
        ],
        " / ",
    );
    let composer_context_keys = join_key_groups_with_sep(
        &[
            fmt_keys(&kb.composer.context_work),
            fmt_keys(&kb.composer.context_personal),
            fmt_keys(&kb.composer.context_clear),
        ],
        " / ",
    );

    let mut composer_entries = if compact {
        vec![
            (
                "Save / Back".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.composer.submit), fmt_keys(&kb.composer.cancel)],
                    " | ",
                ),
            ),
            ("New line".to_string(), fmt_keys(&kb.composer.newline)),
            (
                "Task / Priority".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.composer.task_toggle),
                        fmt_keys(&kb.composer.priority_cycle),
                    ],
                    " | ",
                ),
            ),
            (
                "Date picker".to_string(),
                fmt_keys(&kb.composer.date_picker),
            ),
            (
                "Context: work/personal/clear".to_string(),
                composer_context_keys,
            ),
            (
                "Indent / Outdent".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.composer.indent),
                        fmt_keys(&kb.composer.outdent),
                    ],
                    " | ",
                ),
            ),
            ("Clear".to_string(), fmt_keys(&kb.composer.clear)),
        ]
    } else {
        vec![
            ("Save".to_string(), fmt_keys(&kb.composer.submit)),
            ("New line".to_string(), fmt_keys(&kb.composer.newline)),
            (
                "Toggle task".to_string(),
                fmt_keys(&kb.composer.task_toggle),
            ),
            (
                "Priority cycle".to_string(),
                fmt_keys(&kb.composer.priority_cycle),
            ),
            (
                "Date picker".to_string(),
                fmt_keys(&kb.composer.date_picker),
            ),
            (
                "Context: work/personal/clear".to_string(),
                composer_context_keys,
            ),
            ("Indent".to_string(), fmt_keys(&kb.composer.indent)),
            ("Outdent".to_string(), fmt_keys(&kb.composer.outdent)),
            ("Clear".to_string(), fmt_keys(&kb.composer.clear)),
            ("Back".to_string(), fmt_keys(&kb.composer.cancel)),
        ]
    };
    if app.is_vim_mode() {
        composer_entries.push(("Vim motions".to_string(), "hjkl, w, b, e, ...".to_string()));
        composer_entries.push(("Visual mode".to_string(), "v, V, Ctrl+v".to_string()));
    }
    if compact {
        composer_entries.retain(|(_, keys)| keys != "-");
        if composer_entries.is_empty() {
            composer_entries.push(("No bindings".to_string(), "-".to_string()));
        }
    }

    let global_entries = if compact {
        vec![
            ("Help".to_string(), fmt_keys(&kb.global.help)),
            ("Focus move".to_string(), "Ctrl+H/J/K/L".to_string()),
            (
                "Compose / Search / Tags".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.global.focus_composer),
                        fmt_keys(&kb.global.search),
                        fmt_keys(&kb.global.tags),
                    ],
                    " | ",
                ),
            ),
            (
                "Pomodoro / Activity".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.global.pomodoro), fmt_keys(&kb.global.activity)],
                    " | ",
                ),
            ),
            ("Focus agenda".to_string(), fmt_keys(&kb.global.agenda)),
            (
                "Log dir / Config".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.global.log_dir),
                        fmt_keys(&kb.global.edit_config),
                    ],
                    " | ",
                ),
            ),
            (
                "Theme presets / Editor style".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.global.theme_switcher),
                        fmt_keys(&kb.global.editor_style_switcher),
                    ],
                    " | ",
                ),
            ),
            ("Google sync".to_string(), fmt_keys(&kb.global.sync_google)),
            ("Quit".to_string(), fmt_keys(&kb.global.quit)),
        ]
    } else {
        vec![
            ("Help".to_string(), fmt_keys(&kb.global.help)),
            ("Focus move".to_string(), "Ctrl+H/J/K/L".to_string()),
            ("Compose".to_string(), fmt_keys(&kb.global.focus_composer)),
            ("Search".to_string(), fmt_keys(&kb.global.search)),
            ("Tags".to_string(), fmt_keys(&kb.global.tags)),
            ("Pomodoro".to_string(), fmt_keys(&kb.global.pomodoro)),
            ("Activity".to_string(), fmt_keys(&kb.global.activity)),
            ("Focus agenda".to_string(), fmt_keys(&kb.global.agenda)),
            ("Log dir".to_string(), fmt_keys(&kb.global.log_dir)),
            ("Config".to_string(), fmt_keys(&kb.global.edit_config)),
            (
                "Theme presets".to_string(),
                fmt_keys(&kb.global.theme_switcher),
            ),
            (
                "Editor style".to_string(),
                fmt_keys(&kb.global.editor_style_switcher),
            ),
            (
                "Google sync (experimental)".to_string(),
                fmt_keys(&kb.global.sync_google),
            ),
            ("Quit".to_string(), fmt_keys(&kb.global.quit)),
        ]
    };

    let timeline_entries = if compact {
        vec![
            (
                "Move up/down".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.timeline.up), fmt_keys(&kb.timeline.down)],
                    " | ",
                ),
            ),
            (
                "Move page/top/bottom".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.timeline.page_up),
                        fmt_keys(&kb.timeline.page_down),
                        fmt_keys(&kb.timeline.top),
                        fmt_keys(&kb.timeline.bottom),
                    ],
                    " | ",
                ),
            ),
            (
                "Fold toggle / cycle".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.timeline.fold_toggle),
                        fmt_keys(&kb.timeline.fold_cycle),
                    ],
                    " | ",
                ),
            ),
            (
                "Filter cycle / set".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.timeline.filter_toggle),
                        timeline_filter_keys.clone(),
                    ],
                    " | ",
                ),
            ),
            (
                "Context: work/personal/clear".to_string(),
                timeline_context_keys.clone(),
            ),
            (
                "Edit / Complete tasks".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.timeline.edit),
                        fmt_keys(&kb.timeline.toggle_todo),
                    ],
                    " | ",
                ),
            ),
        ]
    } else {
        vec![
            ("Up".to_string(), fmt_keys(&kb.timeline.up)),
            ("Down".to_string(), fmt_keys(&kb.timeline.down)),
            ("Page up".to_string(), fmt_keys(&kb.timeline.page_up)),
            ("Page down".to_string(), fmt_keys(&kb.timeline.page_down)),
            ("Top".to_string(), fmt_keys(&kb.timeline.top)),
            ("Bottom".to_string(), fmt_keys(&kb.timeline.bottom)),
            (
                "Fold toggle".to_string(),
                fmt_keys(&kb.timeline.fold_toggle),
            ),
            ("Fold cycle".to_string(), fmt_keys(&kb.timeline.fold_cycle)),
            (
                "Filter cycle".to_string(),
                fmt_keys(&kb.timeline.filter_toggle),
            ),
            (
                "Filter: work/personal/all".to_string(),
                timeline_filter_keys.clone(),
            ),
            (
                "Context: work/personal/clear".to_string(),
                timeline_context_keys.clone(),
            ),
            ("Edit".to_string(), fmt_keys(&kb.timeline.edit)),
            (
                "Complete tasks".to_string(),
                fmt_keys(&kb.timeline.toggle_todo),
            ),
        ]
    };

    let tasks_entries = if compact {
        vec![
            (
                "Move up/down".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.tasks.up), fmt_keys(&kb.tasks.down)],
                    " | ",
                ),
            ),
            (
                "Toggle / Open".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.tasks.toggle), fmt_keys(&kb.tasks.open)],
                    " | ",
                ),
            ),
            (
                "Priority / Pomodoro".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.tasks.priority_cycle),
                        fmt_keys(&kb.tasks.start_pomodoro),
                    ],
                    " | ",
                ),
            ),
            ("Edit".to_string(), fmt_keys(&kb.tasks.edit)),
            (
                "Filter cycle / set".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.tasks.filter_toggle), tasks_filter_keys.clone()],
                    " | ",
                ),
            ),
        ]
    } else {
        vec![
            ("Up".to_string(), fmt_keys(&kb.tasks.up)),
            ("Down".to_string(), fmt_keys(&kb.tasks.down)),
            ("Toggle".to_string(), fmt_keys(&kb.tasks.toggle)),
            ("Open memo".to_string(), fmt_keys(&kb.tasks.open)),
            (
                "Priority cycle".to_string(),
                fmt_keys(&kb.tasks.priority_cycle),
            ),
            ("Pomodoro".to_string(), fmt_keys(&kb.tasks.start_pomodoro)),
            ("Edit".to_string(), fmt_keys(&kb.tasks.edit)),
            (
                "Filter cycle".to_string(),
                fmt_keys(&kb.tasks.filter_toggle),
            ),
            (
                "Filter: open/done/all".to_string(),
                tasks_filter_keys.clone(),
            ),
        ]
    };

    let agenda_entries = if compact {
        vec![
            (
                "Move up/down".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.agenda.up), fmt_keys(&kb.agenda.down)],
                    " | ",
                ),
            ),
            (
                "Open / Toggle".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.agenda.open), fmt_keys(&kb.agenda.toggle)],
                    " | ",
                ),
            ),
            ("Filter".to_string(), fmt_keys(&kb.agenda.filter)),
            (
                "Prev/Next day".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.agenda.prev_day), fmt_keys(&kb.agenda.next_day)],
                    " | ",
                ),
            ),
            (
                "Prev/Next week".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.agenda.prev_week),
                        fmt_keys(&kb.agenda.next_week),
                    ],
                    " | ",
                ),
            ),
            (
                "Today / Unscheduled".to_string(),
                join_key_groups_with_sep(
                    &[
                        fmt_keys(&kb.agenda.today),
                        fmt_keys(&kb.agenda.toggle_unscheduled),
                    ],
                    " | ",
                ),
            ),
        ]
    } else {
        vec![
            ("Up".to_string(), fmt_keys(&kb.agenda.up)),
            ("Down".to_string(), fmt_keys(&kb.agenda.down)),
            ("Open memo".to_string(), fmt_keys(&kb.agenda.open)),
            ("Toggle task".to_string(), fmt_keys(&kb.agenda.toggle)),
            ("Filter cycle".to_string(), fmt_keys(&kb.agenda.filter)),
            ("Prev day".to_string(), fmt_keys(&kb.agenda.prev_day)),
            ("Next day".to_string(), fmt_keys(&kb.agenda.next_day)),
            ("Prev week".to_string(), fmt_keys(&kb.agenda.prev_week)),
            ("Next week".to_string(), fmt_keys(&kb.agenda.next_week)),
            ("Today".to_string(), fmt_keys(&kb.agenda.today)),
            (
                "Unscheduled".to_string(),
                fmt_keys(&kb.agenda.toggle_unscheduled),
            ),
        ]
    };

    let mut search_entries = if compact {
        vec![
            (
                "Apply / Cancel".to_string(),
                join_key_groups_with_sep(
                    &[fmt_keys(&kb.search.submit), fmt_keys(&kb.search.cancel)],
                    " | ",
                ),
            ),
            ("Clear".to_string(), fmt_keys(&kb.search.clear)),
            ("AI search".to_string(), "Prefix ? / ai: / ask:".to_string()),
        ]
    } else {
        vec![
            ("Apply".to_string(), fmt_keys(&kb.search.submit)),
            ("Clear".to_string(), fmt_keys(&kb.search.clear)),
            ("Cancel".to_string(), fmt_keys(&kb.search.cancel)),
            ("AI search".to_string(), "Prefix ? / ai: / ask:".to_string()),
        ]
    };
    if compact {
        search_entries.retain(|(_, keys)| keys != "-");
        if search_entries.is_empty() {
            search_entries.push(("No bindings".to_string(), "-".to_string()));
        }
    }

    vec![
        HelpSection {
            title: "Global".to_string(),
            entries: global_entries,
            show_header,
        },
        HelpSection {
            title: "Timeline".to_string(),
            entries: timeline_entries,
            show_header,
        },
        HelpSection {
            title: "Tasks".to_string(),
            entries: tasks_entries,
            show_header,
        },
        HelpSection {
            title: if app.is_vim_mode() {
                "Composer (Vim mode)".to_string()
            } else {
                "Composer (Simple mode)".to_string()
            },
            entries: composer_entries,
            show_header,
        },
        HelpSection {
            title: "Agenda".to_string(),
            entries: agenda_entries,
            show_header,
        },
        HelpSection {
            title: "Search".to_string(),
            entries: search_entries,
            show_header,
        },
    ]
}

fn help_section_height(section: &HelpSection) -> usize {
    let header_lines = if section.show_header { 1 } else { 0 };
    let content_lines = section.entries.len() + header_lines;
    content_lines + 2
}

fn help_column_count(width: u16) -> usize {
    if width >= 120 {
        3
    } else if width >= 70 {
        2
    } else {
        1
    }
}

fn help_column_constraints(column_count: usize) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    if column_count == 0 {
        return constraints;
    }
    let base = 100 / column_count as u16;
    let mut used = 0u16;
    for idx in 0..column_count {
        let value = if idx + 1 == column_count {
            100 - used
        } else {
            base
        };
        used += value;
        constraints.push(Constraint::Percentage(value));
    }
    constraints
}

fn layout_help_sections(
    sections: &[HelpSection],
    column_count: usize,
    column_height: u16,
    block_gap: usize,
) -> (Vec<Vec<HelpSection>>, bool) {
    let mut columns: Vec<Vec<HelpSection>> = vec![Vec::new(); column_count.max(1)];
    let mut heights = vec![0usize; column_count.max(1)];
    let max_height = column_height as usize;
    let mut col_idx = 0usize;
    let mut overflow = false;

    for section in sections {
        let section_height = help_section_height(section);
        let required = if heights[col_idx] == 0 {
            section_height
        } else {
            section_height + block_gap
        };
        if heights[col_idx] + required <= max_height {
            heights[col_idx] += required;
            columns[col_idx].push(section.clone());
            continue;
        }
        if col_idx + 1 < columns.len() {
            col_idx += 1;
            let required = if heights[col_idx] == 0 {
                section_height
            } else {
                section_height + block_gap
            };
            if heights[col_idx] + required <= max_height {
                heights[col_idx] += required;
                columns[col_idx].push(section.clone());
            } else {
                overflow = true;
            }
        } else {
            overflow = true;
        }
        if overflow {
            break;
        }
    }

    (columns, overflow)
}

fn render_help_blocks_column(
    f: &mut Frame,
    area: Rect,
    sections: &[HelpSection],
    tokens: &ThemeTokens,
    block_gap: usize,
) {
    let mut y = area.y;
    let max_y = area.y.saturating_add(area.height);

    for (idx, section) in sections.iter().enumerate() {
        if y >= max_y {
            break;
        }
        let remaining = max_y.saturating_sub(y);
        let block_height = help_section_height(section) as u16;
        if block_height > remaining {
            break;
        }
        let rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height: block_height,
        };
        render_help_block(f, rect, section, tokens);
        y = y.saturating_add(block_height);
        if idx + 1 < sections.len() && y < max_y {
            y = y.saturating_add(block_gap as u16);
        }
    }
}

fn render_help_block(f: &mut Frame, area: Rect, section: &HelpSection, tokens: &ThemeTokens) {
    let header_style = Style::default()
        .fg(tokens.ui_accent)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(tokens.ui_accent)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(tokens.ui_fg);
    let muted_style = Style::default().fg(tokens.ui_muted);

    let block = Block::default()
        .title(Span::styled(format!(" {} ", section.title), header_style))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let max_key_width = section
        .entries
        .iter()
        .map(|(_, keys)| keys.chars().count())
        .max()
        .unwrap_or(0);
    let max_width = inner.width.saturating_sub(6) as usize;
    let key_width = max_key_width.max(6).min(max_width.max(6));

    let mut lines: Vec<Line<'static>> = Vec::new();
    if section.show_header {
        let key_label = "Keys";
        let key_pad = key_width.saturating_sub(key_label.len());
        lines.push(Line::from(vec![
            Span::styled(key_label, muted_style),
            Span::raw(" ".repeat(key_pad + 2)),
            Span::styled("Action", muted_style),
        ]));
    }
    for (label, keys) in &section.entries {
        let key_len = keys.chars().count();
        let padding = key_width.saturating_sub(key_len);
        lines.push(Line::from(vec![
            Span::styled(keys.clone(), key_style),
            Span::raw(" ".repeat(padding + 2)),
            Span::styled(label.clone(), label_style),
        ]));
    }

    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true });
    f.render_widget(paragraph, inner);
}

fn render_visual_help_popup(f: &mut Frame, _app: &App, kind: VisualKind, tokens: &ThemeTokens) {
    let block = Block::default()
        .title(" Visual Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(70, 45, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let lines = visual_help_lines(kind, tokens);

    let inner_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .margin(2)
        .split(area);

    let muted_style = Style::default().fg(tokens.ui_muted);
    f.render_widget(Paragraph::new(lines), inner_area[0]);
    f.render_widget(
        Paragraph::new("Esc / ?: close").style(muted_style),
        inner_area[1],
    );
}

fn visual_help_lines(kind: VisualKind, tokens: &ThemeTokens) -> Vec<Line<'static>> {
    let label_style = Style::default().fg(tokens.ui_muted);
    let key_style = Style::default()
        .fg(tokens.ui_accent)
        .add_modifier(Modifier::BOLD);

    let kind_label = match kind {
        VisualKind::Char => "CHAR",
        VisualKind::Line => "LINE",
        VisualKind::Block => "BLOCK",
    };

    vec![
        Line::from(vec![
            Span::styled("VISUAL (", label_style),
            Span::styled(kind_label, key_style),
            Span::styled(")", label_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Motion", label_style),
            Span::raw("  "),
            Span::styled("h j k l", key_style),
            Span::raw("  "),
            Span::styled("w b e", key_style),
            Span::raw("  "),
            Span::styled("W B E", key_style),
        ]),
        Line::from(vec![
            Span::styled("Actions", label_style),
            Span::raw(" "),
            Span::styled("s", key_style),
            Span::styled(" change", label_style),
            Span::raw("  "),
            Span::styled("y", key_style),
            Span::styled(" yank", label_style),
            Span::raw("  "),
            Span::styled("d/x", key_style),
            Span::styled(" delete", label_style),
        ]),
        Line::from(vec![
            Span::styled("Mode", label_style),
            Span::raw("   "),
            Span::styled("Esc", key_style),
            Span::styled(" normal", label_style),
            Span::raw("  "),
            Span::styled("?", key_style),
            Span::styled(" help", label_style),
        ]),
    ]
}

pub fn render_exit_popup(f: &mut Frame, _app: &App) {
    let block = Block::default()
        .title(" Exit composer? ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let text_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .margin(2)
        .split(area);

    let body = Paragraph::new("Save changes before leaving?")
        .style(Style::default().add_modifier(Modifier::BOLD))
        .wrap(ratatui::widgets::Wrap { trim: true });

    let help_text = Paragraph::new("[Enter]/[y] Save & Exit    [d] Discard    [n]/[Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(body, text_area[0]);
    f.render_widget(help_text, text_area[1]);
}

pub fn render_delete_entry_popup(f: &mut Frame) {
    let block = Block::default()
        .title(" Delete this entry? ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::LightRed));
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let text_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .margin(2)
        .split(area);

    let body = Paragraph::new("Delete this entry? (y)es / (n)o")
        .style(Style::default().add_modifier(Modifier::BOLD))
        .wrap(ratatui::widgets::Wrap { trim: true });

    let help_text = Paragraph::new("Enter/y: delete  Esc/n: cancel")
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(body, text_area[0]);
    f.render_widget(help_text, text_area[1]);
}

pub fn render_pomodoro_popup(f: &mut Frame, app: &App) {
    let block = Block::default()
        .title(" Pomodoro (Task) ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::LightRed));
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let task = app
        .pomodoro_pending_task
        .as_ref()
        .map(|t| t.text.as_str())
        .unwrap_or("<no task selected>");

    let body = vec![
        Line::from(vec![
            Span::styled("Task: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                task.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Minutes: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.pomodoro_minutes_input.clone(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::raw("Enter: start  Esc: cancel  Backspace: edit")),
    ];

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .margin(2)
        .split(area)[0];
    f.render_widget(Paragraph::new(body), inner);
}

pub fn render_theme_switcher_popup(f: &mut Frame, app: &mut App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" Theme Presets ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(70, 40, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .margin(1)
        .split(area);

    let name_style = Style::default().fg(tokens.ui_fg);
    let desc_style = Style::default().fg(tokens.ui_muted);
    let items: Vec<ListItem> = ThemePreset::all()
        .iter()
        .map(|preset| {
            let line = Line::from(vec![
                Span::styled(preset.name(), name_style),
                Span::raw(" - "),
                Span::styled(preset.description(), desc_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let highlight_style = Style::default()
        .bg(tokens.ui_selection_bg)
        .add_modifier(Modifier::BOLD);
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(highlight_style);
    f.render_stateful_widget(list, popup_layout[0], &mut app.theme_list_state);

    let help = Paragraph::new("(Up/Down) Move  (Enter) Apply  (Esc) Cancel")
        .style(Style::default().fg(tokens.ui_muted));
    f.render_widget(help, popup_layout[1]);
}

pub fn render_editor_style_popup(f: &mut Frame, app: &mut App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" Editor Style ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .margin(1)
        .split(area);

    let name_style = Style::default().fg(tokens.ui_fg);
    let desc_style = Style::default().fg(tokens.ui_muted);
    let items: Vec<ListItem> = EditorStyle::all()
        .iter()
        .map(|style| {
            let line = Line::from(vec![
                Span::styled(style.name(), name_style),
                Span::raw(" - "),
                Span::styled(style.description(), desc_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let highlight_style = Style::default()
        .bg(tokens.ui_selection_bg)
        .add_modifier(Modifier::BOLD);
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(highlight_style);
    f.render_stateful_widget(list, popup_layout[0], &mut app.editor_style_list_state);

    let help = Paragraph::new("(Up/Down) Move  (Enter) Apply  (Esc) Cancel")
        .style(Style::default().fg(tokens.ui_muted));
    f.render_widget(help, popup_layout[1]);
}

fn fmt_keys(keys: &[String]) -> String {
    if keys.is_empty() {
        return "-".to_string();
    }
    keys.join(" / ")
}

#[cfg(test)]
mod tests {
    use super::visual_help_lines;
    use crate::config::Theme;
    use crate::models::VisualKind;
    use crate::ui::theme::ThemeTokens;

    fn line_to_string(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn visual_help_includes_motion_and_actions() {
        let tokens = ThemeTokens::from_theme(&Theme::default());
        let lines = visual_help_lines(VisualKind::Block, &tokens);
        let combined = lines
            .iter()
            .map(line_to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(combined.contains("VISUAL (BLOCK)"));
        assert!(combined.contains("h j k l"));
        assert!(combined.contains("w b e"));
        assert!(combined.contains("W B E"));
        assert!(combined.contains("y yank"));
        assert!(combined.contains("d/x delete"));
        assert!(combined.contains("Esc normal"));
    }
}
