//! The file cut into a grid, and what marking it does to the recipe.
//!
//! Everything here is a decision the grid needs and the shell only paints:
//! the zone of a line, the rows with the data folded, the `KEY;value` shape of
//! a line, the role a click gives to lines, the data boundaries, the roles of
//! the columns. The shell turns clicks and drags into the commands of
//! [`GridCmd`] and calls [`AppState::import_grid_cmd`].
//!
//! Lines are **1-based** in every command and note (the numbers of the file),
//! 0-based in the functions that read a [`FileMap`].

use std::collections::BTreeSet;

use stokesforge_import::props::split_kv;
use stokesforge_import::{
    ColumnKind, ColumnRoleChoice, DataEnd, FileMap, HeaderSkip, ImportRecipe, LineRole,
    SubheaderLineRange,
};

use super::import_auto::PropChoice;
use super::import_check::CheckAction;
use super::import_check::CheckKind;
use super::AppState;
use crate::tr;

// -- Zones -------------------------------------------------------------------

/// The region of the file a line belongs to, in plain words.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Info,
    Marker,
    LongName,
    Units,
    Comments,
    XPrime,
    Names,
    Data,
    Footer,
    Ignored,
}

/// Every zone, in display order.
pub const ZONES: [Zone; 10] = [
    Zone::Info,
    Zone::Marker,
    Zone::LongName,
    Zone::Units,
    Zone::Comments,
    Zone::XPrime,
    Zone::Names,
    Zone::Data,
    Zone::Footer,
    Zone::Ignored,
];

impl Zone {
    /// The bit of the zone in the set [`zones_of`] returns.
    pub fn bit(self) -> u16 {
        1 << (self as u16)
    }

    pub fn label(self) -> String {
        match self {
            Zone::Info => tr!("import-grid-zone-info"),
            Zone::Marker => tr!("import-grid-zone-marker"),
            Zone::LongName => tr!("column-long-name"),
            Zone::Units => tr!("column-units"),
            Zone::Comments => tr!("column-comments"),
            Zone::XPrime => tr!("column-x-prime"),
            Zone::Names => tr!("import-column-names"),
            Zone::Data => tr!("import-grid-zone-data"),
            Zone::Footer => tr!("import-grid-zone-footer"),
            Zone::Ignored => tr!("import-grid-zone-ignored"),
        }
    }

    /// Does the whole row get a tint of the zone colour?
    pub fn tints_row(self) -> bool {
        matches!(
            self,
            Zone::LongName
                | Zone::Units
                | Zone::Comments
                | Zone::XPrime
                | Zone::Names
                | Zone::Marker
        )
    }
}

/// The zones of a bit set made by [`zones_of`], in display order.
pub fn zones_in(bits: u16) -> Vec<Zone> {
    ZONES
        .iter()
        .copied()
        .filter(|z| bits & z.bit() != 0)
        .collect()
}

/// Zones of file line `i` (0-based) as a bit set.
pub fn zones_of(map: &FileMap, i: usize, marker: Option<&str>) -> u16 {
    let sub = map.subheader_roles.get(i).copied().unwrap_or_default();
    match map.roles.get(i).copied().unwrap_or(LineRole::Blank) {
        LineRole::MainHeader => {
            let is_marker = marker.is_some_and(|m| {
                !m.trim().is_empty() && map.line(i).trim().to_lowercase() == m.trim().to_lowercase()
            });
            if is_marker {
                Zone::Marker.bit()
            } else {
                Zone::Info.bit()
            }
        }
        LineRole::Subheader
        | LineRole::SubheaderLongName
        | LineRole::SubheaderUnit
        | LineRole::SubheaderComment
        | LineRole::SubheaderXPrime => {
            let mut bits = 0;
            if sub.long_name {
                bits |= Zone::LongName.bit();
            }
            if sub.unit {
                bits |= Zone::Units.bit();
            }
            if sub.comment {
                bits |= Zone::Comments.bit();
            }
            if sub.x_prime {
                bits |= Zone::XPrime.bit();
            }
            if bits == 0 {
                Zone::Ignored.bit()
            } else {
                bits
            }
        }
        LineRole::ColumnHeaderRow => Zone::Names.bit(),
        LineRole::Data => Zone::Data.bit(),
        LineRole::Footer => Zone::Footer.bit(),
        LineRole::Comment | LineRole::Skipped => Zone::Ignored.bit(),
        LineRole::Blank => {
            if i < map.first {
                Zone::Ignored.bit()
            } else {
                Zone::Footer.bit()
            }
        }
    }
}

