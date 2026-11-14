//! JCAMP-DX as a self-describing format Pure and without
//! sentences: it recognises a JCAMP-DX file ([`looks_jcamp`]) and reads its
//! numeric tables ([`read`]) into [`JcampBlock`]s, the problems it met being
//! data ([`JcampIssue`]).
//!
//! What it reads: `(X++(Y..Y))` tables in every ASDF form (AFFN, PAC, SQZ,
//! DIF, DUP, with the DIF Y-check), `(XY..XY)` pairs (`XYPOINTS`, `PEAK TABLE`,
//! also with a decimal comma), `LINK` files and repeated `TITLE` blocks
//! (flattened), and the simple `(X++(R..R))` pages of `NTUPLES`. Blocks
//! without a numeric table (structures, assignments) are skipped.
//!
//! Bounded: the points of a file are capped ([`MAX_POINTS`]), a DUP count
//! ([`MAX_DUP`]) and the number of blocks ([`MAX_BLOCKS`]). A malformed line
//! degrades (it is counted in an issue), never panics.

use crate::layout::split_lines;

/// Most points (x and y pairs) kept over a whole file.
pub const MAX_POINTS: usize = 20_000_000;
/// Most repetitions one DUP letter may produce.
pub const MAX_DUP: u64 = 1_000_000;
/// Most blocks read from a file.
pub const MAX_BLOCKS: usize = 10_000;
/// Lines searched for a data-table label by [`looks_jcamp`] (Bruker puts
/// thousands of parameter lines before the first table).
const RECOGNISE_LINES: usize = 20_000;
/// Non-blank lines in which [`looks_jcamp`] expects the first record.
const TITLE_WITHIN: usize = 20;
/// Header lines and characters of a value kept per block.
const MAX_HEADER_LINES: usize = 5000;
const MAX_VALUE_CHARS: usize = 4096;

/// How the numbers of a table are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableForm {
    /// `(X++(Y..Y))`: regular grid, X checkpoint then a list of Y.
    XppYY,
    /// `(XY..XY)`: pairs, one or several per line.
    XYXY,
    /// A page of an `NTUPLES` section.
    NTuples,
}

/// A problem met while reading a block, as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JcampIssue {
    /// `NPOINTS` is not what was decoded (the decoded count is kept).
    NPointsMismatch { declared: i64, decoded: usize },
    /// An `NTUPLES` table this reader does not decode (2D pages, peaks).
    NtuplesNotRead,
    /// A data table in a form this reader does not know.
    UnreadableTable,
    /// Data lines that could not be decoded; they are skipped.
    BadLines { count: usize },
    /// A DUP count above [`MAX_DUP`] was cut.
    DupCapped,
    /// The points of the file reached [`MAX_POINTS`]; the rest is dropped.
    PointsCapped,
    /// NMR blocks of a file that also holds other spectra were left out
    /// (out of scope); the count of blocks left out.
    NmrLeftOut { blocks: usize },
}

/// One spectrum (or table) of a JCAMP-DX file.
#[derive(Debug, Clone, PartialEq)]
pub struct JcampBlock {
    pub title: String,
    pub data_type: Option<String>,
    /// Raw text, `$$` comment removed.
    pub xunits: Option<String>,
    pub yunits: Option<String>,
    /// `NPOINTS` as declared (not trusted).
    pub npoints: Option<i64>,
    pub form: TableForm,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// 1-based file lines of the data table, both included (0 when none).
    pub first_line: usize,
    pub last_line: usize,
    pub issues: Vec<JcampIssue>,
    /// `LABEL=value` records of the block, `##` and `$$` comments removed.
    pub header: Vec<String>,
}

impl JcampBlock {
    /// Number of decoded points.
    pub fn points(&self) -> usize {
        self.y.len().min(self.x.len())
    }
}

/// What the reader found, for the detector's report.
#[derive(Debug, Clone, PartialEq)]
pub struct JcampSummary {
    /// Blocks that hold points (the importer makes one sheet each).
    pub blocks: usize,
    /// Layout of the first block's table.
    pub form: TableForm,
    /// The issues of every block, in file order (at most 50).
    pub issues: Vec<JcampIssue>,
}

/// A read file: its blocks in file order (those holding points, and those
/// whose table could not be read, which carry an issue and no points).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JcampFile {
    pub blocks: Vec<JcampBlock>,
    /// More than [`MAX_BLOCKS`] blocks: the rest is not read.
    pub blocks_capped: bool,
    /// NMR blocks (out of scope) that were left out of `blocks`.
    pub nmr_blocks: usize,
}

// -- recognising --------------------------------------------------------------

/// Label as compared: spaces, `_`, `-`, `/` removed, upper case.
fn norm_label(raw: &str) -> String {
    raw.chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '_' | '-' | '/'))
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// `(normalised label, raw value)` of a `##LABEL=value` line (leading spaces
/// allowed), `None` for any other line.
pub(crate) fn record_of(line: &str) -> Option<(String, &str)> {
    let t = line.trim_start().trim_start_matches('\u{feff}');
    let t = t.strip_prefix("##")?;
    let (label, value) = match t.find('=') {
        Some(p) => (&t[..p], &t[p + 1..]),
        None => (t, ""),
    };
    Some((norm_label(label), value))
}

fn is_table_label(label: &str) -> bool {
    matches!(
        label,
        "XYDATA" | "XYPOINTS" | "PEAKTABLE" | "DATATABLE" | "NTUPLES"
    )
}

/// Is this a JCAMP-DX file: its first record is `##TITLE=` (within the first
/// 20 non-blank lines) and a data-table label (`XYDATA`, `XYPOINTS`,
/// `PEAK TABLE`, `DATA TABLE`, `NTUPLES`) appears in its first 20 000 lines. A
/// `##` header followed by a plain table is not.
pub fn looks_jcamp(text: &str) -> bool {
    let text = text.trim_start_matches('\u{feff}');
    if !starts_with_title(text) {
        return false;
    }
    text.split(['\n', '\r'])
        .take(RECOGNISE_LINES * 2)
        .filter(|l| !l.is_empty())
        .take(RECOGNISE_LINES)
        .any(|l| record_of(l).is_some_and(|(label, _)| is_table_label(&label)))
}

/// Does the first record (within the first 20 non-blank lines) read `##TITLE=`,
/// with no table label required.
pub fn starts_with_title(text: &str) -> bool {
    let text = text.trim_start_matches('\u{feff}');
    let mut nonblank = 0usize;
    for line in text.split(['\n', '\r']) {
        if line.trim().is_empty() {
            continue;
        }
        nonblank += 1;
        if let Some((label, _)) = record_of(line) {
            return label == "TITLE";
        }
        if nonblank >= TITLE_WITHIN {
            break;
        }
    }
    false
}

/// Does this record mark NMR data: a `DATA TYPE` or `NTUPLES` value naming
/// NMR, or an `.OBSERVE FREQUENCY` / `.OBSERVE NUCLEUS` label.
fn is_nmr_record(label: &str, value: &str) -> bool {
    match label {
        "DATATYPE" | "NTUPLES" => strip_comment(value).to_ascii_uppercase().contains("NMR"),
        ".OBSERVEFREQUENCY" | ".OBSERVENUCLEUS" => true,
        _ => false,
    }
}

