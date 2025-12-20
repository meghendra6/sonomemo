use crate::models::{
    LogEntry, TaskItem, count_trailing_tomatoes, is_timestamped_line, strip_trailing_tomatoes,
};
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn ensure_log_dir(log_path: &Path) -> io::Result<()> {
    let path = PathBuf::from(log_path);
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn get_today_file_path(log_path: &Path) -> PathBuf {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut path = PathBuf::from(log_path);
    path.push(format!("{}.md", today));
    path
}

fn get_file_path_for_date(log_path: &Path, date: &str) -> PathBuf {
    let mut path = PathBuf::from(log_path);
    path.push(format!("{date}.md"));
    path
}

pub fn append_entry(log_path: &Path, content: &str) -> io::Result<()> {
    ensure_log_dir(log_path)?;
    let path = get_today_file_path(log_path);

    let time = Local::now().format("%H:%M:%S").to_string();
    let line = format!("[{}] {}\n", time, content);

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    file.write_all(line.as_bytes())?;
    Ok(())
}

pub fn read_today_entries(log_path: &Path) -> io::Result<Vec<LogEntry>> {
    ensure_log_dir(log_path)?;
    let path = get_today_file_path(log_path);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let path_str = path.to_string_lossy().to_string();
    let content = fs::read_to_string(&path)?;

    Ok(parse_log_content(&content, &path_str))
}

/// Reads log entries for a date range (inclusive).
/// Returns entries ordered from oldest to newest (ascending by date).
pub fn read_entries_for_date_range(
    log_path: &Path,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> io::Result<Vec<LogEntry>> {
    ensure_log_dir(log_path)?;
    let mut all_entries = Vec::new();

    let mut current = start_date;
    while current <= end_date {
        let date_str = current.format("%Y-%m-%d").to_string();
        let path = get_file_path_for_date(log_path, &date_str);

        if path.exists() {
            let path_str = path.to_string_lossy().to_string();
            if let Ok(content) = fs::read_to_string(&path) {
                let entries = parse_log_content(&content, &path_str);
                all_entries.extend(entries);
            }
        }
        current += Duration::days(1);
    }

    Ok(all_entries)
}

/// Returns a list of available log dates in the log directory (sorted ascending).
pub fn get_available_log_dates(log_path: &Path) -> io::Result<Vec<NaiveDate>> {
    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let mut dates = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") {
                        dates.push(date);
                    }
                }
            }
        }
    }

    dates.sort();
    Ok(dates)
}

/// Returns the earliest available log date, if any.
pub fn get_earliest_log_date(log_path: &Path) -> io::Result<Option<NaiveDate>> {
    let dates = get_available_log_dates(log_path)?;
    Ok(dates.first().copied())
}

pub fn read_today_tasks(log_path: &Path) -> io::Result<Vec<TaskItem>> {
    ensure_log_dir(log_path)?;
    let path = get_today_file_path(log_path);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let path_str = path.to_string_lossy().to_string();
    let content = fs::read_to_string(&path)?;
    Ok(parse_task_content(&content, &path_str))
}

#[derive(Clone)]
pub struct CarryoverBlock {
    pub from_date: String,
    pub context: Option<String>,
    pub task_lines: Vec<String>,
}

pub fn get_carryover_blocks_for_date(
    log_path: &Path,
    from_date: &str,
) -> io::Result<Vec<CarryoverBlock>> {
    ensure_log_dir(log_path)?;
    let path = get_file_path_for_date(log_path, from_date);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)?;
    let mut blocks: Vec<CarryoverBlock> = Vec::new();

    let mut current_context: Option<String> = None;
    let mut current_tasks: Vec<String> = Vec::new();
    let mut has_started = false;

    for raw_line in content.lines() {
        if raw_line.contains("System: Carryover Checked") {
            continue;
        }

        let is_new_entry = is_timestamped_line(raw_line) || !has_started;
        if is_new_entry {
            if !current_tasks.is_empty() {
                blocks.push(CarryoverBlock {
                    from_date: from_date.to_string(),
                    context: current_context.take(),
                    task_lines: std::mem::take(&mut current_tasks),
                });
            } else {
                current_context = None;
            }
            has_started = true;
        }

        let mut s = raw_line;
        if is_timestamped_line(s) {
            // Safe due to timestamp format: "[HH:MM:SS] "
            s = &s[11..];
        }

        let trimmed_start = s.trim_start();
        if current_context.is_none()
            && !trimmed_start.is_empty()
            && !trimmed_start.starts_with("- [ ]")
            && !trimmed_start.starts_with("- [x]")
            && !trimmed_start.starts_with("- [X]")
        {
            current_context = Some(trimmed_start.to_string());
        }

        let (indent_bytes, indent_spaces) = parse_indent(s);
        let after_indent = &s[indent_bytes..];
        if let Some(text) = after_indent.strip_prefix("- [ ] ") {
            let level = (indent_spaces + 1) / 2;
            let indent = "  ".repeat(level);

            let (base, tomato_count) = strip_trailing_tomatoes(text);
            let base = base.trim();

            let mut line = format!("{indent}- [ ] {base} ⟦{from_date}⟧");
            if tomato_count > 0 {
                line.push(' ');
                line.push_str(&"🍅".repeat(tomato_count));
            }
            current_tasks.push(line);
        }
    }

    if !current_tasks.is_empty() {
        blocks.push(CarryoverBlock {
            from_date: from_date.to_string(),
            context: current_context.take(),
            task_lines: current_tasks,
        });
    }

    Ok(blocks)
}