// -- Rows, with the fold -----------------------------------------------------

/// A long data zone is folded when it has more lines than this.
pub const FOLD_MIN: usize = 30;
/// Data lines kept visible at each end of a fold.
pub const FOLD_KEEP: usize = 8;

/// A row of the grid: a line, or the folded data lines in the middle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Line(usize),
    /// Folded data lines `from..=to` (0-based file lines).
    Fold(usize, usize),
}

/// The data lines hidden by the fold (0-based, both included), if the data
/// zone is long enough to fold.
pub fn fold_range(map: &FileMap) -> Option<(usize, usize)> {
    let (a, b) = map.data_range?;
    (b + 1 - a > FOLD_MIN).then(|| (a + FOLD_KEEP, b - FOLD_KEEP))
}

/// The rows of the grid. `frozen` keeps the fold where it was while a data
/// boundary is dragged, so that the lines under the pointer do not run away
/// as the data gets shorter.
pub fn build_rows(
    map: &FileMap,
    unfolded: bool,
    frozen: Option<Option<(usize, usize)>>,
) -> Vec<Row> {
    let fold = match frozen {
        Some(fold) => fold,
        None if unfolded => None,
        None => fold_range(map),
    };
    let mut rows = Vec::with_capacity(map.line_count());
    for i in 0..map.line_count() {
        match fold {
            Some((a, b)) if i == a => rows.push(Row::Fold(a, b)),
            Some((a, b)) if i > a && i <= b => {}
            _ => rows.push(Row::Line(i)),
        }
    }
    rows
}

/// The display row that holds file line `i` (0-based).
pub fn row_of_line(rows: &[Row], i: usize) -> usize {
    rows.partition_point(|row| match row {
        Row::Line(l) => *l < i,
        Row::Fold(_, to) => *to < i,
    })
    .min(rows.len().saturating_sub(1))
}

/// 0-based file line that starts right after the boundary above display row `b`.
pub fn boundary_line(rows: &[Row], map: &FileMap, b: usize) -> usize {
    match rows.get(b) {
        Some(Row::Line(i)) => *i,
        Some(Row::Fold(from, _)) => *from,
        None => map.line_count(),
    }
}

// -- Key / value cells -------------------------------------------------------

/// The `KEY<sep>VALUE` shape of a non-data line.
#[derive(Debug, Clone, PartialEq)]
pub struct Kv {
    pub key: String,
    pub value: String,
    /// The cell that holds the value.
    pub cell: usize,
    /// Characters before the value inside its cell (`##KEY=` of a one-cell line).
    pub offset: usize,
}

fn is_number_like(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+' | 'e' | 'E' | ' '))
}

