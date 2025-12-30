use super::components::{centered_rect, parse_markdown_spans, wrap_markdown_line};
use crate::app::App;
use crate::config::{EditorStyle, ThemePreset};
use crate::models::{DatePickerField, EditorMode, InputMode, Mood, VisualKind};
use crate::ui::color_parser::parse_color;
use crate::ui::theme::ThemeTokens;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

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

    for raw_line in entry.content.lines() {
        let wrapped = wrap_markdown_line(raw_line, width);
        for line in wrapped {
            lines.push(Line::from(parse_markdown_spans(
                &line,
                &app.config.theme,
                false,
                None,
                Style::default(),
            )));
        }
    }

    let max_scroll = lines
        .len()
        .saturating_sub(content_area.height as usize);
    let scroll = app.memo_preview_scroll.min(max_scroll);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    f.render_widget(paragraph, content_area);

    let footer = Paragraph::new("Esc close · E edit · J/K scroll")
        .style(Style::default().fg(tokens.ui_muted));
    f.render_widget(footer, footer_area);
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
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
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

fn render_date_picker_fields(f: &mut Frame, app: &App, area: ratatui::layout::Rect, tokens: &ThemeTokens) {
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
            Span::styled(format!("{:<10}", label), Style::default().fg(tokens.ui_accent)),
            Span::styled(value, Style::default().fg(tokens.ui_fg)),
        ]);
        items.push(ListItem::new(line));
    }

    let highlight = Style::default()
        .bg(tokens.ui_selection_bg)
        .add_modifier(Modifier::BOLD);
    let list = List::new(items).highlight_style(highlight).highlight_symbol(" ");
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

    let month_start = NaiveDate::from_ymd_opt(selected.year(), selected.month(), 1)
        .unwrap_or(selected);
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

            let date = NaiveDate::from_ymd_opt(selected.year(), selected.month(), day)
                .unwrap_or(selected);
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
        spans.push(Span::styled(format!("{:>4}", format_duration(preset)), style));
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

    let kb = &app.config.keybindings;

    let rows = vec![
        (
            "Global",
            vec![
                ("Help", fmt_keys(&kb.global.help)),
                ("Focus move", "Ctrl+H/J/K/L".to_string()),
                ("Compose", fmt_keys(&kb.global.focus_composer)),
                ("Search", fmt_keys(&kb.global.search)),
                ("Tags", fmt_keys(&kb.global.tags)),
                ("Pomodoro", fmt_keys(&kb.global.pomodoro)),
                ("Activity", fmt_keys(&kb.global.activity)),
                ("Focus agenda", fmt_keys(&kb.global.agenda)),
                ("Log dir", fmt_keys(&kb.global.log_dir)),
                ("Theme presets", fmt_keys(&kb.global.theme_switcher)),
                ("Editor style", fmt_keys(&kb.global.editor_style_switcher)),
                ("Quit", fmt_keys(&kb.global.quit)),
            ],
        ),
        (
            "Timeline",
            vec![
                ("Up", fmt_keys(&kb.timeline.up)),
                ("Down", fmt_keys(&kb.timeline.down)),
                ("Page up", fmt_keys(&kb.timeline.page_up)),
                ("Page down", fmt_keys(&kb.timeline.page_down)),
                ("Top", fmt_keys(&kb.timeline.top)),
                ("Bottom", fmt_keys(&kb.timeline.bottom)),
                ("Fold toggle", fmt_keys(&kb.timeline.fold_toggle)),
                ("Fold cycle", fmt_keys(&kb.timeline.fold_cycle)),
                ("Edit", fmt_keys(&kb.timeline.edit)),
                ("Toggle checkbox", fmt_keys(&kb.timeline.toggle_todo)),
            ],
        ),
        (
            "Agenda",
            vec![
                ("Up", fmt_keys(&kb.agenda.up)),
                ("Down", fmt_keys(&kb.agenda.down)),
                ("Open memo", fmt_keys(&kb.agenda.open)),
                ("Toggle task", fmt_keys(&kb.agenda.toggle)),
                ("Filter cycle", fmt_keys(&kb.agenda.filter)),
                ("Prev day", fmt_keys(&kb.agenda.prev_day)),
                ("Next day", fmt_keys(&kb.agenda.next_day)),
                ("Prev week", fmt_keys(&kb.agenda.prev_week)),
                ("Next week", fmt_keys(&kb.agenda.next_week)),
                ("Today", fmt_keys(&kb.agenda.today)),
                ("Unscheduled", fmt_keys(&kb.agenda.toggle_unscheduled)),
            ],
        ),
        (
            "Tasks",
            vec![
                ("Up", fmt_keys(&kb.tasks.up)),
                ("Down", fmt_keys(&kb.tasks.down)),
                ("Toggle", fmt_keys(&kb.tasks.toggle)),
                ("Priority cycle", fmt_keys(&kb.tasks.priority_cycle)),
                ("Pomodoro", fmt_keys(&kb.tasks.start_pomodoro)),
                ("Edit", fmt_keys(&kb.tasks.edit)),
                ("Filter toggle", fmt_keys(&kb.tasks.filter_toggle)),
                ("Filter open", fmt_keys(&kb.tasks.filter_open)),
                ("Filter done", fmt_keys(&kb.tasks.filter_done)),
                ("Filter all", fmt_keys(&kb.tasks.filter_all)),
            ],
        ),
        (
            if app.is_vim_mode() {
                "Composer (Vim mode)"
            } else {
                "Composer (Simple mode)"
            },
            {
                let mut composer_entries = vec![
                    ("Save", fmt_keys(&kb.composer.submit)),
                    ("New line", fmt_keys(&kb.composer.newline)),
                    ("Toggle task", fmt_keys(&kb.composer.task_toggle)),
                    ("Priority cycle", fmt_keys(&kb.composer.priority_cycle)),
                    ("Date picker", fmt_keys(&kb.composer.date_picker)),
                    ("Indent", fmt_keys(&kb.composer.indent)),
                    ("Outdent", fmt_keys(&kb.composer.outdent)),
                    ("Clear", fmt_keys(&kb.composer.clear)),
                    ("Back", fmt_keys(&kb.composer.cancel)),
                ];
                if app.is_vim_mode() {
                    composer_entries.push(("Vim motions", "hjkl, w, b, e, ...".to_string()));
                    composer_entries.push(("Visual mode", "v, V, Ctrl+v".to_string()));
                }
                composer_entries
            },
        ),
        (
            "Search",
            vec![
                ("Apply", fmt_keys(&kb.search.submit)),
                ("Clear", fmt_keys(&kb.search.clear)),
                ("Cancel", fmt_keys(&kb.search.cancel)),
            ],
        ),
    ];

    let header_style = Style::default()
        .fg(tokens.ui_accent)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(tokens.ui_accent)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(tokens.ui_fg);
    let muted_style = Style::default().fg(tokens.ui_muted);

    let mut items: Vec<ListItem> = Vec::new();
    for (section, entries) in rows {
        items.push(ListItem::new(Line::from(vec![
            Span::styled("•", header_style),
            Span::raw(" "),
            Span::styled(section, header_style),
        ])));
        for (label, keys) in entries {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{:<18}", keys), key_style),
                Span::styled(label, label_style),
            ])));
        }
        items.push(ListItem::new(Line::from("")));
    }

    let inner_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .margin(2)
        .split(area);

    f.render_widget(List::new(items), inner_area[0]);
    f.render_widget(
        Paragraph::new("Esc / ?: close").style(muted_style),
        inner_area[1],
    );
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

    let help_text = Paragraph::new("[y] Save & exit    [d] Discard    [n]/[Esc] Cancel")
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
