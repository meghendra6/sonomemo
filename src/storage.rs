use crate::models::{
    LogEntry, TaskItem, count_trailing_tomatoes, is_timestamped_line, strip_timestamp_prefix,
    strip_trailing_tomatoes,
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
    let entry_body = content.trim_end_matches('\n');
    let mut entry = format!("## [{}]\n", time);
    if !entry_body.is_empty() {
        entry.push_str(entry_body);
        if !entry.ends_with('\n') {
            entry.push('\n');
        }
    } else {
        entry.push('\n');
    }

    if !entry.ends_with("\n\n") {
        entry.push('\n');
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    if path.exists() && path.metadata()?.len() > 0 {
        let existing = fs::read_to_string(&path).unwrap_or_default();
        if !existing.ends_with("\n\n") {
            if existing.ends_with('\n') {
                file.write_all(b"\n")?;
            } else {
                file.write_all(b"\n\n")?;
            }
        }
    }

    file.write_all(entry.as_bytes())?;
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
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d")
            {
                dates.push(date);
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
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if line.contains("System: Carryover Checked") {
            continue;
        }

        if line.trim().is_empty() {
            if entries.is_empty() {
                continue;
            }
            if next_non_empty_is_timestamp(&lines, i + 1) {
                continue;
            }
        }

        if is_timestamped_line(line) || entries.is_empty() {
            entries.push(LogEntry {
                content: (*line).to_string(),
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

fn next_non_empty_is_timestamp(lines: &[&str], start: usize) -> bool {
    for line in lines.iter().skip(start) {
        if line.contains("System: Carryover Checked") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        return is_timestamped_line(line);
    }
    false
}

fn normalize_task_text(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<&str>>().join(" ");
    normalized.to_lowercase()
}

fn strip_carryover_marker(text: &str) -> (String, Option<String>) {
    let trimmed = text.trim_end();
    let open = '⟦';
    let close = '⟧';
    let Some(start) = trimmed.rfind(open) else {
        return (trimmed.to_string(), None);
    };
    let Some(close_offset) = trimmed[start..].find(close) else {
        return (trimmed.to_string(), None);
    };
    let close_index = start + close_offset;
    let close_len = close.len_utf8();
    if close_index + close_len != trimmed.len() {
        return (trimmed.to_string(), None);
    }

    let date = &trimmed[start + open.len_utf8()..close_index];
    if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        return (trimmed.to_string(), None);
    }

    let base = trimmed[..start].trim_end().to_string();
    (base, Some(date.to_string()))
}

fn task_identity_from_text(text: &str) -> (String, Option<String>) {
    let (base, carryover_from) = strip_carryover_marker(text);
    (normalize_task_text(&base), carryover_from)
}

fn parse_task_content(content: &str, path_str: &str) -> Vec<TaskItem> {
    let mut tasks: Vec<TaskItem> = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if line.contains("System: Carryover Checked") {
            continue;
        }

        let s = strip_timestamp_prefix(line);

        let (indent_bytes, indent_spaces) = parse_indent(s);
        let s = &s[indent_bytes..];

        if let Some(text) = s.strip_prefix("- [ ] ") {
            let (text, tomato_count) = strip_trailing_tomatoes(text);
            let text = text.trim();
            let (task_identity, carryover_from) = task_identity_from_text(text);
            tasks.push(TaskItem {
                text: text.to_string(),
                indent: indent_spaces.div_ceil(2),
                tomato_count,
                file_path: path_str.to_string(),
                line_number: i,
                task_identity,
                carryover_from,
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

struct ParsedTaskLine {
    identity: String,
    carryover_from: Option<String>,
    is_done: bool,
    indent_level: usize,
    base_text: String,
}

fn parse_task_line(line: &str) -> Option<ParsedTaskLine> {
    if line.contains("System: Carryover Checked") {
        return None;
    }

    let s = strip_timestamp_prefix(line);
    let (indent_bytes, indent_spaces) = parse_indent(s);
    let s = &s[indent_bytes..];

    let (is_done, text) = if let Some(text) = s.strip_prefix("- [ ] ") {
        (false, text)
    } else if let Some(text) = s.strip_prefix("- [x] ") {
        (true, text)
    } else if let Some(text) = s.strip_prefix("- [X] ") {
        (true, text)
    } else {
        return None;
    };

    let (text, _) = strip_trailing_tomatoes(text);
    let text = text.trim();
    let (base_text, carryover_from) = strip_carryover_marker(text);
    let identity = normalize_task_text(&base_text);

    Some(ParsedTaskLine {
        identity,
        carryover_from,
        is_done,
        indent_level: indent_spaces.div_ceil(2),
        base_text,
    })
}

fn mark_task_completed_line(line: &str) -> Option<String> {
    if line.contains("- [ ]") {
        Some(line.replacen("- [ ]", "- [x]", 1))
    } else {
        None
    }
}

fn mark_task_completed_at_line(file_path: &str, line_number: usize) -> io::Result<bool> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if line_number >= lines.len() {
        return Ok(false);
    }

    let updated = if let Some(new_line) = mark_task_completed_line(&lines[line_number]) {
        lines[line_number] = new_line;
        true
    } else {
        false
    };

    if updated {
        let mut new_content = lines.join("\n");
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        let mut file = fs::File::create(file_path)?;
        file.write_all(new_content.as_bytes())?;
    }

    Ok(updated)
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

fn extract_date_from_path(file_path: &str) -> Option<NaiveDate> {
    let path = Path::new(file_path);
    let stem = path.file_stem()?.to_str()?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

pub fn complete_task_chain(log_path: &Path, task: &TaskItem) -> io::Result<usize> {
    let mut completed_count = 0usize;
    if mark_task_completed_at_line(&task.file_path, task.line_number)? {
        completed_count += 1;
    }

    let Some(from_date) = task.carryover_from.clone() else {
        return Ok(completed_count);
    };
    let Some(current_date) = extract_date_from_path(&task.file_path) else {
        return Ok(completed_count);
    };

    let mut pending_dates = vec![from_date];
    let mut visited_dates = std::collections::HashSet::new();

    while let Some(date_str) = pending_dates.pop() {
        if !visited_dates.insert(date_str.clone()) {
            continue;
        }

        let Ok(date) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") else {
            continue;
        };
        if date >= current_date {
            continue;
        }

        let path = get_file_path_for_date(log_path, &date_str);
        if !path.exists() {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut changed = false;
        let mut next_dates: Vec<String> = Vec::new();

        for line in &mut lines {
            let Some(parsed) = parse_task_line(line) else {
                continue;
            };
            if parsed.identity != task.task_identity {
                continue;
            }

            if !parsed.is_done
                && let Some(new_line) = mark_task_completed_line(line)
            {
                *line = new_line;
                completed_count += 1;
                changed = true;
            }

            if let Some(next_date) = parsed.carryover_from {
                next_dates.push(next_date);
            }
        }

        if changed {
            let mut new_content = lines.join("\n");
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            fs::write(&path, new_content)?;
        }

        for next_date in next_dates {
            if !visited_dates.contains(&next_date) {
                pending_dates.push(next_date);
            }
        }
    }

    Ok(completed_count)
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

pub fn delete_entry_lines(file_path: &str, start_line: usize, end_line: usize) -> io::Result<()> {
    replace_entry_lines(file_path, start_line, end_line, &[])
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

pub fn collect_carryover_tasks(log_path: &Path, today: &str) -> io::Result<Vec<String>> {
    ensure_log_dir(log_path)?;
    let today_date =
        NaiveDate::parse_from_str(today, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive());
    let mut resolved: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut carryover = Vec::new();

    let today_path = get_file_path_for_date(log_path, today);
    if today_path.exists() {
        let content = fs::read_to_string(&today_path)?;
        for line in content.lines() {
            if let Some(parsed) = parse_task_line(line) {
                resolved.insert(parsed.identity);
            }
        }
    }

    let dates = get_available_log_dates(log_path)?;
    for date in dates.iter().rev() {
        if *date >= today_date {
            continue;
        }
        let date_str = date.format("%Y-%m-%d").to_string();
        let path = get_file_path_for_date(log_path, &date_str);
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let mut ordered: Vec<String> = Vec::new();
        let mut states: std::collections::HashMap<String, ParsedTaskLine> =
            std::collections::HashMap::new();

        for line in content.lines() {
            let Some(parsed) = parse_task_line(line) else {
                continue;
            };
            let identity = parsed.identity.clone();
            match states.entry(identity.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    ordered.push(identity);
                    entry.insert(parsed);
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    if parsed.is_done {
                        entry.get_mut().is_done = true;
                    }
                }
            }
        }

        for identity in ordered {
            let Some(parsed) = states.get(&identity) else {
                continue;
            };
            if resolved.contains(&parsed.identity) {
                continue;
            }
            resolved.insert(parsed.identity.clone());
            if parsed.is_done {
                continue;
            }
            let base_text = parsed.base_text.trim();
            if base_text.is_empty() {
                continue;
            }
            let indent = "  ".repeat(parsed.indent_level);
            carryover.push(format!("{indent}- [ ] {base_text} ⟦{date_str}⟧"));
        }
    }

    Ok(carryover)
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
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Ok(content) = fs::read_to_string(&path)
            {
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
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Some(filename) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(content) = fs::read_to_string(&path)
            {
                let mut line_count = 0usize;
                let mut tomato_count = 0usize;

                for line in content.lines() {
                    if line.trim().is_empty() || line.contains("System: Carryover Checked") {
                        continue;
                    }
                    line_count += 1;

                    // Count tomatoes (only from non-carryover tasks)
                    let s = strip_timestamp_prefix(line).trim_start();

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

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::fs;
    use std::path::PathBuf;

    fn temp_log_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("memolog-test-{}-{}", std::process::id(), stamp));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn append_entry_inserts_blank_line_between_entries() {
        let dir = temp_log_dir();
        append_entry(&dir, "First\n- item").expect("append first");
        append_entry(&dir, "Second").expect("append second");

        let path = get_today_file_path(&dir);
        let content = fs::read_to_string(path).expect("read log");

        let first_line = content.lines().next().unwrap_or("");
        let ts_re = Regex::new(r"^## \[\d{2}:\d{2}:\d{2}\]$").unwrap();
        assert!(ts_re.is_match(first_line));

        let lines: Vec<&str> = content.split('\n').collect();
        assert_eq!(lines.get(1).copied().unwrap_or(""), "First");
        assert_eq!(lines.get(2).copied().unwrap_or(""), "- item");

        let mut timestamps = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if is_timestamped_line(line) {
                timestamps.push(idx);
            }
        }
        assert!(timestamps.len() >= 2);
        let second = timestamps[1];
        assert_eq!(
            lines.get(second.saturating_sub(1)).copied().unwrap_or(""),
            ""
        );
    }

    #[test]
    fn parse_log_content_skips_separator_blank_lines() {
        let content = "## [09:00:00]\nFirst\n- item\n\n## [09:10:00]\nSecond";
        let entries = parse_log_content(content, "test.md");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "## [09:00:00]\nFirst\n- item");
        assert_eq!(entries[0].line_number, 0);
        assert_eq!(entries[0].end_line, 2);
        assert_eq!(entries[1].line_number, 4);
    }

    #[test]
    fn parse_log_content_keeps_internal_blank_lines() {
        let content = "## [09:00:00]\nFirst\n\n- item\n\n## [09:10:00]\nSecond";
        let entries = parse_log_content(content, "test.md");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "## [09:00:00]\nFirst\n\n- item");
        assert_eq!(entries[0].end_line, 3);
        assert_eq!(entries[1].line_number, 5);
    }

    fn write_log(dir: &Path, date: &str, content: &str) {
        let path = get_file_path_for_date(dir, date);
        fs::write(path, content).expect("write log");
    }

    fn read_tasks_for_date(dir: &Path, date: &str) -> Vec<TaskItem> {
        let path = get_file_path_for_date(dir, date);
        let content = fs::read_to_string(&path).expect("read log");
        parse_task_content(&content, &path.to_string_lossy())
    }

    #[test]
    fn complete_task_chain_marks_previous_carryover() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-01-01", "- [ ] Write  Report\n");
        write_log(&dir, "2024-01-02", "- [ ] write report ⟦2024-01-01⟧\n");

        let task = read_tasks_for_date(&dir, "2024-01-02")
            .into_iter()
            .next()
            .expect("task");
        let completed = complete_task_chain(&dir, &task).expect("complete chain");

        assert_eq!(completed, 2);
        let day1 = fs::read_to_string(dir.join("2024-01-01.md")).expect("read day1");
        let day2 = fs::read_to_string(dir.join("2024-01-02.md")).expect("read day2");
        assert_eq!(day1.lines().next().unwrap_or(""), "- [x] Write  Report");
        assert_eq!(
            day2.lines().next().unwrap_or(""),
            "- [x] write report ⟦2024-01-01⟧"
        );
    }

    #[test]
    fn complete_task_chain_skips_unrelated_same_text() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-02-01", "- [ ] Same Task\n");
        write_log(&dir, "2024-02-02", "- [ ] Same Task\n");
        write_log(&dir, "2024-02-03", "- [ ] Same Task ⟦2024-02-02⟧\n");

        let task = read_tasks_for_date(&dir, "2024-02-03")
            .into_iter()
            .next()
            .expect("task");
        let completed = complete_task_chain(&dir, &task).expect("complete chain");

        assert_eq!(completed, 2);
        let day1 = fs::read_to_string(dir.join("2024-02-01.md")).expect("read day1");
        let day2 = fs::read_to_string(dir.join("2024-02-02.md")).expect("read day2");
        assert_eq!(day1.lines().next().unwrap_or(""), "- [ ] Same Task");
        assert_eq!(day2.lines().next().unwrap_or(""), "- [x] Same Task");
    }

    #[test]
    fn complete_task_chain_does_not_touch_future_tasks() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-03-01", "- [ ] Future Task\n");
        write_log(&dir, "2024-03-02", "- [ ] Future Task ⟦2024-03-01⟧\n");
        write_log(&dir, "2024-03-03", "- [ ] Future Task\n");

        let task = read_tasks_for_date(&dir, "2024-03-02")
            .into_iter()
            .next()
            .expect("task");
        let completed = complete_task_chain(&dir, &task).expect("complete chain");

        assert_eq!(completed, 2);
        let day3 = fs::read_to_string(dir.join("2024-03-03.md")).expect("read day3");
        assert_eq!(day3.lines().next().unwrap_or(""), "- [ ] Future Task");
    }

    #[test]
    fn collect_carryover_tasks_across_gap_days() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-12-22", "- [ ] Alpha Task\n- [x] Done Task\n");
        write_log(&dir, "2024-12-20", "- [ ] Beta Task\n");

        let tasks = collect_carryover_tasks(&dir, "2024-12-25").expect("collect");

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], "- [ ] Alpha Task ⟦2024-12-22⟧");
        assert_eq!(tasks[1], "- [ ] Beta Task ⟦2024-12-20⟧");
    }

    #[test]
    fn collect_carryover_tasks_deduplicates_latest() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-12-21", "- [ ] Same Task\n");
        write_log(&dir, "2024-12-22", "- [ ] Same Task\n");

        let tasks = collect_carryover_tasks(&dir, "2024-12-25").expect("collect");

        assert_eq!(tasks, vec!["- [ ] Same Task ⟦2024-12-22⟧"]);
    }

    #[test]
    fn collect_carryover_tasks_skips_completed_in_later_date() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-12-21", "- [ ] Finish Task\n");
        write_log(&dir, "2024-12-22", "- [x] Finish Task\n");

        let tasks = collect_carryover_tasks(&dir, "2024-12-25").expect("collect");

        assert!(tasks.is_empty());
    }

    #[test]
    fn collect_carryover_tasks_skips_tasks_already_today() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-12-22", "- [ ] Today Task\n");
        write_log(&dir, "2024-12-25", "- [ ] Today Task\n");

        let tasks = collect_carryover_tasks(&dir, "2024-12-25").expect("collect");

        assert!(tasks.is_empty());
    }
}
