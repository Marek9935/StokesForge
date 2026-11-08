//! Instrument fingerprints. The detector recognises the
//! software or device that wrote a file from signature lines in its first
//! lines (content only, never the extension), and reports it as data
//! ([`InstrumentMatch`]). The application turns it into a sentence.
//!
//! Pure, bounded: every scan stops after [`FINGERPRINT_LINES`] lines.

use crate::props::{norm_key, split_kv};
use crate::sniff::Family;

/// Lines (from the first non-blank one) the fingerprints look at.
pub const FINGERPRINT_LINES: usize = 60;
/// Lines the `n) TGA ...` segment titles of a PerkinElmer file are searched in.
const DEEP_LINES: usize = 400;
/// Longest signature kept, in characters.
const MAX_SIGNATURE_CHARS: usize = 80;
/// Fewest well labels that make a plate reader.
const MIN_WELLS: usize = 6;

/// The software or instrument family recognised in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    /// `ORIGIN= JASCO` or an `XYDATA` marker.
    Jasco,
    /// JCAMP-DX (`##KEY=VALUE` header).
    Jcamp,
    /// JCAMP-DX exported by Ocean Optics (`##ORIGIN= OCEANOPTICS EXPORT`).
    JcampOcean,
    /// Ocean Optics / Ocean Insight text export (SpectraSuite, OceanView,
    /// Jaz, OOIBase32).
    OceanOptics,
    /// Avantes AvaSoft 6/7/8 text export.
    Avantes,
    /// Netzsch thermal analysis export.
    Netzsch,
    /// PerkinElmer Pyris export.
    PerkinElmerPyris,
    /// TA Instruments export.
    TaInstruments,
    /// SPECS (SpecsLab2) XY export.
    Specs,
    /// CRAIC microspectrophotometer export.
    Craic,
    /// Plate reader export (well labels A1..H12).
    PlateReader,
}

/// A recognised instrument and the line that proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentMatch {
    pub instrument: Instrument,
    /// 1-based file line of the evidence.
    pub line: usize,
    /// The evidence line, trimmed and cut to 80 characters.
    pub signature: String,
}

fn found(instrument: Instrument, index: usize, line: &str) -> Option<InstrumentMatch> {
    Some(InstrumentMatch {
        instrument,
        line: index + 1,
        signature: line.trim().chars().take(MAX_SIGNATURE_CHARS).collect(),
    })
}

/// An Ocean Optics data-block marker: `>>>>>Begin Processed Spectral Data<<<<<`,
/// or the Spanish `>>>>> Comienza Data<<<<< Espectral Procesado Del EL`.
pub fn is_ocean_begin(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with(">>>>>") || !t.contains("<<<<<") {
        return false;
    }
    let word = t.trim_start_matches('>').trim_start().to_lowercase();
    word.starts_with("begin") || word.starts_with("comienza")
}

/// The matching end marker: `>>>>>End Processed Spectral Data<<<<<`.
pub fn is_ocean_end(line: &str) -> bool {
    let t = line.trim();
    t.starts_with(">>>>>")
        && t.contains("<<<<<")
        && t.trim_start_matches('>')
            .trim_start()
            .to_lowercase()
            .starts_with("end")
}

fn is_plus_line(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 20 && t.bytes().all(|b| b == b'+')
}

fn ocean(raw: &[&str], first: usize, end: usize) -> Option<InstrumentMatch> {
    if let Some(i) = (first..end).find(|&i| is_ocean_begin(raw[i])) {
        return found(Instrument::OceanOptics, i, raw[i]);
    }
    let banner = (first..end).find(|&i| {
        let t = raw[i].trim().to_lowercase();
        t.ends_with("data file") || t.ends_with("absolute irradiance file")
    })?;
    let corroborated = (first..end).any(|i| {
        is_plus_line(raw[i])
            || raw[i].trim_start().starts_with("Spectrometers:")
            || raw[i].trim_start().starts_with("Spectrometer:")
    });
    if corroborated {
        found(Instrument::OceanOptics, banner, raw[banner])
    } else {
        None
    }
}