pub fn search_entries(log_path: &Path, query: &str) -> io::Result<Vec<LogEntry>> {
    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let mut results = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let path_str = path.to_string_lossy().to_string();

                if let Ok(content) = fs::read_to_string(&path) {
                    let parsed_entries = parse_log_content(&content, &path_str);
                    for entry in parsed_entries {
                        if entry.content.contains(query) {
                            results.push(LogEntry {
                                content: entry.content,
                                file_path: entry.file_path,
                                line_number: entry.line_number,
                                end_line: entry.end_line,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

fn parse_log_content(content: &str, path_str: &str) -> Vec<LogEntry> {
    let mut entries: Vec<LogEntry> = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if line.contains("System: Carryover Checked") {
            continue;
        }

        if is_timestamped_line(line) || entries.is_empty() {
            if let Some(last) = entries.last_mut() {
                last.end_line = i.saturating_sub(1);
            }
            entries.push(LogEntry {
                content: line.to_string(),
                file_path: path_str.to_string(),
                line_number: i,
                end_line: i,
            });
            continue;
        }

        if let Some(last) = entries.last_mut() {
            last.content.push('\n');
            last.content.push_str(line);
            last.end_line = i;
        }
    }
    entries
}

fn parse_task_content(content: &str, path_str: &str) -> Vec<TaskItem> {
    let mut tasks: Vec<TaskItem> = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if line.contains("System: Carryover Checked") {
            continue;
        }

        let mut s = line;
        if is_timestamped_line(s) {
            // Safe due to timestamp format: "[HH:MM:SS] "
            s = &s[11..];
        }

        let (indent_bytes, indent_spaces) = parse_indent(s);
        let s = &s[indent_bytes..];

        if let Some(text) = s.strip_prefix("- [ ] ") {
            let (text, tomato_count) = strip_trailing_tomatoes(text);
            tasks.push(TaskItem {
                text: text.trim().to_string(),
                indent: (indent_spaces + 1) / 2,
                tomato_count,
                file_path: path_str.to_string(),
                line_number: i,
            });
        }
    }

    tasks
}

fn parse_indent(s: &str) -> (usize, usize) {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut spaces = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' => {
                i += 1;
                spaces += 1;
            }
            b'\t' => {
                i += 1;
                spaces += 4;
            }
            _ => break,
        }
    }
    (i, spaces)
}

/// Toggles the status of a task checkbox ([ ] <-> [x]) at the given line.
/// This reads the entire file and rewrites it, which is inefficient for large files
/// but acceptable for daily memo scale.
pub fn toggle_task_status(file_path: &str, line_number: usize) -> io::Result<()> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if line_number < lines.len() {
        let line = &lines[line_number];
        let new_line = if line.contains("- [ ]") {
            line.replacen("- [ ]", "- [x]", 1)
        } else if line.contains("- [x]") {
            line.replacen("- [x]", "- [ ]", 1)
        } else if line.contains("- [X]") {
            line.replacen("- [X]", "- [ ]", 1)
        } else {
            line.clone()
        };
        lines[line_number] = new_line;
    }

    let mut new_content = lines.join("\n");
    // Ensure file ends with newline (prevents issues with append operations)
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    let mut file = fs::File::create(file_path)?;
    file.write_all(new_content.as_bytes())?;

    Ok(())
}

pub fn toggle_todo_status(entry: &LogEntry) -> io::Result<()> {
    toggle_task_status(&entry.file_path, entry.line_number)
}

pub fn replace_entry_lines(
    file_path: &str,
    start_line: usize,
    end_line: usize,
    new_lines: &[String],
) -> io::Result<()> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if start_line >= lines.len() {
        return Ok(());
    }

    let end_line = end_line.min(lines.len().saturating_sub(1));
    lines.splice(start_line..(end_line + 1), new_lines.iter().cloned());

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    fs::write(file_path, new_content)
}

pub fn read_lines_range(
    file_path: &str,
    start_line: usize,
    end_line: usize,
) -> io::Result<Vec<String>> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if start_line >= lines.len() {
        return Ok(Vec::new());
    }
    let end_line = end_line.min(lines.len().saturating_sub(1));
    Ok(lines[start_line..=end_line].to_vec())
}

pub fn append_tomato_to_line(file_path: &str, line_number: usize) -> io::Result<()> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if line_number < lines.len() {
        let line = lines[line_number].trim_end().to_string();
        lines[line_number] = format!("{line} 🍅");
    }

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    let mut file = fs::File::create(file_path)?;
    file.write_all(new_content.as_bytes())?;
    Ok(())
}

