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
    pub log_dir: Vec<String>,
    pub pomodoro: Vec<String>,
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
            log_dir: vec!["o".to_string()],
            pomodoro: vec!["p".to_string()],
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
    pub toggle_todo: Vec<String>,
    pub open: Vec<String>,
    pub edit: Vec<String>,
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
            toggle_todo: vec!["enter".to_string(), "space".to_string()],
            open: vec!["enter".to_string()],
            edit: vec!["e".to_string()],
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
}

impl Default for ComposerBindings {
    fn default() -> Self {
        Self {
            cancel: vec!["esc".to_string()],
            newline: vec!["enter".to_string()],
            submit: vec!["shift+enter".to_string()],
            clear: vec!["ctrl+l".to_string()],
            indent: vec!["tab".to_string()],
            outdent: vec!["backtab".to_string()],
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
            clear: vec!["ctrl+l".to_string()],
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
}

impl Default for PopupBindings {
    fn default() -> Self {
        Self {
            confirm: vec!["enter".to_string(), "y".to_string()],
            cancel: vec!["esc".to_string(), "n".to_string()],
            up: vec!["k".to_string(), "up".to_string()],
            down: vec!["j".to_string(), "down".to_string()],
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
            match toml::from_str::<Config>(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Failed to parse config.toml ({config_path:?}), using defaults: {e}");
                    Config::default()
                }
            }
        } else {
            Config::default()
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

        changed
    }
}
