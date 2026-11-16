//! Graph themes as records (#540): a theme is stored exactly like a graph
//! spec (`PlotSpecRecord`), with its name and its colours-only flag.
//!
//! Two uses share this codec: the `.stf` (`FileBody::project_themes`) and the
//! user's themes on this machine (#539), which store the bytes of
//! [`encode_themes`] wherever they like. Nothing here touches a file.
//!
//! Reading degrades (invariant 10): a theme whose record does not decode, or
//! whose name is empty, is dropped from a list; a list is cut at
//! [`MAX_THEMES`]; a name is cut at [`MAX_THEME_NAME_CHARS`].

use serde::{Deserialize, Serialize};
use stokesforge_plot::{GraphTheme, MAX_THEME_NAME_CHARS};

use crate::convert::{self, ConvertError};
use crate::schema::PlotSpecRecord;
use crate::{PersistenceError, PersistenceResult};

/// Largest byte string [`decode_themes`] and [`decode_theme`] accept.
pub const MAX_THEME_BYTES: usize = 16 * 1024 * 1024;

/// A theme, flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ThemeRecord {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) colors_only: bool,
    pub(crate) style: PlotSpecRecord,
}

fn corrupt(detail: String) -> PersistenceError {
    PersistenceError::Corrupt {
        path: "theme".into(),
        detail,
    }
}

pub(crate) fn theme_to_record(theme: &GraphTheme) -> ThemeRecord {
    ThemeRecord {
        name: theme.name.clone(),
        colors_only: theme.colors_only,
        style: convert::plot_spec_to_record(&theme.style),
    }
}

pub(crate) fn record_to_theme(record: ThemeRecord) -> Result<GraphTheme, ConvertError> {
    let name: String = record
        .name
        .trim()
        .chars()
        .take(MAX_THEME_NAME_CHARS)
        .collect();
    let style = convert::record_to_plot_spec(record.style)?;
    Ok(GraphTheme {
        name,
        style,
        colors_only: record.colors_only,
    })
}

/// The records of a list of themes, for the `.stf`.
pub(crate) fn themes_to_records(themes: &[GraphTheme]) -> Vec<ThemeRecord> {
    themes.iter().map(theme_to_record).collect()
}

/// The themes of a list of records: undecodable or nameless ones dropped,
/// the list bounded like any store of themes.
pub(crate) fn records_to_themes(records: Vec<ThemeRecord>) -> Vec<GraphTheme> {
    stokesforge_plot::bounded_themes(
        records
            .into_iter()
            .filter_map(|record| record_to_theme(record).ok())
            .collect(),
    )
}

fn to_bytes<T: Serialize>(value: &T) -> PersistenceResult<Vec<u8>> {
    let mut bytes = Vec::new();
    value
        .serialize(&mut rmp_serde::Serializer::new(&mut bytes).with_struct_map())
        .map_err(|err| corrupt(err.to_string()))?;
    Ok(bytes)
}

fn check_size(bytes: &[u8]) -> PersistenceResult<()> {
    if bytes.len() > MAX_THEME_BYTES {
        return Err(corrupt("theme data exceeds 16 MiB".into()));
    }
    Ok(())
}

/// One theme as bytes (MessagePack of its record).
pub fn encode_theme(theme: &GraphTheme) -> PersistenceResult<Vec<u8>> {
    to_bytes(&theme_to_record(theme))
}

/// One theme from [`encode_theme`] bytes. Any defect (not MessagePack, a
/// record that does not decode, an empty name, an out-of-bounds panel) is a
/// `Corrupt` error, never a panic. The name is cut at [`MAX_THEME_NAME_CHARS`].
pub fn decode_theme(bytes: &[u8]) -> PersistenceResult<GraphTheme> {
    check_size(bytes)?;
    let record: ThemeRecord =
        rmp_serde::from_slice(bytes).map_err(|err| corrupt(err.to_string()))?;
    let theme = record_to_theme(record).map_err(|err| corrupt(format!("{err:?}")))?;
    if theme.name.is_empty() {
        return Err(corrupt("theme without a name".into()));
    }
    Ok(theme)
}

/// A list of themes as bytes, in order.
pub fn encode_themes(themes: &[GraphTheme]) -> PersistenceResult<Vec<u8>> {
    to_bytes(&themes_to_records(themes))
}