fn starts_with_ci(line: &str, prefix: &str) -> bool {
    let t = line.trim_start();
    t.len() >= prefix.len()
        && t.is_char_boundary(prefix.len())
        && t[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn avantes(raw: &[&str], first: usize, end: usize) -> Option<InstrumentMatch> {
    let head = first..end.min(first + 8);
    let at = |prefix: &str| head.clone().find(|&i| starts_with_ci(raw[i], prefix));
    if let (Some(i), Some(_), Some(_)) = (
        at("integration time:"),
        at("average:"),
        at("nr of pixels used for smoothing:"),
    ) {
        return found(Instrument::Avantes, i, raw[i]);
    }
    if let (Some(i), Some(_)) = (at("integration time [ms]:"), at("averaging nr. [scans]:")) {
        return found(Instrument::Avantes, i, raw[i]);
    }
    // A `Wave ;...` names row followed by a `[nm] ;...` unit row.
    (first..end.saturating_sub(1)).find_map(|i| {
        let names = raw[i].trim();
        let units = raw[i + 1].trim();
        if starts_with_ci(names, "wave")
            && names.contains(';')
            && units.starts_with('[')
            && units.contains(';')
        {
            found(Instrument::Avantes, i, raw[i])
        } else {
            None
        }
    })
}

/// The first non-empty `;`-separated field after the key (the padded forms
/// `#FORMAT:      ;NETZSCH5      ;;;`).
fn kv_value(line: &str) -> Option<(String, String)> {
    let (key, rest) = split_kv(line)?;
    let value = rest.split(';').map(str::trim).find(|v| !v.is_empty())?;
    Some((norm_key(key), value.to_string()))
}

fn netzsch(raw: &[&str], first: usize, end: usize) -> Option<InstrumentMatch> {
    (first..end).find_map(|i| {
        let (key, value) = kv_value(raw[i])?;
        if matches!(key.as_str(), "FORMAT" | "INSTRUMENT")
            && value.to_uppercase().starts_with("NETZSCH")
        {
            found(Instrument::Netzsch, i, raw[i])
        } else {
            None
        }
    })
}

/// `1) TGA Isothermal`: a numbered segment title.
fn is_segment_title(line: &str) -> bool {
    let t = line.trim_start();
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && t[digits..].starts_with(')') && t[digits + 1..].trim_start().starts_with("TGA")
}

fn perkin_elmer(raw: &[&str], first: usize, end: usize) -> Option<InstrumentMatch> {
    if let Some(i) = (first..end).find(|&i| {
        starts_with_ci(raw[i], "filename:")
            && raw[i]
                .to_lowercase()
                .replace('/', "\\")
                .contains("\\perkinelmer\\pyris\\")
    }) {
        return found(Instrument::PerkinElmerPyris, i, raw[i]);
    }
    let weight = (first..end).find(|&i| starts_with_ci(raw[i], "sample weight:"))?;
    let deep = raw.len().min(first + DEEP_LINES);
    if (first..deep).any(|i| is_segment_title(raw[i])) {
        found(Instrument::PerkinElmerPyris, weight, raw[weight])
    } else {
        None
    }
}

fn ta_instruments(raw: &[&str], first: usize, end: usize) -> Option<InstrumentMatch> {
    let key_line = |key: &str| {
        (first..end).find(|&i| {
            let t = raw[i].trim_end();
            t.starts_with(key) && t[key.len()..].starts_with('\t')
        })
    };
    let instrument = key_line("Instrument")?;
    key_line("InstSerial")?;
    key_line("Nsig")?;
    found(Instrument::TaInstruments, instrument, raw[instrument])
}

fn specs(raw: &[&str], first: usize) -> Option<InstrumentMatch> {
    (first..raw.len().min(first + 3)).find_map(|i| {
        let t = raw[i].trim_start();
        let t = t.strip_prefix('#')?.trim_start();
        if starts_with_ci(t, "created by:") && t.to_lowercase().contains("specslab") {
            found(Instrument::Specs, i, raw[i])
        } else {
            None
        }
    })
}

fn craic(raw: &[&str], first: usize) -> Option<InstrumentMatch> {
    let i = first + 2;
    if i < raw.len() && raw[i].trim_start().starts_with("SciMode:") {
        found(Instrument::Craic, i, raw[i])
    } else {
        None
    }
}

/// `A1`..`P24`, zero-padded or not.
pub fn is_well_label(cell: &str) -> bool {
    let c = cell.trim();
    let mut chars = c.chars();
    let Some(letter) = chars.next() else {
        return false;
    };
    if !('A'..='P').contains(&letter) {
        return false;
    }
    let digits = chars.as_str();
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    matches!(digits.parse::<u32>(), Ok(1..=24))
}

/// `21s`, `0.5 min`, `3h`: a time label.
fn is_time_label(cell: &str) -> bool {
    let c = cell.trim();
    let digits = c
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'.')
        .count();
    digits > 0 && matches!(c[digits..].trim(), "s" | "min" | "h")
}

const PLATE_SEPARATORS: [char; 3] = [',', ';', '\t'];

fn plate_reader(raw: &[&str], first: usize, end: usize) -> Option<InstrumentMatch> {
    // A row of well labels (a names row), or well labels down the first column.
    let mut first_column: Vec<(char, usize)> = Vec::new();
    for (i, line) in raw.iter().enumerate().take(end).skip(first) {
        for sep in &PLATE_SEPARATORS {
            let cells: Vec<&str> = line.split(*sep).collect();
            let non_empty = cells.iter().filter(|c| !c.trim().is_empty()).count();
            let wells = cells.iter().filter(|c| is_well_label(c)).count();
            if wells >= MIN_WELLS && wells * 2 >= non_empty {
                return found(Instrument::PlateReader, i, line);
            }
            if cells.len() >= 2 && is_well_label(cells[0]) {
                first_column.push((*sep, i));
            }
        }
    }
    for sep in PLATE_SEPARATORS {
        let hits: Vec<usize> = first_column
            .iter()
            .filter(|(s, _)| *s == sep)
            .map(|&(_, i)| i)
            .collect();
        if hits.len() >= MIN_WELLS {
            return found(Instrument::PlateReader, hits[0], raw[hits[0]]);
        }
    }
    // Stacked matrices: a time label alone on its line (`21s,,,,`), repeated.
    let stamps: Vec<usize> = (first..end)
        .filter(|&i| {
            PLATE_SEPARATORS.iter().any(|sep| {
                let cells: Vec<&str> = raw[i].split(*sep).collect();
                cells.len() >= 3
                    && is_time_label(cells[0])
                    && cells[1..].iter().all(|c| c.trim().is_empty())
            })
        })
        .collect();
    if stamps.len() >= 3 {
        found(Instrument::PlateReader, stamps[0], raw[stamps[0]])
    } else {
        None
    }
}

fn jasco(raw: &[&str], first: usize) -> Option<InstrumentMatch> {
    let head = first..raw.len().min(first + 80);
    for i in head {
        if let Some((k, v)) = split_kv(raw[i]) {
            if k.eq_ignore_ascii_case("ORIGIN") && v.eq_ignore_ascii_case("JASCO") {
                return found(Instrument::Jasco, i, raw[i]);
            }
        }
    }
    let limit = raw.len().min(first + crate::sniff::MAX_SCAN_LINES);
    (first..limit)
        .find(|&i| raw[i].trim() == "XYDATA")
        .and_then(|i| found(Instrument::Jasco, i, raw[i]))
}

fn jcamp(raw: &[&str], first: usize) -> Option<InstrumentMatch> {
    let head = first..raw.len().min(first + 80);
    let vendor_ocean = head.clone().any(|i| {
        split_kv(raw[i]).is_some_and(|(k, v)| {
            k.eq_ignore_ascii_case("ORIGIN") && v.to_uppercase().contains("OCEANOPTICS")
        })
    });
    let at = head
        .clone()
        .find(|&i| raw[i].trim_start().starts_with("##"))
        .unwrap_or(first);
    let instrument = if vendor_ocean {
        Instrument::JcampOcean
    } else {
        Instrument::Jcamp
    };
    found(instrument, at, raw[at])
}

/// Recognises the instrument from the first lines of the decoded file.
/// `family` is the one the table detector found (its JASCO and JCAMP rules
/// are kept as they were); the other signatures are looked for in the first
/// [`FINGERPRINT_LINES`] lines.
pub(crate) fn detect(
    raw: &[&str],
    first: usize,
    family: Option<Family>,
) -> Option<InstrumentMatch> {
    match family {
        Some(Family::Jasco) => {
            if let Some(m) = jasco(raw, first) {
                return Some(m);
            }
        }
        Some(Family::Jcamp) => return jcamp(raw, first),
        _ => {}
    }
    let end = raw.len().min(first + FINGERPRINT_LINES);
    ocean(raw, first, end)
        .or_else(|| avantes(raw, first, end))
        .or_else(|| netzsch(raw, first, end))
        .or_else(|| perkin_elmer(raw, first, end))
        .or_else(|| ta_instruments(raw, first, end))
        .or_else(|| specs(raw, first))
        .or_else(|| craic(raw, first))
        .or_else(|| plate_reader(raw, first, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<&str> {
        crate::layout::split_lines(text)
    }

    fn detect_text(text: &str) -> Option<Instrument> {
        let raw = lines(text);
        let first = raw.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        detect(&raw, first, None).map(|m| m.instrument)
    }

    #[test]
    fn ocean_begin_marker_in_any_wording() {
        let t = "User: x\n>>>>>Begin Processed Spectral Data<<<<<\n200\t1\n";
        assert_eq!(detect_text(t), Some(Instrument::OceanOptics));
        let es = "Usuario: x\n>>>>> Comienza Data<<<<< Espectral Procesado Del EL\n200\t1\n";
        assert_eq!(detect_text(es), Some(Instrument::OceanOptics));
    }

    #[test]
    fn ocean_banner_needs_its_plus_line_or_spectrometer_key() {
        let plus = format!("Jaz Data File\n{}\nDate: x\n", "+".repeat(36));
        assert_eq!(detect_text(&plus), Some(Instrument::OceanOptics));
        let key = "SpectraSuite Data File\nDate: x\nSpectrometers: USB4A00428\n";
        assert_eq!(detect_text(key), Some(Instrument::OceanOptics));
        // A banner alone is just a title.
        assert_eq!(detect_text("My Data File\nDate: x\n1\t2\n"), None);
    }

    #[test]
    fn avantes_in_its_three_forms() {
        let v7 = "0804016U1- I C\nIntegration time: 100.00 ms\nAverage: 20 scans\nNr of pixels used for smoothing: 0\n";
        assert_eq!(detect_text(v7), Some(Instrument::Avantes));
        let v8 = "\nIntegration time [ms]:    6.00\nAveraging Nr. [scans]: 79\n";
        assert_eq!(detect_text(v8), Some(Instrument::Avantes));
        let names = "Wave   ;Dark     ;Ref\n[nm]   ;[counts] ;[counts]\n300.00;1;2\n";
        assert_eq!(detect_text(names), Some(Instrument::Avantes));
        // The two keys of a v7 file alone are not enough.
        assert_eq!(detect_text("Integration time: 1\nAverage: 2\n1;2\n"), None);
    }

    #[test]
    fn netzsch_plain_and_padded() {
        assert_eq!(
            detect_text("#FORMAT:NETZSCH5\n#DECIMAL:POINT\n"),
            Some(Instrument::Netzsch)
        );
        let padded =
            "#EXPORTTYPE:     ;DATA ALL    ;;;\n#INSTRUMENT:      ;NETZSCH STA 449F5   ;;;\n";
        assert_eq!(detect_text(padded), Some(Instrument::Netzsch));
        assert_eq!(detect_text("#FORMAT:CSV\n#INSTRUMENT:other\n"), None);
    }

    #[test]
    fn perkin_elmer_by_path_or_by_segment_titles() {
        let path =
            "Filename:\tC:\\Program Files\\PerkinElmer\\Pyris\\data\\a.tg8d \nSample ID:\tx\n";
        assert_eq!(detect_text(path), Some(Instrument::PerkinElmerPyris));
        let titles = "Sample Weight:\t7.0 mg\n\n1) TGA Isothermal \n20\t30\n";
        assert_eq!(detect_text(titles), Some(Instrument::PerkinElmerPyris));
        assert_eq!(detect_text("Sample Weight:\t7.0 mg\n1\t2\n"), None);
    }

    #[test]
    fn ta_specs_and_craic() {
        let ta = "Version\t2.0\nInstrument\tTGA Q500\nInstSerial\t0500\nNsig\t5\n";
        assert_eq!(detect_text(ta), Some(Instrument::TaInstruments));
        let specs = "# Created by:        SpecsLab2, Version 2.77\n# x\n1 2\n";
        assert_eq!(detect_text(specs), Some(Instrument::Specs));
        let craic = "Sample\nReflectance\nSciMode:IT1=448.99ms:NS1=10\n280.11\t13.4\n";
        assert_eq!(detect_text(craic), Some(Instrument::Craic));
        // TA needs the serial and signal count keys too.
        let ta_short = "Version\t2.0\nInstrument\tTGA Q500\nInstSerial\t0500\n";
        assert_eq!(detect_text(ta_short), None);
        assert_eq!(detect_text("a\nb\nSciMode is not first\n1\t2\n"), None);
    }

    #[test]
    fn plate_reader_by_names_row_or_first_column_or_stacked_times() {
        let names = "[nm],A1,B1,A2,B2,A3,B3\n200,1,2,3,4,5,6\n";
        assert_eq!(detect_text(names), Some(Instrument::PlateReader));
        let col = "[nm],200,201\nA1,1,2\nB1,1,2\nA2,1,2\nB2,1,2\nA3,1,2\nB3,1,2\n";
        assert_eq!(detect_text(col), Some(Instrument::PlateReader));
        let stacked = "r\n0s,,,\n1,2,3\n\n21s,,,\n1,2,3\n\n43s,,,\n1,2,3\n";
        assert_eq!(detect_text(stacked), Some(Instrument::PlateReader));
        // Five wells are not a plate; a mostly-text row is not a well row.
        assert_eq!(detect_text("A1,A2,A3,A4,A5\n1,2,3,4,5\n"), None);
        // Two stamped blocks are not a stack.
        assert_eq!(detect_text("r\n0s,,,\n1,2,3\n21s,,,\n1,2,3\n"), None);
        assert_eq!(detect_text("A1,B2,C3,D4,E5,F6,x,y,z,w,v,u,t\n1,2\n"), None);
    }

    #[test]
    fn a_plain_table_with_a_date_header_is_no_instrument() {
        assert_eq!(detect_text("Date: 2020-01-01\nUser: me\n1;2\n3;4\n"), None);
    }

    #[test]
    fn well_labels() {
        assert!(is_well_label("A1") && is_well_label("H12") && is_well_label("P24"));
        assert!(is_well_label("B03"));
        assert!(!is_well_label("Q1") && !is_well_label("A25") && !is_well_label("A0"));
        assert!(!is_well_label("A") && !is_well_label("AA1") && !is_well_label("A1x"));
    }

    #[test]
    fn ocean_end_marker() {
        assert!(is_ocean_end(">>>>>End Processed Spectral Data<<<<<"));
        assert!(!is_ocean_end(">>>>>Begin Spectral Data<<<<<"));
        assert!(!is_ocean_begin(">>>>>End Spectral Data<<<<<"));
    }
}