/// The `KEY<sep>VALUE` shape of line `i` (0-based), if it has one.
pub fn kv_of(map: &FileMap, recipe: &ImportRecipe, i: usize) -> Option<Kv> {
    let tokens = map.tokens(i, recipe);
    let alpha = |s: &str| s.chars().any(char::is_alphabetic);
    if tokens.len() >= 2 && map.separator_char().is_some() {
        let key = tokens[0].trim().trim_start_matches('#').trim();
        let value = tokens[1].trim();
        if !key.is_empty()
            && alpha(key)
            && !value.is_empty()
            && !tokens[2..].iter().any(|t| !t.trim().is_empty())
        {
            return Some(Kv {
                key: key.to_string(),
                value: value.to_string(),
                cell: 1,
                offset: 0,
            });
        }
        // `KEY;value;more`: the second cell still is the value.
        if !key.is_empty() && alpha(key) && !value.is_empty() && !is_number_like(value) {
            return Some(Kv {
                key: key.to_string(),
                value: value.to_string(),
                cell: 1,
                offset: 0,
            });
        }
        return None;
    }
    if tokens.len() <= 1 || map.separator_char().is_none() {
        let (key, value) = split_kv(map.line(i))?;
        if alpha(key) && !value.is_empty() {
            let line = map.line(i);
            let at = (value.as_ptr() as usize).saturating_sub(line.as_ptr() as usize);
            let offset = line.get(..at).map_or(0, |head| head.chars().count());
            return Some(Kv {
                key: key.to_string(),
                value: value.to_string(),
                cell: 0,
                offset,
            });
        }
    }
    None
}

// -- Roles given to lines ----------------------------------------------------

/// A role the grid gives to the selected lines (a button of the role bar, the
/// context menu or a key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRoleCmd {
    DataStart,
    LongName,
    Units,
    Comments,
    XPrime,
    Names,
    DataEnd,
    Clear,
}

/// Every role command, in the order of the role bar.
pub const ROLE_CMDS: [LineRoleCmd; 8] = [
    LineRoleCmd::DataStart,
    LineRoleCmd::LongName,
    LineRoleCmd::Units,
    LineRoleCmd::Comments,
    LineRoleCmd::XPrime,
    LineRoleCmd::Names,
    LineRoleCmd::DataEnd,
    LineRoleCmd::Clear,
];

impl LineRoleCmd {
    pub fn name(self) -> String {
        match self {
            Self::DataStart => tr!("import-role-data-start"),
            Self::LongName => tr!("column-long-name"),
            Self::Units => tr!("column-units"),
            Self::Comments => tr!("column-comments"),
            Self::XPrime => tr!("column-x-prime"),
            Self::Names => tr!("import-column-names"),
            Self::DataEnd => tr!("import-role-data-end"),
            Self::Clear => tr!("import-role-clear"),
        }
    }

    /// Roles that take a range of lines (the others take one line).
    pub fn ranged(self) -> bool {
        matches!(self, Self::LongName | Self::Comments)
    }
}

/// The roles of the file lines as the recipe holds them, every number a line
/// of the **file**: the recipe keeps its relative model, [`lines_view`] reads
/// the file lines from it and [`apply_lines`] writes them back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinesView {
    /// File line number of the first non-blank line (the recipe counts from it).
    pub first: usize,
    pub data_start: usize,
    /// (from, to) of each role, file lines; `None` = role off.
    pub long_name: Option<(usize, usize)>,
    pub unit: Option<usize>,
    pub comment: Option<(usize, usize)>,
    pub x_prime: Option<usize>,
    /// Column names line; wired only when it is the line just before the data.
    pub names: Option<usize>,
}

/// The roles of the recipe as file lines. `names_unwired` is the names line
/// that is not the line just before the data (the recipe cannot hold it).
pub fn lines_view(recipe: &ImportRecipe, first: usize, names_unwired: Option<usize>) -> LinesView {
    let main = recipe.header_layout.main_header_lines;
    let sub = recipe.header_layout.subheader_lines;
    let names_row = usize::from(recipe.has_column_header_row);
    let data_start = first + main + sub + names_row;
    let file = |line: usize| first + main + line.max(1) - 1;
    let roles = &recipe.subheader_roles;
    LinesView {
        first,
        data_start,
        long_name: roles
            .long_name
            .map(|r| (file(r.from), file(r.last_joined()))),
        unit: roles.unit.map(|r| file(r.from)),
        comment: roles.comment.map(|r| (file(r.from), file(r.last_joined()))),
        x_prime: roles.x_prime.map(|r| file(r.from)),
        names: if recipe.has_column_header_row {
            Some(data_start - 1)
        } else {
            names_unwired
        },
    }
}

