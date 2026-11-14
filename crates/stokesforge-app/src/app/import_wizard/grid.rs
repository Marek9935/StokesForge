//! The file as a grid. The whole import is one
//! screen: the file already cut into a grid, marked by the detector, corrected
//! by marking the grid itself, with the worksheet that will be created shown
//! live under it.
//!
//! Everything in the grid is painted by hand (rows, cells, gutter, bands,
//! minimap, scrollbars) and hit-tested by arithmetic, so the 630 lines x 241
//! columns of a kinetic file stay smooth: only the visible rows and columns
//! are laid out. The model (lines, zones, tokens) comes from
//! [`stokesforge_import::FileMap`], cached in the state while neither the file
//! nor the recipe changes.

#![allow(clippy::needless_range_loop, clippy::type_complexity)]

use std::collections::BTreeSet;

use eframe::egui;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind};
use stokesforge_app_state::state::{
    boundary_line, build_rows, fold_range, kv_of, prop_choices_on, row_of_line, zones_in, zones_of,
    AutoInfo, GridCmd, Kv, LineRoleCmd, PropChoice, PropRow, Row, Spans, Zone,
};
use stokesforge_app_state::tr;
use stokesforge_import::props::{norm_key, split_kv};
use stokesforge_import::sniff::AutoWhat;
use stokesforge_import::{
    ColumnKind, ColumnRoleChoice, FileMap, ImportRecipe, LineRole, NameSource, Separator,
};

use super::{
    column_letters, encoding_label, import_result_table, prop_color, prop_field_label,
    prop_target_label, refused_card, role_color, same_variant, PILL_WIDTH,
};
use crate::app::cursors;
use crate::theme::{import_color, ImportColor};

mod check;
mod guide;
mod panel;
mod screen;

pub(super) use check::{apply as apply_check, CheckCmd};
pub(super) use screen::{footer_buttons, import_screen};

const ROW_H: f32 = 20.0;
const LETTER_H: f32 = 22.0;
const HINT_H: f32 = 0.0;
const MINI_W: f32 = 46.0;
const BAND_W: f32 = 24.0;
const NUM_W: f32 = 40.0;
const GAP: f32 = 16.0;
const SB: f32 = 9.0;
const MAX_COL: f32 = 112.0;
const MIN_COL: f32 = 34.0;
/// Widest the only column of a file read with a wrong delimiter may grow.
const SINGLE_COL_MAX: f32 = 6000.0;

/// Colour of a zone of the file (band, label, tint of the row).
fn zone_color(zone: Zone, dark: bool) -> Color32 {
    match zone {
        Zone::Info | Zone::Footer | Zone::Marker => import_color(dark, ImportColor::Header),
        Zone::LongName => import_color(dark, ImportColor::LongName),
        Zone::Units => import_color(dark, ImportColor::Units),
        Zone::Comments => import_color(dark, ImportColor::Comments),
        Zone::XPrime => import_color(dark, ImportColor::XPrime),
        Zone::Names => import_color(dark, ImportColor::ColumnNames),
        Zone::Data => import_color(dark, ImportColor::Footer),
        Zone::Ignored => import_color(dark, ImportColor::Header).gamma_multiply(0.6),
    }
}

// -- Persistent state of the grid --------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct ValueSel {
    /// 0-based file line.
    line: usize,
    key: String,
    value: String,
    cell: usize,
    offset: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Sel {
    #[default]
    None,
    /// 1-based file line numbers.
    Lines(BTreeSet<usize>),
    Value(ValueSel),
    Cols(BTreeSet<usize>),
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    Lines,
    Cols,
    HandleStart,
    HandleEnd,
    Mini,
    VBar(f32),
    HBar(f32),
}

#[derive(Clone, Debug)]
enum Extra {
    Pill(AutoWhat, String),
    Tag(usize),
    TagPill(AutoWhat, String),
}

#[derive(Clone)]
struct GridUi {
    sel: Sel,
    anchor: usize,
    pan: egui::Vec2,
    unfolded: bool,
    drag: Option<Drag>,
    result_view: bool,
    note: String,
    extras: Vec<(Rect, Extra)>,
    bar_rect: Rect,
    bar_h: f32,
    /// Pan when the action bar appeared: a scroll closes it.
    bar_pan: Option<egui::Vec2>,
    /// Since when the pointer is away from the bar.
    bar_left: Option<f64>,
    last_path: Option<std::path::PathBuf>,
    /// Target of the context menu: where it was opened.
    menu_info_line: Option<(usize, String)>,
    /// Data boundary being dragged: (first line, last line), 1-based.
    handle_plan: Option<(usize, usize)>,
    follow_scroll_to: Option<usize>,
    /// Fold kept while a data boundary is dragged.
    frozen_fold: Option<Option<(usize, usize)>>,
    /// what the left column asks the grid to show.
    focus: Option<check::Focus>,
    focus_next: Option<check::Focus>,
    focus_line: Option<(usize, bool)>,
    focus_scroll_to: Option<(usize, f32)>,
    /// A click on a line is awaited (the review asked to pick one).
    pick_active: bool,
    /// The step of the first-use guide that is shown (the guide itself is open
    /// or not in the state).
    guide_step: u8,
    /// The « Don't show again » box of the open guide; `None` until the guide
    /// opens (it then starts at the stored preference).
    guide_dont: Option<bool>,
    /// Evidence framed after a click on a « To check » tag, with the time.
    pinned: Option<(Vec<stokesforge_app_state::state::Evidence>, f64)>,
    /// The reason of an automatic choice, open after a click on its button.
    auto_why: Option<AutoWhy>,
}

/// The small window that explains an automatic choice.
#[derive(Clone)]
struct AutoWhy {
    anchor: Rect,
    what: AutoWhat,
    why: String,
    /// Pan when it opened: a scroll closes it.
    pan: egui::Vec2,
}

impl Default for GridUi {
    fn default() -> Self {
        Self {
            sel: Sel::None,
            anchor: 0,
            pan: egui::Vec2::ZERO,
            unfolded: false,
            drag: None,
            result_view: false,
            note: String::new(),
            extras: Vec::new(),
            bar_rect: Rect::NOTHING,
            bar_h: 60.0,
            bar_pan: None,
            bar_left: None,
            last_path: None,
            menu_info_line: None,
            handle_plan: None,
            follow_scroll_to: None,
            frozen_fold: None,
            focus: None,
            focus_next: None,
            focus_line: None,
            focus_scroll_to: None,
            pick_active: false,
            guide_step: 0,
            guide_dont: None,
            pinned: None,
            auto_why: None,
        }
    }
}

fn gs_id() -> egui::Id {
    egui::Id::new("stokesforge_import_e_grid")
}

fn load_gs(ctx: &egui::Context) -> GridUi {
    ctx.data(|d| d.get_temp::<GridUi>(gs_id()))
        .unwrap_or_default()
}

fn store_gs(ctx: &egui::Context, gs: GridUi) {
    ctx.data_mut(|d| d.insert_temp(gs_id(), gs));
}

/// Escape clears the selection first (before the dialog sees the key).
pub(super) fn grid_escape(ctx: &egui::Context) {
    let mut gs = load_gs(ctx);
    if gs.sel != Sel::None
        && !ctx.text_edit_focused()
        && !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|i| i.consume_shortcut(&crate::app::shortcuts::KEY_ESCAPE))
    {
        gs.sel = Sel::None;
        gs.note.clear();
        store_gs(ctx, gs);
    }
}

// -- Column metrics ----------------------------------------------------------

#[derive(Clone)]
struct Metrics {
    serial: u64,
    cw: f32,
    widths: Vec<f32>,
    /// Left edge of each column, plus the total at the end.
    colx: Vec<f32>,
}

fn metrics(
    ctx: &egui::Context,
    map: &FileMap,
    recipe: &ImportRecipe,
    cw: f32,
) -> std::sync::Arc<Metrics> {
    let id = egui::Id::new("stokesforge_import_e_metrics");
    if let Some(m) = ctx.data(|d| d.get_temp::<std::sync::Arc<Metrics>>(id)) {
        if m.serial == map.serial && (m.cw - cw).abs() < 0.01 {
            return m;
        }
    }
    let mut ncols = 1usize;
    let mut maxc: Vec<usize> = Vec::new();
    for i in 0..map.line_count() {
        let tokens = map.tokens(i, recipe);
        ncols = ncols.max(tokens.len());
        if i < 220 {
            if maxc.len() < tokens.len() {
                maxc.resize(tokens.len(), 0);
            }
            for (c, t) in tokens.iter().enumerate() {
                maxc[c] = maxc[c].max(t.trim().chars().count());
            }
        }
    }
    let ncols = ncols.min(3000);
    // One column only (a wrong delimiter): the column takes the width of the
    // longest data line, so that the real delimiter shows inside it. The grid
    // scrolls sideways beyond its own width.
    let single = if ncols == 1 {
        let last = map.line_count().saturating_sub(1);
        let (a, b) = map.data_range.unwrap_or((0, last));
        let longest = (a..=b.min(a + 220).min(last))
            .map(|i| map.line(i).chars().count())
            .max()
            .unwrap_or(0);
        Some((longest as f32 * cw + 10.0).clamp(MIN_COL, SINGLE_COL_MAX))
    } else {
        None
    };
    let widths: Vec<f32> = (0..ncols)
        .map(|c| {
            if let Some(w) = single {
                return w;
            }
            let chars = maxc.get(c).copied().unwrap_or(6);
            (chars as f32 * cw + 10.0).clamp(MIN_COL, MAX_COL)
        })
        .collect();
    let mut colx = Vec::with_capacity(ncols + 1);
    let mut x = 0.0;
    for w in &widths {
        colx.push(x);
        x += w + GAP;
    }
    colx.push(x);
    let m = std::sync::Arc::new(Metrics {
        serial: map.serial,
        cw,
        widths,
        colx,
    });
    ctx.data_mut(|d| d.insert_temp(id, m.clone()));
    m
}

// -- The grid ----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Hit {
    None,
    Letter(usize),
    Num(usize),
    Cell(usize, usize),
    Value(usize),
    Fold,
    Band(usize),
    Pill(AutoWhat, String),
    Tag(usize),
    TagPill(AutoWhat, String),
    HandleStart,
    HandleEnd,
    Mini(f32),
    VBar(f32),
    HBar(f32),
}

struct Band {
    r0: usize,
    r1: usize,
    zones: u16,
}

struct Geom {
    body: Rect,
    mini: Rect,
    hint: Rect,
    letters: Rect,
    nums: Rect,
    bands_x: (f32, f32),
    grid: Rect,
    rows_top: f32,
    view_h: f32,
    view_w: f32,
    vtrack: Rect,
    htrack: Rect,
    vthumb: Rect,
    hthumb: Rect,
    content_h: f32,
    content_w: f32,
}

/// The display rows that intersect the view: the only ones laid out and
/// painted, whatever the length of the file (a kinetic has 630 lines and 241
/// columns, and the frame budget does not allow more).
fn visible_rows(pan_y: f32, view_h: f32, n_rows: usize) -> std::ops::Range<usize> {
    let first = ((pan_y / ROW_H).floor() as usize).min(n_rows);
    let last = (((pan_y + view_h) / ROW_H).ceil() as usize).min(n_rows);
    first..last
}

