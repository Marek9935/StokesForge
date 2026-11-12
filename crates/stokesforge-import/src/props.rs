//! `KEY<sep>VALUE` lines of the header and the footer that
//! fill the Long Name, Units or Comments of the columns ([`HeaderProp`]).

use crate::recipe::{HeaderProp, PropField, PropTarget};

/// Splits a `KEY;VALUE`, `KEY<tab>VALUE`, `KEY=VALUE`, `KEY: VALUE` or
/// `##KEY=VALUE` line at the first of `; tab = :`. `None` without a key.
pub fn split_kv(line: &str) -> Option<(&str, &str)> {
    let text = line.trim_start().trim_start_matches('#');
    let pos = text.find([';', '\t', '=', ':'])?;
    let blank = |c: char| c.is_whitespace() || c == '\0';
    let key = text[..pos].trim_matches(blank);
    if key.is_empty() {
        return None;
    }
    Some((key, text[pos + 1..].trim_matches(blank)))
}

/// Key as compared: surrounding spaces and leading `#` removed, upper case.
pub fn norm_key(key: &str) -> String {
    key.trim().trim_start_matches('#').trim().to_uppercase()
}

/// `Wavelength [nm]` gives `("Wavelength", Some("nm"))`; `Abs (a.u.)` too.
pub fn split_name_unit(value: &str) -> (String, Option<String>) {
    let value = value.trim();
    for (open, close) in [('[', ']'), ('(', ')')] {
        if let (Some(a), Some(b)) = (value.find(open), value.find(close)) {
            if a < b {
                return (
                    value[..a].trim().to_string(),
                    Some(value[a + 1..b].trim().to_string()),
                );
            }
        }
    }
    (value.to_string(), None)
}

/// `Temp./°C` gives `("Temp.", Some("°C"))`, `DSC/(mW/mg)` gives
/// `("DSC", Some("mW/mg"))`: split at the first `/` that has a name before it
/// and a unit after it; leading `#`s are dropped and the outer parentheses of
/// the unit too. A token without such a `/` is read by [`split_name_unit`].
pub fn split_name_unit_slash(value: &str) -> (String, Option<String>) {
    let value = value.trim().trim_start_matches('#').trim();
    if let Some(pos) = value.find('/') {
        let name = value[..pos].trim();
        let unit = value[pos + 1..].trim();
        let unit = unit
            .strip_prefix('(')
            .and_then(|u| u.strip_suffix(')'))
            .map_or(unit, str::trim);
        if !name.is_empty() && !unit.is_empty() {
            return (name.to_string(), Some(unit.to_string()));
        }
    }
    split_name_unit(value)
}

/// `[nm]` gives `nm`; a token without square brackets is returned trimmed.
pub fn strip_square_brackets(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .map_or(value, str::trim)
        .to_string()
}

/// `NANOMETERS` -> `nm`, `1/CM` -> `cm⁻¹`; anything else is kept as is.
pub fn short_unit(value: &str) -> String {
    let upper = value.trim().to_uppercase();
    match upper.as_str() {
        "NANOMETERS" | "NANOMETER" => "nm",
        "MICROMETERS" | "MICROMETER" => "µm",
        "1/CM" => "cm⁻¹",
        "SECONDS" => "s",
        "MINUTES" => "min",
        "ABSORBANCE" => "Abs",
        _ => return value.trim().to_string(),
    }
    .to_string()
}

/// The text a property puts in its field, from the raw value of its line.
pub fn prop_text(prop: &HeaderProp, value: &str) -> Option<String> {
    let text = match prop.field {
        PropField::LongName => split_name_unit(value).0,
        PropField::Units => {
            let (whole, bracket) = split_name_unit(value);
            let unit = bracket.unwrap_or(whole);
            if prop.short_unit {
                short_unit(&unit)
            } else {
                unit
            }
        }
        PropField::Comment => value.trim().to_string(),
    };
    (!text.is_empty()).then_some(text)
}

/// Does the property apply to column `index`?
pub fn applies_to(target: PropTarget, index: usize) -> bool {
    match target {
        PropTarget::X => index == 0,
        PropTarget::Y => index > 0,
        PropTarget::All => true,
    }
}

/// First value of `key` among `lines`.
pub fn find_value<'a>(lines: impl Iterator<Item = &'a str>, key: &str) -> Option<String> {
    let wanted = norm_key(key);
    lines.into_iter().find_map(|line| {
        let (k, v) = split_kv(line)?;
        (norm_key(k) == wanted).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_names_split_at_the_first_slash_with_a_unit() {
        let split = |s: &str| split_name_unit_slash(s);
        assert_eq!(
            split("##Temp./\u{b0}C"),
            ("Temp.".into(), Some("\u{b0}C".into()))
        );
        assert_eq!(split("DSC/(mW/mg)"), ("DSC".into(), Some("mW/mg".into())));
        assert_eq!(split("Mass/%"), ("Mass".into(), Some("%".into())));
        // No unit after the slash, or no name before it: nothing to split.
        assert_eq!(split("Time/"), ("Time/".into(), None));
        assert_eq!(split("/min"), ("/min".into(), None));
        // Without a slash the bracket reading applies.
        assert_eq!(split("Abs [a.u.]"), ("Abs".into(), Some("a.u.".into())));
    }

    #[test]
    fn square_brackets_come_off_the_units() {
        assert_eq!(strip_square_brackets("[nm]"), "nm");
        assert_eq!(
            strip_square_brackets(" [% transmittance] "),
            "% transmittance"
        );
        assert_eq!(strip_square_brackets("nm"), "nm");
        assert_eq!(strip_square_brackets("[nm"), "[nm");
    }
}
