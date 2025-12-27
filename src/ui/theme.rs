use crate::config::Theme;
use crate::ui::color_parser::parse_color;
use ratatui::style::Color;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ThemeTokens {
    pub ui_border_default: Color,
    pub ui_border_editing: Color,
    pub ui_border_search: Color,
    pub tasks_header: Color,
    pub ui_highlight: Color,
    pub tasks_done: Color,
    pub tasks_wip: Color,
    pub content_tag: Color,
    pub content_mood: Color,
    pub content_timestamp: Color,
    pub ui_fg: Color,
    pub ui_bg: Color,
    pub ui_muted: Color,
    pub ui_accent: Color,
    pub ui_selection_bg: Color,
    pub ui_cursorline_bg: Color,
    pub ui_toast_info: Color,
    pub ui_toast_success: Color,
    pub ui_toast_error: Color,
}

impl ThemeTokens {
    pub fn from_theme(theme: &Theme) -> Self {
        let ui_border_default = parse_color(&theme.border_default);
        let ui_border_editing = parse_color(&theme.border_editing);
        let ui_border_search = parse_color(&theme.border_search);
        let tasks_header = parse_color(&theme.border_todo_header);
        let ui_highlight = parse_color(&theme.text_highlight);
        let tasks_done = parse_color(&theme.todo_done);
        let tasks_wip = parse_color(&theme.todo_wip);
        let content_tag = parse_color(&theme.tag);
        let content_mood = parse_color(&theme.mood);
        let content_timestamp = parse_color(&theme.timestamp);

        let ui_fg = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.fg.as_deref())
                .unwrap_or("Reset"),
        );
        let ui_bg = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.bg.as_deref())
                .unwrap_or("Reset"),
        );
        let ui_muted = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.muted.as_deref())
                .unwrap_or(theme.border_default.as_str()),
        );
        let ui_accent = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.accent.as_deref())
                .unwrap_or(theme.border_editing.as_str()),
        );
        let ui_selection_bg = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.selection_bg.as_deref())
                .unwrap_or(theme.text_highlight.as_str()),
        );
        let ui_cursorline_bg = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.cursorline_bg.as_deref())
                .unwrap_or(theme.text_highlight.as_str()),
        );

        let ui_toast_info = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.toast.as_ref())
                .and_then(|toast| toast.info.as_deref())
                .unwrap_or(theme.border_search.as_str()),
        );
        let ui_toast_success = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.toast.as_ref())
                .and_then(|toast| toast.success.as_deref())
                .unwrap_or(theme.todo_done.as_str()),
        );
        let ui_toast_error = parse_color(
            theme
                .ui
                .as_ref()
                .and_then(|ui| ui.toast.as_ref())
                .and_then(|toast| toast.error.as_deref())
                .unwrap_or(theme.todo_wip.as_str()),
        );

        Self {
            ui_border_default,
            ui_border_editing,
            ui_border_search,
            tasks_header,
            ui_highlight,
            tasks_done,
            tasks_wip,
            content_tag,
            content_mood,
            content_timestamp,
            ui_fg,
            ui_bg,
            ui_muted,
            ui_accent,
            ui_selection_bg,
            ui_cursorline_bg,
            ui_toast_info,
            ui_toast_success,
            ui_toast_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ThemeTokens;
    use crate::config::{Theme, ThemeToastOverrides, ThemeUiOverrides};
    use ratatui::style::Color;

    #[test]
    fn maps_legacy_keys_to_tokens() {
        let theme = Theme {
            border_default: "Red".to_string(),
            border_editing: "Green".to_string(),
            border_search: "Blue".to_string(),
            border_todo_header: "Yellow".to_string(),
            text_highlight: "1,2,3".to_string(),
            todo_done: "LightGreen".to_string(),
            todo_wip: "LightRed".to_string(),
            tag: "Cyan".to_string(),
            mood: "Magenta".to_string(),
            timestamp: "LightCyan".to_string(),
            ui: None,
        };

        let tokens = ThemeTokens::from_theme(&theme);
        assert_eq!(tokens.ui_border_default, Color::Red);
        assert_eq!(tokens.ui_border_editing, Color::Green);
        assert_eq!(tokens.ui_border_search, Color::Blue);
        assert_eq!(tokens.tasks_header, Color::Yellow);
        assert_eq!(tokens.ui_highlight, Color::Rgb(1, 2, 3));
        assert_eq!(tokens.tasks_done, Color::LightGreen);
        assert_eq!(tokens.tasks_wip, Color::LightRed);
        assert_eq!(tokens.content_tag, Color::Cyan);
        assert_eq!(tokens.content_mood, Color::Magenta);
        assert_eq!(tokens.content_timestamp, Color::LightCyan);
    }

    #[test]
    fn honors_ui_overrides() {
        let theme = Theme {
            ui: Some(ThemeUiOverrides {
                fg: Some("White".to_string()),
                bg: Some("Black".to_string()),
                muted: Some("DarkGray".to_string()),
                accent: Some("Cyan".to_string()),
                selection_bg: Some("10,20,30".to_string()),
                cursorline_bg: Some("20,30,40".to_string()),
                toast: Some(ThemeToastOverrides {
                    info: Some("Magenta".to_string()),
                    success: Some("LightGreen".to_string()),
                    error: Some("LightRed".to_string()),
                }),
            }),
            ..Default::default()
        };

        let tokens = ThemeTokens::from_theme(&theme);
        assert_eq!(tokens.ui_fg, Color::White);
        assert_eq!(tokens.ui_bg, Color::Black);
        assert_eq!(tokens.ui_muted, Color::DarkGray);
        assert_eq!(tokens.ui_accent, Color::Cyan);
        assert_eq!(tokens.ui_selection_bg, Color::Rgb(10, 20, 30));
        assert_eq!(tokens.ui_cursorline_bg, Color::Rgb(20, 30, 40));
        assert_eq!(tokens.ui_toast_info, Color::Magenta);
        assert_eq!(tokens.ui_toast_success, Color::LightGreen);
        assert_eq!(tokens.ui_toast_error, Color::LightRed);
    }
}