/// Returns pending (uncompleted) todos from the most recent log file before today.
pub fn get_last_file_pending_todos(log_path: &Path) -> io::Result<Vec<String>> {
    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let today = Local::now().format("%Y-%m-%d").to_string();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut file_paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                // Exclude today's file (only look at past days)
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem != today {
                        file_paths.push(path);
                    }
                }
            }
        }
        // Sort by date
        file_paths.sort();

        // Check only the most recent (last) file
        if let Some(last_path) = file_paths.last() {
            let mut todos = Vec::new();
            if let Ok(content) = fs::read_to_string(last_path) {
                for line in content.lines() {
                    if line.contains("- [ ]") {
                        // Strip timestamp "[HH:MM:SS] " prefix
                        let clean_line = if line.trim_start().starts_with('[') {
                            if let Some(idx) = line.find("] ") {
                                &line[idx + 2..]
                            } else {
                                line
                            }
                        } else {
                            line
                        };
                        todos.push(clean_line.trim().to_string());
                    }
                }
            }
            return Ok(todos);
        }
    }
    Ok(Vec::new())
}

/// Returns all tags found in log files with their occurrence counts, sorted by frequency.
pub fn get_all_tags(log_path: &Path) -> io::Result<Vec<(String, usize)>> {
    use std::collections::HashMap;

    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let mut tag_counts = HashMap::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(content) = fs::read_to_string(&path) {
                    for line in content.lines() {
                        for word in line.split_whitespace() {
                            if word.starts_with('#') && word.len() > 1 {
                                *tag_counts.entry(word.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    // Sort by frequency (descending)
    tags.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(tags)
}

pub fn is_carryover_done(log_path: &Path) -> io::Result<bool> {
    let state = load_state(log_path)?;
    let today = Local::now().format("%Y-%m-%d").to_string();
    Ok(state.carryover_checked_date.as_deref() == Some(today.as_str()))
}

pub fn mark_carryover_done(log_path: &Path) -> io::Result<()> {
    let mut state = load_state(log_path)?;
    state.carryover_checked_date = Some(Local::now().format("%Y-%m-%d").to_string());
    save_state(log_path, &state)
}

/// Returns activity statistics for each date: (line_count, tomato_count).
/// Tomato count excludes carryover tasks (marked with ⟦date⟧).
pub fn get_activity_stats(
    log_path: &Path,
) -> io::Result<std::collections::HashMap<String, (usize, usize)>> {
    use std::collections::HashMap;

    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let mut stats = HashMap::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let mut line_count = 0usize;
                        let mut tomato_count = 0usize;

                        for line in content.lines() {
                            if line.trim().is_empty() || line.contains("System: Carryover Checked")
                            {
                                continue;
                            }
                            line_count += 1;

                            // Count tomatoes (only from non-carryover tasks)
                            let s = if is_timestamped_line(line) && line.len() >= 11 {
                                &line[11..]
                            } else {
                                line
                            };
                            let s = s.trim_start();

                            if let Some(text) = s
                                .strip_prefix("- [ ] ")
                                .or_else(|| s.strip_prefix("- [x] "))
                                .or_else(|| s.strip_prefix("- [X] "))
                            {
                                // Only count tomatoes if not a carryover task
                                if !text.contains("⟦") {
                                    tomato_count += count_trailing_tomatoes(text);
                                }
                            }
                        }

                        stats.insert(filename.to_string(), (line_count, tomato_count));
                    }
                }
            }
        }
    }
    Ok(stats)
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AppState {
    #[serde(default)]
    carryover_checked_date: Option<String>,
}

fn state_dir_path(log_path: &Path) -> PathBuf {
    let mut dir = PathBuf::from(log_path);
    dir.push(".memolog");
    dir
}

fn state_file_path(log_path: &Path) -> PathBuf {
    let mut path = state_dir_path(log_path);
    path.push("state.toml");
    path
}

fn load_state(log_path: &Path) -> io::Result<AppState> {
    ensure_log_dir(log_path)?;
    let state_dir = state_dir_path(log_path);
    if !state_dir.exists() {
        fs::create_dir_all(&state_dir)?;
    }

    let path = state_file_path(log_path);
    if !path.exists() {
        return Ok(AppState::default());
    }

    let content = fs::read_to_string(path)?;
    match toml::from_str::<AppState>(&content) {
        Ok(state) => Ok(state),
        Err(_) => Ok(AppState::default()),
    }
}

fn save_state(log_path: &Path, state: &AppState) -> io::Result<()> {
    ensure_log_dir(log_path)?;
    let state_dir = state_dir_path(log_path);
    if !state_dir.exists() {
        fs::create_dir_all(&state_dir)?;
    }

    let path = state_file_path(log_path);
    let content = toml::to_string(state).unwrap_or_default();
    fs::write(path, content)
}
