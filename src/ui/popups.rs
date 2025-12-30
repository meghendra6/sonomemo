use super::components::{centered_rect, parse_markdown_spans, wrap_markdown_line};
use crate::app::App;
use crate::config::{EditorStyle, ThemePreset};
use crate::models::{EditorMode, InputMode, Mood, VisualKind};
use crate::ui::color_parser::parse_color;
use crate::ui::theme::ThemeTokens;
use chrono::Local;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
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

pub fn render_agenda_popup(f: &mut Frame, app: &App) {
    let tokens = ThemeTokens::from_theme(&app.config.theme);
    let block = Block::default()
        .title(" 📅 Agenda (Last 7 Days) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(tokens.ui_border_default));
    let area = centered_rect(80, 70, f.area());
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let list_width = inner.width.saturating_sub(2).max(1) as usize;

    let mut items: Vec<ListItem> = Vec::new();
    let mut last_date: Option<chrono::NaiveDate> = None;
    let mut ui_selected_index: Option<usize> = None;
    let mut ui_index = 0usize;
    let selected = app.agenda_state.selected();

    for (idx, item) in app.agenda_items.iter().enumerate() {
        if last_date != Some(item.date) {
            items.push(ListItem::new(Line::from(Span::styled(
                item.date.format("%Y-%m-%d").to_string(),
                Style::default()
                    .fg(tokens.ui_accent)
                    .add_modifier(Modifier::BOLD),
            ))));
            last_date = Some(item.date);
            ui_index += 1;
        }

        if selected == Some(idx) {
            ui_selected_index = Some(ui_index);
        }

        let mut line = String::new();
        line.push_str("  ");
        line.push_str(&"  ".repeat(item.indent));
        if item.is_done {
            line.push_str("[x] ");
        } else {
            line.push_str("[ ] ");
        }
        line.push_str(&item.text);

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
        ui_index += 1;
    }

    let highlight_style = Style::default()
        .bg(tokens.ui_selection_bg)
        .add_modifier(Modifier::BOLD);
    let list = List::new(items)
        .highlight_symbol("")
        .highlight_style(highlight_style);

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(ui_selected_index);
    f.render_stateful_widget(list, inner, &mut list_state);
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
                ("Focus timeline", fmt_keys(&kb.global.focus_timeline)),
                ("Focus tasks", fmt_keys(&kb.global.focus_tasks)),
                ("Compose", fmt_keys(&kb.global.focus_composer)),
                ("Search", fmt_keys(&kb.global.search)),
                ("Tags", fmt_keys(&kb.global.tags)),
                ("Pomodoro", fmt_keys(&kb.global.pomodoro)),
                ("Activity", fmt_keys(&kb.global.activity)),
                ("Agenda", fmt_keys(&kb.global.agenda)),
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

pub fn render_discard_popup(f: &mut Frame, _app: &App) {
    let block = Block::default()
        .title(" Discard changes? ")
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

    let body = Paragraph::new("You have unsaved text.\nDiscard it and go back?")
        .style(Style::default().add_modifier(Modifier::BOLD))
        .wrap(ratatui::widgets::Wrap { trim: true });

    let help_text = Paragraph::new("[y] Discard    [n] Keep editing")
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