/// Does a JCAMP-DX text hold NMR data (labels only, first 20 000 lines).
pub fn mentions_nmr(text: &str) -> bool {
    text.split(['\n', '\r'])
        .filter(|l| !l.is_empty())
        .take(RECOGNISE_LINES)
        .any(|l| record_of(l).is_some_and(|(label, value)| is_nmr_record(&label, value)))
}

// -- numbers ------------------------------------------------------------------

/// Header number: a decimal comma is accepted (`176,36`), `$$` comment removed.
fn header_number(value: &str) -> Option<f64> {
    let v = strip_comment(value).trim();
    if v.is_empty() {
        return None;
    }
    let n = match v.parse::<f64>() {
        Ok(n) => n,
        Err(_) => v.replace(',', ".").parse::<f64>().ok()?,
    };
    n.is_finite().then_some(n)
}

fn strip_comment(s: &str) -> &str {
    s.find("$$").map_or(s, |p| &s[..p])
}

/// Digits `first` then `rest` (digits, maybe a dot) as a number, with `sign`.
fn compose(sign: f64, first: u8, rest: &str) -> f64 {
    let mut value = f64::from(first - b'0');
    if rest.len() <= 14 && rest.bytes().all(|b| b.is_ascii_digit()) {
        for b in rest.bytes() {
            value = value * 10.0 + f64::from(b - b'0');
        }
        return sign * value;
    }
    let text = format!("{}{rest}", first as char);
    sign * text.parse::<f64>().unwrap_or(f64::NAN)
}

// -- ASDF decoding ------------------------------------------------------------

#[derive(Clone, Copy)]
enum Ev {
    Abs(f64),
    Dif(f64),
    Dup(u64),
}

/// SQZ letter: `(sign, digit)`.
fn sqz(c: u8) -> Option<(f64, u8)> {
    match c {
        b'@' => Some((1.0, b'0')),
        b'A'..=b'I' => Some((1.0, b'1' + (c - b'A'))),
        b'a'..=b'i' => Some((-1.0, b'1' + (c - b'a'))),
        _ => None,
    }
}

/// DIF letter: `(sign, digit)`.
fn dif(c: u8) -> Option<(f64, u8)> {
    match c {
        b'%' => Some((1.0, b'0')),
        b'J'..=b'R' => Some((1.0, b'1' + (c - b'J'))),
        b'j'..=b'r' => Some((-1.0, b'1' + (c - b'j'))),
        _ => None,
    }
}

/// DUP letter: its first digit.
fn dup(c: u8) -> Option<u8> {
    match c {
        b'S'..=b'Z' => Some(b'1' + (c - b'S')),
        b's' => Some(b'9'),
        _ => None,
    }
}

/// Next number event of `b` from `*i`. `None` at the end of the line.
fn next_event(line: &str, i: &mut usize, checkpoint: bool) -> Option<Ev> {
    let b = line.as_bytes();
    // Skip separators and any byte that cannot start a value.
    while *i < b.len() {
        let c = b[*i];
        let starts = c.is_ascii_digit()
            || matches!(c, b'.' | b'+' | b'-' | b'?')
            || sqz(c).is_some()
            || dif(c).is_some()
            || dup(c).is_some();
        if starts {
            break;
        }
        *i += 1;
    }
    if *i >= b.len() {
        return None;
    }
    let c = b[*i];
    if c == b'?' {
        *i += 1;
        return Some(Ev::Abs(f64::NAN));
    }
    if c.is_ascii_digit() || c == b'.' || c == b'+' || c == b'-' {
        // AFFN or PAC: an exponent marker is part of the number.
        let start = *i;
        let mut j = start + 1;
        if (c == b'+' || c == b'-') && !b.get(j).is_some_and(|d| d.is_ascii_digit() || *d == b'.') {
            *i = j;
            return next_event(line, i, checkpoint);
        }
        while j < b.len() {
            let d = b[j];
            if d.is_ascii_digit() || d == b'.' {
                j += 1;
            } else if matches!(d, b'e' | b'E')
                && (b[j - 1].is_ascii_digit() || b[j - 1] == b'.')
                && match b.get(j + 1) {
                    // At the X checkpoint `16345e944` is 16345 then a squeezed
                    // value: only a signed exponent counts there.
                    Some(n) if n.is_ascii_digit() => !checkpoint,
                    Some(b'+' | b'-') => b.get(j + 2).is_some_and(u8::is_ascii_digit),
                    _ => false,
                }
            {
                j += 2;
            } else {
                break;
            }
        }
        *i = j;
        return Some(Ev::Abs(line[start..j].parse::<f64>().unwrap_or(f64::NAN)));
    }
    // A letter form: the letter replaces the first digit.
    let start = *i + 1;
    let mut j = start;
    while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
        j += 1;
    }
    *i = j;
    if let Some((sign, digit)) = sqz(c) {
        return Some(Ev::Abs(compose(sign, digit, &line[start..j])));
    }
    if let Some((sign, digit)) = dif(c) {
        return Some(Ev::Dif(compose(sign, digit, &line[start..j])));
    }
    let digit = dup(c).unwrap_or(b'1');
    let mut count = u64::from(digit - b'0');
    for d in line[start..j].bytes().filter(u8::is_ascii_digit) {
        count = count.saturating_mul(10).saturating_add(u64::from(d - b'0'));
    }
    Some(Ev::Dup(count))
}

/// State kept from one data line to the next.
#[derive(Default)]
struct XppState {
    prev: Option<f64>,
    last_is_dif: bool,
    last_diff: f64,
}

/// Counters of what went wrong while decoding a table.
#[derive(Default)]
struct Tally {
    bad_lines: usize,
    dup_capped: bool,
    points_capped: bool,
}

/// Decodes one `(X++(Y..Y))` line: pushes the raw Y values to `ys` and
/// returns the X checkpoint. `None` when the line holds no usable checkpoint.
fn decode_xpp_line(
    line: &str,
    st: &mut XppState,
    ys: &mut Vec<f64>,
    room: &mut usize,
    tally: &mut Tally,
) -> Option<f64> {
    let mut i = 0usize;
    let checkpoint = match next_event(line, &mut i, true)? {
        Ev::Abs(v) => v,
        _ => return None,
    };
    // After a line that ended in DIF form, the first Y repeats the last one.
    let mut check_pending = st.last_is_dif;
    while let Some(ev) = next_event(line, &mut i, false) {
        let first = std::mem::take(&mut check_pending);
        match ev {
            Ev::Abs(v) => {
                if first {
                    continue;
                }
                if *room == 0 {
                    tally.points_capped = true;
                    break;
                }
                *room -= 1;
                ys.push(v);
                st.prev = Some(v);
                st.last_is_dif = false;
            }
            Ev::Dif(d) => {
                let Some(prev) = st.prev else {
                    tally.bad_lines += 1;
                    continue;
                };
                if *room == 0 {
                    tally.points_capped = true;
                    break;
                }
                *room -= 1;
                let v = prev + d;
                ys.push(v);
                st.prev = Some(v);
                st.last_is_dif = true;
                st.last_diff = d;
            }
            Ev::Dup(n) => {
                let Some(mut prev) = st.prev else {
                    tally.bad_lines += 1;
                    continue;
                };
                let mut extra = n.saturating_sub(1);
                if extra > MAX_DUP {
                    extra = MAX_DUP;
                    tally.dup_capped = true;
                }
                while extra > 0 {
                    if *room == 0 {
                        tally.points_capped = true;
                        break;
                    }
                    *room -= 1;
                    extra -= 1;
                    if st.last_is_dif {
                        prev += st.last_diff;
                    }
                    ys.push(prev);
                }
                st.prev = Some(prev);
            }
        }
        if tally.points_capped {
            break;
        }
    }
    Some(checkpoint)
}