/// The columns that intersect the view, from the left edge of every column
/// (`colx`, one more entry than columns), the horizontal scroll `sx` and the
/// width of the view.
fn visible_columns(colx: &[f32], sx: f32, view_w: f32, ncols: usize) -> (usize, usize) {
    let first = colx
        .partition_point(|cx| *cx <= sx)
        .saturating_sub(1)
        .min(ncols.saturating_sub(1));
    let last = colx.partition_point(|cx| *cx < sx + view_w).min(ncols);
    (first, last)
}

fn row_y(g: &Geom, scroll: egui::Vec2, r: usize) -> f32 {
    g.rows_top + r as f32 * ROW_H - scroll.y
}

fn make_geom(body: Rect, n_rows: usize, total_w: f32, scroll: &mut egui::Vec2) -> Geom {
    let mini = Rect::from_min_size(body.min, vec2(MINI_W, body.height()));
    let origin_x = mini.right() + 6.0;
    let hint = Rect::from_min_max(
        pos2(origin_x, body.top()),
        pos2(body.right(), body.top() + HINT_H),
    );
    let letters_top = hint.bottom();
    let rows_top = letters_top + LETTER_H;
    let nums_left = origin_x + BAND_W;
    let grid_left = nums_left + NUM_W + 4.0;
    let rows_bottom = (body.bottom() - SB).max(rows_top + ROW_H);
    let grid_right = (body.right() - SB).max(grid_left + 40.0);
    let view_h = rows_bottom - rows_top;
    let view_w = grid_right - grid_left;
    let content_h = n_rows as f32 * ROW_H;
    let content_w = total_w;
    scroll.y = scroll.y.clamp(0.0, (content_h - view_h).max(0.0));
    scroll.x = scroll.x.clamp(0.0, (content_w - view_w).max(0.0));
    let vtrack = Rect::from_min_max(pos2(grid_right, rows_top), pos2(body.right(), rows_bottom));
    let htrack = Rect::from_min_max(
        pos2(grid_left, rows_bottom),
        pos2(grid_right, body.bottom()),
    );
    let thumb = |track_len: f32, view: f32, content: f32, off: f32| -> (f32, f32) {
        if content <= view {
            return (0.0, track_len);
        }
        let len = (track_len * view / content).clamp(26.0, track_len);
        let pos = (track_len - len) * off / (content - view);
        (pos, len)
    };
    let (vp, vl) = thumb(vtrack.height(), view_h, content_h, scroll.y);
    let (hp, hl) = thumb(htrack.width(), view_w, content_w, scroll.x);
    Geom {
        body,
        mini,
        hint,
        letters: Rect::from_min_max(
            pos2(grid_left, letters_top),
            pos2(grid_right, letters_top + LETTER_H),
        ),
        nums: Rect::from_min_max(
            pos2(nums_left, rows_top),
            pos2(nums_left + NUM_W, rows_bottom),
        ),
        bands_x: (origin_x, origin_x + BAND_W - 3.0),
        grid: Rect::from_min_max(pos2(grid_left, rows_top), pos2(grid_right, rows_bottom)),
        rows_top,
        view_h,
        view_w,
        vtrack,
        htrack,
        vthumb: Rect::from_min_size(
            pos2(vtrack.left() + 1.0, vtrack.top() + vp),
            vec2(SB - 2.0, vl),
        ),
        hthumb: Rect::from_min_size(
            pos2(htrack.left() + hp, htrack.top() + 1.0),
            vec2(hl, SB - 2.0),
        ),
        content_h,
        content_w,
    }
}

/// Everything the grid needs from the state, read once per frame.
struct View<'a> {
    map: &'a FileMap,
    recipe: &'a ImportRecipe,
    info: Option<&'a AutoInfo>,
    props: &'a [PropRow],
    rows: &'a [Row],
    metrics: &'a Metrics,
    marker: Option<String>,
    bands: Vec<Band>,
    dark: bool,
    font: FontId,
    small: FontId,
    cw: f32,
    /// The roles of the file lines as the recipe holds them.
    spans: Option<Spans>,
}

fn build_bands(view_rows: &[Row], map: &FileMap, marker: Option<&str>) -> Vec<Band> {
    let mut bands: Vec<Band> = Vec::new();
    for (r, row) in view_rows.iter().enumerate() {
        let bits = match row {
            Row::Line(i) => zones_of(map, *i, marker),
            Row::Fold(..) => Zone::Data.bit(),
        };
        match bands.last_mut() {
            Some(last) if last.zones == bits => last.r1 = r,
            _ => bands.push(Band {
                r0: r,
                r1: r,
                zones: bits,
            }),
        }
    }
    bands
}

fn hatch(painter: &egui::Painter, rect: Rect, color: Color32) {
    let p = painter.with_clip_rect(rect);
    let step = 7.0;
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        p.line_segment(
            [pos2(x, rect.bottom()), pos2(x + rect.height(), rect.top())],
            Stroke::new(1.0, color.gamma_multiply(0.45)),
        );
        x += step;
    }
}

fn fit_text(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_string();
    }
    let keep = chars.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

/// What the grid says after a frame.
#[derive(Default)]
struct GridOut {
    cmds: Vec<GridCmd>,
    auto: Vec<AutoWhat>,
    check: Vec<check::CheckCmd>,
}

