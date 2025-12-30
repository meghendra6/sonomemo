use crate::models::TaskSchedule;
use chrono::{NaiveDate, NaiveTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskMetadataKey {
    Scheduled,
    Due,
    Start,
    Time,
    Duration,
}

impl TaskMetadataKey {
    fn from_token(token: &str) -> Option<Self> {
        match token {
            "sched" | "scheduled" => Some(TaskMetadataKey::Scheduled),
            "due" => Some(TaskMetadataKey::Due),
            "start" => Some(TaskMetadataKey::Start),
            "time" => Some(TaskMetadataKey::Time),
            "dur" | "duration" => Some(TaskMetadataKey::Duration),
            _ => None,
        }
    }

    fn as_token(self) -> &'static str {
        match self {
            TaskMetadataKey::Scheduled => "sched",
            TaskMetadataKey::Due => "due",
            TaskMetadataKey::Start => "start",
            TaskMetadataKey::Time => "time",
            TaskMetadataKey::Duration => "dur",
        }
    }
}

#[derive(Clone, Debug)]
struct TokenMatch {
    key: TaskMetadataKey,
    range: std::ops::Range<usize>,
    value: String,
}

pub fn parse_task_metadata(text: &str) -> (TaskSchedule, String) {
    let tokens = scan_tokens(text);
    let mut schedule = TaskSchedule::default();
    let mut valid: Vec<TokenMatch> = Vec::new();

    for token in tokens {
        let parsed = match token.key {
            TaskMetadataKey::Scheduled => parse_date(&token.value).map(|d| {
                schedule.scheduled = Some(d);
                ()
            }),
            TaskMetadataKey::Due => parse_date(&token.value).map(|d| {
                schedule.due = Some(d);
                ()
            }),
            TaskMetadataKey::Start => parse_date(&token.value).map(|d| {
                schedule.start = Some(d);
                ()
            }),
            TaskMetadataKey::Time => parse_time(&token.value).map(|t| {
                schedule.time = Some(t);
                ()
            }),
            TaskMetadataKey::Duration => parse_duration_minutes(&token.value).map(|m| {
                schedule.duration_minutes = Some(m);
                ()
            }),
        };

        if parsed.is_some() {
            valid.push(token);
        }
    }

    let display = strip_tokens(text, &valid);
    (schedule, display)
}

pub fn strip_task_metadata_tokens(text: &str) -> String {
    let tokens = scan_tokens(text);
    let valid = tokens
        .into_iter()
        .filter(|token| match token.key {
            TaskMetadataKey::Scheduled | TaskMetadataKey::Due | TaskMetadataKey::Start => {
                parse_date(&token.value).is_some()
            }
            TaskMetadataKey::Time => parse_time(&token.value).is_some(),
            TaskMetadataKey::Duration => parse_duration_minutes(&token.value).is_some(),
        })
        .collect::<Vec<_>>();
    strip_tokens(text, &valid)
}

pub fn upsert_task_metadata_token(text: &str, key: TaskMetadataKey, value: &str) -> String {
    let mut output = remove_tokens_by_key(text, key);
    let trimmed = output.trim_end();
    output = trimmed.to_string();

    if !value.trim().is_empty() {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push('@');
        output.push_str(key.as_token());
        output.push('(');
        output.push_str(value.trim());
        output.push(')');
    }

    output
}

pub fn remove_task_metadata_token(text: &str, key: TaskMetadataKey) -> String {
    remove_tokens_by_key(text, key)
}

fn scan_tokens(text: &str) -> Vec<TokenMatch> {
    let mut tokens = scan_at_tokens(text);
    tokens.extend(scan_dataview_tokens(text));
    tokens.sort_by_key(|token| token.range.start);
    tokens
}

fn scan_at_tokens(text: &str) -> Vec<TokenMatch> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }

        let start = i;
        i += 1;
        let key_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if key_start == i || i >= bytes.len() || bytes[i] != b'(' {
            continue;
        }
        let key = &text[key_start..i].to_lowercase();
        let Some(key) = TaskMetadataKey::from_token(key) else {
            continue;
        };
        i += 1;
        let value_start = i;
        while i < bytes.len() && bytes[i] != b')' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let value = text[value_start..i].trim().to_string();
        let end = i + 1;
        tokens.push(TokenMatch {
            key,
            range: start..end,
            value,
        });
        i = end;
    }

    tokens
}

fn scan_dataview_tokens(text: &str) -> Vec<TokenMatch> {
    let keys = [
        ("scheduled", TaskMetadataKey::Scheduled),
        ("due", TaskMetadataKey::Due),
        ("start", TaskMetadataKey::Start),
        ("time", TaskMetadataKey::Time),
        ("duration", TaskMetadataKey::Duration),
    ];
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();

    for (label, key) in keys {
        let needle = format!("{label}::");
        let mut offset = 0usize;
        while let Some(pos) = text[offset..].find(&needle) {
            let start = offset + pos;
            if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
                offset = start + needle.len();
                continue;
            }
            let mut i = start + needle.len();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let value_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if value_start == i {
                offset = i;
                continue;
            }
            let value = text[value_start..i].trim().to_string();
            tokens.push(TokenMatch {
                key,
                range: start..i,
                value,
            });
            offset = i;
        }
    }

    tokens
}