// -- (XY..XY) -----------------------------------------------------------------

/// A comma between two digits.
fn has_digit_comma(line: &str) -> bool {
    let b = line.as_bytes();
    (1..b.len().saturating_sub(1))
        .any(|k| b[k] == b',' && b[k - 1].is_ascii_digit() && b[k + 1].is_ascii_digit())
}

/// Do the table lines use a decimal comma (`176,36, 32822,795`)?
fn decimal_comma_table(lines: &[(usize, &str)]) -> bool {
    lines
        .iter()
        .take(50)
        .any(|(_, l)| l.contains(", ") && has_digit_comma(l))
}

fn parse_pairs(
    lines: &[(usize, &str)],
    comma_decimal: bool,
    room: &mut usize,
    tally: &mut Tally,
) -> (Vec<f64>, Vec<f64>) {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let mut values: Vec<f64> = Vec::new();
    for (_, line) in lines {
        values.clear();
        let mut bad = false;
        let mut take = |token: &str| {
            if token.is_empty() {
                return;
            }
            let parsed = if comma_decimal {
                token.replace(',', ".").parse::<f64>()
            } else {
                token.parse::<f64>()
            };
            match parsed {
                Ok(v) => values.push(v),
                Err(_) => bad = true,
            }
        };
        if comma_decimal {
            line.split(|c: char| c.is_whitespace() || c == ';')
                .for_each(|t| take(t.trim_end_matches(',')));
        } else {
            line.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
                .for_each(&mut take);
        }
        if bad || values.is_empty() || !values.len().is_multiple_of(2) {
            tally.bad_lines += 1;
            continue;
        }
        for pair in values.chunks(2) {
            if *room == 0 {
                tally.points_capped = true;
                return (xs, ys);
            }
            *room -= 1;
            xs.push(pair[0]);
            ys.push(pair[1]);
        }
    }
    (xs, ys)
}

// -- building blocks ----------------------------------------------------------

/// X axis of an `(X++(Y..Y))` table of `m` points, scaled by `xfactor`.
/// `marks` are the line checkpoints with the index of the first point of each.
fn build_x(
    m: usize,
    firstx: Option<f64>,
    lastx: Option<f64>,
    deltax: Option<f64>,
    xfactor: f64,
    marks: &[(f64, usize)],
) -> Vec<f64> {
    if m == 0 {
        return Vec::new();
    }
    if let (Some(f), Some(l)) = (firstx, lastx) {
        if m == 1 {
            return vec![f];
        }
        if f != l || deltax.is_none_or(|d| d == 0.0) {
            let step = (l - f) / (m - 1) as f64;
            return (0..m).map(|i| f + i as f64 * step).collect();
        }
    }
    if let Some(d) = deltax.filter(|d| *d != 0.0) {
        let f = firstx
            .or_else(|| marks.first().map(|(cp, _)| cp * xfactor))
            .unwrap_or(0.0);
        return (0..m).map(|i| f + i as f64 * d).collect();
    }
    if marks.is_empty() {
        return (0..m).map(|i| i as f64).collect();
    }
    // Only the checkpoints: interpolate inside each line.
    let mut x = Vec::with_capacity(m);
    let mut last_step = 0.0;
    for (k, &(cp, start)) in marks.iter().enumerate() {
        let end = marks.get(k + 1).map_or(m, |&(_, s)| s.min(m));
        let count = end.saturating_sub(start);
        let cp = cp * xfactor;
        if let Some(&(next, _)) = marks.get(k + 1) {
            if count > 0 {
                last_step = (next * xfactor - cp) / count as f64;
            }
        }
        for j in 0..count {
            x.push(cp + j as f64 * last_step);
        }
    }
    x.resize(m, f64::NAN);
    x
}

fn factor_of(f: Option<f64>) -> f64 {
    f.filter(|v| *v != 0.0 && v.is_finite()).unwrap_or(1.0)
}

/// A data-table record waiting to be decoded.
struct RawTable {
    label: String,
    /// The form text, upper case, without spaces.
    form: String,
    first_line: usize,
    last_line: usize,
    /// Data lines, `$$` comment removed and trimmed; 1-based line numbers.
    lines: Vec<(usize, String)>,
    /// `NPOINTS` and `PAGE` records seen just before the table (NTUPLES).
    npoints: Option<f64>,
    page: Option<String>,
}

/// The comma-separated fields of an `NTUPLES` record, by position.
fn fields(value: &str) -> Vec<String> {
    strip_comment(value)
        .split(',')
        .take(64)
        .map(|f| f.trim().to_string())
        .collect()
}

#[derive(Default)]
struct NTuples {
    names: Vec<String>,
    symbols: Vec<String>,
    units: Vec<String>,
    dims: Vec<String>,
    first: Vec<String>,
    last: Vec<String>,
    factor: Vec<String>,
}

/// One block under construction.
#[derive(Default)]
struct Builder {
    title: String,
    data_type: Option<String>,
    xunits: Option<String>,
    yunits: Option<String>,
    xfactor: Option<f64>,
    yfactor: Option<f64>,
    firstx: Option<f64>,
    lastx: Option<f64>,
    deltax: Option<f64>,
    npoints: Option<f64>,
    header: Vec<String>,
    tables: Vec<RawTable>,
    nt: Option<NTuples>,
    page: Option<String>,
    page_npoints: Option<f64>,
    /// An NMR block: left out.
    nmr: bool,
}

fn text_of(value: &str) -> Option<String> {
    let v = strip_comment(value).trim();
    (!v.is_empty()).then(|| v.chars().take(MAX_VALUE_CHARS).collect())
}

/// `(sym++(sym..sym))` -> the two symbols; `None` for anything else (2D
/// `PROFILE` pages, peaks).
fn xpp_symbols(form: &str) -> Option<(String, String)> {
    if form.contains("PROFILE") {
        return None;
    }
    let open = form.find('(')?;
    let plus = form.find("++(")?;
    let dots = form.find("..")?;
    let x = &form[open + 1..plus];
    let y = &form[plus + 3..dots];
    (!x.is_empty() && !y.is_empty()).then(|| (x.to_string(), y.to_string()))
}

impl Builder {
    /// Decodes the block, or counts it as left out when it is NMR.
    fn flush(self, room: &mut usize, file: &mut JcampFile) {
        if self.nmr {
            file.nmr_blocks += 1;
        } else {
            self.decode_all(room, &mut file.blocks);
        }
    }