#[allow(clippy::too_many_arguments)]
fn file_grid(ui: &mut egui::Ui, body: Rect, v: &View<'_>, gs: &mut GridUi, out: &mut GridOut) {
    let ctx = ui.ctx().clone();
    let map = v.map;
    let rows = v.rows;
    let ncols = v.metrics.widths.len();
    let total_w = *v.metrics.colx.last().unwrap_or(&0.0);
    let mut scroll = gs.pan;
    let g = make_geom(body, rows.len(), total_w, &mut scroll);
    gs.pan = scroll;

    // ---- input ----
    let resp = ui.interact(body, gs_id().with("body"), Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let modifiers = ui.input(|i| i.modifiers);
    let over_body = pointer.is_some_and(|p| body.contains(p));
    if over_body && !gs.bar_rect.contains(pointer.unwrap_or(pos2(-1.0, -1.0))) {
        let delta = ui.input(|i| i.smooth_scroll_delta);
        if delta != egui::Vec2::ZERO {
            if modifiers.shift {
                gs.pan.x -= delta.x + delta.y;
            } else {
                gs.pan.x -= delta.x;
                gs.pan.y -= delta.y;
            }
        }
    }
    if let Some(line) = gs.follow_scroll_to.take() {
        let r = row_of_line(rows, line);
        gs.pan.y = (r as f32 * ROW_H - g.view_h / 2.0).max(0.0);
    }
    let mut scroll = gs.pan;
    let g = make_geom(body, rows.len(), total_w, &mut scroll);
    gs.pan = scroll;
    let sx = gs.pan.x;
    let scroll0 = gs.pan;

    let col_at = |x: f32| -> Option<usize> {
        let rel = x - g.grid.left() + sx;
        if rel < 0.0 {
            return None;
        }
        let c = v
            .metrics
            .colx
            .partition_point(|cx| *cx <= rel)
            .saturating_sub(1);
        (c < ncols).then_some(c)
    };
    let row_at = |y: f32| -> Option<usize> {
        let rel = y - g.rows_top + scroll0.y;
        if rel < 0.0 {
            return None;
        }
        let r = (rel / ROW_H) as usize;
        (r < rows.len()).then_some(r)
    };

    // Value cell geometry of a key:value line (screen space, may be off-screen).
    let value_rect = |kv: &Kv, r: usize| -> Rect {
        let x0 =
            g.grid.left() + v.metrics.colx[kv.cell.min(ncols - 1)] - sx + kv.offset as f32 * v.cw;
        let w = (kv.value.chars().count() as f32 * v.cw + 8.0).clamp(20.0, 600.0);
        Rect::from_min_size(pos2(x0, row_y(&g, scroll0, r) + 1.0), vec2(w, ROW_H - 2.0))
    };

    let hit_test = |pos: egui::Pos2, gs: &GridUi| -> Hit {
        if gs.bar_rect.contains(pos) {
            return Hit::None;
        }
        if g.vtrack.contains(pos) {
            return Hit::VBar(pos.y);
        }
        if g.htrack.contains(pos) {
            return Hit::HBar(pos.x);
        }
        if g.mini.contains(pos) {
            return Hit::Mini(pos.y);
        }
        for (rect, extra) in &gs.extras {
            if rect.contains(pos) {
                return match extra {
                    Extra::Pill(w, t) => Hit::Pill(w.clone(), t.clone()),
                    Extra::Tag(l) => Hit::Tag(*l),
                    Extra::TagPill(w, t) => Hit::TagPill(w.clone(), t.clone()),
                };
            }
        }
        if g.letters.contains(pos) {
            return col_at(pos.x).map_or(Hit::None, Hit::Letter);
        }
        // Data boundary handles.
        if let Some(handles) = handle_rects(&g, scroll0, v) {
            if handles.0.expand(3.0).contains(pos) {
                return Hit::HandleStart;
            }
            if handles.1.expand(3.0).contains(pos) {
                return Hit::HandleEnd;
            }
        }
        if pos.y >= g.rows_top && pos.y < g.rows_top + g.view_h {
            let Some(r) = row_at(pos.y) else {
                return Hit::None;
            };
            if pos.x >= g.bands_x.0 && pos.x < g.bands_x.1 {
                return v
                    .bands
                    .iter()
                    .position(|b| b.r0 <= r && r <= b.r1)
                    .map_or(Hit::None, Hit::Band);
            }
            if g.nums.contains(pos) || (pos.x >= g.nums.left() && pos.x < g.grid.left()) {
                return match rows[r] {
                    Row::Line(i) => Hit::Num(i),
                    Row::Fold(..) => Hit::Fold,
                };
            }
            if g.grid.contains(pos) {
                return match rows[r] {
                    Row::Fold(..) => Hit::Fold,
                    Row::Line(i) => {
                        if map.roles.get(i) != Some(&LineRole::Data) {
                            if let Some(kv) = kv_of(map, v.recipe, i) {
                                if value_rect(&kv, r).contains(pos) {
                                    return Hit::Value(i);
                                }
                            }
                        }
                        col_at(pos.x).map_or(Hit::Num(i), |c| Hit::Cell(i, c))
                    }
                };
            }
        }
        Hit::None
    };

    let hit_now = pointer.map(|p| hit_test(p, gs)).unwrap_or(Hit::None);

    // Cursor and tooltip.
    let dragging_handle = matches!(gs.drag, Some(Drag::HandleStart | Drag::HandleEnd));
    match &hit_now {
        Hit::HandleStart | Hit::HandleEnd => ctx.set_cursor_icon(cursors::RESIZE_VERTICAL),
        Hit::Pill(..)
        | Hit::TagPill(..)
        | Hit::Tag(_)
        | Hit::Value(_)
        | Hit::Fold
        | Hit::Band(_)
        | Hit::Letter(_) => {
            let _ = cursors::clickable(resp.clone());
        }
        _ => {}
    }
    if dragging_handle {
        ctx.set_cursor_icon(cursors::RESIZE_VERTICAL);
    }

    // ---- press / drag ----
    if resp.drag_started() {
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .unwrap_or(pos2(-1.0, -1.0));
        gs.drag = match hit_test(origin, gs) {
            Hit::HandleStart => {
                gs.frozen_fold = Some(if gs.unfolded { None } else { fold_range(map) });
                Some(Drag::HandleStart)
            }
            Hit::HandleEnd => {
                gs.frozen_fold = Some(if gs.unfolded { None } else { fold_range(map) });
                Some(Drag::HandleEnd)
            }
            Hit::Mini(_) => Some(Drag::Mini),
            Hit::VBar(y) => {
                let grab = if g.vthumb.contains(pos2(g.vthumb.center().x, y)) {
                    y - g.vthumb.top()
                } else {
                    g.vthumb.height() / 2.0
                };
                Some(Drag::VBar(grab))
            }
            Hit::HBar(x) => {
                let grab = if g.hthumb.contains(pos2(x, g.hthumb.center().y)) {
                    x - g.hthumb.left()
                } else {
                    g.hthumb.width() / 2.0
                };
                Some(Drag::HBar(grab))
            }
            Hit::Letter(c) => {
                let mut set = BTreeSet::new();
                if modifiers.ctrl || modifiers.command {
                    if let Sel::Cols(old) = &gs.sel {
                        set = old.clone();
                    }
                }
                set.insert(c);
                gs.anchor = c;
                gs.sel = Sel::Cols(set);
                Some(Drag::Cols)
            }
            Hit::Num(i) | Hit::Cell(i, _) => {
                let n = i + 1;
                if modifiers.shift && gs.anchor != 0 {
                    // Extends from the anchor, as a click would.
                } else {
                    let mut set = BTreeSet::new();
                    if modifiers.ctrl || modifiers.command {
                        if let Sel::Lines(old) = &gs.sel {
                            set = old.clone();
                        }
                    }
                    set.insert(n);
                    gs.anchor = n;
                    gs.sel = Sel::Lines(set);
                }
                gs.note.clear();
                Some(Drag::Lines)
            }
            _ => None,
        };
    }
    if resp.dragged() {
        if let (Some(drag), Some(p)) = (gs.drag.clone(), pointer) {
            // Auto-scroll when the pointer leaves the rows while dragging.
            if matches!(drag, Drag::Lines | Drag::HandleStart | Drag::HandleEnd) {
                if p.y < g.rows_top + 12.0 {
                    gs.pan.y -= 10.0;
                    ctx.request_repaint();
                } else if p.y > g.rows_top + g.view_h - 12.0 {
                    gs.pan.y += 10.0;
                    ctx.request_repaint();
                }
            }
            match drag {
                Drag::Lines => {
                    if let Some(r) = row_at(p.y.clamp(g.rows_top, g.rows_top + g.view_h - 1.0)) {
                        let line = match rows[r] {
                            Row::Line(i) => i + 1,
                            Row::Fold(from, to) => {
                                if p.y > row_y(&g, scroll0, r) + ROW_H / 2.0 {
                                    to + 1
                                } else {
                                    from + 1
                                }
                            }
                        };
                        let (a, b) = (gs.anchor.min(line), gs.anchor.max(line));
                        let keep: BTreeSet<usize> = if modifiers.ctrl || modifiers.command {
                            match &gs.sel {
                                Sel::Lines(old) => old.clone(),
                                _ => BTreeSet::new(),
                            }
                        } else {
                            BTreeSet::new()
                        };
                        let mut set = keep;
                        for n in a..=b {
                            set.insert(n);
                        }
                        gs.sel = Sel::Lines(set);
                    }
                }
                Drag::Cols => {
                    if let Some(c) = col_at(p.x.clamp(g.grid.left(), g.grid.right() - 1.0)) {
                        let (a, b) = (gs.anchor.min(c), gs.anchor.max(c));
                        gs.sel = Sel::Cols((a..=b).collect());
                    }
                    if p.x < g.grid.left() + 12.0 {
                        gs.pan.x -= 12.0;
                        ctx.request_repaint();
                    } else if p.x > g.grid.right() - 12.0 {
                        gs.pan.x += 12.0;
                        ctx.request_repaint();
                    }
                }
                Drag::HandleStart | Drag::HandleEnd => {
                    if let Some((a, b)) = map.data_range {
                        let rel = p.y - g.rows_top + scroll0.y;
                        let boundary = ((rel / ROW_H).round().max(0.0) as usize).min(rows.len());
                        let line0 = boundary_line(rows, map, boundary);
                        let (cur_first, cur_last) = gs.handle_plan.unwrap_or((a + 1, b + 1));
                        let plan = if drag == Drag::HandleStart {
                            // New first data line (1-based), at most the last one.
                            let first = (line0 + 1).min(cur_last).max(map.first + 2);
                            (first, cur_last)
                        } else {
                            let last = line0.max(cur_first).min(map.line_count());
                            (cur_first, last)
                        };
                        if Some(plan) != gs.handle_plan {
                            gs.handle_plan = Some(plan);
                            out.cmds.push(GridCmd::Handle {
                                first: plan.0,
                                last: plan.1,
                            });
                        }
                    }
                }
                Drag::Mini => {
                    mini_jump(gs, &g, v, p.y, map);
                }
                Drag::VBar(grab) => {
                    let track = g.vtrack.height() - g.vthumb.height();
                    if track > 0.0 {
                        let t = ((p.y - grab - g.vtrack.top()) / track).clamp(0.0, 1.0);
                        gs.pan.y = t * (g.content_h - g.view_h).max(0.0);
                    }
                }
                Drag::HBar(grab) => {
                    let track = g.htrack.width() - g.hthumb.width();
                    if track > 0.0 {
                        let t = ((p.x - grab - g.htrack.left()) / track).clamp(0.0, 1.0);
                        gs.pan.x = t * (g.content_w - g.view_w).max(0.0);
                    }
                }
            }
        }
    }
    if resp.drag_stopped() {
        gs.drag = None;
        gs.handle_plan = None;
        gs.frozen_fold = None;
    }

    // ---- click ----
    if resp.clicked() {
        let pos = ui.input(|i| i.pointer.interact_pos()).or(pointer);
        let hit = pos.map(|p| hit_test(p, gs)).unwrap_or(Hit::None);
        let ctrl = modifiers.ctrl || modifiers.command;
        match hit {
            Hit::Pill(what, why) | Hit::TagPill(what, why) => {
                // Explain first; deleting is a separate, explicit action.
                let anchor = pos
                    .and_then(|p| {
                        gs.extras
                            .iter()
                            .find(|(r, _)| r.contains(p))
                            .map(|(r, _)| *r)
                    })
                    .unwrap_or(Rect::from_min_size(
                        pos.unwrap_or(body.center()),
                        vec2(1.0, 1.0),
                    ));
                gs.auto_why = Some(AutoWhy {
                    anchor,
                    what,
                    why,
                    pan: gs.pan,
                });
            }
            Hit::Fold => gs.unfolded = true,
            Hit::Mini(y) => {
                if let Some(p) = pos {
                    let _ = y;
                    mini_jump(gs, &g, v, p.y, map);
                }
            }
            Hit::Num(i) | Hit::Cell(i, _) if gs.pick_active => {
                out.check.push(check::CheckCmd::Picked(i + 1));
            }
            Hit::Num(i) | Hit::Cell(i, _) => {
                let n = i + 1;
                gs.note.clear();
                if modifiers.shift && gs.anchor != 0 {
                    let (a, b) = (gs.anchor.min(n), gs.anchor.max(n));
                    gs.sel = Sel::Lines((a..=b).collect());
                } else if ctrl {
                    let mut set = match &gs.sel {
                        Sel::Lines(old) => old.clone(),
                        _ => BTreeSet::new(),
                    };
                    if !set.remove(&n) {
                        set.insert(n);
                    }
                    gs.anchor = n;
                    gs.sel = if set.is_empty() {
                        Sel::None
                    } else {
                        Sel::Lines(set)
                    };
                } else {
                    gs.anchor = n;
                    gs.sel = Sel::Lines(std::iter::once(n).collect());
                }
            }
            Hit::Value(i) | Hit::Tag(i) => {
                if let Some(kv) = kv_of(map, v.recipe, i) {
                    gs.sel = Sel::Value(ValueSel {
                        line: i,
                        key: kv.key,
                        value: kv.value,
                        cell: kv.cell,
                        offset: kv.offset,
                    });
                }
            }
            Hit::Letter(c) => {
                if modifiers.shift && gs.anchor != usize::MAX {
                    let (a, b) = (gs.anchor.min(c), gs.anchor.max(c));
                    gs.sel = Sel::Cols((a..=b).collect());
                } else if ctrl {
                    let mut set = match &gs.sel {
                        Sel::Cols(old) => old.clone(),
                        _ => BTreeSet::new(),
                    };
                    if !set.remove(&c) {
                        set.insert(c);
                    }
                    gs.sel = if set.is_empty() {
                        Sel::None
                    } else {
                        Sel::Cols(set)
                    };
                    gs.anchor = c;
                } else {
                    gs.anchor = c;
                    gs.sel = Sel::Cols(std::iter::once(c).collect());
                }
            }
            Hit::Band(b) => {
                if let Some(band) = v.bands.get(b) {
                    let first = match rows[band.r0] {
                        Row::Line(i) => i,
                        Row::Fold(from, _) => from,
                    };
                    let last = match rows[band.r1] {
                        Row::Line(i) => i,
                        Row::Fold(_, to) => to,
                    };
                    gs.anchor = first + 1;
                    gs.sel = Sel::Lines((first + 1..=last + 1).collect());
                }
            }
            _ => {
                if matches!(hit, Hit::None) {
                    gs.sel = Sel::None;
                }
            }
        }
    }

    // ---- right click: pick the target, the menu is drawn by the caller ----
    if resp.secondary_clicked() {
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()).or(pointer) {
            match hit_test(p, gs) {
                Hit::Num(i) | Hit::Cell(i, _) => {
                    let n = i + 1;
                    let inside = matches!(&gs.sel, Sel::Lines(set) if set.contains(&n));
                    if !inside {
                        gs.anchor = n;
                        gs.sel = Sel::Lines(std::iter::once(n).collect());
                    }
                }
                Hit::Value(i) | Hit::Tag(i) => {
                    if let Some(kv) = kv_of(map, v.recipe, i) {
                        gs.menu_info_line = Some((i, kv.key.clone()));
                        gs.sel = Sel::Value(ValueSel {
                            line: i,
                            key: kv.key,
                            value: kv.value,
                            cell: kv.cell,
                            offset: kv.offset,
                        });
                    }
                }
                Hit::Letter(c) => {
                    let inside = matches!(&gs.sel, Sel::Cols(set) if set.contains(&c));
                    if !inside {
                        gs.anchor = c;
                        gs.sel = Sel::Cols(std::iter::once(c).collect());
                    }
                }
                _ => {}
            }
            // Info lines: remember the key for « do not keep this info ».
            if let Sel::Lines(set) = &gs.sel {
                gs.menu_info_line = set.iter().next().and_then(|n| {
                    let i = n - 1;
                    kv_of(map, v.recipe, i).map(|kv| (i, kv.key))
                });
            }
        }
    }

    // Tooltips.
    let tip = match &hit_now {
        Hit::Pill(_, why) | Hit::TagPill(_, why) => Some(why.clone()),
        Hit::HandleStart | Hit::HandleEnd if !dragging_handle => Some(
            map.data_range
                .map(|(a, b)| {
                    let (from, to): (usize, usize) = (a + 1, b + 1);
                    tr!("import-grid-handle-tip", from = from, to = to)
                })
                .unwrap_or_default(),
        ),
        Hit::Band(b) => v.bands.get(*b).map(|band| {
            zones_in(band.zones)
                .iter()
                .map(|z| z.label())
                .collect::<Vec<_>>()
                .join(" + ")
        }),
        Hit::Cell(i, c) => {
            let tokens = map.tokens(*i, v.recipe);
            tokens.get(*c).and_then(|t| {
                let t = t.trim();
                let w = v.metrics.widths.get(*c).copied().unwrap_or(0.0);
                let is_data = map.roles.get(*i) == Some(&LineRole::Data);
                let text_kept = is_data && map.token_is_text(*c, t, v.recipe);
                let number = is_data.then(|| map.token_number(*c, t, v.recipe)).flatten();
                if text_kept {
                    Some(tr!("import-grid-cell-text", text = t))
                } else if let Some(n) = number {
                    Some(tr!("import-grid-cell-number", value = format!("{n}")))
                } else if t.chars().count() as f32 * v.cw + 8.0 > w {
                    Some(t.to_string())
                } else {
                    None
                }
            })
        }
        _ => None,
    };
    if let Some(tip) = tip.filter(|t| !t.is_empty()) {
        if gs.drag.is_none() {
            resp.clone().on_hover_text_at_pointer(tip);
        }
    }

    // ---- paint ----
    paint_grid(ui, &g, v, gs, &hit_now, pointer, &value_rect);
    context_menu(&resp, v, gs, out);

    // A tooltip while a boundary is dragged.
    if dragging_handle {
        if let (Some((a, b)), Some(p)) = (
            gs.handle_plan
                .or_else(|| map.data_range.map(|(a, b)| (a + 1, b + 1))),
            pointer,
        ) {
            let text = tr!("import-grid-handle-tip", from = a, to = b);
            let painter = ui.painter_at(body);
            let galley = painter.layout_no_wrap(text, v.font.clone(), ui.visuals().text_color());
            let size = galley.size() + vec2(14.0, 8.0);
            let pos = pos2(
                (p.x + 16.0).min(body.right() - size.x - 4.0),
                (p.y - size.y - 6.0).max(body.top() + 2.0),
            );
            let rect = Rect::from_min_size(pos, size);
            painter.rect_filled(rect, 5.0, ui.visuals().window_fill());
            painter.rect_stroke(
                rect,
                5.0,
                Stroke::new(1.0, crate::theme::accent()),
                StrokeKind::Inside,
            );
            painter.galley(rect.min + vec2(7.0, 4.0), galley, ui.visuals().text_color());
        }
    }
}

