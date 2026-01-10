use crate::models::{
    AgendaItem, AgendaItemKind, FoldOverride, Priority, LogEntry, TaskItem, TaskSchedule,
    count_trailing_tomatoes,
    is_heading_timestamp_line, is_timestamped_line, strip_timestamp_prefix, strip_trailing_tomatoes,
};
use crate::task_metadata::{
    TaskMetadataKey, parse_task_metadata, strip_task_metadata_tokens, upsert_task_metadata_token,
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
    let today = Local::now().date_naive();
    append_entry_to_date(log_path, today, content)
}

pub fn append_entry_to_date(
    log_path: &Path,
    date: NaiveDate,
    content: &str,
) -> io::Result<()> {
    ensure_log_dir(log_path)?;
    let date_str = date.format("%Y-%m-%d").to_string();
    let path = get_file_path_for_date(log_path, &date_str);

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

pub fn read_entry_containing_line(
    file_path: &str,
    line_number: usize,
) -> io::Result<Option<LogEntry>> {
    let content = fs::read_to_string(file_path)?;
    let entries = parse_log_content(&content, file_path);
    Ok(entries
        .into_iter()
        .find(|entry| entry.line_number <= line_number && line_number <= entry.end_line))
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

/// Reads task items for a date range (inclusive), returning agenda items.
pub fn read_tasks_for_date_range(
    log_path: &Path,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> io::Result<Vec<AgendaItem>> {
    ensure_log_dir(log_path)?;
    let mut items = Vec::new();
    let dates = get_available_log_dates(log_path).unwrap_or_default();
    for date in dates {
        let date_str = date.format("%Y-%m-%d").to_string();
        let path = get_file_path_for_date(log_path, &date_str);
        if !path.exists() {
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        if let Ok(content) = fs::read_to_string(&path) {
            let tasks = parse_task_content(&content, &path_str);
            for task in tasks {
                let agenda_date = agenda_date_for_task(&task, date);
                let is_unscheduled = task.schedule.is_empty();
                if !is_unscheduled && (agenda_date < start_date || agenda_date > end_date) {
                    continue;
                }
                items.push(AgendaItem {
                    kind: AgendaItemKind::Task,
                    date: agenda_date,
                    time: task.schedule.time,
                    duration_minutes: task.schedule.duration_minutes,
                    text: task.text,
                    indent: task.indent,
                    is_done: task.is_done,
                    priority: task.priority,
                    schedule: task.schedule.clone(),
                    file_path: task.file_path,
                    line_number: task.line_number,
                });
            }
        }
    }

    Ok(items)
}

pub fn read_agenda_entries(
    log_path: &Path,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> io::Result<Vec<AgendaItem>> {
    let mut items = read_tasks_for_date_range(log_path, start_date, end_date)?;
    items.extend(read_note_entries(log_path, start_date, end_date)?);
    Ok(items)
}

fn agenda_date_for_task(task: &TaskItem, file_date: NaiveDate) -> NaiveDate {
    task.schedule
        .scheduled
        .or(task.schedule.due)
        .or(task.schedule.start)
        .unwrap_or(file_date)
}

fn read_note_entries(
    log_path: &Path,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> io::Result<Vec<AgendaItem>> {
    ensure_log_dir(log_path)?;
    let mut items = Vec::new();
    let dates = get_available_log_dates(log_path).unwrap_or_default();

    for date in dates {
        let date_str = date.format("%Y-%m-%d").to_string();
        let path = get_file_path_for_date(log_path, &date_str);
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let path_str = path.to_string_lossy().to_string();

        for (idx, line) in content.lines().enumerate() {
            if line.contains("System: Carryover Checked") {
                continue;
            }
            if parse_task_line(line).is_some() {
                continue;
            }
            if is_timestamped_line(line) {
                continue;
            }

            let stripped = strip_timestamp_prefix(line);
            if stripped.trim().is_empty() {
                continue;
            }
            let (schedule, text) = parse_task_metadata(stripped);
            if schedule.is_empty() {
                continue;
            }
            if text.trim().is_empty() {
                continue;
            }

            let agenda_date = schedule
                .scheduled
                .or(schedule.due)
                .or(schedule.start)
                .unwrap_or(date);
            if agenda_date < start_date || agenda_date > end_date {
                continue;
            }

            let (_indent_bytes, indent_spaces) = parse_indent(stripped);

            items.push(AgendaItem {
                kind: AgendaItemKind::Note,
                date: agenda_date,
                time: schedule.time,
                duration_minutes: schedule.duration_minutes,
                text: text.trim().to_string(),
                indent: indent_spaces.div_ceil(2),
                is_done: false,
                priority: None,
                schedule,
                file_path: path_str.clone(),
                line_number: idx,
            });
        }
    }

    Ok(items)
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

pub fn search_entries_by_keywords(
    log_path: &Path,
    keywords: &[String],
) -> io::Result<Vec<LogEntry>> {
    ensure_log_dir(log_path)?;
    let mut results: Vec<(usize, LogEntry)> = Vec::new();
    let mut normalized = Vec::new();

    for keyword in keywords {
        let trimmed = keyword.trim();
        if trimmed.is_empty() {
            continue;
        }
        normalized.push(trimmed.to_lowercase());
    }

    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(log_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                paths.push(path);
            }
        }
    }
    paths.sort();

    for path in paths {
        let path_str = path.to_string_lossy().to_string();
        if let Ok(content) = fs::read_to_string(&path) {
            let parsed_entries = parse_log_content(&content, &path_str);
            for entry in parsed_entries {
                let haystack = entry.content.to_lowercase();
                let mut score = 0usize;
                for keyword in &normalized {
                    if haystack.contains(keyword) {
                        score += 1;
                    }
                }
                if score > 0 {
                    results.push((score, entry));
                }
            }
        }
    }

    results.sort_by(|(score_a, entry_a), (score_b, entry_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| entry_b.file_path.cmp(&entry_a.file_path))
            .then_with(|| entry_b.line_number.cmp(&entry_a.line_number))
    });

    Ok(results.into_iter().map(|(_, entry)| entry).collect())
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

fn is_context_tag_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn strip_context_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if ch != '#' {
            out.push(ch);
            continue;
        }

        let prev = text[..idx].chars().last();
        let prev_ok = prev.map_or(true, |c| !is_context_tag_char(c));

        let mut token = String::new();
        let mut token_lower = String::new();
        let mut end_idx = idx + ch.len_utf8();
        while let Some(&(next_idx, next_ch)) = chars.peek() {
            if is_context_tag_char(next_ch) {
                token.push(next_ch);
                token_lower.push(next_ch.to_ascii_lowercase());
                end_idx = next_idx + next_ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let next = text[end_idx..].chars().next();
        let next_ok = next.map_or(true, |c| !is_context_tag_char(c));
        let is_context = token_lower == "work" || token_lower == "personal";

        if prev_ok && next_ok && is_context {
            if let Some(&(_, next_ch)) = chars.peek() {
                let out_has_ws =
                    out.is_empty() || out.chars().last().is_some_and(|c| c.is_whitespace());
                if next_ch.is_whitespace() && out_has_ws {
                    chars.next();
                }
            }
            continue;
        }

        out.push('#');
        out.push_str(&token);
    }

    out
}

fn trailing_context_tag_start(text: &str) -> usize {
    let mut end = text.len();
    let mut tail_start = text.len();

    loop {
        let trimmed = text[..end].trim_end_matches(|c: char| c.is_whitespace());
        let trimmed_end = trimmed.len();
        if trimmed_end == 0 {
            break;
        }

        let Some(hash_idx) = trimmed.rfind('#') else {
            break;
        };
        let tag = &trimmed[hash_idx + 1..trimmed_end];
        if tag.is_empty() || !tag.chars().all(is_context_tag_char) {
            break;
        }
        let tag_lower = tag.to_ascii_lowercase();
        if tag_lower != "work" && tag_lower != "personal" {
            break;
        }

        if let Some(prev) = trimmed[..hash_idx].chars().last()
            && !prev.is_whitespace()
        {
            break;
        }

        let mut ws_start = hash_idx;
        while ws_start > 0 {
            // Safely get the previous character without panicking on empty string
            if let Some(prev) = text[..ws_start].chars().last() {
                if prev.is_whitespace() {
                    ws_start = ws_start.saturating_sub(prev.len_utf8());
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        tail_start = ws_start;
        end = ws_start;
    }

    tail_start
}

fn split_trailing_context_tags(text: &str) -> (&str, &str) {
    let start = trailing_context_tag_start(text);
    text.split_at(start)
}

fn strip_carryover_marker_at_end(text: &str) -> (String, Option<String>) {
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

fn strip_carryover_marker(text: &str) -> (String, Option<String>) {
    let trimmed = text.trim_end();
    let (head, tail) = split_trailing_context_tags(trimmed);
    let (base, carryover) = strip_carryover_marker_at_end(head);
    if carryover.is_none() {
        return (trimmed.to_string(), None);
    }

    let mut output = base.trim_end().to_string();
    output.push_str(tail);
    (output, carryover)
}

fn parse_priority_marker(text: &str) -> Option<Priority> {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("[#") else {
        return None;
    };
    let mut chars = rest.chars();
    let Some(letter) = chars.next() else {
        return None;
    };
    if !matches!(chars.next(), Some(']')) {
        return None;
    }
    Priority::from_char(letter)
}

fn strip_priority_marker(text: &str) -> String {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("[#") else {
        return text.to_string();
    };
    let mut chars = rest.chars();
    let Some(letter) = chars.next() else {
        return text.to_string();
    };
    if !matches!(chars.next(), Some(']')) {
        return text.to_string();
    }
    if Priority::from_char(letter).is_none() {
        return text.to_string();
    }
    chars.as_str().trim_start().to_string()
}

fn task_identity_from_text(text: &str) -> (String, Option<String>) {
    let without_priority = strip_priority_marker(text);
    let without_metadata = strip_task_metadata_tokens(&without_priority);
    let without_context = strip_context_tags(&without_metadata);
    let (base, carryover_from) = strip_carryover_marker(&without_context);
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

        let (is_done, text) = if let Some(text) = s.strip_prefix("- [ ] ") {
            (false, text)
        } else if let Some(text) = s.strip_prefix("- [x] ") {
            (true, text)
        } else if let Some(text) = s.strip_prefix("- [X] ") {
            (true, text)
        } else {
            continue;
        };

        let (text, tomato_count) = strip_trailing_tomatoes(text);
        let text = text.trim();
        let priority = parse_priority_marker(text);
        let (schedule, display_text) = parse_task_metadata(text);
        let (task_identity, carryover_from) = task_identity_from_text(text);
        tasks.push(TaskItem {
            text: display_text,
            indent: indent_spaces.div_ceil(2),
            tomato_count,
            file_path: path_str.to_string(),
            line_number: i,
            is_done,
            priority,
            schedule,
            task_identity,
            carryover_from,
        });
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
    raw_text: String,
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
    let (raw_text, carryover_from) = strip_carryover_marker(text);
    let (identity, _) = task_identity_from_text(text);

    Some(ParsedTaskLine {
        identity,
        carryover_from,
        is_done,
        indent_level: indent_spaces.div_ceil(2),
        raw_text: raw_text.to_string(),
    })
}

fn format_task_body(update: &TaskLineUpdate) -> String {
    let checkbox = if update.is_done { "- [x] " } else { "- [ ] " };
    let mut body = String::new();
    body.push_str(checkbox);
    if let Some(priority) = update.priority {
        body.push_str(&format!("[#{}] ", priority.as_char()));
    }
    body.push_str(update.text.trim());
    apply_schedule_tokens(&body, &update.schedule)
}

fn apply_schedule_tokens(text: &str, schedule: &TaskSchedule) -> String {
    let mut output = text.trim_end().to_string();
    let sched = schedule
        .scheduled
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    output = upsert_task_metadata_token(&output, TaskMetadataKey::Scheduled, &sched);
    let due = schedule
        .due
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    output = upsert_task_metadata_token(&output, TaskMetadataKey::Due, &due);
    let start = schedule
        .start
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    output = upsert_task_metadata_token(&output, TaskMetadataKey::Start, &start);
    let time = schedule
        .time
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default();
    output = upsert_task_metadata_token(&output, TaskMetadataKey::Time, &time);
    let duration = schedule
        .duration_minutes
        .map(|m| m.to_string())
        .unwrap_or_default();
    output = upsert_task_metadata_token(&output, TaskMetadataKey::Duration, &duration);
    output
}

fn extract_carryover_marker(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    let (head, _) = split_trailing_context_tags(trimmed);
    let (_, carryover) = strip_carryover_marker_at_end(head);
    carryover
}

fn split_note_prefix(text: &str) -> (String, &str) {
    if let Some(rest) = text.strip_prefix("- ") {
        return ("- ".to_string(), rest);
    }
    if let Some(rest) = text.strip_prefix("* ") {
        return ("* ".to_string(), rest);
    }
    if let Some(rest) = text.strip_prefix("+ ") {
        return ("+ ".to_string(), rest);
    }
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i + 1 < bytes.len() {
        let punct = bytes[i];
        if (punct == b'.' || punct == b')') && bytes[i + 1] == b' ' {
            let prefix = text[..i + 2].to_string();
            let rest = &text[i + 2..];
            return (prefix, rest);
        }
    }
    (String::new(), text)
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

pub struct TaskLineUpdate {
    pub text: String,
    pub is_done: bool,
    pub priority: Option<Priority>,
    pub schedule: TaskSchedule,
}

pub struct NoteLineUpdate {
    pub text: String,
    pub schedule: TaskSchedule,
}

pub fn update_task_line(
    file_path: &str,
    line_number: usize,
    update: TaskLineUpdate,
) -> io::Result<bool> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if line_number >= lines.len() {
        return Ok(false);
    }

    let line = lines[line_number].clone();
    let stripped = strip_timestamp_prefix(&line);
    let prefix_len = line.len().saturating_sub(stripped.len());
    let (indent_bytes, _) = parse_indent(stripped);
    let prefix = &line[..prefix_len.saturating_add(indent_bytes)];

    let (without_tomatoes, tomato_count) = strip_trailing_tomatoes(stripped);
    let carryover = extract_carryover_marker(without_tomatoes);

    let mut body = format_task_body(&update);
    if let Some(carryover) = carryover {
        body.push_str(&format!(" ⟦{carryover}⟧"));
    }
    if tomato_count > 0 {
        body.push(' ');
        body.push_str(&"🍅".repeat(tomato_count));
    }

    lines[line_number] = format!("{prefix}{body}");

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    let mut file = fs::File::create(file_path)?;
    file.write_all(new_content.as_bytes())?;

    Ok(true)
}

pub fn update_note_line(
    file_path: &str,
    line_number: usize,
    update: NoteLineUpdate,
) -> io::Result<bool> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if line_number >= lines.len() {
        return Ok(false);
    }

    let line = lines[line_number].clone();
    let stripped = strip_timestamp_prefix(&line);
    let prefix_len = line.len().saturating_sub(stripped.len());
    let (indent_bytes, _) = parse_indent(stripped);
    let indent = &stripped[..indent_bytes];
    let rest = &stripped[indent_bytes..];
    let (list_prefix, _) = split_note_prefix(rest);

    let mut body = String::new();
    body.push_str(update.text.trim());
    body = apply_schedule_tokens(&body, &update.schedule);

    let prefix = format!("{}{}{}", &line[..prefix_len], indent, list_prefix);
    lines[line_number] = format!("{prefix}{body}");

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    let mut file = fs::File::create(file_path)?;
    file.write_all(new_content.as_bytes())?;

    Ok(true)
}

pub fn compose_task_line(update: &TaskLineUpdate) -> String {
    format_task_body(update)
}

pub fn compose_note_line(update: &NoteLineUpdate) -> String {
    let mut body = update.text.trim().to_string();
    body = apply_schedule_tokens(&body, &update.schedule);
    body
}

/// Cycles task priority marker (None -> A -> B -> C -> None) at the given line.
pub fn cycle_task_priority(file_path: &str, line_number: usize) -> io::Result<bool> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if line_number >= lines.len() {
        return Ok(false);
    }

    let line = lines[line_number].clone();
    let stripped = strip_timestamp_prefix(&line);
    let prefix_len = line.len().saturating_sub(stripped.len());
    let (indent_bytes, _) = parse_indent(stripped);
    let body_start = prefix_len.saturating_add(indent_bytes);
    if body_start > line.len() {
        return Ok(false);
    }
    let (prefix, body) = line.split_at(body_start);

    let (checkbox, after_checkbox) = if let Some(text) = body.strip_prefix("- [ ] ") {
        ("- [ ] ", text)
    } else if let Some(text) = body.strip_prefix("- [x] ") {
        ("- [x] ", text)
    } else if let Some(text) = body.strip_prefix("- [X] ") {
        ("- [X] ", text)
    } else {
        return Ok(false);
    };

    let trimmed = after_checkbox.trim_start();
    let current = parse_priority_marker(trimmed);
    let base_text = strip_priority_marker(trimmed);
    let next = match current {
        None => Some(Priority::High),
        Some(Priority::High) => Some(Priority::Medium),
        Some(Priority::Medium) => Some(Priority::Low),
        Some(Priority::Low) => None,
    };

    let mut new_body = String::new();
    new_body.push_str(checkbox);
    if let Some(priority) = next {
        new_body.push_str("[#");
        new_body.push(priority.as_char());
        new_body.push(']');
        if !base_text.is_empty() {
            new_body.push(' ');
        }
    }
    new_body.push_str(&base_text);

    let updated_line = format!("{prefix}{new_body}");
    if updated_line == line {
        return Ok(false);
    }

    lines[line_number] = updated_line;
    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    let mut file = fs::File::create(file_path)?;
    file.write_all(new_content.as_bytes())?;

    Ok(true)
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

pub fn complete_entry_tasks(entry: &LogEntry) -> io::Result<usize> {
    let content = fs::read_to_string(&entry.file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if entry.line_number >= lines.len() {
        return Ok(0);
    }

    let end_line = entry.end_line.min(lines.len().saturating_sub(1));
    let mut updated = 0usize;
    for line in lines.iter_mut().take(end_line + 1).skip(entry.line_number) {
        if let Some(new_line) = mark_task_completed_line(line) {
            *line = new_line;
            updated += 1;
        }
    }

    if updated > 0 {
        let mut new_content = lines.join("\n");
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        fs::write(&entry.file_path, new_content)?;
    }

    Ok(updated)
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

pub fn write_file_lines(file_path: &str, lines: &[String]) -> io::Result<()> {
    let path = Path::new(file_path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let mut content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(path, content)
}

pub fn update_fold_marker(
    file_path: &str,
    start_line: usize,
    state: FoldOverride,
) -> io::Result<()> {
    let content = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if start_line >= lines.len() {
        return Ok(());
    }

    if !is_heading_timestamp_line(&lines[start_line]) {
        return Ok(());
    }

    let marker = fold_marker_line(state);
    let insert_index = start_line + 1;
    if insert_index < lines.len() && is_fold_marker_line(&lines[insert_index]) {
        lines[insert_index] = marker.to_string();
    } else {
        lines.insert(insert_index, marker.to_string());
    }

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    fs::write(file_path, new_content)
}

fn fold_marker_line(state: FoldOverride) -> &'static str {
    match state {
        FoldOverride::Expanded => "<!-- memolog:expanded -->",
        FoldOverride::Folded => "<!-- memolog:folded -->",
    }
}

fn is_fold_marker_line(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(inner) = trimmed
        .strip_prefix("<!--")
        .and_then(|rest| rest.strip_suffix("-->"))
    else {
        return false;
    };
    let marker = inner.trim();
    matches!(
        marker,
        "memolog:expanded" | "memolog:folded" | "memolog:collapsed"
    )
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
            let raw_text = parsed.raw_text.trim();
            if raw_text.is_empty() {
                continue;
            }
            let indent = "  ".repeat(parsed.indent_level);
            carryover.push(format!("{indent}- [ ] {raw_text} ⟦{date_str}⟧"));
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
        // Use OnceLock to avoid recompiling regex in tests
        static TS_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let ts_re = TS_REGEX.get_or_init(|| {
            Regex::new(r"^## \[\d{2}:\d{2}:\d{2}\]$").expect("Valid regex pattern")
        });
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

    #[test]
    fn parse_task_content_reads_priority_marker() {
        let content = "- [ ] [#A] Important\n- [x] [#c] Later\n";
        let tasks = parse_task_content(content, "test.md");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].priority, Some(Priority::High));
        assert_eq!(tasks[1].priority, Some(Priority::Low));
    }

    #[test]
    fn parse_task_line_ignores_priority_in_identity() {
        let line = "- [ ] [#B] Task Name";
        let parsed = parse_task_line(line).expect("parsed");
        assert_eq!(parsed.identity, normalize_task_text("Task Name"));
    }

    #[test]
    fn parse_task_line_ignores_context_tags_in_identity() {
        let line = "- [ ] Task Name ⟦2026-01-02⟧ #work";
        let parsed = parse_task_line(line).expect("parsed");
        assert_eq!(parsed.identity, normalize_task_text("Task Name"));
    }

    #[test]
    fn strip_carryover_marker_keeps_context_suffix() {
        let (base, carryover) = strip_carryover_marker("Task Name ⟦2026-01-02⟧ #personal");
        assert_eq!(carryover, Some("2026-01-02".to_string()));
        assert_eq!(base, "Task Name #personal");
    }

    #[test]
    fn cycle_task_priority_updates_marker() {
        let dir = temp_log_dir();
        let path = get_file_path_for_date(&dir, "2024-01-01");
        fs::write(&path, "- [ ] Task\n").expect("write log");
        let path_str = path.to_string_lossy().to_string();

        assert!(cycle_task_priority(&path_str, 0).expect("cycle A"));
        let content = fs::read_to_string(&path).expect("read log");
        assert_eq!(content.lines().next().unwrap_or(""), "- [ ] [#A] Task");

        assert!(cycle_task_priority(&path_str, 0).expect("cycle B"));
        let content = fs::read_to_string(&path).expect("read log");
        assert_eq!(content.lines().next().unwrap_or(""), "- [ ] [#B] Task");

        assert!(cycle_task_priority(&path_str, 0).expect("cycle C"));
        let content = fs::read_to_string(&path).expect("read log");
        assert_eq!(content.lines().next().unwrap_or(""), "- [ ] [#C] Task");

        assert!(cycle_task_priority(&path_str, 0).expect("cycle clear"));
        let content = fs::read_to_string(&path).expect("read log");
        assert_eq!(content.lines().next().unwrap_or(""), "- [ ] Task");
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
    fn collect_carryover_tasks_deduplicates_context_tags() {
        let dir = temp_log_dir();
        write_log(&dir, "2024-12-21", "- [ ] Same Task\n");
        write_log(&dir, "2024-12-22", "- [ ] Same Task #work\n");

        let tasks = collect_carryover_tasks(&dir, "2024-12-25").expect("collect");

        assert_eq!(tasks, vec!["- [ ] Same Task #work ⟦2024-12-22⟧"]);
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