    fn decode_all(self, room: &mut usize, out: &mut Vec<JcampBlock>) {
        let Builder {
            title,
            data_type,
            xunits,
            yunits,
            xfactor,
            yfactor,
            firstx,
            lastx,
            deltax,
            npoints,
            header,
            tables,
            nt,
            ..
        } = self;
        let declared = npoints.filter(|n| n.is_finite()).map(|n| n.round() as i64);
        // A 2D file has hundreds of pages this reader skips: one note is enough.
        let mut not_read_pushed = false;
        for (table_index, table) in tables.into_iter().enumerate() {
            if out.len() >= MAX_BLOCKS {
                return;
            }
            let mut block = JcampBlock {
                title: title.clone(),
                data_type: data_type.clone(),
                xunits: xunits.clone(),
                yunits: yunits.clone(),
                npoints: declared,
                form: TableForm::XppYY,
                x: Vec::new(),
                y: Vec::new(),
                first_line: table.first_line,
                last_line: table.last_line,
                issues: Vec::new(),
                // The header is shared by the first tables only (a 2D file
                // has a thousand pages).
                header: if table_index < 8 {
                    header.clone()
                } else {
                    Vec::new()
                },
            };
            let lines: Vec<(usize, &str)> =
                table.lines.iter().map(|(n, l)| (*n, l.as_str())).collect();
            let mut tally = Tally::default();
            let mut xf = factor_of(xfactor);
            let mut yf = factor_of(yfactor);
            let (mut fx, mut lx, mut dx) = (firstx, lastx, deltax);
            if nt.is_some() && table.label == "DATATABLE" {
                block.form = TableForm::NTuples;
                let pairs = table.form.contains("(XY..XY)");
                let symbols = if pairs {
                    Some(("X".to_string(), "Y".to_string()))
                } else {
                    xpp_symbols(&table.form)
                };
                let pos = |nt: &NTuples, s: &str| {
                    nt.symbols.iter().position(|t| t.eq_ignore_ascii_case(s))
                };
                let found = nt
                    .as_ref()
                    .zip(symbols)
                    .and_then(|(nt, (xs, ys))| Some((nt, pos(nt, &xs)?, pos(nt, &ys)?, ys)));
                let Some((nt, xi, yi, ys)) = found else {
                    if !not_read_pushed {
                        not_read_pushed = true;
                        block.issues.push(JcampIssue::NtuplesNotRead);
                        out.push(block);
                    }
                    continue;
                };
                let num = |v: &[String], i: usize| v.get(i).and_then(|s| header_number(s));
                xf = factor_of(num(&nt.factor, xi));
                yf = factor_of(num(&nt.factor, yi));
                fx = num(&nt.first, xi);
                lx = num(&nt.last, xi);
                dx = None;
                block.xunits = nt.units.get(xi).filter(|s| !s.is_empty()).cloned();
                block.yunits = nt.units.get(yi).filter(|s| !s.is_empty()).cloned();
                block.npoints = table
                    .npoints
                    .or_else(|| num(&nt.dims, xi))
                    .filter(|n| n.is_finite())
                    .map(|n| n.round() as i64);
                let name = nt
                    .names
                    .get(yi)
                    .filter(|s| !s.is_empty())
                    .map_or(ys, Clone::clone);
                block.title = match (&table.page, pairs) {
                    (Some(page), true) => format!("{title} ({name}, {page})"),
                    _ => format!("{title} ({name})"),
                };
                if pairs {
                    let comma = decimal_comma_table(&lines);
                    let (xs, ys) = parse_pairs(&lines, comma, room, &mut tally);
                    block.x = xs.into_iter().map(|v| v * xf).collect();
                    block.y = ys.into_iter().map(|v| v * yf).collect();
                } else {
                    decode_xpp(&lines, &mut block, (xf, yf), (fx, lx, dx), room, &mut tally);
                }
            } else if table.form.contains("++") && !table.form.contains("PROFILE") {
                decode_xpp(&lines, &mut block, (xf, yf), (fx, lx, dx), room, &mut tally);
            } else if table.form.contains("(XY..XY)") {
                block.form = TableForm::XYXY;
                let comma = decimal_comma_table(&lines);
                let (xs, ys) = parse_pairs(&lines, comma, room, &mut tally);
                block.x = xs.into_iter().map(|v| v * xf).collect();
                block.y = ys.into_iter().map(|v| v * yf).collect();
            } else {
                block.issues.push(JcampIssue::UnreadableTable);
                out.push(block);
                continue;
            }
            if let Some(d) = block.npoints {
                if d != block.points() as i64 {
                    block.issues.push(JcampIssue::NPointsMismatch {
                        declared: d,
                        decoded: block.points(),
                    });
                }
            }
            if tally.bad_lines > 0 {
                block.issues.push(JcampIssue::BadLines {
                    count: tally.bad_lines,
                });
            }
            if tally.dup_capped {
                block.issues.push(JcampIssue::DupCapped);
            }
            if tally.points_capped {
                block.issues.push(JcampIssue::PointsCapped);
            }
            out.push(block);
        }
    }
}

type Scales = (f64, f64);
type Range = (Option<f64>, Option<f64>, Option<f64>);

fn decode_xpp(
    lines: &[(usize, &str)],
    block: &mut JcampBlock,
    (xf, yf): Scales,
    (fx, lx, dx): Range,
    room: &mut usize,
    tally: &mut Tally,
) {
    let mut st = XppState::default();
    let mut ys: Vec<f64> = Vec::new();
    let mut marks: Vec<(f64, usize)> = Vec::new();
    for (_, line) in lines {
        let before = ys.len();
        match decode_xpp_line(line, &mut st, &mut ys, room, tally) {
            Some(cp) => marks.push((cp, before)),
            None => tally.bad_lines += 1,
        }
        if tally.points_capped {
            break;
        }
    }
    block.x = build_x(ys.len(), fx, lx, dx, xf, &marks);
    block.y = ys.into_iter().map(|v| v * yf).collect();
}

// -- reading ------------------------------------------------------------------