/// Writes `view` back into the recipe: main header = lines before the first
/// role line, sub-header = the rest up to the data (names line excluded).
pub fn apply_lines(recipe: &mut ImportRecipe, view: &LinesView) {
    let first = view.first;
    let data = view.data_start.max(first);
    let names_wired = view.names.is_some_and(|n| n + 1 == data && n >= first);
    let header_end = data - usize::from(names_wired);
    let total = header_end - first;
    let role_starts = [
        view.long_name.map(|r| r.0),
        view.unit,
        view.comment.map(|r| r.0),
        view.x_prime,
    ];
    let main = role_starts
        .into_iter()
        .flatten()
        .map(|line| line.saturating_sub(first))
        .min()
        .map_or(total, |m| m.min(total));
    let sub = total - main;
    let old_main = recipe.header_layout.main_header_lines;
    recipe.header = HeaderSkip::None;
    recipe.header_layout = stokesforge_import::HeaderLayout {
        main_header_lines: main,
        subheader_lines: sub,
        auto_detect: false,
    };
    recipe.has_column_header_row = names_wired;
    // The marker stays while the main header keeps its length.
    if recipe.main_until_marker.is_some() && main != old_main {
        recipe.main_until_marker = None;
    }
    let rel = |line: usize| (line + 1).saturating_sub(first + main).max(1);
    let range = |from: usize, to: usize| SubheaderLineRange {
        from: rel(from),
        to: rel(to.max(from)),
    };
    let roles = &mut recipe.subheader_roles;
    roles.long_name = view.long_name.map(|r| range(r.0, r.1));
    roles.unit = view.unit.map(|l| range(l, l));
    roles.comment = view.comment.map(|r| range(r.0, r.1));
    roles.x_prime = view.x_prime.map(|l| range(l, l));
    roles.join_lines = true;
}

/// The roles of the file lines as the recipe holds them now.
#[derive(Debug, Clone, Copy)]
pub struct Spans {
    pub view: LinesView,
    /// First file line **not** read, when the data end is a line count.
    pub end_mark: Option<usize>,
}

/// The spans of a recipe (`first`: file line of the first non-blank line).
pub fn spans_of(recipe: &ImportRecipe, first: usize, unwired: Option<usize>) -> Spans {
    spans_of_view(lines_view(recipe, first, unwired), &recipe.data_end)
}

fn spans_of_view(view: LinesView, data_end: &DataEnd) -> Spans {
    let end_mark = match data_end {
        DataEnd::LinesAfterHeader(n) => Some(view.data_start + n),
        _ => None,
    };
    Spans { view, end_mark }
}

impl Spans {
    /// The lines that carry `role` (from, to), if the role is on.
    pub fn span(&self, role: LineRoleCmd) -> Option<(usize, usize)> {
        match role {
            LineRoleCmd::DataStart => Some((self.view.data_start, self.view.data_start)),
            LineRoleCmd::LongName => self.view.long_name,
            LineRoleCmd::Units => self.view.unit.map(|l| (l, l)),
            LineRoleCmd::Comments => self.view.comment,
            LineRoleCmd::XPrime => self.view.x_prime.map(|l| (l, l)),
            LineRoleCmd::Names => self.view.names.map(|l| (l, l)),
            LineRoleCmd::DataEnd => self.end_mark.map(|m| (m, m)),
            LineRoleCmd::Clear => None,
        }
    }

    /// Does file line `line` carry `role`?
    pub fn carries(&self, role: LineRoleCmd, line: usize) -> bool {
        match (role, self.span(role)) {
            (LineRoleCmd::DataEnd, Some((m, _))) => line == m || line + 1 == m,
            (_, Some((from, to))) => (from..=to).contains(&line),
            _ => false,
        }
    }