/// Vertical scale of the file bar. While the data is folded in the grid, the
/// folded lines take a short fixed band in the bar (with the cut mark), so the
/// header and footer lines get the room. Unfolded, the scale is the true one.
struct MiniScale {
    top: f32,
    per_line: f32,
    n: usize,
    /// Folded file lines `from..=to`.
    fold: Option<(usize, usize)>,
}

const MINI_FOLD_BAND: f32 = 16.0;

impl MiniScale {
    fn new(inner_top: f32, inner_h: f32, n: usize, rows: &[Row]) -> Self {
        let fold = rows.iter().find_map(|r| match r {
            Row::Fold(a, b) => Some((*a, *b)),
            Row::Line(_) => None,
        });
        let fold = fold.filter(|_| inner_h > MINI_FOLD_BAND * 3.0);
        let kept = match fold {
            Some((a, b)) => n - (b + 1 - a),
            None => n,
        };
        let room = if fold.is_some() {
            inner_h - MINI_FOLD_BAND
        } else {
            inner_h
        };
        Self {
            top: inner_top,
            per_line: room / kept.max(1) as f32,
            n,
            fold,
        }
    }

    /// Top of file line `i` (`i == n` is the bottom).
    fn y(&self, i: usize) -> f32 {
        match self.fold {
            Some((a, b)) if i > b => {
                self.top + (i - (b + 1 - a)) as f32 * self.per_line + MINI_FOLD_BAND
            }
            Some((a, b)) if i > a => {
                let t = (i - a) as f32 / (b + 1 - a) as f32;
                self.top + a as f32 * self.per_line + t * MINI_FOLD_BAND
            }
            _ => self.top + i as f32 * self.per_line,
        }
    }

    /// The band of the folded lines, if any.
    fn band(&self) -> Option<(f32, f32)> {
        self.fold.map(|(a, b)| (self.y(a), self.y(b + 1)))
    }

    /// File line under bar height `y`, and whether it lies in the folded band.
    fn line_at(&self, y: f32) -> (usize, bool) {
        if let (Some((a, b)), Some((y0, y1))) = (self.fold, self.band()) {
            if y >= y0 && y < y1 {
                let t = (y - y0) / (y1 - y0).max(1.0);
                return (a + (t * (b + 1 - a) as f32) as usize, true);
            }
            if y >= y1 {
                let i = b + 1 + ((y - y1) / self.per_line.max(0.01)) as usize;
                return (i.min(self.n - 1), false);
            }
        }
        let i = ((y - self.top) / self.per_line.max(0.01)).max(0.0) as usize;
        (i.min(self.n - 1), false)
    }
}

fn mini_inner(g: &Geom) -> Rect {
    Rect::from_min_max(
        pos2(g.mini.left() + 6.0, g.mini.top() + 4.0),
        pos2(g.mini.right() - 6.0, g.mini.bottom() - 22.0),
    )
}

fn mini_jump(gs: &mut GridUi, g: &Geom, v: &View<'_>, y: f32, map: &FileMap) {
    let n = map.line_count().max(1);
    let inner = mini_inner(g);
    let scale = MiniScale::new(inner.top(), inner.height(), n, v.rows);
    let (line, _) = scale.line_at(y);
    // The folded band maps to the fold row: the bar never unfolds the data.
    let r = row_of_line(v.rows, line);
    gs.pan.y = (r as f32 * ROW_H - g.view_h / 2.0).max(0.0);
}

/// The two handles of the data band, when they are on screen.
fn handle_rects(g: &Geom, scroll: egui::Vec2, v: &View<'_>) -> Option<(Rect, Rect)> {
    let band = v.bands.iter().find(|b| b.zones == Zone::Data.bit())?;
    let (x0, x1) = g.bands_x;
    let cx = (x0 + x1) / 2.0;
    let top = row_y(g, scroll, band.r0);
    let bottom = row_y(g, scroll, band.r1) + ROW_H;
    let mk = |y: f32| Rect::from_center_size(pos2(cx, y), vec2(38.0, 11.0));
    let (h0, h1) = (mk(top), mk(bottom));
    let visible =
        |r: Rect| r.center().y >= g.rows_top - 1.0 && r.center().y <= g.rows_top + g.view_h + 1.0;
    let r0 = if visible(h0) { h0 } else { Rect::NOTHING };
    let r1 = if visible(h1) { h1 } else { Rect::NOTHING };
    Some((r0, r1))
}