/// Reads every numeric table of a JCAMP-DX text. Never panics; a text that is
/// not JCAMP gives no block.
pub fn read(text: &str) -> JcampFile {
    let lines = split_lines(text);
    let mut file = JcampFile::default();
    let mut room = MAX_POINTS;
    let mut builder: Option<Builder> = None;
    // The record being read.
    struct Rec {
        label: String,
        raw: String,
        value: String,
        table: Option<RawTable>,
    }
    let mut rec: Option<Rec> = None;

    let finish = |rec: Option<Rec>,
                  builder: &mut Option<Builder>,
                  file: &mut JcampFile,
                  room: &mut usize| {
        let Some(r) = rec else { return };
        match r.label.as_str() {
            "TITLE" => {
                if let Some(b) = builder.take() {
                    b.flush(room, file);
                }
                let mut b = Builder {
                    title: text_of(&r.value).unwrap_or_default(),
                    ..Builder::default()
                };
                b.header.push(format!("TITLE={}", b.title));
                *builder = Some(b);
            }
            "END" => {
                if let Some(b) = builder.take() {
                    b.flush(room, file);
                }
            }
            label => {
                let Some(b) = builder.as_mut() else { return };
                if let Some(mut t) = r.table {
                    t.npoints = b.page_npoints;
                    t.page = b.page.clone();
                    b.tables.push(t);
                    return;
                }
                let v = &r.value;
                b.nmr |= is_nmr_record(label, v);
                match label {
                    "DATATYPE" => b.data_type = b.data_type.take().or_else(|| text_of(v)),
                    "XUNITS" => b.xunits = b.xunits.take().or_else(|| text_of(v)),
                    "YUNITS" => b.yunits = b.yunits.take().or_else(|| text_of(v)),
                    "XFACTOR" => b.xfactor = b.xfactor.or_else(|| header_number(v)),
                    "YFACTOR" => b.yfactor = b.yfactor.or_else(|| header_number(v)),
                    "FIRSTX" => b.firstx = b.firstx.or_else(|| header_number(v)),
                    "LASTX" => b.lastx = b.lastx.or_else(|| header_number(v)),
                    "DELTAX" => b.deltax = b.deltax.or_else(|| header_number(v)),
                    "NPOINTS" => {
                        b.npoints = b.npoints.or_else(|| header_number(v));
                        b.page_npoints = header_number(v);
                    }
                    "PAGE" => {
                        b.page = text_of(v);
                        b.page_npoints = None;
                    }
                    _ => {}
                }
                if b.nt.is_some() || label == "NTUPLES" {
                    let nt = b.nt.get_or_insert_with(NTuples::default);
                    match label {
                        "VARNAME" => nt.names = fields(v),
                        "SYMBOL" => nt.symbols = fields(v),
                        "UNITS" => nt.units = fields(v),
                        "VARDIM" => nt.dims = fields(v),
                        "FIRST" => nt.first = fields(v),
                        "LAST" => nt.last = fields(v),
                        "FACTOR" => nt.factor = fields(v),
                        _ => {}
                    }
                }
                if b.header.len() < MAX_HEADER_LINES && label != "PAGE" {
                    let value: String = strip_comment(v)
                        .trim()
                        .chars()
                        .take(MAX_VALUE_CHARS)
                        .collect();
                    b.header.push(format!("{}={}", r.raw.trim(), value));
                }
            }
        }
    };

    for (idx, line) in lines.iter().enumerate() {
        if file.blocks.len() >= MAX_BLOCKS {
            file.blocks_capped = true;
            break;
        }
        let line_no = idx + 1;
        if let Some((label, value)) = record_of(line) {
            finish(rec.take(), &mut builder, &mut file, &mut room);
            let t = line.trim_start().trim_start_matches('\u{feff}');
            let raw = t[2..].split('=').next().unwrap_or("").to_string();
            let table =
                (is_table_label(&label) && label != "NTUPLES" && builder.is_some()).then(|| {
                    RawTable {
                        label: label.clone(),
                        form: strip_comment(value)
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .map(|c| c.to_ascii_uppercase())
                            .collect(),
                        first_line: line_no + 1,
                        last_line: line_no,
                        lines: Vec::new(),
                        npoints: None,
                        page: None,
                    }
                });
            rec = Some(Rec {
                label,
                raw,
                value: value.to_string(),
                table,
            });
        } else if let Some(r) = rec.as_mut() {
            if let Some(t) = r.table.as_mut() {
                let data = strip_comment(line).trim();
                if !data.is_empty() {
                    t.lines.push((line_no, data.to_string()));
                    t.last_line = line_no;
                }
            } else if r.value.len() < MAX_VALUE_CHARS {
                // A continuation line belongs to the value (not to a table).
                let extra = strip_comment(line).trim();
                if !extra.is_empty() {
                    r.value.push(' ');
                    r.value.push_str(extra);
                }
            }
        }
    }
    finish(rec.take(), &mut builder, &mut file, &mut room);
    if let Some(b) = builder.take() {
        b.flush(&mut room, &mut file);
    }
    file.blocks.truncate(MAX_BLOCKS);
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes the Y values of one `(X++(Y..Y))` data line (after `X`).
    fn ys(line: &str) -> Vec<f64> {
        let mut st = XppState::default();
        let mut out = Vec::new();
        let mut room = 1000;
        let mut tally = Tally::default();
        decode_xpp_line(line, &mut st, &mut out, &mut room, &mut tally);
        out
    }

    fn jcamp(header: &str, form: &str, table: &str) -> String {
        format!("##TITLE=t\n##JCAMP-DX=4.24\n{header}##{form}\n{table}\n##END=\n")
    }

    fn first(text: &str) -> JcampBlock {
        read(text).blocks.remove(0)
    }

    #[test]
    fn every_encoding_of_one_line_gives_the_same_values() {
        let want = vec![99.0, 98.0, 97.0, 96.0, 98.0, 93.0];
        // Checkpoint 1 then the values, in DIF, DIF+DUP, AFFN and PAC.
        assert_eq!(ys("1 99I8I7I6I8I3"), want);
        assert_eq!(ys("1 99jjjKn"), want);
        assert_eq!(ys("1 99jUKn"), want);
        assert_eq!(ys("1 99,98,97,96,98,93"), want);
        assert_eq!(ys("1 99+98+97+96+98+93"), want);
    }

    #[test]
    fn dif_dup_after_a_dif_repeats_the_difference() {
        let want = vec![1000.0, 2000.0, 2001.0, 2002.0, 2003.0, 2003.0, 2003.0];
        assert_eq!(ys("1 A000J000JJJ%%"), want);
        assert_eq!(ys("1 A000J000JU%%"), want);
    }

    #[test]
    fn multi_digit_dup_counts_and_squeezed_values() {
        // T1 = 21 occurrences of A (=1).
        let v = ys("1 AT1");
        assert_eq!(v.len(), 21);
        assert!(v.iter().all(|&y| y == 1.0));
        assert_eq!(ys("1 b21"), vec![-221.0]);
        assert_eq!(ys("1 A000"), vec![1000.0]);
    }

    #[test]
    fn affn_exponents_are_not_squeeze_letters() {
        assert_eq!(ys("1 4.5e+02 1E3 2.5e-1"), vec![450.0, 1000.0, 0.25]);
    }

    #[test]
    fn dup_count_is_bounded() {
        let v = ys("1 AT999999999");
        // The cap of one DUP, and the test room of 1000 points.
        assert!(v.len() <= 1001);
        let mut st = XppState::default();
        let mut out = Vec::new();
        let mut room = 5_000_000;
        let mut tally = Tally::default();
        decode_xpp_line("1 AT999999999", &mut st, &mut out, &mut room, &mut tally);
        assert_eq!(out.len() as u64, MAX_DUP + 1);
        assert!(tally.dup_capped);
    }

    #[test]
    fn dif_y_check_is_dropped_on_the_next_line() {
        // Line 1 ends in DIF form (100, 101, 102); line 2 starts with the
        // check value 102, which is not a new point.
        let text = jcamp(
            "##FIRSTX=0\n##LASTX=5\n##NPOINTS=6\n",
            "XYDATA=(X++(Y..Y))",
            "0 A00JJ\n3 A02JJJ",
        );
        let b = first(&text);
        assert_eq!(b.y, vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0]);
        assert!(b.issues.is_empty(), "{:?}", b.issues);
    }

    #[test]
    fn a_squeezed_line_has_no_y_check() {
        let text = jcamp(
            "##FIRSTX=0\n##LASTX=3\n##NPOINTS=4\n",
            "XYDATA=(X++(Y..Y))",
            "0 A1 A2\n2 A3 A4",
        );
        assert_eq!(first(&text).y, vec![11.0, 12.0, 13.0, 14.0]);
    }

    #[test]
    fn x_is_rebuilt_from_first_last_and_the_decoded_count() {
        let b = first(&jcamp(
            "##FIRSTX=10\n##LASTX=15\n##NPOINTS=6\n",
            "XYDATA=(X++(Y..Y))",
            "10 AX",
        ));
        assert_eq!(b.x, vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);
        assert_eq!(b.y, vec![1.0; 6]);
        let b = first(&jcamp(
            "##FIRSTX=10\n##LASTX=15\n##NPOINTS=6\n",
            "XYDATA=(X++(Y..Y))",
            "10 aX",
        ));
        assert_eq!(b.y, vec![-1.0; 6]);
    }

    #[test]
    fn factors_scale_and_missing_factors_mean_one() {
        let b = first(&jcamp(
            "##FIRSTX=0\n##LASTX=2\n##XFACTOR=0\n##YFACTOR=0.5\n",
            "XYDATA=(X++(Y..Y))",
            "0 2 4 6",
        ));
        assert_eq!(b.y, vec![1.0, 2.0, 3.0]);
        assert_eq!(b.x, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn x_comes_from_deltax_or_the_checkpoints_when_first_last_are_missing() {
        let b = first(&jcamp(
            "##FIRSTX=400\n##DELTAX=2\n",
            "XYDATA=(X++(Y..Y))",
            "400 1 2 3",
        ));
        assert_eq!(b.x, vec![400.0, 402.0, 404.0]);
        let b = first(&jcamp(
            "##XFACTOR=0.5\n",
            "XYDATA=(X++(Y..Y))",
            "10 1 2\n14 3 4",
        ));
        assert_eq!(b.x, vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn npoints_is_not_trusted_and_a_mismatch_is_an_issue() {
        // Off by one (Varian): the decoded count wins.
        let b = first(&jcamp(
            "##FIRSTX=0\n##LASTX=3\n##NPOINTS=5.000000\n",
            "XYDATA=(X++(Y..Y))",
            "0 1 2 3 4",
        ));
        assert_eq!(b.points(), 4);
        assert_eq!(b.x, vec![0.0, 1.0, 2.0, 3.0]);
        assert_eq!(
            b.issues,
            vec![JcampIssue::NPointsMismatch {
                declared: 5,
                decoded: 4
            }]
        );
    }

    #[test]
    fn pac_and_dollar_comments_on_data_lines() {
        let b = first(&jcamp(
            "##FIRSTX=0\n##LASTX=3\n##NPOINTS=4\n",
            "XYDATA=(X++(Y..Y))",
            "0 +10-20 $$ a comment 99\n2 +30+40",
        ));
        assert_eq!(b.y, vec![10.0, -20.0, 30.0, 40.0]);
        assert_eq!((b.first_line, b.last_line), (7, 8));
    }

    #[test]
    fn leading_spaces_before_the_labels_and_cr_only_line_ends() {
        let text = " ##TITLE=t\r ##JCAMP-DX=4.24\r ##FIRSTX=0\r ##LASTX=2\r ##XYDATA=(X++(Y..Y))\r 0 1 2 3\r ##END=\r";
        assert!(looks_jcamp(text));
        let b = first(text);
        assert_eq!(b.y, vec![1.0, 2.0, 3.0]);
        assert_eq!((b.first_line, b.last_line), (6, 6));
    }

    #[test]
    fn a_missing_closing_paren_still_reads() {
        let b = first(&jcamp(
            "##FIRSTX=0\n##LASTX=1\n",
            "XYDATA= (X++(Y..Y)",
            "0 5 6",
        ));
        assert_eq!(b.y, vec![5.0, 6.0]);
    }

    #[test]
    fn xy_pairs_with_several_pairs_per_line() {
        let b = first(&jcamp(
            "##XFACTOR=1\n",
            "PEAK TABLE=(XY..XY)",
            "50, 2.52; 51, 9.32;\n52 1.5",
        ));
        assert_eq!(b.form, TableForm::XYXY);
        assert_eq!(b.x, vec![50.0, 51.0, 52.0]);
        assert_eq!(b.y, vec![2.52, 9.32, 1.5]);
    }

    #[test]
    fn xy_pairs_are_scaled_by_the_factors() {
        let b = first(&jcamp(
            "##XFACTOR=2\n##YFACTOR=10\n",
            "XYPOINTS=(XY..XY)",
            "1,1 2,3",
        ));
        assert_eq!(b.x, vec![2.0, 4.0]);
        assert_eq!(b.y, vec![10.0, 30.0]);
    }

    #[test]
    fn decimal_comma_pairs_as_ocean_writes_them() {
        let b = first(&jcamp(
            "##FIRSTX= 176,36\n##LASTX= 893,69\n##NPOINTS=2\n",
            "XYDATA=(XY..XY)",
            "176,36, 32822,795\n177,02, 100,5",
        ));
        assert_eq!(b.x, vec![176.36, 177.02]);
        assert_eq!(b.y, vec![32822.795, 100.5]);
        assert!(b.issues.is_empty());
    }

    #[test]
    fn a_link_of_two_blocks_gives_two_blocks_and_skips_the_structure() {
        let text = "##TITLE=link\n##DATA TYPE=LINK\n##BLOCKS=3\n\
            ##TITLE=a\n##XYDATA=(X++(Y..Y))\n##FIRSTX=0\n##LASTX=1\n0 1 2\n##END=\n\
            ##TITLE=struct\n##JCAMP-CS=3.7\n##ATOMLIST=(1..3)\n1 C\n##END=\n\
            ##TITLE=b\n##FIRSTX=0\n##LASTX=2\n##XYDATA=(X++(Y..Y))\n0 3 4 5\n##END=\n##END=\n";
        assert!(looks_jcamp(text));
        let f = read(text);
        assert_eq!(f.blocks.len(), 2);
        assert_eq!(f.blocks[0].title, "a");
        assert_eq!(f.blocks[1].y, vec![3.0, 4.0, 5.0]);
        assert_eq!(f.blocks[1].x, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn repeated_titles_without_a_link_are_flattened() {
        let text = "##TITLE=a\n##PEAK TABLE=(XY..XY)\n1 2\n##END=\n##TITLE=b\n##PEAK TABLE=(XY..XY)\n3 4\n##END=\n";
        let f = read(text);
        assert_eq!(
            f.blocks
                .iter()
                .map(|b| b.title.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn ntuples_pages_are_read_with_their_factors() {
        let text = "##TITLE=nmr\n##DATA TYPE=RAMAN SPECTRUM\n##NTUPLES=RAMAN SPECTRUM\n\
            ##VAR_NAME= FREQUENCY, REAL, IMAGINARY\n##SYMBOL= X, R, I\n\
            ##UNITS= HZ, ARBITRARY UNITS, ARBITRARY UNITS\n##VAR_DIM= 3, 3, 3\n\
            ##FACTOR= 1, 2, 3\n##FIRST= 100, 1, 1\n##LAST= 102, 1, 1\n\
            ##PAGE=N=1\n##DATA TABLE=(X++(R..R)), XYDATA\n0 1 2 3\n\
            ##PAGE=N=2\n##DATA TABLE=(X++(I..I)), XYDATA\n0 1 1 1\n##END NTUPLES=RAMAN SPECTRUM\n##END=\n";
        assert!(looks_jcamp(text));
        let f = read(text);
        assert_eq!(f.blocks.len(), 2);
        let b = &f.blocks[0];
        assert_eq!(b.form, TableForm::NTuples);
        assert_eq!(b.x, vec![100.0, 101.0, 102.0]);
        assert_eq!(b.y, vec![2.0, 4.0, 6.0]);
        assert_eq!(b.xunits.as_deref(), Some("HZ"));
        assert_eq!(b.title, "nmr (REAL)");
        assert_eq!(f.blocks[1].y, vec![3.0, 3.0, 3.0]);
    }

    #[test]
    fn a_two_dimensional_ntuples_page_is_reported_not_read() {
        let text = "##TITLE=2d\n##NTUPLES=nD RAMAN SPECTRUM\n##SYMBOL= F1, F2, Y\n\
            ##DATA TABLE=(F2++(Y..Y)), PROFILE\n0 1 2\n##END=\n";
        let f = read(text);
        assert_eq!(f.blocks.len(), 1);
        assert!(f.blocks[0].x.is_empty());
        assert_eq!(f.blocks[0].issues, vec![JcampIssue::NtuplesNotRead]);
    }

    #[test]
    fn an_unknown_table_form_is_reported() {
        let f = read("##TITLE=t\n##PEAK TABLE=(XYW..XYW)\n1 2 3\n##END=\n");
        assert_eq!(f.blocks[0].issues, vec![JcampIssue::UnreadableTable]);
    }

    #[test]
    fn garbage_and_truncated_tables_do_not_panic() {
        let texts = [
            "##TITLE=t\n##XYDATA=(X++(Y..Y))\n\u{0}\u{1}é€ ???\n@@@ %%% TTT\n",
            "##TITLE=t\n##XYDATA=(X++(Y..Y))\n5 J\n6 T9\n",
            "##TITLE=t\n##PEAK TABLE=(XY..XY)\n1 2 3\nabc\n,,,;;;\n",
            "##TITLE=t\n##XYDATA=(X++(Y..Y))",
            "##TITLE",
            "##",
            "",
        ];
        for t in texts {
            let _ = looks_jcamp(t);
            let f = read(t);
            for b in &f.blocks {
                assert_eq!(b.x.len(), b.y.len());
            }
        }
        let b = first("##TITLE=t\n##PEAK TABLE=(XY..XY)\n1 2 3\n4 5\n");
        assert_eq!((b.x.clone(), b.y.clone()), (vec![4.0], vec![5.0]));
        assert_eq!(b.issues, vec![JcampIssue::BadLines { count: 1 }]);
    }

    #[test]
    fn recognition_needs_the_title_first_and_a_table_label() {
        // The tuned look-alike: a `##` header and a plain table.
        assert!(!looks_jcamp(
            "##TITLE=x\n##XUNITS=1/CM\n401.1 91.3\n405.0 90.8\n"
        ));
        // Table label but another first record.
        assert!(!looks_jcamp(
            "##JCAMP-DX=4.24\n##TITLE=x\n##XYDATA=(X++(Y..Y))\n"
        ));
        assert!(looks_jcamp("##TITLE=x\n##XYDATA=(X++(Y..Y))\n1 2\n"));
        // Labels are normalised: `x_y data` is `XYDATA`.
        assert!(looks_jcamp("##TITLE=x\n##x_y data=(X++(Y..Y))\n"));
        assert!(looks_jcamp("##TITLE=x\n##PEAK TABLE=(XY..XY)\n"));
        assert!(looks_jcamp("##TITLE=x\n##NTUPLES=NMR SPECTRUM\n"));
        assert!(!looks_jcamp(""));
    }

    #[test]
    fn a_table_label_far_below_the_head_is_not_searched() {
        let mut t = String::from("##TITLE=x\n");
        for _ in 0..20_100 {
            t.push_str("1 2\n");
        }
        t.push_str("##XYDATA=(X++(Y..Y))\n");
        assert!(!looks_jcamp(&t));
    }

    #[test]
    fn points_are_capped_per_file() {
        let mut st = XppState::default();
        let mut out = Vec::new();
        let mut room = 3;
        let mut tally = Tally::default();
        decode_xpp_line("1 1 2 3 4 5 6", &mut st, &mut out, &mut room, &mut tally);
        assert_eq!(out.len(), 3);
        assert!(tally.points_capped);
    }

    // -- through sniff and parse ------------------------------------------

    use crate::recipe::FileFormat;
    use crate::sniff::{sniff, Check, Doubt, Family};

    const COMPRESSED: &str =
        "##TITLE=Ethanol $$ Begin\n##JCAMP-DX=4.24\n##DATA TYPE=INFRARED SPECTRUM\n\
        ##XUNITS=1/CM\n##YUNITS=ABSORBANCE\n##XFACTOR=1\n##YFACTOR=1\n\
        ##FIRSTX=4000\n##LASTX=3995\n##NPOINTS=6\n\
        ##XYDATA=(X++(Y..Y))\n4000 A00JJ\n3997 A02JJJ\n##END=\n";

    #[test]
    fn a_compressed_table_is_proposed_not_refused() {
        let s = sniff(COMPRESSED.as_bytes(), "e.jdx").unwrap();
        assert_eq!(s.family, Some(Family::Jcamp));
        assert_eq!(s.recipe.format, FileFormat::Jcamp);
        assert_eq!((s.rows, s.columns), (6, 2));
        assert_eq!(
            s.checks,
            vec![Check::NPoints {
                declared: 6,
                read: 6
            }]
        );
        assert!(s.doubts.is_empty(), "{:?}", s.doubts);
        let summary = s.jcamp.as_ref().unwrap();
        assert_eq!((summary.blocks, summary.form), (1, TableForm::XppYY));
        let t = crate::parse(COMPRESSED.as_bytes(), &s.recipe).unwrap();
        assert_eq!(t.columns.len(), 2);
        assert_eq!(t.columns[0].values.first(), Some(&4000.0));
        assert_eq!(t.columns[0].values.last(), Some(&3995.0));
        assert_eq!(
            t.columns[1].values,
            vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0]
        );
        assert_eq!(t.columns[0].unit.as_deref(), Some("cm\u{207b}\u{b9}"));
        assert_eq!(t.columns[1].unit.as_deref(), Some("Abs"));
        assert_eq!(t.columns[1].long_name.as_deref(), Some("Ethanol"));
        assert_eq!(
            t.header_value("DATA TYPE").as_deref(),
            Some("INFRARED SPECTRUM")
        );
        assert_eq!(crate::resolve_series_name(&s.recipe, &t, "file"), "Ethanol");
    }

    #[test]
    fn a_wrong_npoints_is_a_failed_check_not_a_refusal() {
        let text = COMPRESSED.replace("NPOINTS=6", "NPOINTS=7");
        let s = sniff(text.as_bytes(), "e.jdx").unwrap();
        assert_eq!(s.rows, 6);
        assert!(!s.checks[0].ok());
        assert_eq!(s.recipe.format, FileFormat::Jcamp);
    }

    #[test]
    fn the_ocean_decimal_comma_table_reads_as_two_columns() {
        let text = "##TITLE=oo\n##JCAMP-DX=4.24\n##XUNITS=NANOMETERS\n##YUNITS=ARBITRARY\n\
            ##NPOINTS=3\n##XYDATA=(XY..XY)\n176,36, 32822,795\n177,02, 100,5\n178,1, 7,25\n##END=\n";
        let s = sniff(text.as_bytes(), "o.jdx").unwrap();
        assert_eq!((s.rows, s.columns), (3, 2));
        assert!(s.checks.iter().all(Check::ok));
        let t = crate::parse(text.as_bytes(), &s.recipe).unwrap();
        assert_eq!(t.columns.len(), 2);
        assert_eq!(t.columns[0].values, vec![176.36, 177.02, 178.1]);
        assert_eq!(t.columns[1].values, vec![32822.795, 100.5, 7.25]);
        assert_eq!(t.columns[0].unit.as_deref(), Some("nm"));
    }

    #[test]
    fn a_link_gives_one_table_per_spectrum_and_says_so() {
        let text = "##TITLE=link\n##DATA TYPE=LINK\n##BLOCKS=2\n\
            ##TITLE=a\n##FIRSTX=0\n##LASTX=1\n##XYDATA=(X++(Y..Y))\n0 1 2\n##END=\n\
            ##TITLE=b\n##FIRSTX=0\n##LASTX=2\n##XYDATA=(X++(Y..Y))\n0 3 4 5\n##END=\n##END=\n";
        let s = sniff(text.as_bytes(), "l.jdx").unwrap();
        assert_eq!(s.blocks.len(), 2);
        assert!(s.doubts.contains(&Doubt::SeveralBlocks {
            count: 2,
            rows_total: 5,
            split: true
        }));
        let tables = crate::parse_blocks(text.as_bytes(), &s.recipe).unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[1].columns[1].values, vec![3.0, 4.0, 5.0]);
        assert_eq!(crate::resolve_series_name(&s.recipe, &tables[1], "f"), "b");
        // `parse` is the first table.
        let first = crate::parse(text.as_bytes(), &s.recipe).unwrap();
        assert_eq!(first.row_count(), 2);
    }

    #[test]
    fn an_unread_two_dimensional_page_is_a_doubt() {
        let text = "##TITLE=two\n##NTUPLES=nD RAMAN SPECTRUM\n##SYMBOL= F1, F2, Y\n\
            ##DATA TABLE=(F2++(Y..Y)), PROFILE\n0 1 2\n##END=\n\
            ##TITLE=ok\n##PEAK TABLE=(XY..XY)\n1 2\n3 4\n##END=\n";
        let s = sniff(text.as_bytes(), "t.jdx").unwrap();
        assert_eq!(s.rows, 2);
        assert!(s.doubts.contains(&Doubt::Jcamp {
            block: 0,
            issue: JcampIssue::NtuplesNotRead
        }));
        assert_eq!(s.jcamp.unwrap().issues, vec![JcampIssue::NtuplesNotRead]);
    }

    #[test]
    fn a_header_only_jcamp_without_points_is_not_taken_over() {
        let text = "##TITLE=x\n##XYDATA=(X++(Y..Y))\n##END=\n";
        let refused = sniff(text.as_bytes(), "x.jdx");
        assert!(refused.is_err(), "{refused:?}");
    }

    #[test]
    fn the_jcamp_format_survives_a_json_round_trip_and_defaults_to_table() {
        let s = sniff(COMPRESSED.as_bytes(), "e.jdx").unwrap();
        let back = crate::ImportRecipe::from_json(&s.recipe.to_json()).unwrap();
        assert_eq!(back.format, FileFormat::Jcamp);
        let mut json: serde_json::Value = serde_json::from_str(&s.recipe.to_json()).unwrap();
        json.as_object_mut().unwrap().remove("format");
        let old = crate::ImportRecipe::from_json(&json.to_string()).unwrap();
        assert_eq!(old.format, FileFormat::Table);
    }

    #[test]
    fn a_jcamp_recipe_applied_to_a_plain_table_reads_it_as_a_table() {
        let mut recipe = crate::ImportRecipe::new("t");
        recipe.format = FileFormat::Jcamp;
        let t = crate::parse(b"1,2\n3,4\n", &recipe).unwrap();
        assert_eq!(t.row_count(), 2);
    }

    #[test]
    fn the_tuned_look_alike_stays_on_the_table_path() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/import/");
        let bytes = std::fs::read(format!("{dir}ir_jcamp_lookalike.txt")).unwrap();
        let s = sniff(&bytes, "ir_jcamp_lookalike.txt").unwrap();
        assert_eq!(s.recipe.format, FileFormat::Table);
        assert!(s.jcamp.is_none());
    }

    #[test]
    fn a_squeeze_letter_right_after_the_checkpoint_is_not_an_exponent() {
        // `16345e944` is the checkpoint 16345 then e944 = -5944.
        assert_eq!(ys("16345e944d815"), vec![-5944.0, -4815.0]);
        // A signed exponent is still one at the checkpoint.
        let mut st = XppState::default();
        let mut out = Vec::new();
        let (mut room, mut tally) = (10, Tally::default());
        let cp = decode_xpp_line("1.5e+3 7", &mut st, &mut out, &mut room, &mut tally);
        assert_eq!((cp, out), (Some(1500.0), vec![7.0]));
    }

    #[test]
    fn ntuples_peak_pages_keep_their_own_npoints_and_page() {
        let text = "##TITLE=ms\n##NTUPLES=MASS SPECTRUM\n##VAR_NAME= MASS, INTENSITY, TIME\n\
            ##SYMBOL= X, Y, T\n##UNITS= M/Z, REL, SECONDS\n\
            ##PAGE= T= 272\n##NPOINTS= 3\n##DATA TABLE= (XY..XY), PEAKS\n50, 2.5; 51, 9.5\n52, 7\n\
            ##PAGE= T= 301\n##NPOINTS= 2\n##DATA TABLE= (XY..XY), PEAKS\n60, 1; 61, 2\n##END=\n";
        let f = read(text);
        assert_eq!(f.blocks.len(), 2);
        assert_eq!(f.blocks[0].title, "ms (INTENSITY, T= 272)");
        assert_eq!(f.blocks[0].x, vec![50.0, 51.0, 52.0]);
        assert_eq!(f.blocks[0].npoints, Some(3));
        assert!(f.blocks[0].issues.is_empty());
        assert_eq!(f.blocks[1].npoints, Some(2));
        assert_eq!(f.blocks[1].yunits.as_deref(), Some("REL"));
    }

    #[test]
    fn a_table_label_after_thousands_of_parameter_lines_is_recognised() {
        let mut t = String::from("##TITLE=x\n##$PARAM=(0..9)\n");
        for _ in 0..5000 {
            t.push_str("1 2 3\n");
        }
        t.push_str("##NTUPLES=NMR FID\n");
        assert!(looks_jcamp(&t));
    }

    #[test]
    fn nothing_decoded_keeps_the_table_proposal_and_reports_the_unread_part() {
        let text = "##TITLE=abc\n##NTUPLES=TGA\n##SYMBOL= X, Y, T\n\
            ##DATA TABLE= (XYT..XYT), PEAKS\n100 20 1\n90 30 2\n80 40 3\n##END\n";
        let s = sniff(text.as_bytes(), "m.jdx").unwrap();
        assert_eq!(s.recipe.format, FileFormat::Table);
        assert_eq!(s.rows, 3);
        assert!(s.doubts.contains(&Doubt::Jcamp {
            block: 0,
            issue: JcampIssue::NtuplesNotRead
        }));
    }
}
