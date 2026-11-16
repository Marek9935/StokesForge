//! The theme picker (#541, #543): ONE component for the Apply Theme window and,
//! in #542, the Theme tab of Graph Properties.
//!
//! # Layout
//!
//! Tiles in two columns, in **row groups**: « Project » (the themes saved in
//! the open project, only when there are some) then « Mine » (the built-ins and
//! the user's themes, Light and Dark first, then the rest by use:
//! [`stokesforge_app_state::state::AppState::ranked_themes`]). The theme copied
//! by Copy Theme comes first, in the first group, in italics. A group shows
//! [`CELLS`] cells (two columns, three rows); with more themes the last cell is
//! the drop-down « More themes », which lists the others in the same order.
//!
//! # A tile
//!
//! The **specimen** of the theme ([`stokesforge_app_state::theme_specimen`]:
//! its own structure and style on reference data) drawn through the one
//! [`Drawings`] the application holds, and its name under it (the translated
//! name of a built-in, the typed name of the others). The look of a list row
//! (`list_row::paint_row_look`): selected = pale background and accent strip,
//! hovered = light grey, the pointing finger, a focus outline; Tab reaches a
//! tile and the arrows move the choice inside a group. Resting the pointer on a
//! tile shows the specimen large (about [`LARGE_WIDTH`] points), the way the
//! import window shows the rules of a template (`on_hover_ui`).
//!
//! What the pointer rests on is returned to the caller, which previews it
//! before any click; only a click changes the choice.

use std::collections::HashSet;

use eframe::egui;
use stokesforge_app_state::figure_texts::figure_texts;
use stokesforge_app_state::theme_specimen::{reference_tree, specimen};
use stokesforge_app_state::theme_window::{PickerEntries, PickerEntry, ThemeSource};
use stokesforge_app_state::tr;
use stokesforge_render_egui::drawings::ScaledDrawings;

use super::cursors;
use super::list_row::{list_row_text, list_row_text_color, paint_focus_outline, paint_row_look};
use super::shortcuts::{KEY_ARROW_DOWN, KEY_ARROW_LEFT, KEY_ARROW_RIGHT, KEY_ARROW_UP};
use crate::theme;

/// What a cached drawing of the application's set stands for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum DrawKey {
    /// The specimen of a theme, tile size.
    Tile(ThemeSource),
    /// The same specimen for the large view (its own entry: the tile and the
    /// large view are shown together and would otherwise rescale each other).
    Large(ThemeSource),
    /// A preview of the Apply Theme window: the graph before (`false`) or after.
    Preview(bool),
}

/// The set of cached drawings the application holds once.
pub(super) type Drawings = ScaledDrawings<DrawKey>;

/// Cells of a row group: two columns by three rows.
const CELLS: usize = 6;
/// Height of a tile: the specimen, the gap, two lines of name.
const TILE_HEIGHT: f32 = 84.0;
/// Height of the specimen inside a tile.
const SPECIMEN_HEIGHT: f32 = 44.0;
/// Inner margin of a tile.
const TILE_PAD: f32 = 5.0;
/// Width of the large view, in points.
pub(super) const LARGE_WIDTH: f32 = 480.0;
/// Height the large view never exceeds.
const LARGE_HEIGHT: f32 = 360.0;

/// Lets go of the drawings of themes that are no longer listed (a deleted
/// theme, another project's themes): called with the sources the picker shows.
pub(super) fn retain_listed(drawings: &mut Drawings, entries: &PickerEntries) {
    let listed: HashSet<&ThemeSource> = entries.sources().collect();
    drawings.retain(|key| match key {
        DrawKey::Tile(source) | DrawKey::Large(source) => listed.contains(source),
        DrawKey::Preview(_) => true,
    });
}

/// The picker. `selected` is the chosen theme (a click or an arrow changes it);
/// returns the theme the pointer rests on, for the caller's preview.
pub(super) fn theme_picker(
    ui: &mut egui::Ui,
    salt: &str,
    drawings: &mut Drawings,
    entries: &PickerEntries,
    selected: &mut ThemeSource,
) -> Option<ThemeSource> {
    retain_listed(drawings, entries);
    let mut project: Vec<&PickerEntry> = entries.project.iter().collect();
    let mut mine: Vec<&PickerEntry> = entries.mine.iter().collect();
    if let Some(copied) = &entries.copied {
        if project.is_empty() {
            mine.insert(0, copied);
        } else {
            project.insert(0, copied);
        }
    }
    let titled = !project.is_empty();
    let mut hovered = None;
    for (title, group) in [
        (tr!("theme-row-project"), project),
        (tr!("theme-row-mine"), mine),
    ] {
        if group.is_empty() {
            continue;
        }
        if titled {
            ui.label(theme::section_heading(title.clone()));
        }
        let at = group_picker(ui, (salt, &title), drawings, &group, selected);
        hovered = hovered.or(at);
    }
    hovered
}