#[allow(clippy::too_many_arguments)]
fn paint_grid(
    ui: &mut egui::Ui,
    g: &Geom,
    v: &View<'_>,
    gs: &mut GridUi,
    hit: &Hit,
    pointer: Option<egui::Pos2>,
    value_rect: &dyn Fn(&Kv, usize) -> Rect,
) {
    let map = v.map;
    let rows = v.rows;
    let dark = v.dark;
    let visuals = ui.visuals().clone();
    let weak = visuals.weak_text_color();
    let text_color = visuals.text_color();
    let sel_fill = visuals.selection.bg_fill;
    let accent = crate::theme::accent();
    let panel = visuals.panel_fill;
    let sx = gs.pan.x;
    let ncols = v.metrics.widths.len();
    let painter = ui.painter().clone();

    // Backgrounds.
    painter.rect_filled(g.body, 0.0, visuals.extreme_bg_color);
    painter.rect_filled(
        Rect::from_min_max(
            pos2(g.hint.left() - 3.0, g.hint.top()),
            pos2(g.body.right(), g.letters.bottom()),
        ),
        0.0,
        panel,
    );

    // Visible rows and columns.
    let visible = visible_rows(gs.pan.y, g.view_h, rows.len());
    let (r_first, r_last) = (visible.start, visible.end);
    let (c_first, c_last) = visible_columns(&v.metrics.colx, sx, g.view_w, ncols);

    let grid_p = painter.with_clip_rect(g.grid);
    let gutter_rect = Rect::from_min_max(
        pos2(g.bands_x.0, g.rows_top),
        pos2(g.grid.left() - 2.0, g.rows_top + g.view_h),
    );
    let gutter_p = painter.with_clip_rect(gutter_rect);
    gutter_p.rect_filled(gutter_rect, 0.0, panel);

    // Selection sets.
    let sel_lines: Option<&BTreeSet<usize>> = match &gs.sel {
        Sel::Lines(s) => Some(s),
        _ => None,
    };
    let sel_cols: Option<&BTreeSet<usize>> = match &gs.sel {
        Sel::Cols(s) => Some(s),
        _ => None,
    };

    // Properties by line, and the sheet-name line.
    let mut prop_by_line: std::collections::HashMap<usize, Vec<&PropRow>> =
        std::collections::HashMap::new();
    for row in v.props {
        if let Some(line) = row.line {
            prop_by_line.entry(line - 1).or_default().push(row);
        }
    }
    let sheet_line: Option<usize> = match &v.recipe.name_source {
        NameSource::HeaderKey(key) => {
            let wanted = norm_key(key);
            (0..map.line_count().min(400)).find(|i| {
                split_kv(map.line(*i))
                    .is_some_and(|(k, val)| norm_key(k) == wanted && !val.is_empty())
            })
        }
        NameSource::Filename => None,
    };
    let excluded: Vec<String> = v
        .recipe
        .excluded_metadata
        .iter()
        .map(|k| norm_key(k))
        .collect();

    let mut extras: Vec<(Rect, Extra)> = Vec::new();
    let marker = v.marker.as_deref();

    for r in r_first..r_last {
        let y = row_y(g, gs.pan, r);
        let row_rect = Rect::from_min_size(pos2(g.grid.left(), y), vec2(g.grid.width(), ROW_H));
        let num_rect = Rect::from_min_size(pos2(g.nums.left(), y), vec2(NUM_W, ROW_H));
        match rows[r] {
            Row::Fold(from, to) => {
                let count = to - from + 1;
                let teal = zone_color(Zone::Data, dark);
                let full =
                    Rect::from_min_max(pos2(g.nums.left(), y), pos2(g.grid.right(), y + ROW_H));
                painter
                    .with_clip_rect(Rect::from_min_max(
                        pos2(g.nums.left(), g.rows_top),
                        pos2(g.grid.right(), g.rows_top + g.view_h),
                    ))
                    .rect_filled(full.shrink2(vec2(0.0, 1.0)), 4.0, teal.gamma_multiply(0.12));
                let hovered = matches!(hit, Hit::Fold) && pointer.is_some_and(|p| full.contains(p));
                if hovered {
                    painter.with_clip_rect(full).rect_stroke(
                        full.shrink2(vec2(0.0, 1.0)),
                        4.0,
                        Stroke::new(1.0, teal),
                        StrokeKind::Inside,
                    );
                }
                painter.with_clip_rect(full).text(
                    pos2(g.grid.left() + 12.0, y + ROW_H / 2.0),
                    Align2::LEFT_CENTER,
                    tr!("import-grid-fold-row", count = count),
                    v.font.clone(),
                    teal,
                );
                gutter_p.text(
                    pos2(g.nums.right() - 6.0, y + ROW_H / 2.0),
                    Align2::RIGHT_CENTER,
                    "⋯",
                    v.small.clone(),
                    weak,
                );
            }
            Row::Line(i) => {
                let bits = zones_of(map, i, marker);
                let zones = zones_in(bits);
                let line_no = i + 1;
                let selected = sel_lines.is_some_and(|s| s.contains(&line_no));
                let is_data = map.roles.get(i) == Some(&LineRole::Data);
                // Row tint by role.
                for z in &zones {
                    if z.tints_row() {
                        let c = zone_color(*z, dark)
                            .gamma_multiply(0.10 + 0.04 * (zones.len() > 1) as u8 as f32);
                        grid_p.rect_filled(row_rect, 0.0, c);
                        gutter_p.rect_filled(num_rect, 0.0, c);
                    }
                }
                if selected {
                    grid_p.rect_filled(row_rect, 0.0, sel_fill.gamma_multiply(0.55));
                    gutter_p.rect_filled(num_rect, 0.0, sel_fill.gamma_multiply(0.55));
                }
                // Line number.
                gutter_p.text(
                    pos2(g.nums.right() - 6.0, y + ROW_H / 2.0),
                    Align2::RIGHT_CENTER,
                    line_no.to_string(),
                    v.small.clone(),
                    if selected { text_color } else { weak },
                );
                // Cells.
                let tokens = map.tokens(i, v.recipe);
                let footer_like = bits & (Zone::Footer.bit() | Zone::Ignored.bit()) != 0;
                let kv = if !is_data {
                    kv_of(map, v.recipe, i)
                } else {
                    None
                };
                let excluded_line = kv
                    .as_ref()
                    .is_some_and(|kv| excluded.contains(&norm_key(&kv.key)));
                let mut strike_end = 0.0f32;
                for c in c_first..c_last.min(tokens.len()) {
                    let tok = tokens[c].trim();
                    let x = g.grid.left() + v.metrics.colx[c] - sx;
                    let w = v.metrics.widths[c];
                    // Data cell tints: X column, ignored columns.
                    let role = v.recipe.column_role(c);
                    if is_data && role == ColumnRoleChoice::X {
                        grid_p.rect_filled(
                            Rect::from_min_size(pos2(x - 2.0, y), vec2(w + 4.0, ROW_H)),
                            0.0,
                            zone_color(Zone::Data, dark).gamma_multiply(0.09),
                        );
                    }
                    if tok.is_empty() {
                        continue;
                    }
                    let mut avail = w;
                    if !is_data {
                        let mut k = c + 1;
                        while k < tokens.len() && tokens[k].trim().is_empty() {
                            avail += GAP + v.metrics.widths.get(k).copied().unwrap_or(60.0);
                            k += 1;
                        }
                        if k >= tokens.len() {
                            avail += 900.0;
                        }
                    }
                    let max_chars = (((avail - 6.0) / v.cw).floor().max(1.0)) as usize;
                    let text = fit_text(tok, max_chars);
                    let kept = is_data && map.token_is_text(c, tok, v.recipe);
                    // A cell read as a number is right-aligned, as in a spreadsheet.
                    let is_number =
                        is_data && !kept && map.token_number(c, tok, v.recipe).is_some();
                    let mut color = if role == ColumnRoleChoice::Ignore {
                        weak.gamma_multiply(0.6)
                    } else if kept {
                        visuals.warn_fg_color
                    } else if footer_like {
                        weak
                    } else {
                        text_color
                    };
                    if let Some(kv) = &kv {
                        if kv.cell == c {
                            color = text_color;
                        } else if c == 0 {
                            color = weak.gamma_multiply(1.2);
                        }
                    }
                    if kept {
                        grid_p.rect_filled(
                            Rect::from_min_size(pos2(x - 1.0, y + 2.0), vec2(w + 2.0, ROW_H - 4.0)),
                            3.0,
                            visuals.warn_fg_color.gamma_multiply(0.15),
                        );
                    }
                    let shown_w = text.chars().count() as f32 * v.cw;
                    let (anchor, text_x) = if is_number {
                        (Align2::RIGHT_CENTER, x + w - 3.0)
                    } else {
                        (Align2::LEFT_CENTER, x + 3.0)
                    };
                    grid_p.text(
                        pos2(text_x, y + ROW_H / 2.0),
                        anchor,
                        text,
                        v.font.clone(),
                        color,
                    );
                    strike_end = strike_end.max(x + 3.0 + shown_w);
                    // The delimiter, faint, in the gap.
                    let next_filled = tokens.get(c + 1).is_some();
                    let fits = tok.chars().count() as f32 * v.cw <= w - 4.0;
                    if next_filled
                        && (fits || !tokens.get(c + 1).is_some_and(|t| t.trim().is_empty()))
                    {
                        grid_p.text(
                            pos2(x + w + GAP / 2.0, y + ROW_H / 2.0),
                            Align2::CENTER_CENTER,
                            map.separator_glyph(),
                            v.small.clone(),
                            weak.gamma_multiply(0.45),
                        );
                    }
                }
                // Gaps after empty tokens still carry a delimiter.
                for c in c_first..c_last.min(tokens.len().saturating_sub(1)) {
                    if tokens[c].trim().is_empty() {
                        let x = g.grid.left() + v.metrics.colx[c] - sx;
                        let w = v.metrics.widths[c];
                        grid_p.text(
                            pos2(x + w + GAP / 2.0, y + ROW_H / 2.0),
                            Align2::CENTER_CENTER,
                            map.separator_glyph(),
                            v.small.clone(),
                            weak.gamma_multiply(0.45),
                        );
                    }
                }
                if excluded_line {
                    let x0 = g.grid.left() + 3.0 - sx;
                    grid_p.line_segment(
                        [
                            pos2(x0, y + ROW_H / 2.0),
                            pos2(strike_end.max(x0 + 20.0), y + ROW_H / 2.0),
                        ],
                        Stroke::new(1.2, weak),
                    );
                }
                // Value cells used by a property: outline and tag.
                if let Some(kv) = &kv {
                    let rows_here = prop_by_line.get(&i);
                    let is_sheet = sheet_line == Some(i);
                    if rows_here.is_some() || is_sheet {
                        let vr = value_rect(kv, r);
                        let main_color = rows_here
                            .and_then(|rs| rs.first())
                            .map(|row| prop_color(dark, row.prop.field))
                            .unwrap_or(accent);
                        grid_p.rect_stroke(
                            vr.expand2(vec2(1.0, 0.0)),
                            5.0,
                            Stroke::new(1.5, main_color),
                            StrokeKind::Inside,
                        );
                        let mut tx = vr.right() + 8.0;
                        let tag_rect_start = tx;
                        let tag_y = y + ROW_H / 2.0;
                        let mut labels: Vec<(String, Color32, Option<(AutoWhat, String)>)> =
                            Vec::new();
                        if let Some(rs) = rows_here {
                            for row in rs {
                                let (target, field, result): (String, String, String) = (
                                    prop_target_label(row.prop.target),
                                    prop_field_label(row.prop.field),
                                    row.result.clone(),
                                );
                                labels.push((
                                    tr!(
                                        "import-auto-prop-suffix",
                                        target = target,
                                        field = field,
                                        result = result
                                    ),
                                    prop_color(dark, row.prop.field),
                                    row.pill.clone().map(|why| (row.pill_what.clone(), why)),
                                ));
                            }
                        }
                        if is_sheet {
                            let pill = v
                                .info
                                .and_then(|info| info.pill(&AutoWhat::NameSource))
                                .map(|p| (AutoWhat::NameSource, p.text.clone()));
                            labels.push((tr!("import-grid-sheet-tag"), accent, pill));
                        }
                        for (text, color, pill) in labels {
                            let galley = grid_p.layout_no_wrap(text, v.small.clone(), color);
                            let tag = Rect::from_min_size(
                                pos2(tx, tag_y - galley.size().y / 2.0 - 1.0),
                                galley.size() + vec2(8.0, 2.0),
                            );
                            grid_p.rect_filled(tag, 4.0, color.gamma_multiply(0.14));
                            grid_p.galley(tag.min + vec2(4.0, 1.0), galley, color);
                            extras.push((tag, Extra::Tag(i)));
                            tx = tag.right() + 4.0;
                            if let Some((what, why)) = pill {
                                let pr = Rect::from_min_size(
                                    pos2(tx, tag_y - 7.0),
                                    vec2(PILL_WIDTH, 14.0),
                                );
                                let hovered = pointer.is_some_and(|p| pr.contains(p));
                                paint_pill_on(&grid_p, pr, hovered, &visuals);
                                extras.push((pr, Extra::TagPill(what, why)));
                                tx = pr.right() + 6.0;
                            }
                        }
                        let _ = tag_rect_start;
                    }
                }
                // Value selected.
                if let Sel::Value(vs) = &gs.sel {
                    if vs.line == i {
                        let kv = Kv {
                            key: vs.key.clone(),
                            value: vs.value.clone(),
                            cell: vs.cell,
                            offset: vs.offset,
                        };
                        let vr = value_rect(&kv, r);
                        grid_p.rect_filled(vr, 4.0, sel_fill.gamma_multiply(0.6));
                        grid_p.rect_stroke(vr, 4.0, Stroke::new(2.0, accent), StrokeKind::Inside);
                    }
                }
                // Hover wash on a line.
                if let Hit::Num(h) | Hit::Cell(h, _) = hit {
                    if *h == i && !selected {
                        grid_p.rect_filled(
                            row_rect,
                            0.0,
                            visuals.widgets.hovered.bg_fill.gamma_multiply(0.25),
                        );
                    }
                }
            }
        }
    }

    // Selected lines: one outline per contiguous run of display rows.
    if let Some(set) = sel_lines {
        let mut run: Option<(usize, usize)> = None;
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for r in 0..rows.len() {
            let on = match rows[r] {
                Row::Line(i) => set.contains(&(i + 1)),
                Row::Fold(from, to) => (from..=to).all(|l| set.contains(&(l + 1))),
            };
            match (on, run) {
                (true, Some((a, _))) => run = Some((a, r)),
                (true, None) => run = Some((r, r)),
                (false, Some(done)) => {
                    runs.push(done);
                    run = None;
                }
                (false, None) => {}
            }
        }
        if let Some(done) = run {
            runs.push(done);
        }
        let clip = Rect::from_min_max(
            pos2(g.nums.left() - 2.0, g.rows_top),
            pos2(g.grid.right(), g.rows_top + g.view_h),
        );
        let sp = painter.with_clip_rect(clip);
        for (a, b) in runs {
            let rect = Rect::from_min_max(
                pos2(g.nums.left() - 1.0, row_y(g, gs.pan, a)),
                pos2(g.grid.right() - 1.0, row_y(g, gs.pan, b) + ROW_H),
            );
            sp.rect_stroke(rect, 3.0, Stroke::new(2.0, accent), StrokeKind::Inside);
        }
    }

    // Letters.
    let letters_p = painter.with_clip_rect(g.letters);
    painter.rect_filled(g.letters, 0.0, panel);
    for c in c_first..c_last {
        let x = g.grid.left() + v.metrics.colx[c] - sx;
        let w = v.metrics.widths[c];
        let rect = Rect::from_min_size(
            pos2(x - 2.0, g.letters.top() + 1.0),
            vec2(w + 4.0, LETTER_H - 2.0),
        );
        let selected = sel_cols.is_some_and(|s| s.contains(&c));
        let role = v.recipe.column_role(c);
        let fill = if selected {
            sel_fill
        } else if matches!(hit, Hit::Letter(h) if *h == c) {
            visuals.widgets.hovered.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        letters_p.rect_filled(rect, 3.0, fill);
        let color = if role == ColumnRoleChoice::Ignore {
            weak.gamma_multiply(0.6)
        } else {
            text_color
        };
        letters_p.text(
            pos2(rect.left() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            column_letters(c),
            v.font.clone(),
            color,
        );
        let tag = match role {
            ColumnRoleChoice::X => Some(("X", zone_color(Zone::Data, dark))),
            ColumnRoleChoice::Ignore => Some(("–", weak)),
            ColumnRoleChoice::Y => None,
            ColumnRoleChoice::XError => Some(("x±", weak)),
            ColumnRoleChoice::YError => Some(("y±", weak)),
            ColumnRoleChoice::Auto => None,
        };
        if let Some((t, col)) = tag {
            letters_p.text(
                pos2(rect.right() - 4.0, rect.center().y),
                Align2::RIGHT_CENTER,
                t,
                v.small.clone(),
                col,
            );
        }
        if v.recipe.column_kind(c) == ColumnKind::Text {
            letters_p.text(
                pos2(rect.right() - 20.0, rect.center().y),
                Align2::RIGHT_CENTER,
                "T",
                v.small.clone(),
                visuals.warn_fg_color,
            );
        }
        if selected {
            // The column, outlined over the visible rows.
            let col_rect = Rect::from_min_max(
                pos2(x - 2.0, g.rows_top),
                pos2(x + w + 2.0, g.rows_top + g.view_h),
            );
            let gp = painter.with_clip_rect(g.grid);
            gp.rect_filled(col_rect, 0.0, sel_fill.gamma_multiply(0.3));
            gp.rect_stroke(col_rect, 0.0, Stroke::new(1.5, accent), StrokeKind::Inside);
        }
    }

    // Bands in the gutter.
    // Pills by band.
    let mut band_pills: Vec<Vec<(AutoWhat, String)>> = vec![Vec::new(); v.bands.len()];
    if let Some(info) = v.info {
        if !info.pills_hidden {
            for pill in &info.pills {
                if matches!(
                    pill.what,
                    AutoWhat::Separator
                        | AutoWhat::Decimal
                        | AutoWhat::Encoding
                        | AutoWhat::KeepFooter
                        | AutoWhat::NameSource
                        | AutoWhat::Prop(_)
                        | AutoWhat::SameLine
                ) {
                    continue;
                }
                let Some(line) = pill.lines.first().copied() else {
                    continue;
                };
                if line == 0 || line > map.line_count() {
                    continue;
                }
                let r = row_of_line(rows, line - 1);
                if let Some(b) = v.bands.iter().position(|b| b.r0 <= r && r <= b.r1) {
                    band_pills[b].push((pill.what.clone(), pill.text.clone()));
                }
            }
        }
    }
    let (bx0, bx1) = g.bands_x;
    for (bi, band) in v.bands.iter().enumerate() {
        let top = row_y(g, gs.pan, band.r0);
        let bottom = row_y(g, gs.pan, band.r1) + ROW_H;
        if bottom < g.rows_top || top > g.rows_top + g.view_h {
            continue;
        }
        let rect = Rect::from_min_max(pos2(bx0, top + 1.0), pos2(bx1, bottom - 1.0));
        let zones = zones_in(band.zones);
        let n = zones.len().max(1);
        let sw = rect.width() / n as f32;
        let hovered = matches!(hit, Hit::Band(h) if *h == bi);
        for (k, z) in zones.iter().enumerate() {
            let col = zone_color(*z, dark);
            let zr = Rect::from_min_size(
                pos2(rect.left() + k as f32 * sw, rect.top()),
                vec2(sw - if k + 1 < n { 1.0 } else { 0.0 }, rect.height()),
            );
            let fill = col.gamma_multiply(if hovered { 0.34 } else { 0.24 });
            gutter_p.rect_filled(zr, 4.0, fill);
            if *z == Zone::Footer {
                hatch(&gutter_p, zr, col);
            }
            gutter_p.rect_stroke(
                zr,
                4.0,
                Stroke::new(1.0, col.gamma_multiply(0.8)),
                StrokeKind::Inside,
            );
        }
        // Pill dot at the top, label written vertically, inside the visible part.
        let vis_top = rect.top().max(g.rows_top);
        let vis_bottom = rect.bottom().min(g.rows_top + g.view_h);
        let vis_h = vis_bottom - vis_top;
        let lp = gutter_p.with_clip_rect(rect);
        let pills = &band_pills[bi];
        let mut label_top = vis_top + 4.0;
        if let Some((what, why)) = pills.first() {
            let pr =
                Rect::from_min_size(pos2(rect.center().x - 8.0, vis_top + 3.0), vec2(16.0, 14.0));
            let ph = pointer.is_some_and(|p| pr.contains(p));
            paint_pill_dot(&lp, pr, ph, &visuals);
            extras.push((pr, Extra::Pill(what.clone(), why.clone())));
            label_top = pr.bottom() + 5.0;
        }
        if n == 1 {
            let z = zones[0];
            let avail = vis_bottom - label_top - 4.0;
            if avail >= 28.0 && vis_h >= 28.0 {
                let chars = (avail / 5.4).floor().max(1.0) as usize;
                let text = fit_text(&z.label().to_uppercase(), chars);
                let galley =
                    lp.layout_no_wrap(text, FontId::proportional(9.5), zone_color(z, dark));
                let pos = pos2(rect.center().x + galley.size().y / 2.0, label_top);
                lp.add(
                    egui::epaint::TextShape::new(pos, galley, zone_color(z, dark))
                        .with_angle(std::f32::consts::FRAC_PI_2),
                );
            }
        }
    }

    // Handles of the data band.
    if let Some((h0, h1)) = handle_rects(g, gs.pan, v) {
        let col = zone_color(Zone::Data, dark);
        for (h, hot) in [
            (
                h0,
                matches!(hit, Hit::HandleStart) || gs.drag == Some(Drag::HandleStart),
            ),
            (
                h1,
                matches!(hit, Hit::HandleEnd) || gs.drag == Some(Drag::HandleEnd),
            ),
        ] {
            if h == Rect::NOTHING {
                continue;
            }
            let h = if hot { h.expand2(vec2(5.0, 2.0)) } else { h };
            painter.with_clip_rect(g.body).rect_filled(
                h,
                h.height() / 2.0,
                if hot { col } else { col.gamma_multiply(0.85) },
            );
            // The grip: three small dashes.
            for k in -1..=1 {
                let cx = h.center().x + k as f32 * 6.0;
                painter.with_clip_rect(g.body).line_segment(
                    [pos2(cx - 2.0, h.center().y), pos2(cx + 2.0, h.center().y)],
                    Stroke::new(1.5, crate::theme::text_on_accent()),
                );
            }
        }
    }

    // what the left column points at.
    check::paint_focus(ui, g, v, gs, value_rect);

    // Minimap.
    paint_minimap(ui, g, v, gs, marker);

    // Scrollbars.
    let track_col = visuals.extreme_bg_color.gamma_multiply(0.9);
    painter.rect_filled(g.vtrack, 0.0, track_col);
    painter.rect_filled(g.htrack, 0.0, track_col);
    if g.content_h > g.view_h {
        painter.rect_filled(g.vthumb, 4.0, weak.gamma_multiply(0.6));
    }
    if g.content_w > g.view_w {
        painter.rect_filled(g.hthumb, 4.0, weak.gamma_multiply(0.6));
    }

    gs.extras = extras;
}

/// A small button of the grid, in the look of the application's buttons: the
/// fill, stroke and text of an inactive (or hovered) widget.
fn paint_small_button(
    painter: &egui::Painter,
    rect: Rect,
    hovered: bool,
    visuals: &egui::Visuals,
    label: &str,
    size: f32,
) {
    let w = if hovered {
        &visuals.widgets.hovered
    } else {
        &visuals.widgets.inactive
    };
    painter.rect(
        rect,
        w.corner_radius,
        w.weak_bg_fill,
        w.bg_stroke,
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(size),
        w.fg_stroke.color,
    );
}

/// The « auto » mark of a narrow band: a small button.
fn paint_pill_dot(painter: &egui::Painter, rect: Rect, hovered: bool, visuals: &egui::Visuals) {
    paint_small_button(painter, rect, hovered, visuals, "a", 9.5);
}

fn paint_pill_on(painter: &egui::Painter, rect: Rect, hovered: bool, visuals: &egui::Visuals) {
    paint_small_button(
        painter,
        rect,
        hovered,
        visuals,
        &tr!("import-auto-pill"),
        10.0,
    );
}

fn paint_minimap(ui: &mut egui::Ui, g: &Geom, v: &View<'_>, gs: &GridUi, marker: Option<&str>) {
    let map = v.map;
    let painter = ui.painter().clone();
    let m = g.mini;
    painter.rect_filled(m, 4.0, ui.visuals().extreme_bg_color.gamma_multiply(0.9));
    let n = map.line_count().max(1);
    let inner = mini_inner(g);
    let mp = painter.with_clip_rect(m);
    let scale = MiniScale::new(inner.top(), inner.height(), n, v.rows);
    let y_of = |i: usize| scale.y(i);
    let dark = v.dark;
    let mut start = 0usize;
    let mut cur = zones_of(map, 0, marker);
    for i in 1..=n {
        let bits = if i < n {
            zones_of(map, i, marker)
        } else {
            u16::MAX
        };
        if bits != cur {
            let zones = zones_in(cur);
            let k = zones.len().max(1);
            let h = (y_of(i) - y_of(start)).max(1.0);
            for (j, z) in zones.iter().enumerate() {
                let w = inner.width() / k as f32;
                let r =
                    Rect::from_min_size(pos2(inner.left() + j as f32 * w, y_of(start)), vec2(w, h));
                mp.rect_filled(r, 1.0, zone_color(*z, dark).gamma_multiply(0.85));
            }
            start = i;
            cur = bits;
        }
    }
    // The cut mark over the folded band: two thin slanted hairlines.
    if let Some((y0, y1)) = scale.band() {
        let band = Rect::from_min_max(pos2(inner.left(), y0), pos2(inner.right(), y1));
        mp.rect_filled(band, 1.0, ui.visuals().extreme_bg_color);
        let stroke = Stroke::new(1.0, ui.visuals().weak_text_color());
        let cy = band.center().y;
        for off in [-2.5f32, 2.5] {
            mp.line_segment(
                [
                    pos2(band.left() + 1.0, cy + off + 2.0),
                    pos2(band.right() - 1.0, cy + off - 2.0),
                ],
                stroke,
            );
        }
    }
    // The window on the grid.
    let first_row = (gs.pan.y / ROW_H).floor() as usize;
    let last_row = (((gs.pan.y + g.view_h) / ROW_H).ceil() as usize).min(v.rows.len());
    let line_of = |r: usize| -> usize {
        match v.rows.get(r) {
            Some(Row::Line(i)) => *i,
            Some(Row::Fold(from, _)) => *from,
            None => n,
        }
    };
    let (a, b) = (line_of(first_row), line_of(last_row.max(first_row + 1)));
    let win = Rect::from_min_max(
        pos2(m.left() + 1.0, y_of(a)),
        pos2(m.right() - 1.0, y_of(b.max(a + 1)).max(y_of(a) + 4.0)),
    );
    mp.rect_stroke(
        win,
        2.0,
        Stroke::new(1.6, crate::theme::accent()),
        StrokeKind::Inside,
    );
    mp.rect_filled(win, 2.0, crate::theme::accent().gamma_multiply(0.12));
    painter.text(
        pos2(m.center().x, m.bottom() - 11.0),
        Align2::CENTER_CENTER,
        tr!("import-grid-mini-lines", count = n),
        FontId::proportional(8.5),
        ui.visuals().weak_text_color(),
    );
}

// -- Action bar and its buttons ----------------------------------------------

/// A coloured toggle: a check mark when it holds, click again to remove.
fn toggle_button(
    ui: &mut egui::Ui,
    label: &str,
    color: Option<Color32>,
    pressed: bool,
    enabled: bool,
) -> egui::Response {
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        font,
        color.unwrap_or(ui.visuals().text_color()),
    );
    let pad = 10.0;
    let check_w = if pressed { 16.0 } else { 0.0 };
    let size = vec2(galley.size().x + 2.0 * pad + check_w, 26.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let col = color.unwrap_or(ui.visuals().text_color());
    let hovered = response.hovered() && enabled;
    let fill = if pressed {
        col.gamma_multiply(0.28)
    } else if hovered {
        col.gamma_multiply(0.16)
    } else {
        ui.visuals().faint_bg_color
    };
    ui.painter().rect(
        rect,
        7.0,
        fill,
        Stroke::new(
            if pressed { 2.0 } else { 1.2 },
            col.gamma_multiply(if enabled { 0.9 } else { 0.35 }),
        ),
        StrokeKind::Inside,
    );
    let mut x = rect.left() + pad;
    if pressed {
        let stroke = Stroke::new(2.0, col);
        let c = pos2(x + 1.0, rect.center().y);
        ui.painter()
            .line_segment([c + vec2(0.0, 0.0), c + vec2(3.5, 4.0)], stroke);
        ui.painter()
            .line_segment([c + vec2(3.5, 4.0), c + vec2(10.0, -5.0)], stroke);
        x += check_w;
    }
    let tc = if enabled {
        col
    } else {
        col.gamma_multiply(0.4)
    };
    ui.painter()
        .galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, tc);
    response
}

struct SelCtx<'a> {
    recipe: &'a ImportRecipe,
    spans: Option<Spans>,
    map: &'a FileMap,
    dark: bool,
}

