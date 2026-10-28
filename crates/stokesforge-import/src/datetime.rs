//! Reading of dates and times into POSIX timestamps (seconds), the
//! representation of `stokesforge_core::ColumnType::DateTime`. No dependency
//! on a calendar crate: a few common instrument formats.
//!
//! Recognized formats (date separators `-`, `/` or `.`):
//! - `YYYY-MM-DD`, with an optional time (`T` or space, then `HH:MM` or
//!   `HH:MM:SS`, fractional seconds allowed);
//! - `DD/MM/YYYY` and `DD/MM/YY` (day first by default; `YY < 70` →
//!   20YY, otherwise 19YY), same optional time. With
//!   [`DateOrder::MonthFirst`] (the recipe's `date_order` setting) the first
//!   two fields are swapped: `03/04/2026` is then March 4. A month outside
//!   1..=12 after the swap gives `None`, never a guessed date. The ISO form
//!   `YYYY-MM-DD` does not depend on the order. The `stokesforge-core` crate,
//!   for its part, keeps day first (#217).
//! - a time alone (`HH:MM[:SS]`), counted from midnight on 1970-01-01.
//!
//! Everything else is `None`: the cell stays text, never an invented date.

use crate::recipe::DateOrder;

/// POSIX timestamp (seconds, UTC) of the text, day first, or `None`.
pub fn parse_datetime(text: &str) -> Option<f64> {
    parse_datetime_with(text, DateOrder::DayFirst)
}

/// Like [`parse_datetime`], with the recipe's day/month order.
pub fn parse_datetime_with(text: &str, order: DateOrder) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (date_part, time_part) = match text.split_once(['T', ' ']) {
        Some((date, time)) => (Some(date.trim()), Some(time.trim())),
        None if text.contains(':') => (None, Some(text)),
        None => (Some(text), None),
    };
    let days = match date_part {
        Some(date) => days_since_epoch(date, order)?,
        None => 0,
    };
    let seconds = match time_part {
        Some(time) if !time.is_empty() => seconds_of_day(time)?,
        Some(_) => return None,
        None => 0.0,
    };
    Some(days as f64 * 86_400.0 + seconds)
}

fn seconds_of_day(time: &str) -> Option<f64> {
    let time = time.trim_end_matches('Z');
    let mut parts = time.split(':');
    let hours: u32 = parts.next()?.parse().ok()?;
    let minutes: u32 = parts.next()?.parse().ok()?;
    let seconds: f64 = match parts.next() {
        Some(part) => part.replace(',', ".").parse().ok()?,
        None => 0.0,
    };
    if parts.next().is_some() || hours > 23 || minutes > 59 || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    Some(f64::from(hours) * 3600.0 + f64::from(minutes) * 60.0 + seconds)
}

fn days_since_epoch(date: &str, order: DateOrder) -> Option<i64> {
    let fields: Vec<&str> = date.split(['-', '/', '.']).collect();
    if fields.len() != 3
        || fields
            .iter()
            .any(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let numbers: Vec<i64> = fields
        .iter()
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    // The first two fields of a date ending with the year: (day, month) or
    // (month, day) depending on the recipe.
    let (first, second) = match order {
        DateOrder::DayFirst => (numbers[0], numbers[1]),
        DateOrder::MonthFirst => (numbers[1], numbers[0]),
    };
    let (year, month, day) = if fields[0].len() == 4 {
        (numbers[0], numbers[1], numbers[2])
    } else if fields[2].len() == 4 {
        (numbers[2], second, first)
    } else if fields[2].len() == 2 {
        let yy = numbers[2];
        (if yy < 70 { 2000 + yy } else { 1900 + yy }, second, first)
    } else {
        return None;
    };
    if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::parse_datetime;

    #[test]
    fn known_timestamps() {
        assert_eq!(parse_datetime("1970-01-01"), Some(0.0));
        assert_eq!(parse_datetime("2000-01-01 00:00:00"), Some(946_684_800.0));
        assert_eq!(parse_datetime("2000-03-01T12:30:15"), Some(951_913_815.0));
        assert_eq!(parse_datetime("01/01/2000"), Some(946_684_800.0));
        // The Jasco example: 25/11/24 at 15:17:22.
        assert_eq!(parse_datetime("25/11/24 15:17:22"), Some(1_732_547_842.0));
        assert_eq!(parse_datetime("25.11.2024"), Some(1_732_492_800.0));
        assert_eq!(parse_datetime("15:17:22"), Some(55_042.0));
        assert_eq!(parse_datetime("1969-12-31"), Some(-86_400.0));
        assert_eq!(parse_datetime("2000-01-01 00:00:01,5"), Some(946_684_801.5));
    }

    #[test]
    fn month_first_swaps_day_and_month_but_not_iso() {
        use super::parse_datetime_with;
        use crate::recipe::DateOrder::{DayFirst, MonthFirst};
        // March 4, 2026 month first; April 3, 2026 day first.
        assert_eq!(
            parse_datetime_with("03/04/2026", MonthFirst),
            parse_datetime_with("2026-03-04", DayFirst)
        );
        assert_eq!(
            parse_datetime_with("03/04/2026", DayFirst),
            parse_datetime_with("2026-04-03", MonthFirst)
        );
        assert_ne!(
            parse_datetime_with("03/04/2026", MonthFirst),
            parse_datetime_with("03/04/2026", DayFirst)
        );
        // Month 13: not a date.
        assert_eq!(parse_datetime_with("13/01/2026", MonthFirst), None);
        assert!(parse_datetime_with("13/01/2026", DayFirst).is_some());
        // Two-digit year, with time.
        assert_eq!(
            parse_datetime_with("11/25/24 15:17:22", MonthFirst),
            Some(1_732_547_842.0)
        );
        // ISO: identical in both orders.
        assert_eq!(
            parse_datetime_with("2026-03-04 10:00", MonthFirst),
            parse_datetime_with("2026-03-04 10:00", DayFirst)
        );
    }

    #[test]
    fn leap_years_and_bad_dates() {
        assert!(parse_datetime("2024-02-29").is_some());
        assert!(parse_datetime("2023-02-29").is_none());
        assert!(parse_datetime("1900-02-29").is_none());
        assert!(parse_datetime("2000-02-29").is_some());
        for bad in [
            "",
            "abc",
            "2024-13-01",
            "2024-00-10",
            "31/04/2024",
            "12:61",
            "24:00",
            "1.5",
            "2024-01",
        ] {
            assert_eq!(parse_datetime(bad), None, "{bad:?}");
        }
    }
}