/// One row group: its grid of tiles and, past [`CELLS`], « More themes ».
fn group_picker(
    ui: &mut egui::Ui,
    salt: (&str, &str),
    drawings: &mut Drawings,
    group: &[&PickerEntry],
    selected: &mut ThemeSource,
) -> Option<ThemeSource> {
    let overflow = group.len() > CELLS;
    let shown = if overflow { CELLS - 1 } else { group.len() };
    let rows = (shown + usize::from(overflow)).div_ceil(2).max(1);
    let gap = theme::SPACE_ITEM;
    let width = ui.available_width();
    let cell_w = ((width - gap) / 2.0).max(40.0);
    let (area, _) = ui.allocate_exact_size(
        egui::vec2(width, rows as f32 * TILE_HEIGHT + (rows - 1) as f32 * gap),
        egui::Sense::hover(),
    );
    let cell_rect = |index: usize| {
        let (col, row) = ((index % 2) as f32, (index / 2) as f32);
        egui::Rect::from_min_size(
            area.min + egui::vec2(col * (cell_w + gap), row * (TILE_HEIGHT + gap)),
            egui::vec2(cell_w, TILE_HEIGHT),
        )
    };
    let texts = figure_texts();
    let mut hovered = None;
    let mut focused = None;
    for (index, entry) in group.iter().take(shown).enumerate() {
        let is_selected = *selected == entry.source;
        let response = tile(
            ui,
            salt,
            drawings,
            &texts,
            cell_rect(index),
            entry,
            is_selected,
        );
        if response.clicked() {
            *selected = entry.source.clone();
        }
        if response.hovered() {
            hovered = Some(entry.source.clone());
        }
        if response.has_focus() {
            focused = Some(index);
        }
    }
    if let Some(at) = focused {
        move_with_arrows(ui, salt, group, shown, at, selected);
    }
    if overflow {
        let at = more_themes(ui, salt, cell_rect(shown), &group[shown..], selected);
        hovered = at.or(hovered);
    }
    hovered
}

/// The arrows move the choice from tile to tile (two columns).
fn move_with_arrows(
    ui: &mut egui::Ui,
    salt: (&str, &str),
    group: &[&PickerEntry],
    shown: usize,
    at: usize,
    selected: &mut ThemeSource,
) {
    let (dx, dy) = ui.input_mut(|input| {
        if input.consume_shortcut(&KEY_ARROW_LEFT) {
            (-1, 0)
        } else if input.consume_shortcut(&KEY_ARROW_RIGHT) {
            (1, 0)
        } else if input.consume_shortcut(&KEY_ARROW_UP) {
            (0, -1)
        } else if input.consume_shortcut(&KEY_ARROW_DOWN) {
            (0, 1)
        } else {
            (0, 0)
        }
    });
    let target = at as i64 + dx + 2 * dy;
    if (dx, dy) != (0, 0) && (0..shown as i64).contains(&target) {
        if let Some(entry) = group.get(target as usize) {
            *selected = entry.source.clone();
            let id = tile_id(ui, salt, &entry.source);
            ui.memory_mut(|memory| memory.request_focus(id));
        }
    }
}

fn tile_id(ui: &egui::Ui, salt: (&str, &str), source: &ThemeSource) -> egui::Id {
    ui.id().with((salt, "tile", source))
}

/// One tile: specimen and name, hover and selected looks, focus outline, and
/// the large view while the pointer rests on it.
fn tile(
    ui: &mut egui::Ui,
    salt: (&str, &str),
    drawings: &mut Drawings,
    texts: &stokesforge_render::FigureTexts,
    rect: egui::Rect,
    entry: &PickerEntry,
    selected: bool,
) -> egui::Response {
    let id = tile_id(ui, salt, &entry.source);
    let response = ui.interact(rect, id, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            true,
            selected,
            entry.name.as_str(),
        )
    });
    let response = cursors::clickable(response);
    paint_row_look(ui, rect, selected, response.hovered());
    let specimen_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(TILE_PAD, 4.0),
        egui::vec2(rect.width() - 2.0 * TILE_PAD, SPECIMEN_HEIGHT),
    );
    let ctx = ui.ctx().clone();
    let drawn = draw_specimen(
        &ctx,
        drawings,
        DrawKey::Tile(entry.source.clone()),
        entry,
        texts,
    )
    .and_then(|_| {
        drawings.paint(
            &ctx,
            &ui.painter_at(specimen_rect),
            specimen_rect,
            1.0,
            &DrawKey::Tile(entry.source.clone()),
        )
    });
    if let Some(figure) = drawn {
        ui.painter().rect_stroke(
            figure,
            0.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Outside,
        );
    }
    paint_name(ui, rect, specimen_rect, entry, selected);
    if response.has_focus() {
        paint_focus_outline(ui, rect);
    }
    response.on_hover_ui(|ui| large_view(ui, drawings, texts, entry))
}

