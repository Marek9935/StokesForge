//! **Le** lecteur et **le** formateur des dates (#218, #217 D1/D4) : une colonne
//! `ColumnType::DateTime` stocke un timestamp POSIX (secondes, UTC) ; la
//! grille l'affiche en ISO 8601 (`2026-10-10 14:30`) et l'édite sous cette
//! forme. Aucune dépendance de calendrier ni de locale.
//!
//! Formes lues à la saisie (séparateurs de date `-`, `/` ou `.`) :
//! - `AAAA-MM-JJ`, heure facultative (`T` ou espace, puis `HH:MM` ou
//!   `HH:MM:SS`, fraction de seconde admise) ;
//! - `JJ/MM/AAAA` et `JJ/MM/AA` : **jour d'abord**, jamais mois d'abord
//!   (`10/11/2026` = 10 novembre) ; `AA < 70` → 20AA, sinon 19AA ;
//! - une heure seule (`HH:MM[:SS]`), comptée depuis minuit le 1970-01-01.
//!
//! Tout le reste est `None` : la saisie est refusée ou promeut la colonne en
//! `Mixed`, jamais une date inventée. Le lecteur de `stokesforge-import` est
//! une copie volontaire (ce crate ne dépend d'aucun crate du workspace) ; leur
//! accord est gardé par un test de `stokesforge-importer`.

/// Timestamp POSIX (secondes, UTC) du texte, ou `None`.
pub fn parse_datetime(text: &str) -> Option<f64> {
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
        Some(date) => days_since_epoch(date)?,
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

fn days_since_epoch(date: &str) -> Option<i64> {
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
    let (year, month, day) = if fields[0].len() == 4 {
        (numbers[0], numbers[1], numbers[2])
    } else if fields[2].len() == 4 {
        (numbers[2], numbers[1], numbers[0])
    } else if fields[2].len() == 2 {
        let yy = numbers[2];
        (
            if yy < 70 { 2000 + yy } else { 1900 + yy },
            numbers[1],
            numbers[0],
        )
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

/// Jours depuis le 1970-01-01 (algorithme de Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Années représentables à l'affichage (ISO à quatre chiffres) ; au-delà, la
/// cellule retombe sur le nombre brut plutôt que sur une date fausse.
const MAX_YEAR: i64 = 9999;

/// Forme ISO 8601 d'un timestamp : `AAAA-MM-JJ HH:MM`, avec `:SS` si les
/// secondes ne sont pas nulles et `.fff` si une fraction subsiste. C'est la
/// forme d'affichage **et** d'édition : [`parse_datetime`] la relit à
/// l'identique (à la milliseconde). `None` pour un NaN, un infini ou une date
/// hors de l'an 0–9999.
pub fn format_datetime(timestamp: f64) -> Option<String> {
    if !timestamp.is_finite() {
        return None;
    }
    let millis = (timestamp * 1000.0).round();
    if millis.abs() > 1e15 {
        return None;
    }
    let total_millis = millis as i64;
    let days = total_millis.div_euclid(86_400_000);
    let of_day = total_millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    if !(0..=MAX_YEAR).contains(&year) {
        return None;
    }
    let hours = of_day / 3_600_000;
    let minutes = of_day / 60_000 % 60;
    let seconds = of_day / 1000 % 60;
    let fraction = of_day % 1000;
    let mut text = format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}");
    if seconds != 0 || fraction != 0 {
        text.push_str(&format!(":{seconds:02}"));
    }
    if fraction != 0 {
        let digits = format!("{fraction:03}");
        text.push('.');
        text.push_str(digits.trim_end_matches('0'));
    }
    Some(text)
}

/// Inverse de [`days_from_civil`] (algorithme de Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
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
        // L'exemple Jasco : 25/11/24 à 15:17:22.
        assert_eq!(parse_datetime("25/11/24 15:17:22"), Some(1_732_547_842.0));
        assert_eq!(parse_datetime("25.11.2024"), Some(1_732_492_800.0));
        assert_eq!(parse_datetime("15:17:22"), Some(55_042.0));
        assert_eq!(parse_datetime("1969-12-31"), Some(-86_400.0));
        assert_eq!(parse_datetime("2000-01-01 00:00:01,5"), Some(946_684_801.5));
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

    #[test]
    fn iso_display_and_round_trip() {
        use super::format_datetime;
        assert_eq!(format_datetime(0.0).as_deref(), Some("1970-01-01 00:00"));
        assert_eq!(
            format_datetime(946_684_800.0).as_deref(),
            Some("2000-01-01 00:00")
        );
        assert_eq!(
            format_datetime(1_732_547_842.0).as_deref(),
            Some("2024-11-25 15:17:22")
        );
        assert_eq!(
            format_datetime(-86_400.0).as_deref(),
            Some("1969-12-31 00:00")
        );
        assert_eq!(
            format_datetime(946_684_801.5).as_deref(),
            Some("2000-01-01 00:00:01.5")
        );
        for value in [
            0.0,
            -1.0,
            951_913_815.0,
            1_790_000_000.0,
            946_684_801.25,
            -2_000_000_000.0,
        ] {
            let text = format_datetime(value).unwrap();
            assert_eq!(parse_datetime(&text), Some(value), "{text}");
        }
    }

    #[test]
    fn day_first_slash_form_with_time() {
        // #218 : `10/10/2026 14:30` et l'ordre jour/mois.
        assert_eq!(
            parse_datetime("10/10/2026 14:30"),
            parse_datetime("2026-10-10 14:30")
        );
        assert_eq!(parse_datetime("10/11/2026"), parse_datetime("2026-11-10"));
        assert_eq!(parse_datetime("11/10/2026"), parse_datetime("2026-10-11"));
        assert_eq!(parse_datetime("13/01/2026"), parse_datetime("2026-01-13"));
        // Mois d'abord : refusé, jamais réinterprété.
        assert_eq!(parse_datetime("01/13/2026"), None);
    }

    #[test]
    fn display_refuses_what_is_not_a_date() {
        use super::format_datetime;
        assert_eq!(format_datetime(f64::NAN), None);
        assert_eq!(format_datetime(f64::INFINITY), None);
        assert_eq!(format_datetime(1e14), None);
        assert_eq!(format_datetime(-1e14), None);
    }
}
