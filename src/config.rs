use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn key_match(key: &KeyEvent, bindings: &[String]) -> bool {
    bindings.iter().any(|binding| is_match(key, binding))
}

fn is_match(key: &KeyEvent, binding: &str) -> bool {
    let binding = binding.to_lowercase();
    let parts: Vec<&str> = binding.split('+').collect();

    let mut target_modifiers = KeyModifiers::NONE;
    let mut target_code = KeyCode::Null;

    for part in parts {
        match part {
            "ctrl" => target_modifiers.insert(KeyModifiers::CONTROL),
            "opt" | "alt" => target_modifiers.insert(KeyModifiers::ALT),
            "shift" => target_modifiers.insert(KeyModifiers::SHIFT),
            "enter" => target_code = KeyCode::Enter,
            "esc" => target_code = KeyCode::Esc,
            "backspace" => target_code = KeyCode::Backspace,
            "tab" => target_code = KeyCode::Tab,
            "backtab" => target_code = KeyCode::BackTab,
            "space" => target_code = KeyCode::Char(' '),
            "up" => target_code = KeyCode::Up,
            "down" => target_code = KeyCode::Down,
            "left" => target_code = KeyCode::Left,
            "right" => target_code = KeyCode::Right,
            "home" => target_code = KeyCode::Home,
            "end" => target_code = KeyCode::End,
            "pageup" => target_code = KeyCode::PageUp,
            "pagedown" => target_code = KeyCode::PageDown,
            "delete" => target_code = KeyCode::Delete,
            "insert" => target_code = KeyCode::Insert,
            c if c.chars().count() == 1 => {
                if let Some(ch) = c.chars().next() {
                    target_code = KeyCode::Char(ch);
                }
            }
            _ => {}
        }
    }

    // KeyCode match (case-insensitive for Char).
    let code_matches = if key.code == target_code {
        true
    } else if let (KeyCode::Char(c), KeyCode::Char(tc)) = (key.code, target_code) {
        c.to_lowercase().next() == Some(tc)
    } else {
        false
    };
    if !code_matches {
        return false;
    }

    // Modifier match:
    // - Enter must match modifiers exactly so `enter` and `shift+enter` can coexist.
    // - For other keys, ignore Shift unless explicitly requested (helps BackTab and char keys like '?').
    // - Alphabetic keys should not match when Shift is held unless explicitly bound (avoid "shift+t" matching "t").
    if let KeyCode::Char(tc) = target_code
        && tc.is_ascii_alphabetic()
        && !target_modifiers.contains(KeyModifiers::SHIFT)
        && key.modifiers.contains(KeyModifiers::SHIFT)
    {
        return false;
    }
    if target_code == KeyCode::Enter {
        return key.modifiers == target_modifiers;
    }

    let mut key_mods = key.modifiers;
    let mut target_mods = target_modifiers;

    if !target_mods.contains(KeyModifiers::SHIFT) {
        key_mods.remove(KeyModifiers::SHIFT);
    }

    if !target_mods.contains(KeyModifiers::SHIFT) {
        target_mods.remove(KeyModifiers::SHIFT);
    }

    key_mods.contains(target_mods)
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "meghendra", "memolog")
}

fn default_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("MEMOLOG_DATA_DIR") {
        return PathBuf::from(path);
    }
    if let Some(dirs) = project_dirs() {
        return dirs.data_dir().to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".memolog")
}

fn default_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("MEMOLOG_LOG_DIR") {
        return PathBuf::from(path);
    }
    default_data_dir().join("logs")
}

fn theme_table_present(content: &str) -> bool {
    toml::from_str::<toml::Value>(content)
        .ok()
        .and_then(|value| value.get("theme").cloned())
        .is_some()
}

pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MEMOLOG_CONFIG") {
        return PathBuf::from(path);
    }
    if let Some(dirs) = project_dirs() {
        return dirs.config_dir().join("config.toml");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".memolog-config.toml")
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub keybindings: KeyBindings,
    pub theme: Theme,
    pub ui: UiConfig,
    pub editor: EditorConfig,
    pub data: DataConfig,
    pub pomodoro: PomodoroConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct DataConfig {
    pub log_path: PathBuf,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            log_path: default_log_dir(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct EditorConfig {
    pub column_width: u16,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self { column_width: 88 }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct UiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_style: Option<String>,
    pub line_numbers: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme_preset: None,
            editor_style: None,
            line_numbers: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct KeyBindings {
    pub global: GlobalBindings,
    pub timeline: TimelineBindings,
    pub tasks: TasksBindings,
    pub composer: ComposerBindings,
    pub search: SearchBindings,
    pub popup: PopupBindings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct GlobalBindings {
    pub quit: Vec<String>,
    pub help: Vec<String>,
    pub focus_timeline: Vec<String>,
    pub focus_tasks: Vec<String>,
    pub focus_composer: Vec<String>,
    pub focus_next: Vec<String>,
    pub focus_prev: Vec<String>,
    pub search: Vec<String>,
    pub tags: Vec<String>,
    pub activity: Vec<String>,
    pub agenda: Vec<String>,
    pub log_dir: Vec<String>,
    pub pomodoro: Vec<String>,
    pub theme_switcher: Vec<String>,
    pub editor_style_switcher: Vec<String>,
}

impl Default for GlobalBindings {
    fn default() -> Self {
        Self {
            quit: vec!["ctrl+q".to_string(), "q".to_string()],
            help: vec!["?".to_string()],
            focus_timeline: vec!["h".to_string()],
            focus_tasks: vec!["l".to_string()],
            focus_composer: vec!["i".to_string()],
            focus_next: vec!["tab".to_string()],
            focus_prev: vec!["backtab".to_string()],
            search: vec!["/".to_string()],
            tags: vec!["t".to_string()],
            activity: vec!["g".to_string()],
            agenda: vec!["shift+a".to_string()],
            log_dir: vec!["o".to_string()],
            pomodoro: vec!["p".to_string()],
            theme_switcher: vec!["shift+t".to_string()],
            editor_style_switcher: vec!["shift+v".to_string()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct TimelineBindings {
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub page_up: Vec<String>,
    pub page_down: Vec<String>,
    pub top: Vec<String>,
    pub bottom: Vec<String>,
    pub fold_toggle: Vec<String>,
    pub fold_cycle: Vec<String>,
    pub toggle_todo: Vec<String>,
    pub open: Vec<String>,
    pub edit: Vec<String>,
    pub delete_entry: Vec<String>,
}

impl Default for TimelineBindings {
    fn default() -> Self {
        Self {
            up: vec!["k".to_string(), "up".to_string()],
            down: vec!["j".to_string(), "down".to_string()],
            page_up: vec!["ctrl+u".to_string(), "pageup".to_string()],
            page_down: vec!["ctrl+d".to_string(), "pagedown".to_string()],
            top: vec!["home".to_string()],
            bottom: vec!["end".to_string()],
            fold_toggle: vec!["tab".to_string()],
            fold_cycle: vec!["backtab".to_string()],
            toggle_todo: vec!["enter".to_string(), "space".to_string()],
            open: vec!["enter".to_string()],
            edit: vec!["e".to_string()],
            delete_entry: vec!["x".to_string()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct TasksBindings {
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub toggle: Vec<String>,
    pub start_pomodoro: Vec<String>,
    pub open: Vec<String>,
    pub edit: Vec<String>,
    pub priority_cycle: Vec<String>,
    pub filter_toggle: Vec<String>,
    pub filter_open: Vec<String>,
    pub filter_done: Vec<String>,
    pub filter_all: Vec<String>,
}

impl Default for TasksBindings {
    fn default() -> Self {
        Self {
            up: vec!["k".to_string(), "up".to_string()],
            down: vec!["j".to_string(), "down".to_string()],
            toggle: vec!["space".to_string(), "enter".to_string()],
            start_pomodoro: vec!["p".to_string()],
            open: vec!["enter".to_string()],
            edit: vec!["e".to_string()],
            priority_cycle: vec!["shift+p".to_string()],
            filter_toggle: vec!["f".to_string()],
            filter_open: vec!["1".to_string()],
            filter_done: vec!["2".to_string()],
            filter_all: vec!["3".to_string()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ComposerBindings {
    pub submit: Vec<String>,
    pub newline: Vec<String>,
    pub cancel: Vec<String>,
    pub clear: Vec<String>,
    pub indent: Vec<String>,
    pub outdent: Vec<String>,
    pub task_toggle: Vec<String>,
    pub priority_cycle: Vec<String>,
    pub date_picker: Vec<String>,
}

impl Default for ComposerBindings {
    fn default() -> Self {
        Self {
            cancel: vec!["esc".to_string()],
            newline: vec!["enter".to_string()],
            submit: vec!["shift+enter".to_string()],
            clear: Vec::new(),
            indent: vec!["tab".to_string()],
            outdent: vec!["backtab".to_string()],
            task_toggle: vec!["ctrl+t".to_string()],
            priority_cycle: vec!["ctrl+p".to_string()],
            date_picker: vec!["ctrl+;".to_string()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SearchBindings {
    pub submit: Vec<String>,
    pub cancel: Vec<String>,
    pub clear: Vec<String>,
}

impl Default for SearchBindings {
    fn default() -> Self {
        Self {
            submit: vec!["enter".to_string()],
            cancel: vec!["esc".to_string()],
            clear: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PopupBindings {
    pub confirm: Vec<String>,
    pub cancel: Vec<String>,
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub agenda_toggle_view: Vec<String>,
    pub agenda_prev_day: Vec<String>,
    pub agenda_next_day: Vec<String>,
    pub agenda_prev_week: Vec<String>,
    pub agenda_next_week: Vec<String>,
    pub agenda_filter: Vec<String>,
}

impl Default for PopupBindings {
    fn default() -> Self {
        Self {
            confirm: vec!["enter".to_string(), "y".to_string()],
            cancel: vec!["esc".to_string(), "n".to_string()],
            up: vec!["k".to_string(), "up".to_string()],
            down: vec!["j".to_string(), "down".to_string()],
            agenda_toggle_view: vec!["t".to_string()],
            agenda_prev_day: vec!["h".to_string(), "left".to_string()],
            agenda_next_day: vec!["l".to_string(), "right".to_string()],
            agenda_prev_week: vec!["pageup".to_string()],
            agenda_next_week: vec!["pagedown".to_string()],
            agenda_filter: vec!["f".to_string()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Theme {
    pub border_default: String,
    pub border_editing: String,
    pub border_search: String,
    pub border_todo_header: String,
    pub text_highlight: String,
    pub todo_done: String,
    pub todo_wip: String,
    pub tag: String,
    pub mood: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<ThemeUiOverrides>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_default: "Reset".to_string(),
            border_editing: "Green".to_string(),
            border_search: "Cyan".to_string(),
            border_todo_header: "Yellow".to_string(),
            text_highlight: "50,50,50".to_string(),
            todo_done: "Green".to_string(),
            todo_wip: "Red".to_string(),
            tag: "Yellow".to_string(),
            mood: "Magenta".to_string(),
            timestamp: "Blue".to_string(),
            ui: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ThemeUiOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursorline_bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toast: Option<ThemeToastOverrides>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ThemeToastOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    DraculaDark,
    SolarizedDark,
    SolarizedLight,
    NordCalm,
    MonoContrast,
}

impl ThemePreset {
    pub fn default() -> Self {
        ThemePreset::DraculaDark
    }

    pub fn all() -> &'static [ThemePreset] {
        &[
            ThemePreset::DraculaDark,
            ThemePreset::SolarizedDark,
            ThemePreset::SolarizedLight,
            ThemePreset::NordCalm,
            ThemePreset::MonoContrast,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemePreset::DraculaDark => "Dracula Dark",
            ThemePreset::SolarizedDark => "Solarized Dark",
            ThemePreset::SolarizedLight => "Solarized Light",
            ThemePreset::NordCalm => "Nord Calm",
            ThemePreset::MonoContrast => "Mono Contrast",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ThemePreset::DraculaDark => "High-contrast dark with vivid accents.",
            ThemePreset::SolarizedDark => "Low-contrast dark for long sessions.",
            ThemePreset::SolarizedLight => "Soft light theme for bright rooms.",
            ThemePreset::NordCalm => "Cool, calm tones with muted contrast.",
            ThemePreset::MonoContrast => "Minimal colors with a single accent.",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ThemePreset::all()
            .iter()
            .copied()
            .find(|preset| preset.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorStyle {
    Vim,
    Simple,
}

impl EditorStyle {
    pub fn default() -> Self {
        EditorStyle::Vim
    }

    pub fn all() -> &'static [EditorStyle] {
        &[EditorStyle::Vim, EditorStyle::Simple]
    }

    pub fn name(self) -> &'static str {
        match self {
            EditorStyle::Vim => "Vim",
            EditorStyle::Simple => "Simple",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            EditorStyle::Vim => "Full Vim keybindings with modal editing.",
            EditorStyle::Simple => "Simple editing without Vim modes.",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        EditorStyle::all()
            .iter()
            .copied()
            .find(|style| style.name().eq_ignore_ascii_case(name.trim()))
    }
}

impl Theme {
    pub fn preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::DraculaDark => Theme {
                border_default: "80,82,96".to_string(),
                border_editing: "189,147,249".to_string(),
                border_search: "139,233,253".to_string(),
                border_todo_header: "241,250,140".to_string(),
                text_highlight: "68,71,90".to_string(),
                todo_done: "80,250,123".to_string(),
                todo_wip: "255,85,85".to_string(),
                tag: "255,184,108".to_string(),
                mood: "255,121,198".to_string(),
                timestamp: "98,114,164".to_string(),
                ui: Some(ThemeUiOverrides {
                    fg: Some("248,248,242".to_string()),
                    bg: Some("40,42,54".to_string()),
                    muted: Some("98,114,164".to_string()),
                    accent: Some("189,147,249".to_string()),
                    selection_bg: Some("68,71,90".to_string()),
                    cursorline_bg: Some("68,71,90".to_string()),
                    toast: Some(ThemeToastOverrides {
                        info: Some("139,233,253".to_string()),
                        success: Some("80,250,123".to_string()),
                        error: Some("255,85,85".to_string()),
                    }),
                }),
            },
            ThemePreset::SolarizedDark => Theme {
                border_default: "88,110,117".to_string(),
                border_editing: "38,139,210".to_string(),
                border_search: "42,161,152".to_string(),
                border_todo_header: "181,137,0".to_string(),
                text_highlight: "7,54,66".to_string(),
                todo_done: "133,153,0".to_string(),
                todo_wip: "220,50,47".to_string(),
                tag: "181,137,0".to_string(),
                mood: "211,54,130".to_string(),
                timestamp: "38,139,210".to_string(),
                ui: Some(ThemeUiOverrides {
                    fg: Some("131,148,150".to_string()),
                    bg: Some("0,43,54".to_string()),
                    muted: Some("88,110,117".to_string()),
                    accent: Some("38,139,210".to_string()),
                    selection_bg: Some("7,54,66".to_string()),
                    cursorline_bg: Some("7,54,66".to_string()),
                    toast: Some(ThemeToastOverrides {
                        info: Some("42,161,152".to_string()),
                        success: Some("133,153,0".to_string()),
                        error: Some("220,50,47".to_string()),
                    }),
                }),
            },
            ThemePreset::SolarizedLight => Theme {
                border_default: "147,161,161".to_string(),
                border_editing: "38,139,210".to_string(),
                border_search: "42,161,152".to_string(),
                border_todo_header: "181,137,0".to_string(),
                text_highlight: "238,232,213".to_string(),
                todo_done: "133,153,0".to_string(),
                todo_wip: "220,50,47".to_string(),
                tag: "38,139,210".to_string(),
                mood: "211,54,130".to_string(),
                timestamp: "147,161,161".to_string(),
                ui: Some(ThemeUiOverrides {
                    fg: Some("101,123,131".to_string()),
                    bg: Some("253,246,227".to_string()),
                    muted: Some("147,161,161".to_string()),
                    accent: Some("38,139,210".to_string()),
                    selection_bg: Some("238,232,213".to_string()),
                    cursorline_bg: Some("238,232,213".to_string()),
                    toast: Some(ThemeToastOverrides {
                        info: Some("42,161,152".to_string()),
                        success: Some("133,153,0".to_string()),
                        error: Some("220,50,47".to_string()),
                    }),
                }),
            },
            ThemePreset::NordCalm => Theme {
                border_default: "76,86,106".to_string(),
                border_editing: "136,192,208".to_string(),
                border_search: "143,188,187".to_string(),
                border_todo_header: "129,161,193".to_string(),
                text_highlight: "59,66,82".to_string(),
                todo_done: "163,190,140".to_string(),
                todo_wip: "191,97,106".to_string(),
                tag: "235,203,139".to_string(),
                mood: "180,142,173".to_string(),
                timestamp: "94,129,172".to_string(),
                ui: Some(ThemeUiOverrides {
                    fg: Some("216,222,233".to_string()),
                    bg: Some("46,52,64".to_string()),
                    muted: Some("94,129,172".to_string()),
                    accent: Some("136,192,208".to_string()),
                    selection_bg: Some("59,66,82".to_string()),
                    cursorline_bg: Some("59,66,82".to_string()),
                    toast: Some(ThemeToastOverrides {
                        info: Some("143,188,187".to_string()),
                        success: Some("163,190,140".to_string()),
                        error: Some("191,97,106".to_string()),
                    }),
                }),
            },
            ThemePreset::MonoContrast => Theme {
                border_default: "64,64,64".to_string(),
                border_editing: "224,192,64".to_string(),
                border_search: "128,128,128".to_string(),
                border_todo_header: "224,192,64".to_string(),
                text_highlight: "42,42,42".to_string(),
                todo_done: "200,200,200".to_string(),
                todo_wip: "220,80,80".to_string(),
                tag: "200,200,200".to_string(),
                mood: "200,200,200".to_string(),
                timestamp: "160,160,160".to_string(),
                ui: Some(ThemeUiOverrides {
                    fg: Some("240,240,240".to_string()),
                    bg: Some("16,16,16".to_string()),
                    muted: Some("128,128,128".to_string()),
                    accent: Some("224,192,64".to_string()),
                    selection_bg: Some("42,42,42".to_string()),
                    cursorline_bg: Some("42,42,42".to_string()),
                    toast: Some(ThemeToastOverrides {
                        info: Some("224,192,64".to_string()),
                        success: Some("200,200,200".to_string()),
                        error: Some("220,80,80".to_string()),
                    }),
                }),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PomodoroConfig {
    pub work_minutes: u64,
    pub short_break_minutes: u64,
    pub long_break_minutes: u64,
    pub long_break_every: u64,
    pub alert_seconds: u64,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            work_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            long_break_every: 4,
            alert_seconds: 5,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = config_path();

        let mut config = if let Ok(content) = fs::read_to_string(&config_path) {
            let has_theme = theme_table_present(&content);
            match toml::from_str::<Config>(&content) {
                Ok(mut config) => {
                    if !has_theme {
                        let preset = config
                            .ui
                            .theme_preset
                            .as_deref()
                            .and_then(ThemePreset::from_name)
                            .unwrap_or_else(ThemePreset::default);
                        config.theme = Theme::preset(preset);
                    }
                    config
                }
                Err(e) => {
                    eprintln!("Failed to parse config.toml ({config_path:?}), using defaults: {e}");
                    Config::default()
                }
            }
        } else {
            let mut config = Config::default();
            let preset = config
                .ui
                .theme_preset
                .as_deref()
                .and_then(ThemePreset::from_name)
                .unwrap_or_else(ThemePreset::default);
            config.theme = Theme::preset(preset);
            config
        };

        let mut changed = config.normalize_paths();
        changed |= config.normalize_keybindings();

        if changed || !config_path.exists() {
            let _ = config.save_to_path(&config_path);
        }

        config
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).unwrap_or_default();
        fs::write(path, content)
    }

    fn normalize_paths(&mut self) -> bool {
        let mut changed = false;

        if self.data.log_path.as_os_str().is_empty() {
            self.data.log_path = default_log_dir();
            changed = true;
        }

        if self.data.log_path.is_relative() {
            self.data.log_path = default_data_dir().join(&self.data.log_path);
            changed = true;
        }

        changed
    }

    fn normalize_keybindings(&mut self) -> bool {
        let mut changed = false;

        // Migration: old default save bindings were Ctrl+S/Ctrl+D (often unreliable under some IME setups).
        // Move to Shift+Enter by default.
        if self
            .keybindings
            .composer
            .submit
            .iter()
            .any(|k| k.eq_ignore_ascii_case("ctrl+s") || k.eq_ignore_ascii_case("ctrl+d"))
            && !self
                .keybindings
                .composer
                .submit
                .iter()
                .any(|k| k.eq_ignore_ascii_case("shift+enter"))
        {
            self.keybindings.composer.submit = vec!["shift+enter".to_string()];
            changed = true;
        }

        let removed_composer = remove_keybinding(&mut self.keybindings.composer.clear, "ctrl+l");
        let removed_search = remove_keybinding(&mut self.keybindings.search.clear, "ctrl+l");
        if removed_composer || removed_search {
            changed = true;
        }

        changed
    }
}

fn remove_keybinding(list: &mut Vec<String>, key: &str) -> bool {
    let before = list.len();
    list.retain(|k| !k.eq_ignore_ascii_case(key));
    before != list.len()
}

#[cfg(test)]
mod tests {
    use super::{Theme, ThemePreset};

    #[test]
    fn presets_construct_without_panicking() {
        let _dracula = Theme::preset(ThemePreset::DraculaDark);
        let _solarized_dark = Theme::preset(ThemePreset::SolarizedDark);
        let _solarized_light = Theme::preset(ThemePreset::SolarizedLight);
        let _nord = Theme::preset(ThemePreset::NordCalm);
        let _mono = Theme::preset(ThemePreset::MonoContrast);
    }

    #[test]
    fn preset_name_lookup_is_case_insensitive() {
        assert_eq!(
            ThemePreset::from_name("dracula dark"),
            Some(ThemePreset::DraculaDark)
        );
        assert_eq!(
            ThemePreset::from_name("Solarized Light"),
            Some(ThemePreset::SolarizedLight)
        );
        assert_eq!(ThemePreset::from_name("unknown"), None);
    }
}
