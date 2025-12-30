use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};

use crate::task_metadata::{parse_date, parse_duration_minutes, parse_time};

pub(crate) fn parse_relative_date_input(input: &str, base: NaiveDate) -> Option<NaiveDate> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(date) = parse_date(&trimmed) {
        return Some(date);
    }

    match trimmed.as_str() {
        "today" => return Some(base),
        "tomorrow" => return Some(base + Duration::days(1)),
        "yesterday" => return Some(base - Duration::days(1)),
        _ => {}
    }

    if let Some(date) = parse_relative_offset(&trimmed, base) {
        return Some(date);
    }

    if let Some(date) = parse_weekday_input(&trimmed, base) {
        return Some(date);
    }

    None
}

pub(crate) fn parse_time_input(input: &str) -> Option<NaiveTime> {
    parse_time(input)
}

pub(crate) fn parse_duration_input(input: &str) -> Option<u32> {
    parse_duration_minutes(input)
}

fn parse_relative_offset(input: &str, base: NaiveDate) -> Option<NaiveDate> {
    let mut chars = input.chars().peekable();
    let mut sign: i32 = 1;
    if let Some(&c) = chars.peek() {
        if c == '+' || c == '-' {
            if c == '-' {
                sign = -1;
            }
            chars.next();
        }
    }

    let mut digits = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            digits.push(c);
            chars.next();
        } else {
            break;
        }
    }

    if digits.is_empty() {
        return None;
    }

    let qty: i32 = digits.parse().ok()?;
    let unit = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    match unit {
        'd' => Some(base + Duration::days((sign * qty) as i64)),
        'w' => Some(base + Duration::weeks((sign * qty) as i64)),
        'm' => Some(add_months(base, sign * qty)),
        _ => None,
    }
}

fn parse_weekday_input(input: &str, base: NaiveDate) -> Option<NaiveDate> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    if parts.len() == 1 {
        let weekday = parse_weekday(parts[0])?;
        return Some(next_weekday(base, weekday, false));
    }

    if parts.len() == 2 && parts[0] == "next" {
        let weekday = parse_weekday(parts[1])?;
        return Some(next_weekday(base, weekday, true));
    }

    None
}

fn parse_weekday(token: &str) -> Option<Weekday> {
    let token = token.trim().to_lowercase();
    let token = token.as_str();
    if token.starts_with("mon") {
        Some(Weekday::Mon)
    } else if token.starts_with("tue") {
        Some(Weekday::Tue)
    } else if token.starts_with("wed") {
        Some(Weekday::Wed)
    } else if token.starts_with("thu") {
        Some(Weekday::Thu)
    } else if token.starts_with("fri") {
        Some(Weekday::Fri)
    } else if token.starts_with("sat") {
        Some(Weekday::Sat)
    } else if token.starts_with("sun") {
        Some(Weekday::Sun)
    } else {
        None
    }
}

fn next_weekday(base: NaiveDate, weekday: Weekday, force_next: bool) -> NaiveDate {
    let base_num = base.weekday().num_days_from_monday() as i32;
    let target_num = weekday.num_days_from_monday() as i32;
    let mut delta = (target_num - base_num + 7) % 7;
    if force_next && delta == 0 {
        delta = 7;
    }
    base + Duration::days(delta as i64)
}

fn add_months(base: NaiveDate, months: i32) -> NaiveDate {
    let total = base.year() * 12 + (base.month() as i32 - 1) + months;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12);
    let month = (month0 + 1) as u32;
    let last_day = last_day_of_month(year, month);
    let day = base.day().min(last_day);
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(base)
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year, month, 1).unwrap());
    let last = first_next - Duration::days(1);
    last.day()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keywords() {
        let base = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        assert_eq!(parse_relative_date_input("today", base), Some(base));
        assert_eq!(
            parse_relative_date_input("tomorrow", base),
            Some(base + Duration::days(1))
        );
        assert_eq!(
            parse_relative_date_input("yesterday", base),
            Some(base - Duration::days(1))
        );
    }

    #[test]
    fn parses_offsets() {
        let base = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        assert_eq!(
            parse_relative_date_input("+3d", base),
            Some(base + Duration::days(3))
        );
        assert_eq!(
            parse_relative_date_input("+2w", base),
            Some(base + Duration::weeks(2))
        );
        assert_eq!(
            parse_relative_date_input("-1w", base),
            Some(base - Duration::weeks(1))
        );
    }

    #[test]
    fn parses_next_weekday() {
        let base = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(); // Wed
        let next_mon = NaiveDate::from_ymd_opt(2025, 1, 20).unwrap();
        assert_eq!(parse_relative_date_input("mon", base), Some(next_mon));
        assert_eq!(parse_relative_date_input("next mon", base), Some(next_mon));
    }

    #[test]
    fn parses_explicit_date() {
        let base = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 2, 2).unwrap();
        assert_eq!(
            parse_relative_date_input("2025-02-02", base),
            Some(date)
        );
    }

    #[test]
    fn clamps_month_length() {
        let base = NaiveDate::from_ymd_opt(2025, 1, 31).unwrap();
        let expected = NaiveDate::from_ymd_opt(2025, 2, 28).unwrap();
        assert_eq!(parse_relative_date_input("+1m", base), Some(expected));
    }
}