    /// Is the role bar button of `role` pressed for these selected lines?
    pub fn pressed(&self, role: LineRoleCmd, lines: &BTreeSet<usize>) -> bool {
        role != LineRoleCmd::Clear && lines.iter().any(|l| self.carries(role, *l))
    }
}

fn remove_role(view: &mut LinesView, data_end: &mut DataEnd, role: LineRoleCmd) {
    match role {
        LineRoleCmd::LongName => view.long_name = None,
        LineRoleCmd::Units => view.unit = None,
        LineRoleCmd::Comments => view.comment = None,
        LineRoleCmd::XPrime => view.x_prime = None,
        LineRoleCmd::Names => view.names = None,
        LineRoleCmd::DataEnd => {
            if matches!(data_end, DataEnd::LinesAfterHeader(_)) {
                *data_end = DataEnd::None;
            }
        }
        LineRoleCmd::DataStart | LineRoleCmd::Clear => {}
    }
}

/// Applies a role to the selected lines. Returns the note to show, if any.
pub fn assign_role(
    view: &mut LinesView,
    data_end: &mut DataEnd,
    role: LineRoleCmd,
    lines: &BTreeSet<usize>,
    is_data: &dyn Fn(usize) -> bool,
) -> String {
    let (Some(first), Some(last)) = (lines.first().copied(), lines.last().copied()) else {
        return tr!("import-role-note-select-first");
    };
    let spans = spans_of_view(*view, data_end);
    if role == LineRoleCmd::Clear {
        for r in ROLE_CMDS {
            if spans.pressed(r, lines) {
                remove_role(view, data_end, r);
            }
        }
        return String::new();
    }
    if role != LineRoleCmd::DataStart && spans.pressed(role, lines) {
        remove_role(view, data_end, role);
        return String::new();
    }
    let mut note = String::new();
    if role.ranged() {
        if last - first + 1 != lines.len() {
            note = tr!(
                "import-role-note-span",
                role = role.name(),
                from = first,
                to = last
            );
        }
    } else if !matches!(role, LineRoleCmd::DataStart | LineRoleCmd::DataEnd) && lines.len() > 1 {
        note = tr!(
            "import-role-note-one-line",
            role = role.name(),
            line = first
        );
    }
    match role {
        LineRoleCmd::DataStart => view.data_start = first,
        LineRoleCmd::LongName => view.long_name = Some((first, last)),
        LineRoleCmd::Units => view.unit = Some(first),
        LineRoleCmd::Comments => view.comment = Some((first, last)),
        LineRoleCmd::XPrime => view.x_prime = Some(first),
        LineRoleCmd::Names => view.names = Some(first),
        LineRoleCmd::DataEnd => {
            // On a data line, the data stops after it; elsewhere, before it.
            let n = if is_data(last) {
                (last + 1).saturating_sub(view.data_start)
            } else {
                last.saturating_sub(view.data_start)
            };
            if n == 0 {
                return tr!("import-role-note-end-after-start");
            }
            *data_end = DataEnd::LinesAfterHeader(n);
        }
        LineRoleCmd::Clear => {}
    }
    note
}

// -- Rules rather than counts (#488) --------------------------------------------

/// Does file line `line` (0-based) of the map start with a number?
fn starts_with_number(map: &FileMap, recipe: &ImportRecipe, line: usize) -> bool {
    map.tokens(line, recipe)
        .first()
        .is_some_and(|token| map.token_number(0, token.trim(), recipe).is_some())
}