/// Title of the action bar, in words.
fn selection_title(sel: &Sel) -> String {
    match sel {
        Sel::None => String::new(),
        Sel::Lines(set) => {
            let (a, b) = (
                set.first().copied().unwrap_or(0),
                set.last().copied().unwrap_or(0),
            );
            if set.len() == 1 {
                tr!("import-grid-sel-line", line = a)
            } else {
                tr!("import-grid-sel-lines", count = set.len(), from = a, to = b)
            }
        }
        Sel::Cols(set) => {
            let (a, b) = (
                set.first().copied().unwrap_or(0),
                set.last().copied().unwrap_or(0),
            );
            if set.len() == 1 {
                tr!("import-grid-sel-col", col = column_letters(a))
            } else {
                tr!(
                    "import-grid-sel-cols",
                    from = column_letters(a),
                    to = column_letters(b)
                )
            }
        }
        Sel::Value(v) => tr!(
            "import-grid-sel-value",
            key = v.key.clone(),
            value = fit_text(&v.value, 28)
        ),
    }
}

fn selection_buttons(
    ui: &mut egui::Ui,
    sel: &Sel,
    sc: &SelCtx<'_>,
    out: &mut Vec<GridCmd>,
    first_line_note: &mut String,
) {
    let dark = sc.dark;
    match sel {
        Sel::None => {}
        Sel::Lines(set) => {
            let lines: Vec<usize> = set.iter().copied().collect();
            let first = lines.first().copied().unwrap_or(0);
            for role in [
                LineRoleCmd::LongName,
                LineRoleCmd::Units,
                LineRoleCmd::Comments,
                LineRoleCmd::XPrime,
                LineRoleCmd::Names,
                LineRoleCmd::DataStart,
                LineRoleCmd::DataEnd,
            ] {
                let pressed = sc.spans.as_ref().is_some_and(|s| s.pressed(role, set));
                let label = match role {
                    LineRoleCmd::Units if lines.len() > 1 => {
                        tr!("import-grid-units-line", line = first)
                    }
                    LineRoleCmd::XPrime if lines.len() > 1 => {
                        tr!("import-grid-xprime-line", line = first)
                    }
                    other => other.name(),
                };
                let color = role_color(role, dark);
                if toggle_button(ui, &label, color, pressed, true).clicked() {
                    out.push(GridCmd::Role(role, lines.clone()));
                    first_line_note.clear();
                }
            }
            ui.add_enabled_ui(false, |ui| {
                toggle_button(ui, &tr!("import-grid-btn-ignore"), None, false, false)
            })
            .response
            .on_disabled_hover_text(tr!("import-grid-ignore-todo"));
            let all_info = set
                .iter()
                .all(|n| sc.map.roles.get(n - 1) == Some(&LineRole::MainHeader));
            if toggle_button(
                ui,
                &Zone::Info.label(),
                Some(zone_color(Zone::Info, dark)),
                all_info,
                true,
            )
            .clicked()
            {
                out.push(GridCmd::Role(LineRoleCmd::Clear, lines.clone()));
            }
        }
        Sel::Value(vs) => {
            let on = prop_choices_on(sc.recipe, &vs.key);
            for choice in PropChoice::ALL {
                let (label, color) = match choice {
                    PropChoice::LongNameY => (
                        tr!("import-auto-prop-long-name-y"),
                        Some(import_color(dark, ImportColor::LongName)),
                    ),
                    PropChoice::UnitsX => (
                        tr!("import-auto-prop-units-x"),
                        Some(import_color(dark, ImportColor::Units)),
                    ),
                    PropChoice::UnitsY => (
                        tr!("import-auto-prop-units-y"),
                        Some(import_color(dark, ImportColor::Units)),
                    ),
                    PropChoice::CommentY => (
                        tr!("import-auto-prop-comment-y"),
                        Some(import_color(dark, ImportColor::Comments)),
                    ),
                    PropChoice::SheetName => (
                        tr!("import-auto-prop-sheet-name"),
                        Some(crate::theme::accent()),
                    ),
                    PropChoice::Nothing => (tr!("import-auto-prop-nothing"), None),
                };
                let pressed = on.contains(&choice);
                if toggle_button(ui, &label, color, pressed, true).clicked() {
                    out.push(GridCmd::Prop(vs.key.clone(), choice));
                }
            }
        }
        Sel::Cols(set) => {
            let cols: Vec<usize> = set.iter().copied().collect();
            for (role, label, color) in [
                (
                    ColumnRoleChoice::X,
                    "X".to_string(),
                    Some(zone_color(Zone::Data, dark)),
                ),
                (
                    ColumnRoleChoice::Y,
                    "Y".to_string(),
                    Some(import_color(dark, ImportColor::LongName)),
                ),
                (
                    ColumnRoleChoice::Ignore,
                    tr!("import-grid-btn-ignore"),
                    None,
                ),
            ] {
                let pressed = cols.iter().all(|c| sc.recipe.column_role(*c) == role);
                if toggle_button(ui, &label, color, pressed, true).clicked() {
                    out.push(GridCmd::ColumnRole(cols.clone(), role));
                }
            }
            let text = cols
                .iter()
                .all(|c| sc.recipe.column_kind(*c) == ColumnKind::Text);
            if toggle_button(
                ui,
                &tr!("import-grid-btn-text"),
                Some(ui.visuals().warn_fg_color),
                text,
                true,
            )
            .clicked()
            {
                out.push(GridCmd::ColumnText(cols));
            }
        }
    }
}