/// Draws the specimen of `entry` under `key`; its natural size.
fn draw_specimen(
    ctx: &egui::Context,
    drawings: &mut Drawings,
    key: DrawKey,
    entry: &PickerEntry,
    texts: &stokesforge_render::FigureTexts,
) -> Option<egui::Vec2> {
    let tree = reference_tree()?;
    drawings.draw(ctx, key, tree, &specimen(&entry.theme), texts)
}

/// The name under the specimen: bold when selected, italic for the copied
/// theme, cut with an ellipsis (the large view carries the whole name).
fn paint_name(
    ui: &egui::Ui,
    rect: egui::Rect,
    specimen_rect: egui::Rect,
    entry: &PickerEntry,
    selected: bool,
) {
    let mut text = list_row_text(&entry.name, selected);
    if entry.source == ThemeSource::Copied {
        text = text.italics();
    }
    let max_width = rect.width() - 2.0 * TILE_PAD;
    // A name that does not fit takes a second line (never cut: « Publication
    // large », a longer translation); the large view carries it whole anyway.
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 2;
    job.halign = egui::Align::Center;
    text.append_to(
        &mut job,
        ui.style(),
        egui::FontSelection::Default,
        egui::Align::Center,
    );
    let galley = ui.painter().layout_job(job);
    // Centred rows are laid out around the anchor's x.
    let at = egui::pos2(rect.center().x, specimen_rect.bottom() + theme::SPACE_ITEM);
    ui.painter()
        .galley(at, galley, list_row_text_color(ui, selected, false));
    if entry.modified {
        // The distinctive mark of an edited built-in: an accent dot.
        ui.painter().circle_filled(
            egui::pos2(rect.right() - TILE_PAD - 3.0, rect.top() + TILE_PAD + 3.0),
            3.0,
            theme::accent(),
        );
    }
}

/// The large view of a specimen: about [`LARGE_WIDTH`] points, the whole name
/// and what is special about the theme under it.
fn large_view(
    ui: &mut egui::Ui,
    drawings: &mut Drawings,
    texts: &stokesforge_render::FigureTexts,
    entry: &PickerEntry,
) {
    let ctx = ui.ctx().clone();
    let key = DrawKey::Large(entry.source.clone());
    if let Some(natural) = draw_specimen(&ctx, drawings, key.clone(), entry, texts) {
        let scale = (LARGE_WIDTH / natural.x).min(LARGE_HEIGHT / natural.y);
        let (rect, _) = ui.allocate_exact_size(natural * scale, egui::Sense::hover());
        drawings.paint(&ctx, &ui.painter_at(rect), rect, scale, &key);
    }
    ui.label(theme::bold(entry.name.as_str()));
    if entry.source == ThemeSource::Copied {
        ui.label(theme::secondary_text(tr!("theme-tile-copied")));
    }
    if entry.modified {
        ui.label(theme::secondary_text(tr!("theme-tile-edited")));
    }
}

/// The last cell of a full group: a drop-down of the other themes, in rank
/// order. Returns the one the pointer rests on, for the preview.
fn more_themes(
    ui: &mut egui::Ui,
    salt: (&str, &str),
    cell: egui::Rect,
    rest: &[&PickerEntry],
    selected: &mut ThemeSource,
) -> Option<ThemeSource> {
    let height = ui.spacing().interact_size.y;
    let slot = egui::Rect::from_min_size(
        cell.min + egui::vec2(TILE_PAD, (SPECIMEN_HEIGHT - height) / 2.0 + 4.0),
        egui::vec2(cell.width() - 2.0 * TILE_PAD, height),
    );
    let label = rest
        .iter()
        .find(|entry| entry.source == *selected)
        .map_or_else(|| tr!("theme-more"), |entry| entry.name.clone());
    let mut hovered = None;
    // A child that allocates nothing in the parent: the cell is already
    // reserved, and a scope would move the cursor back to the slot.
    let mut cell_ui = ui.new_child(egui::UiBuilder::new().max_rect(slot));
    egui::ComboBox::from_id_salt((salt, "more"))
        .width(slot.width())
        .selected_text(label)
        .show_ui(&mut cell_ui, |ui| {
            for entry in rest {
                let response = ui.selectable_value(
                    selected,
                    entry.source.clone(),
                    if entry.modified {
                        format!("{} •", entry.name)
                    } else {
                        entry.name.clone()
                    },
                );
                if response.hovered() {
                    hovered = Some(entry.source.clone());
                }
            }
        })
        .response
        .on_hover_text(tr!("theme-more", "hint"));
    hovered
}