/// What a hand-chosen last data line (1-based) means for a file that is not this
/// one: the **rule** that stops there when the file offers one, else the count.
/// A blank line after it gives "first blank line", a non-numeric line after it
/// gives that line as the marker, nothing after it gives "to the end of the
/// file"; a number after it (the cut is in the middle of the data) keeps the
/// count `n`. A rule is used only if it would not cut earlier, inside the data.
pub(crate) fn data_end_rule(
    map: &FileMap,
    recipe: &ImportRecipe,
    data_start: usize,
    last: usize,
    n: usize,
) -> DataEnd {
    let inside = (data_start.saturating_sub(1))..last.min(map.line_count());
    let after = last; // 0-based index of the line after `last`
    if after >= map.end {
        return DataEnd::None;
    }
    let text = map.line(after).trim();
    if text.is_empty() {
        if inside.clone().all(|i| !map.line(i).trim().is_empty()) {
            return DataEnd::FirstBlankLine;
        }
    } else if !starts_with_number(map, recipe, after)
        && inside
            .clone()
            .all(|i| !map.line(i).trim().eq_ignore_ascii_case(text))
    {
        return DataEnd::Marker(text.to_string());
    }
    DataEnd::LinesAfterHeader(n)
}

/// After the header was moved by hand: the main header then ends on a line
/// that reads the same in another file, so the recipe looks for that line
/// (the marker) instead of counting lines. Only when that line is text, is
/// the first of its kind in the file, and no marker is set yet.
pub(crate) fn anchor_header_marker(recipe: &mut ImportRecipe, map: &FileMap) {
    let main = recipe.header_layout.main_header_lines;
    if main == 0 || recipe.main_until_marker.is_some() {
        return;
    }
    let last_main = map.first + main - 1;
    let text = map.line(last_main).trim();
    if text.is_empty()
        || starts_with_number(map, recipe, last_main)
        || (map.first..last_main).any(|i| map.line(i).trim().eq_ignore_ascii_case(text))
    {
        return;
    }
    recipe.main_until_marker = Some(text.to_string());
}

// -- Commands of the grid ------------------------------------------------------

/// What the grid asks of the recipe.
#[derive(Debug, Clone)]
pub enum GridCmd {
    /// A role on file lines (1-based).
    Role(LineRoleCmd, Vec<usize>),
    /// Move the data boundaries (1-based first / last data line).
    Handle { first: usize, last: usize },
    /// Take the value of a header key as a field of the columns.
    Prop(String, PropChoice),
    /// The role (X, Y, error...) of columns; the same role again goes back to Auto.
    ColumnRole(Vec<usize>, ColumnRoleChoice),
    /// Read columns as text; again goes back to Auto.
    ColumnText(Vec<usize>),
    /// Leave out (true) or keep (false) the metadata of this key.
    Exclude(String, bool),
}

impl AppState {
    /// The file line of the first non-blank line of the file shown.
    fn grid_first_line(&self) -> Option<usize> {
        let map = self.import_file_map()?;
        let map = map.as_ref().as_ref().ok()?;
        Some(map.first + 1)
    }