fn sel_anchor_y(g: &Geom, gs: &GridUi, v: &View<'_>) -> (f32, f32) {
    let rows = v.rows;
    let range = |pred: &dyn Fn(&Row) -> bool| -> Option<(usize, usize)> {
        let first = rows.iter().position(pred)?;
        let last = rows.iter().rposition(pred)?;
        Some((first, last))
    };
    match &gs.sel {
        Sel::Lines(set) => {
            let found = range(&|row| match row {
                Row::Line(i) => set.contains(&(i + 1)),
                Row::Fold(from, to) => (*from..=*to).any(|l| set.contains(&(l + 1))),
            });
            match found {
                Some((a, b)) => (row_y(g, gs.pan, a), row_y(g, gs.pan, b) + ROW_H),
                None => (g.rows_top, g.rows_top),
            }
        }
        Sel::Value(vs) => {
            let r = row_of_line(rows, vs.line);
            (row_y(g, gs.pan, r), row_y(g, gs.pan, r) + ROW_H)
        }
        _ => (g.rows_top, g.rows_top),
    }
}

/// The reason of an automatic choice with its explicit actions: Keep (close),
/// Change… (select the value to correct) and Don't use (the only one that
/// deletes the choice). Closes on a click elsewhere, on Escape and on a scroll.
fn auto_why_popup(ui: &mut egui::Ui, v: &View<'_>, gs: &mut GridUi, out: &mut GridOut) {
    let Some(open) = gs.auto_why.clone() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let mut close = (open.pan - gs.pan).abs().max_elem() > 0.5
        || ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut change_line: Option<usize> = None;
    let mut drop = false;
    // Where the choice can be corrected in the file, if it can.
    let target = match &open.what {
        AutoWhat::Prop(prop) => (0..v.map.line_count()).find_map(|i| {
            let kv = kv_of(v.map, v.recipe, i)?;
            kv.key
                .trim()
                .eq_ignore_ascii_case(prop.key.trim())
                .then_some((i, kv))
        }),
        _ => None,
    };
    let area = egui::Area::new(gs_id().with("auto_why"))
        .order(egui::Order::Foreground)
        .fixed_pos(open.anchor.left_bottom() + vec2(0.0, 4.0))
        .constrain(true)
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(320.0);
                ui.label(crate::theme::bold(tr!("import-auto-pick-why-title")));
                ui.add(egui::Label::new(open.why.clone()).wrap());
                ui.add_space(crate::theme::SPACE_SECTION);
                ui.horizontal(|ui| {
                    let keep = egui::Button::new(tr!("import-auto-pick-keep"))
                        .fill(ui.visuals().selection.bg_fill);
                    if cursors::clickable(ui.add(keep)).clicked() {
                        close = true;
                    }
                    if target.is_some()
                        && cursors::clickable(ui.button(tr!("import-auto-pick-change"))).clicked()
                    {
                        change_line = target.as_ref().map(|(i, _)| *i);
                    }
                    if cursors::clickable(ui.button(tr!("import-auto-pick-drop"))).clicked() {
                        drop = true;
                    }
                });
            });
        });
    let pressed_outside = ctx.input(|i| i.pointer.any_pressed())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !area.response.rect.contains(p) && !open.anchor.contains(p));
    if pressed_outside {
        close = true;
    }
    if let (Some(line), Some((_, kv))) = (change_line, target) {
        gs.sel = Sel::Value(ValueSel {
            line,
            key: kv.key,
            value: kv.value,
            cell: kv.cell,
            offset: kv.offset,
        });
        close = true;
    }
    if drop {
        out.auto.push(open.what.clone());
        close = true;
    }
    if close {
        gs.auto_why = None;
        ctx.request_repaint();
    }
}

