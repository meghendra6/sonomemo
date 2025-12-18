use crate::models::{LogEntry, TaskItem};
use chrono::Local;
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
                let date_str = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();

                if let Ok(content) = fs::read_to_string(&path) {
                    let parsed_entries = parse_log_content(&content, &path_str);
                    for entry in parsed_entries {
                        if entry.content.contains(query) {
                            // 날짜 정보 추가
                            let display_content = format!("[{}] {}", date_str, entry.content);

                            results.push(LogEntry {
                                content: display_content,
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
                indent: indent_spaces,
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

fn strip_trailing_tomatoes(s: &str) -> (&str, usize) {
    let mut count = 0;
    let mut text = s.trim_end();
    while let Some(rest) = text.strip_suffix('🍅') {
        count += 1;
        text = rest.trim_end();
    }
    (text, count)
}

fn is_timestamped_line(line: &str) -> bool {
    // Format: "[HH:MM:SS] " (11+ chars)
    let bytes = line.as_bytes();
    if bytes.len() < 11 {
        return false;
    }
    if bytes[0] != b'[' || bytes[9] != b']' || bytes[10] != b' ' {
        return false;
    }
    bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b':'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
        && bytes[6] == b':'
        && bytes[7].is_ascii_digit()
        && bytes[8].is_ascii_digit()
}

pub fn toggle_task_status(file_path: &str, line_number: usize) -> io::Result<()> {
    // 파일을 전부 읽어서 해당 라인만 수정 후 다시 저장
    // 대용량 파일에는 비효율적이나, 일일 메모장 스케일에는 충분함
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
    // 파일 끝에 개행 문자가 없으면 추가 (append 시 문제 방지)
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

pub fn get_last_file_pending_todos(log_path: &Path) -> io::Result<Vec<String>> {
    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let today = Local::now().format("%Y-%m-%d").to_string();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut file_paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                // 오늘 파일은 제외 (지난 일만 가져오기 위함)
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem != today {
                        file_paths.push(path);
                    }
                }
            }
        }
        // 날짜순 정렬
        file_paths.sort();

        // 가장 최신(마지막) 파일 하나만 확인
        if let Some(last_path) = file_paths.last() {
            let mut todos = Vec::new();
            if let Ok(content) = fs::read_to_string(last_path) {
                for line in content.lines() {
                    if line.contains("- [ ]") {
                        // 타임스탬프 "[HH:MM:SS] " 제거
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
    // 많이 쓰인 순서대로 정렬 (내림차순)
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

pub fn get_activity_stats(log_path: &Path) -> io::Result<std::collections::HashMap<String, usize>> {
    use std::collections::HashMap;

    ensure_log_dir(log_path)?;
    let dir = PathBuf::from(log_path);
    let mut stats = HashMap::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    // 파일명(YYYY-MM-DD)을 키로 사용
                    if let Ok(content) = fs::read_to_string(&path) {
                        // 빈 줄이나 시스템 마커 제외하고 카운트
                        let count = content
                            .lines()
                            .filter(|l| {
                                !l.trim().is_empty() && !l.contains("System: Carryover Checked")
                            })
                            .count();
                        stats.insert(filename.to_string(), count);
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
    dir.push(".sonomemo");
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