    /// Applies a command of the grid to the recipe.
    pub fn import_grid_cmd(&mut self, cmd: GridCmd) {
        match cmd {
            GridCmd::Prop(key, choice) => self.import_set_prop_choice(&key, choice),
            GridCmd::Role(role, lines) => {
                self.import_apply_role(role, &lines);
            }
            GridCmd::Handle { first, last } => self.import_apply_handle(first, last),
            GridCmd::ColumnRole(cols, role) => {
                let Some(import) = self.import_state.as_mut() else {
                    return;
                };
                let recipe = &mut import.recipe;
                let all_same = cols.iter().all(|c| recipe.column_role(*c) == role);
                let need = cols.iter().copied().max().map_or(0, |m| m + 1);
                if recipe.column_roles.len() < need {
                    recipe.column_roles.resize(need, ColumnRoleChoice::Auto);
                }
                for c in cols {
                    recipe.column_roles[c] = if all_same {
                        ColumnRoleChoice::Auto
                    } else {
                        role
                    };
                }
            }
            GridCmd::ColumnText(cols) => {
                let Some(import) = self.import_state.as_mut() else {
                    return;
                };
                let recipe = &mut import.recipe;
                let all_text = cols
                    .iter()
                    .all(|c| recipe.column_kind(*c) == ColumnKind::Text);
                let need = cols.iter().copied().max().map_or(0, |m| m + 1);
                if recipe.column_kinds.len() < need {
                    recipe.column_kinds.resize(need, ColumnKind::Auto);
                }
                for c in cols {
                    recipe.column_kinds[c] = if all_text {
                        ColumnKind::Auto
                    } else {
                        ColumnKind::Text
                    };
                }
            }
            GridCmd::Exclude(key, exclude) => {
                let Some(import) = self.import_state.as_mut() else {
                    return;
                };
                let recipe = &mut import.recipe;
                let same = |name: &String| name.trim().eq_ignore_ascii_case(key.trim());
                recipe.excluded_metadata.retain(|name| !same(name));
                if exclude {
                    recipe.excluded_metadata.push(key);
                }
            }
        }
    }

    /// Gives `role` to file lines `lines` (1-based) of the file shown. Returns
    /// the note to show (a role that takes one line, a span with a hole...).
    pub fn import_apply_role(&mut self, role: LineRoleCmd, lines: &[usize]) -> String {
        let Some(map) = self.import_file_map() else {
            return String::new();
        };
        let Ok(map) = &*map else {
            return String::new();
        };
        let first = map.first + 1;
        let Some(import) = self.import_state.as_mut() else {
            return String::new();
        };
        let unwired = import.names_unwired;
        let recipe = &mut import.recipe;
        recipe.unify_header();
        let mut view = lines_view(recipe, first, unwired);
        let before = view;
        let end_mark_before = spans_of(recipe, first, unwired).end_mark;
        let set: BTreeSet<usize> = lines.iter().copied().collect();
        let is_data = |n: usize| map.roles.get(n.wrapping_sub(1)) == Some(&LineRole::Data);
        let note = assign_role(&mut view, &mut recipe.data_end, role, &set, &is_data);
        if role == LineRoleCmd::DataEnd {
            if let DataEnd::LinesAfterHeader(n) = recipe.data_end {
                recipe.data_end =
                    data_end_rule(map, recipe, view.data_start, view.data_start + n - 1, n);
            }
        }
        if role == LineRoleCmd::DataStart {
            // The start moves, the end stays where it was.
            if let (DataEnd::LinesAfterHeader(n), Some(mark)) =
                (&mut recipe.data_end, end_mark_before)
            {
                *n = mark.saturating_sub(view.data_start).max(1);
            }
        }
        if view != before {
            Self::store_view(import, &view);
            anchor_header_marker(&mut import.recipe, map);
        }
        note
    }

    /// Moves the data boundaries to file lines `first_line..=last_line` (1-based).
    pub fn import_apply_handle(&mut self, first_line: usize, last_line: usize) {
        let Some(first) = self.grid_first_line() else {
            return;
        };
        let Some(map) = self.import_file_map() else {
            return;
        };
        let Ok(map) = &*map else {
            return;
        };
        let Some(import) = self.import_state.as_mut() else {
            return;
        };
        let recipe = &mut import.recipe;
        recipe.unify_header();
        let mut view = lines_view(recipe, first, import.names_unwired);
        let before = view;
        view.data_start = first_line;
        let count = (last_line + 1).saturating_sub(first_line).max(1);
        recipe.data_end = data_end_rule(map, recipe, first_line, last_line, count);
        if view != before {
            Self::store_view(import, &view);
        }
        anchor_header_marker(&mut import.recipe, map);
    }

    /// Writes a changed view back to the recipe and remembers a names line
    /// that is not just above the data.
    fn store_view(import: &mut super::ImportState, view: &LinesView) {
        let wired = view.names.is_some_and(|n| n + 1 == view.data_start);
        import.names_unwired = if wired { None } else { view.names };
        apply_lines(&mut import.recipe, view);
    }