/// How long the pointer may stay away from the action bar before it closes.
const BAR_GRACE: f64 = 0.45;

fn action_bar(ui: &mut egui::Ui, g: &Geom, v: &View<'_>, gs: &mut GridUi, out: &mut GridOut) {
    // A menu that is open says the same things: the bar steps aside.
    if gs.sel == Sel::None || egui::Popup::is_any_open(ui.ctx()) {
        gs.bar_rect = Rect::NOTHING;
        if gs.sel == Sel::None {
            gs.bar_pan = None;
            gs.bar_left = None;
        }
        return;
    }
    // The bar closes by itself: when the grid is scrolled, when the pointer
    // stays away from it, and on a click outside the grid.
    let now = ui.input(|i| i.time);
    let pan_at_open = *gs.bar_pan.get_or_insert(gs.pan);
    let scrolled = (pan_at_open - gs.pan).abs().max_elem() > 0.5;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let (sel_top0, _) = sel_anchor_y(g, gs, v);
    let corridor_top = if matches!(gs.sel, Sel::Cols(_)) {
        g.letters.top()
    } else {
        sel_top0
    };
    let hull = if gs.bar_rect == Rect::NOTHING {
        Rect::NOTHING
    } else {
        gs.bar_rect.expand(24.0).union(Rect::from_min_max(
            pos2(g.grid.left(), corridor_top - 4.0),
            pos2(g.grid.right(), gs.bar_rect.bottom() + 24.0),
        ))
    };
    let away = pointer.is_some_and(|p| hull != Rect::NOTHING && !hull.contains(p));
    if !away || gs.drag.is_some() {
        gs.bar_left = None;
    } else if gs.bar_left.is_none() {
        gs.bar_left = Some(now);
    }
    let waited = gs.bar_left.is_some_and(|t| now - t > BAR_GRACE);
    let clicked_outside = ui.input(|i| i.pointer.any_pressed())
        && pointer.is_some_and(|p| !g.grid.contains(p) && !gs.bar_rect.contains(p));
    if scrolled || waited || clicked_outside {
        gs.sel = Sel::None;
        gs.bar_rect = Rect::NOTHING;
        gs.bar_pan = None;
        gs.bar_left = None;
        // The row highlight was painted this frame: paint once more.
        ui.ctx().request_repaint();
        return;
    }
    if gs.bar_left.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let spans = v.spans;
    let sc = SelCtx {
        recipe: v.recipe,
        spans,
        map: v.map,
        dark: v.dark,
    };
    let (sel_top, sel_bottom) = sel_anchor_y(g, gs, v);
    let width = (g.grid.width() - 12.0).clamp(300.0, 760.0);
    let left = (g.grid.left() + 6.0).min(g.grid.right() - width);
    let h = gs.bar_h;
    let below = sel_bottom + 6.0;
    let y = if below + h <= g.rows_top + g.view_h {
        below
    } else if sel_top - 6.0 - h >= g.rows_top {
        sel_top - 6.0 - h
    } else {
        (g.rows_top + g.view_h - h - 4.0).max(g.rows_top)
    };
    let rect = Rect::from_min_size(pos2(left, y), vec2(width, h));
    // Blocks the grid under the bar (known from the previous frame).
    if gs.bar_rect != Rect::NOTHING {
        ui.interact(gs.bar_rect, gs_id().with("bar"), Sense::click_and_drag());
    }
    let title = selection_title(&gs.sel);
    let sel = gs.sel.clone();
    let mut note = String::new();
    let inner = ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        egui::Frame::popup(ui.style())
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_max_width(width - 24.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    ui.label(egui::RichText::new(title).size(13.5));
                    selection_buttons(ui, &sel, &sc, &mut out.cmds, &mut note);
                });
            })
            .response
            .rect
    });
    gs.bar_h = inner.inner.height().max(30.0);
    gs.bar_rect = Rect::from_min_size(
        pos2(inner.inner.left(), inner.inner.top()),
        inner.inner.size(),
    );
}

fn separator_options() -> Vec<Separator> {
    vec![
        Separator::Semicolon,
        Separator::Comma,
        Separator::Tab,
        Separator::Whitespace,
        Separator::Custom('|'),
        Separator::FixedWidth(vec![10]),
        Separator::Auto,
    ]
}

fn context_menu(resp: &egui::Response, v: &View<'_>, gs: &mut GridUi, out: &mut GridOut) {
    if gs.sel == Sel::None {
        return;
    }
    let sc = SelCtx {
        recipe: v.recipe,
        spans: v.spans,
        map: v.map,
        dark: v.dark,
    };
    let sel = gs.sel.clone();
    let info_line = gs.menu_info_line.clone();
    let excluded: Vec<String> = v
        .recipe
        .excluded_metadata
        .iter()
        .map(|k| norm_key(k))
        .collect();
    resp.context_menu(|ui| {
        ui.set_min_width(220.0);
        let mut sections = crate::app::menu_sections::MenuSections::new();
        sections.section(ui, true, |ui| {
            ui.label(egui::RichText::new(selection_title(&sel)).size(13.5));
        });
        sections.section(ui, true, |ui| {
            ui.vertical(|ui| {
                let mut note = String::new();
                selection_buttons(ui, &sel, &sc, &mut out.cmds, &mut note);
            });
        });
        sections.section(ui, info_line.is_some(), |ui| {
            let Some((_, key)) = &info_line else {
                return;
            };
            let is_excluded = excluded.contains(&norm_key(key));
            let label = if is_excluded {
                tr!("import-grid-include")
            } else {
                tr!("import-grid-exclude")
            };
            if ui.button(label).clicked() {
                out.cmds.push(GridCmd::Exclude(key.clone(), !is_excluded));
                ui.close();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use egui::pos2;

    use super::super::test_bench::{bench, escape, paint, settle, SURE};
    use super::{load_gs, store_gs, visible_columns, visible_rows, Sel, ROW_H};
    use crate::app::test_support::context;

    /// The rows laid out are those that intersect the view, never the whole
    /// file.
    #[test]
    fn only_the_visible_rows_are_laid_out() {
        // 630 lines, a view of 20 rows.
        let view = 20.0 * ROW_H;
        assert_eq!(visible_rows(0.0, view, 630), 0..20);
        assert_eq!(visible_rows(100.0 * ROW_H, view, 630), 100..120);
        // At the end the range stops with the file.
        assert_eq!(visible_rows(620.0 * ROW_H, view, 630), 620..630);
        // 241 columns of 50 pt: a view of 400 pt sees about nine of them.
        let colx: Vec<f32> = (0..=241).map(|c| c as f32 * 50.0).collect();
        let (first, last) = visible_columns(&colx, 0.0, 400.0, 241);
        assert_eq!((first, last), (0, 8));
        let (first, last) = visible_columns(&colx, 5000.0, 400.0, 241);
        assert!(last - first <= 10, "{first}..{last}");
        assert!(first >= 99, "the columns before the view are skipped");
    }

    /// A kinetic of 630 lines and 241 columns, unfolded, paints only what is
    /// in view: the texts of the whole file would be about 150 000.
    #[test]
    fn the_grid_paints_only_the_visible_rows_and_columns() {
        let mut bench = bench(&super::super::test_bench::kinetic_630_by_241());
        let ctx = context();
        let mut actions = Vec::new();
        for _ in 0..3 {
            paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        }
        paint(&ctx, &mut bench.state, &mut actions, vec![escape()]);
        let mut gs = load_gs(&ctx);
        gs.unfolded = true;
        store_gs(&ctx, gs);
        let painted = settle(&ctx, &mut bench.state, &mut actions);
        assert!(
            painted.len() > 100,
            "the grid is painted: {}",
            painted.len()
        );
        assert!(
            painted.len() < 2500,
            "{} texts: rows or columns out of view are laid out",
            painted.len()
        );
    }

    /// Selected lines open the role bar beside them. It closes by itself when
    /// the pointer stays away from it, and when the grid is scrolled; a mouse
    /// user does not have to find Escape.
    #[test]
    fn the_role_bar_closes_when_the_pointer_leaves_and_when_the_grid_scrolls() {
        let open_bar = |ctx: &egui::Context,
                        state: &mut stokesforge_app_state::state::AppState,
                        actions: &mut Vec<stokesforge_app_state::state::Action>|
         -> egui::Rect {
            // The guide is open first: one Escape closes it.
            for _ in 0..3 {
                paint(ctx, state, actions, Vec::new());
            }
            paint(ctx, state, actions, vec![escape()]);
            settle(ctx, state, actions);
            let mut gs = load_gs(ctx);
            gs.sel = Sel::Lines([5usize].into());
            // The whole file, so that there is something to scroll.
            gs.unfolded = true;
            store_gs(ctx, gs);
            paint(ctx, state, actions, Vec::new());
            paint(ctx, state, actions, Vec::new());
            let gs = load_gs(ctx);
            assert_eq!(gs.sel, Sel::Lines([5usize].into()), "the bar is open");
            assert_ne!(gs.bar_rect, egui::Rect::NOTHING);
            gs.bar_rect
        };

        // The pointer leaves: after the grace period the bar is gone.
        let mut left = bench(SURE);
        let ctx = context();
        let mut actions = Vec::new();
        let bar = open_bar(&ctx, &mut left.state, &mut actions);
        paint(
            &ctx,
            &mut left.state,
            &mut actions,
            vec![egui::Event::PointerMoved(bar.center())],
        );
        assert_eq!(
            load_gs(&ctx).sel,
            Sel::Lines([5usize].into()),
            "kept while over it"
        );
        for _ in 0..8 {
            paint(
                &ctx,
                &mut left.state,
                &mut actions,
                vec![egui::Event::PointerMoved(pos2(4.0, 4.0))],
            );
        }
        assert_eq!(load_gs(&ctx).sel, Sel::None, "the pointer left: closed");

        // The bench holds the configuration lock: release it for the next.
        drop(left);

        // The grid scrolls: closed at once, the pointer still over the grid.
        let mut scrolled = bench(&super::super::test_bench::kinetic_630_by_241());
        let ctx = context();
        let mut actions = Vec::new();
        let bar = open_bar(&ctx, &mut scrolled.state, &mut actions);
        let over_grid = pos2(bar.left() - 3.0, bar.center().y);
        paint(
            &ctx,
            &mut scrolled.state,
            &mut actions,
            vec![egui::Event::PointerMoved(over_grid)],
        );
        assert_eq!(load_gs(&ctx).sel, Sel::Lines([5usize].into()));
        // The wheel of the grid moves its pan (`gs.pan`); move it as it does.
        let mut gs = load_gs(&ctx);
        gs.pan.y += 80.0;
        store_gs(&ctx, gs);
        for _ in 0..2 {
            paint(&ctx, &mut scrolled.state, &mut actions, Vec::new());
        }
        assert_eq!(load_gs(&ctx).sel, Sel::None, "the grid scrolled: closed");
    }
}