/// A list of themes from [`encode_themes`] bytes. The bytes must be a valid
/// list (else `Corrupt`); inside it, a theme that does not decode is dropped
/// and the others are kept, the list is cut at [`MAX_THEMES`].
pub fn decode_themes(bytes: &[u8]) -> PersistenceResult<Vec<GraphTheme>> {
    check_size(bytes)?;
    let values: Vec<rmpv::Value> =
        rmp_serde::from_slice(bytes).map_err(|err| corrupt(err.to_string()))?;
    Ok(records_to_themes(values_to_records(values)))
}

/// The records a list of loose values holds: a value that is not a theme
/// record (a field of a type this reader does not know, a variant it has not
/// heard of) is dropped, the others stay. Goes through bytes: `rmpv`'s own
/// deserializer reads a unit variant written as a string differently from
/// `rmp_serde`.
fn values_to_records(values: Vec<rmpv::Value>) -> Vec<ThemeRecord> {
    values
        .into_iter()
        .filter_map(|value| {
            let mut bytes = Vec::new();
            rmpv::encode::write_value(&mut bytes, &value).ok()?;
            rmp_serde::from_slice::<ThemeRecord>(&bytes).ok()
        })
        .collect()
}

/// `deserialize_with` of `FileBody::project_themes`: one theme the reader
/// cannot decode (written by a newer version, or hostile) does not make the
/// project unreadable; it is dropped alone.
pub(crate) fn lenient_records<'de, D>(deserializer: D) -> Result<Vec<ThemeRecord>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<rmpv::Value>::deserialize(deserializer)?;
    Ok(values_to_records(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{PlotSpec, MAX_THEMES, THEMES};

    fn sample(name: &str) -> GraphTheme {
        let mut spec = PlotSpec::empty("T");
        spec.background = "#112233".into();
        GraphTheme::capture(name, &spec)
    }

    #[test]
    fn one_theme_round_trips() {
        let theme = sample("Mine");
        assert_eq!(decode_theme(&encode_theme(&theme).unwrap()).unwrap(), theme);
        let colors = GraphTheme::from_colors("Dark", &THEMES[1]);
        assert_eq!(
            decode_theme(&encode_theme(&colors).unwrap()).unwrap(),
            colors
        );
    }

    #[test]
    fn a_list_round_trips_in_order() {
        let list = vec![sample("b"), sample("a"), sample("c")];
        assert_eq!(decode_themes(&encode_themes(&list).unwrap()).unwrap(), list);
        assert!(decode_themes(&encode_themes(&[]).unwrap())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        for bytes in [
            &b""[..],
            b"\xc1",
            b"not msgpack at all",
            &[0x91, 0xc1],
            &[0xdd, 0xff, 0xff, 0xff, 0xff],
        ] {
            assert!(decode_theme(bytes).is_err());
            assert!(decode_themes(bytes).is_err());
        }
    }

    #[test]
    fn nameless_theme_is_refused_and_dropped_from_a_list() {
        let mut nameless = sample("x");
        nameless.name = "   ".into();
        assert!(decode_theme(&encode_theme(&nameless).unwrap()).is_err());
        let list = vec![sample("keep"), nameless];
        let back = decode_themes(&encode_themes(&list).unwrap()).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "keep");
    }

    #[test]
    fn a_hostile_record_in_a_list_is_dropped_the_others_stay() {
        let good = theme_to_record(&sample("good"));
        let mut bad = theme_to_record(&sample("bad"));
        bad.style.panels[0].row = u64::MAX;
        let bytes = to_bytes(&vec![good, bad]).unwrap();
        let back = decode_themes(&bytes).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "good");
    }

    #[test]
    fn huge_name_and_long_list_are_bounded() {
        let mut long = sample("n");
        long.name = "é".repeat(10_000);
        let back = decode_theme(&encode_theme(&long).unwrap()).unwrap();
        assert_eq!(back.name.chars().count(), MAX_THEME_NAME_CHARS);
        let list: Vec<GraphTheme> = (0..MAX_THEMES + 50)
            .map(|i| sample(&format!("t{i}")))
            .collect();
        assert_eq!(
            decode_themes(&encode_themes(&list).unwrap()).unwrap().len(),
            MAX_THEMES
        );
    }
}