    /// The roles of the file lines of the recipe being edited (what the grid
    /// and the role bar show as pressed).
    pub fn import_spans(&self) -> Option<Spans> {
        let first = self.grid_first_line()?;
        let import = self.import_state.as_ref()?;
        Some(spans_of(&import.recipe, first, import.names_unwired))
    }

    // -- Answers of the review that edit the recipe ------------------------------

    /// The user clicked file line `line` (1-based) while the review waits for
    /// a pick: the line becomes the x' line (or the last data line), the doubt
    /// is settled and the review moves on. Returns the log sentence.
    pub fn check_picked(&mut self, line: usize) -> Option<String> {
        let kind = self.check_pick()?;
        let log = match kind {
            CheckKind::XPrime => {
                self.import_apply_role(LineRoleCmd::XPrime, &[line]);
                self.check_decide(CheckKind::XPrime);
                tr!("import-check-log-xprime", line = line.to_string())
            }
            _ => {
                let first = self
                    .import_check()
                    .and_then(|v| v.data)
                    .map_or(1, |(a, _)| a);
                self.import_apply_handle(first, line.max(first));
                self.check_decide(CheckKind::Points);
                tr!("import-check-log-last", line = line.to_string())
            }
        };
        self.check_review_next(Some(log.clone()));
        Some(log)
    }

    /// Answer `index` of the doubt the review is on: the recipe edits it asks
    /// for, the decision, the log line, and the move to the next doubt (except
    /// for "pick a line", which waits for the click).
    pub fn check_answer(&mut self, index: usize) {
        let Some(info) = self.check_review_info() else {
            return;
        };
        let Some(answer) = info
            .finding
            .question
            .as_ref()
            .and_then(|q| q.answers.get(index))
            .cloned()
        else {
            return;
        };
        let mut keep_going = true;
        match answer.action {
            CheckAction::Confirm(k) => self.check_decide(k),
            CheckAction::XPrimeAsData { line } => {
                self.import_apply_role(LineRoleCmd::XPrime, &[line]);
                self.import_apply_role(LineRoleCmd::DataStart, &[line]);
                self.check_decide(CheckKind::XPrime);
            }
            CheckAction::NamesOnly { line } => {
                self.import_apply_role(LineRoleCmd::XPrime, &[line]);
                self.check_decide(CheckKind::NameXPrime);
            }
            CheckAction::LineAsData { line, kind } => {
                self.import_apply_role(LineRoleCmd::Clear, &[line]);
                self.import_apply_role(LineRoleCmd::DataStart, &[line]);
                self.check_decide(kind);
            }
            CheckAction::NameForSheetOnly { key } => self.check_drop_name_prop(&key, true),
            CheckAction::NoName { key } => self.check_drop_name_prop(&key, false),
            CheckAction::CommaGroupsThousands => self.check_comma_groups_thousands(),
            CheckAction::PickLine(k) => {
                self.check_set_pick(Some(k));
                keep_going = false;
            }
            CheckAction::UnitFromKey { kind, key } => {
                let choice = if kind == CheckKind::YUnit {
                    PropChoice::UnitsY
                } else {
                    PropChoice::UnitsX
                };
                let already = self.import_state.as_ref().is_some_and(|i| {
                    super::import_auto::prop_choices_on(&i.recipe, &key).contains(&choice)
                });
                if !already {
                    self.import_set_prop_choice(&key, choice);
                }
                self.check_decide(kind);
            }
        }
        if keep_going {
            self.check_review_next(Some(answer.label));
        }
    }

    /// "Mark as Checked" on a finding of the panel: the answer is kept, the
    /// recipe is not touched.
    pub fn check_mark_checked(&mut self, kind: CheckKind) {
        self.check_decide(kind);
    }
}