fn strip_tokens(text: &str, tokens: &[TokenMatch]) -> String {
    if tokens.is_empty() {
        return text.trim().to_string();
    }

    let mut cleaned = String::new();
    let mut last = 0usize;
    for token in tokens {
        let start = token.range.start.min(text.len());
        let end = token.range.end.min(text.len());
        if start > last {
            cleaned.push_str(&text[last..start]);
        }
        last = end;
    }
    if last < text.len() {
        cleaned.push_str(&text[last..]);
    }

    normalize_display_text(&cleaned)
}

fn strip_tokens_raw(text: &str, tokens: &[TokenMatch]) -> String {
    if tokens.is_empty() {
        return text.trim_end().to_string();
    }

    let mut cleaned = String::new();
    let mut last = 0usize;
    for token in tokens {
        let start = token.range.start.min(text.len());
        let end = token.range.end.min(text.len());
        if start > last {
            cleaned.push_str(&text[last..start]);
        }
        last = end;
    }
    if last < text.len() {
        cleaned.push_str(&text[last..]);
    }

    let (prefix, rest) = split_leading_whitespace(&cleaned);
    let normalized = normalize_display_text(rest);
    let mut out = String::new();
    out.push_str(prefix);
    if !normalized.is_empty() {
        out.push_str(&normalized);
    }
    out.trim_end().to_string()
}

fn remove_tokens_by_key(text: &str, key: TaskMetadataKey) -> String {
    let tokens = scan_tokens(text)
        .into_iter()
        .filter(|token| token.key == key)
        .collect::<Vec<_>>();
    strip_tokens_raw(text, &tokens)
}

fn split_leading_whitespace(text: &str) -> (&str, &str) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    (&text[..i], &text[i..])
}

fn normalize_display_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim().to_string()
}

pub(crate) fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

pub(crate) fn parse_time(value: &str) -> Option<NaiveTime> {
    let trimmed = value.trim();
    if trimmed.contains(':') {
        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() == 2 {
            let hour: u32 = parts[0].parse().ok()?;
            let minute: u32 = parts[1].parse().ok()?;
            return NaiveTime::from_hms_opt(hour, minute, 0);
        }
        if parts.len() == 3 {
            let hour: u32 = parts[0].parse().ok()?;
            let minute: u32 = parts[1].parse().ok()?;
            let second: u32 = parts[2].parse().ok()?;
            return NaiveTime::from_hms_opt(hour, minute, second);
        }
        return None;
    }

    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        let digits = trimmed;
        if digits.len() == 3 || digits.len() == 4 {
            let (hour_str, min_str) = digits.split_at(digits.len() - 2);
            let hour: u32 = hour_str.parse().ok()?;
            let minute: u32 = min_str.parse().ok()?;
            return NaiveTime::from_hms_opt(hour, minute, 0);
        }
    }

    None
}

pub(crate) fn parse_duration_minutes(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut total = 0u32;
    let mut digits = String::new();
    let mut used_unit = false;

    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        let qty: u32 = digits.parse().ok()?;
        digits.clear();
        match ch {
            'h' | 'H' => {
                total = total.saturating_add(qty.saturating_mul(60));
                used_unit = true;
            }
            'm' | 'M' => {
                total = total.saturating_add(qty);
                used_unit = true;
            }
            _ => return None,
        }
    }

    if !digits.is_empty() {
        let qty: u32 = digits.parse().ok()?;
        if used_unit {
            total = total.saturating_add(qty);
        } else {
            total = qty;
        }
        used_unit = true;
    }

    if used_unit { Some(total) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    #[test]
    fn parses_at_tokens_and_strips() {
        let input = "Do stuff @sched(2025-01-15) @time(09:30) @dur(90m)";
        let (schedule, text) = parse_task_metadata(input);
        assert_eq!(text, "Do stuff");
        assert_eq!(schedule.scheduled, Some(NaiveDate::from_ymd_opt(2025, 1, 15).unwrap()));
        assert_eq!(schedule.time, Some(NaiveTime::from_hms_opt(9, 30, 0).unwrap()));
        assert_eq!(schedule.duration_minutes, Some(90));
    }

    #[test]
    fn parses_dataview_aliases() {
        let input = "Task due:: 2025-02-01 time:: 18:00";
        let (schedule, text) = parse_task_metadata(input);
        assert_eq!(text, "Task");
        assert_eq!(schedule.due, Some(NaiveDate::from_ymd_opt(2025, 2, 1).unwrap()));
        assert_eq!(schedule.time, Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap()));
    }

    #[test]
    fn invalid_tokens_remain() {
        let input = "Task @due(2025-99-01) something";
        let (schedule, text) = parse_task_metadata(input);
        assert_eq!(schedule.due, None);
        assert_eq!(text, "Task @due(2025-99-01) something");
    }

    #[test]
    fn upsert_replaces_existing_token() {
        let input = "Task @due(2025-01-01) notes";
        let updated = upsert_task_metadata_token(input, TaskMetadataKey::Due, "2025-01-02");
        assert_eq!(updated, "Task notes @due(2025-01-02)");
    }
}
