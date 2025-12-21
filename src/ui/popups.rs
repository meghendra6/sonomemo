use super::components::centered_rect;
use crate::app::App;
use crate::models::Mood;
use crate::ui::color_parser::parse_color;
use crate::ui::theme::ThemeTokens;
use chrono::Local;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
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
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            "Date        Logs  🍅   Activity                    Pomodoros",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
    ])));
    items.push(ListItem::new(Line::from("")));

    for i in 0..14 {
        let date = today - chrono::Duration::days(i);
        let date_str = date.format("%Y-%m-%d").to_string();
        let (line_count, tomato_count) = app.activity_data.get(&date_str).cloned().unwrap_or((0, 0));

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
            Span::styled(format!("{:3}", line_count), Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(format!("{:2}", tomato_count), Style::default().fg(Color::Red)),
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
                Span::styled(tag.clone(), Style::default().fg(tag_color).add_modifier(Modifier::BOLD)),
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
                ("Log dir", fmt_keys(&kb.global.log_dir)),
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
                ("Pomodoro", fmt_keys(&kb.tasks.start_pomodoro)),
                ("Edit", fmt_keys(&kb.tasks.edit)),
            ],
        ),
        (
            "Composer",
            vec![
                ("Save", fmt_keys(&kb.composer.submit)),
                ("New line", fmt_keys(&kb.composer.newline)),
                ("Indent", fmt_keys(&kb.composer.indent)),
                ("Outdent", fmt_keys(&kb.composer.outdent)),
                ("Clear", fmt_keys(&kb.composer.clear)),
                ("Back", fmt_keys(&kb.composer.cancel)),
            ],
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

fn fmt_keys(keys: &[String]) -> String {
    if keys.is_empty() {
        return "-".to_string();
    }
    keys.join(" / ")
}
