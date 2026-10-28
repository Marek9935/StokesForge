//! **The** selectable list row of the application (#169, option A of the
//! tabs mock-up): a tree or list entry — Project Manager window, Object
//! Manager object or series, series of the graph properties "Series"
//! page — is painted as a full-width row, never as a
//! `selectable_label` / `Button::selectable` button.
//!
//! Look: selected, a pale accent background over the whole width remaining
//! since the row's indent, an accent strip of [`LIST_ROW_MARK`] points on
//! the left, bold label (`theme::bold`); hovered, a light grey background;
//! keyboard focus, an outline of `selection.stroke`. The colors come from
//! `crate::theme`, nowhere else.
//!
//! Two uses:
//!
//! - [`list_row`] paints the whole row (optional icon + label) and returns
//!   its response;
//! - a row made of several widgets (visibility checkbox, swatch, Object
//!   Manager label) reserves its background **before** its widgets with
//!   [`ListRowBackground::reserve`], then lays it down once the row is
//!   measured ([`ListRowBackground::paint`]): the background goes under the
//!   widgets, without adding a sub-`Ui` or changing their egui ids.
//!
//! A set of **tabs** is not a list: it goes through `tab_strip`. An
//! **exclusive choice** in a dropdown list or a menu keeps egui's
//! `selectable_label` (guard `no_selectable_button_outside_a_menu`).

use eframe::egui;

use crate::icons::ICON_SIZE;
use crate::theme;

/// Width of the accent strip on the left of a selected row.
pub(super) const LIST_ROW_MARK: f32 = 3.0;

/// **Single** height of a list row (#170, point 5): the same for a
/// [`list_row`] row with or without an icon, selected or not, and for a
/// composed row (visibility checkbox, swatch, label: group, series,
/// Object Manager object, Project Manager folder header, properties series
/// row). Value: the natural height of a row with an icon (an 18 px line of
/// text plus 2 x 3 px of margin, i.e. 24 px), which fits the checkbox
/// (`interact_size.y`) and the 18 px swatch without clipping them.
pub(crate) const LIST_ROW_HEIGHT: f32 = 24.0;

/// Inner margins of a row painted by [`list_row`].
const LIST_ROW_PADDING: egui::Vec2 = egui::vec2(6.0, 3.0);

/// Gap between the icon and the label.
const LIST_ROW_GAP: f32 = 5.0;

/// Imposes [`LIST_ROW_HEIGHT`] on the widgets of a composed row: `add` is
/// written with the interaction height (`interact_size.y`) raised to the
/// row height, which `ui.horizontal` (starting height of the row), the
/// checkbox, the button and the input field read; each widget thus centers
/// in a row of [`LIST_ROW_HEIGHT`], without a new sub-`Ui` (egui ids and
/// drag and drop do not change). The setting is handed back to the caller
/// as soon as `add` is done.
pub(super) fn with_list_row_height<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let before = raise_to_list_row_height(ui);
    let result = add(ui);
    restore_row_height(ui, before);
    result
}

/// The "set" half of [`with_list_row_height`], for a collapsing header
/// whose response (`HeaderResponse`) borrows the `Ui` until its `.body(…)`:
/// sets [`LIST_ROW_HEIGHT`] and returns the previous height, to be handed
/// back by [`restore_row_height`] once the header and its body are painted.
pub(super) fn raise_to_list_row_height(ui: &mut egui::Ui) -> f32 {
    let before = ui.spacing().interact_size.y;
    ui.spacing_mut().interact_size.y = LIST_ROW_HEIGHT;
    before
}

/// The "restore" half: see [`raise_to_list_row_height`].
pub(super) fn restore_row_height(ui: &mut egui::Ui, before: f32) {
    ui.spacing_mut().interact_size.y = before;
}

/// The label of a row: bold if it is selected.
pub(super) fn list_row_text(text: &str, selected: bool) -> egui::RichText {
    if selected {
        theme::bold(text)
    } else {
        egui::RichText::new(text)
    }
}

/// Label color: faded for a hidden entry (`dimmed`), otherwise
/// ordinary text, a notch darker when the row is selected.
pub(super) fn list_row_text_color(ui: &egui::Ui, selected: bool, dimmed: bool) -> egui::Color32 {
    if dimmed {
        ui.visuals().weak_text_color()
    } else if selected {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().text_color()
    }
}

/// The clickable label of a composed row ([`ListRowBackground`]): a
/// **frameless** button — neither selection background nor hover
/// background, which the whole row carries —, marked selected for the accessibility tree.
pub(super) fn list_row_label(text: egui::RichText, selected: bool) -> egui::Button<'static> {
    egui::Button::new(text).frame(false).selected(selected)
}

/// [`list_row_label`] followed by an area that takes the rest of the row,
/// both clickable and draggable: click, double-click and
/// drag and drop apply to the whole row, outside the visibility checkbox. The
/// label keeps its place on the left (a widened button would center it).
pub(super) fn list_row_grip(
    ui: &mut egui::Ui,
    text: egui::RichText,
    selected: bool,
) -> egui::Response {
    let label = ui.add(list_row_label(text, selected).sense(egui::Sense::click_and_drag()));
    let rest = ui.allocate_response(
        egui::vec2(ui.available_width().max(0.0), label.rect.height()),
        egui::Sense::click_and_drag(),
    );
    label.union(rest)
}

/// The keyboard focus outline of a row (#170, point 6): that of
/// `selection.stroke`, inset by one point. A single implementation for
/// all rows: a [`list_row`] row that has egui focus, the row of the Project
/// Manager keyboard cursor (which follows its own focus, not egui's)
/// and that of the Object Manager (a focus anchor for the whole
/// panel).
pub(super) fn paint_focus_outline(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        0.0,
        ui.visuals().selection.stroke,
        egui::StrokeKind::Inside,
    );
}

/// Where the keyboard cursor goes from `cursor` when ↑ (`step` = -1)
/// or ↓ (+1) is pressed in `visible`, the **visible** rows in reading order:
/// the neighbor, without wrapping from one end to the other. Without a cursor (or if the cursor
/// is not visible), ↓ goes to the first row and ↑ to the last. `None` if
/// nothing is visible. Shared by the Project Manager and the Object Manager
/// (#170, point 6).
pub(super) fn neighbour<T: Copy + PartialEq>(
    visible: &[T],
    cursor: Option<T>,
    step: i32,
) -> Option<T> {
    let last = visible.len().checked_sub(1)?;
    let target = match cursor.and_then(|entry| visible.iter().position(|e| *e == entry)) {
        Some(at) if step < 0 => at.saturating_sub(1),
        Some(at) => (at + 1).min(last),
        None if step < 0 => last,
        None => 0,
    };
    visible.get(target).copied()
}

/// The navigation keys of a list, read (and consumed) once per
/// frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavKey {
    Up,
    Down,
    Enter,
}

/// Reads ↑, ↓ and Enter in the key registry. The Shift (↑ ↓) and
/// Alt+↓ variants are consumed **first** and do nothing: `consume_shortcut`
/// ignores extra Shift and Alt and would take them for plain ↑ or ↓. Ctrl+↑ and
/// Ctrl+↓ (move a series) do not match: the Ctrl modifier, for its part,
/// must be exact.
pub(super) fn read_navigation(input: &mut egui::InputState) -> Option<NavKey> {
    use super::shortcuts::{
        KEY_ALT_ARROW_DOWN, KEY_ARROW_DOWN, KEY_ARROW_UP, KEY_ENTER, KEY_SHIFT_ARROW_DOWN,
        KEY_SHIFT_ARROW_UP,
    };
    let swallowed = [
        input.consume_shortcut(&KEY_SHIFT_ARROW_UP),
        input.consume_shortcut(&KEY_SHIFT_ARROW_DOWN),
        input.consume_shortcut(&KEY_ALT_ARROW_DOWN),
    ]
    .contains(&true);
    if swallowed {
        return None;
    }
    if input.consume_shortcut(&KEY_ARROW_UP) {
        Some(NavKey::Up)
    } else if input.consume_shortcut(&KEY_ARROW_DOWN) {
        Some(NavKey::Down)
    } else if input.consume_shortcut(&KEY_ENTER) {
        Some(NavKey::Enter)
    } else {
        None
    }
}

/// The background of a row: pale background and accent strip if it is
/// selected, light grey if it is only hovered, nothing otherwise. Two
/// flat shapes (background, then strip), never a `Shape::Vec`: the tests and
/// the tessellation read them one by one.
fn list_row_background(rect: egui::Rect, selected: bool, hovered: bool) -> [egui::Shape; 2] {
    if selected {
        let mark = egui::Rect::from_min_size(rect.min, egui::vec2(LIST_ROW_MARK, rect.height()));
        [
            egui::Shape::rect_filled(rect, 0.0, theme::list_row_selected_fill()),
            egui::Shape::rect_filled(mark, 0.0, theme::accent()),
        ]
    } else if hovered {
        [
            egui::Shape::rect_filled(rect, 0.0, theme::list_row_hover_fill()),
            egui::Shape::Noop,
        ]
    } else {
        [egui::Shape::Noop, egui::Shape::Noop]
    }
}

/// Background of a row made of several widgets: reserved before them, laid
/// down after (see the module head doc).
pub(super) struct ListRowBackground([egui::layers::ShapeIdx; 2]);

impl ListRowBackground {
    /// Reserves the background's place in the shape list, under what follows.
    pub(super) fn reserve(ui: &egui::Ui) -> Self {
        let painter = ui.painter();
        Self([
            painter.add(egui::Shape::Noop),
            painter.add(egui::Shape::Noop),
        ])
    }

    /// Lays the background under row `row`, extended to the right edge of
    /// the `Ui` (the row runs across the whole remaining width), and the keyboard
    /// focus outline when it carries the cursor (`focused`, #170 point 6).
    pub(super) fn paint(self, ui: &egui::Ui, row: egui::Rect, selected: bool, focused: bool) {
        let rect = egui::Rect::from_min_max(
            row.min,
            egui::pos2(
                row.right().max(ui.max_rect().right()),
                row.bottom().max(row.top() + LIST_ROW_HEIGHT),
            ),
        );
        let hovered = ui.rect_contains_pointer(rect);
        for (index, shape) in self
            .0
            .into_iter()
            .zip(list_row_background(rect, selected, hovered))
        {
            ui.painter().set(index, shape);
        }
        if focused {
            paint_focus_outline(ui, rect);
        }
    }
}

/// A list row: optional icon and label, over the whole width
/// remaining since its indent. `dimmed` fades the label (hidden
/// window of the Project Manager). The response is a click; the accessibility
/// tree sees it as a selected or unselected button.
pub(super) fn list_row(
    ui: &mut egui::Ui,
    icon: Option<egui::Image<'static>>,
    text: &str,
    selected: bool,
    dimmed: bool,
) -> egui::Response {
    list_row_sensed(
        ui,
        icon,
        text,
        selected,
        dimmed,
        false,
        egui::Sense::click(),
    )
}

/// [`list_row`] whose focus outline is also painted when the caller knows
/// that the row carries the keyboard cursor without having egui focus (the Project
/// Manager follows its focus itself, #170 point 6).
pub(super) fn list_row_focused(
    ui: &mut egui::Ui,
    icon: Option<egui::Image<'static>>,
    text: &str,
    selected: bool,
    dimmed: bool,
    focused: bool,
) -> egui::Response {
    list_row_sensed(
        ui,
        icon,
        text,
        selected,
        dimmed,
        focused,
        egui::Sense::click(),
    )
}

/// [`list_row`] that can also be dragged: the whole row grabs (drag and drop
/// of series, `object_dnd.rs`).
pub(super) fn list_row_draggable(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    list_row_sensed(
        ui,
        None,
        text,
        selected,
        false,
        false,
        egui::Sense::click_and_drag(),
    )
}

fn list_row_sensed(
    ui: &mut egui::Ui,
    icon: Option<egui::Image<'static>>,
    text: &str,
    selected: bool,
    dimmed: bool,
    focused: bool,
    sense: egui::Sense,
) -> egui::Response {
    let galley = egui::WidgetText::from(list_row_text(text, selected)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let icon_width = if icon.is_some() {
        ICON_SIZE + LIST_ROW_GAP
    } else {
        0.0
    };
    let natural = 2.0 * LIST_ROW_PADDING.x + icon_width + galley.size().x;
    let size = egui::vec2(ui.available_width().max(natural), LIST_ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let response = super::cursors::clickable(response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, text)
    });
    if ui.is_rect_visible(rect) {
        ui.painter()
            .extend(list_row_background(rect, selected, response.hovered()));
        let mut text_left = rect.left() + LIST_ROW_PADDING.x;
        if let Some(icon) = icon {
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(text_left, rect.center().y - ICON_SIZE / 2.0),
                egui::Vec2::splat(ICON_SIZE),
            );
            icon.paint_at(ui, icon_rect);
            text_left = icon_rect.right() + LIST_ROW_GAP;
        }
        let text_index = egui::pos2(text_left, rect.center().y - galley.size().y / 2.0);
        let color = list_row_text_color(ui, selected, dimmed);
        ui.painter().galley(text_index, galley, color);
        if focused || response.has_focus() {
            paint_focus_outline(ui, rect);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::test_support::{context, texts};

    fn rects(output: &egui::FullOutput, fill: egui::Color32) -> Vec<egui::Rect> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                _ => None,
            })
            .collect()
    }

    fn image(context: &egui::Context, paint: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let mut paint = paint;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| paint(ui));
        output.textures_delta.clear();
        output
    }

    /// A selected row: pale full-width background, accent strip on the
    /// left, bold label, and never the `selection.bg_fill` background of a
    /// button. Witness: unselected, none of this.
    #[test]
    fn a_selected_row_carries_background_strip_and_bold() {
        let context = context();
        let output = image(&context, |ui| {
            list_row(ui, None, "Series A", true, false);
            list_row(ui, None, "Series B", false, false);
        });
        let rows = rects(&output, theme::list_row_selected_fill());
        let marks = rects(&output, theme::accent());
        assert_eq!(rows.len(), 1);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].left(), rows[0].left());
        assert_eq!(marks[0].height(), rows[0].height());
        assert_eq!(marks[0].width(), LIST_ROW_MARK);
        assert!(rows[0].width() > 300.0, "full width: {:?}", rows[0]);
        let button_fill = context.global_style().visuals.selection.bg_fill;
        assert!(rects(&output, button_fill).is_empty());
        let painted_texts = texts(&output.shapes);
        assert!(painted_texts.iter().any(|t| t.text == "Series A" && t.bold));
        assert!(painted_texts
            .iter()
            .any(|t| t.text == "Series B" && !t.bold));
    }

    /// #170, point 5: a list row always has `LIST_ROW_HEIGHT`, with or
    /// without an icon, selected (bold) or not. Witness: the height of a row
    /// is not that of a plain label.
    #[test]
    fn every_list_row_has_the_single_height() {
        let context = context();
        let mut heights = Vec::new();
        image(&context, |ui| {
            for (has_icon, selected) in [(false, false), (false, true), (true, false), (true, true)]
            {
                let icon = has_icon.then(|| {
                    crate::icons::icon_image(
                        crate::icons::Icon::Folder,
                        crate::icons::IconRole::Folder,
                    )
                });
                heights.push(list_row(ui, icon, "Series", selected, false).rect.height());
            }
            heights.push(list_row_draggable(ui, "Series", false).rect.height());
            heights.push(ui.label("Series").rect.height());
        });
        assert_eq!(heights[..5], [LIST_ROW_HEIGHT; 5], "{heights:?}");
        assert!(
            heights[5] < LIST_ROW_HEIGHT,
            "witness: a bare label is shorter: {heights:?}"
        );
    }

    /// `with_list_row_height` fits checkbox, button and field in a row of
    /// `LIST_ROW_HEIGHT`, centered on its middle, then hands back the
    /// interaction height from before. Witness: without it, the row is shorter.
    #[test]
    fn the_composed_row_takes_the_single_height_and_hands_it_back() {
        let context = context();
        let mut measured = Vec::new();
        image(&context, |ui| {
            let before = ui.spacing().interact_size.y;
            for impose in [true, false] {
                let mut checkbox = true;
                let mut row_fn = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        let c = ui.checkbox(&mut checkbox, "").rect.center().y;
                        let b = ui.button("Name").rect.center().y;
                        (c, b)
                    })
                };
                let outcome = if impose {
                    with_list_row_height(ui, row_fn)
                } else {
                    row_fn(ui)
                };
                let (c, b) = outcome.inner;
                measured.push((outcome.response.rect, c, b));
            }
            assert_eq!(ui.spacing().interact_size.y, before, "height handed back");
        });
        let (with, c, b) = measured[0];
        assert_eq!(with.height(), LIST_ROW_HEIGHT);
        assert!((c - with.center().y).abs() < 0.5 && (b - with.center().y).abs() < 0.5);
        assert!(
            measured[1].0.height() < LIST_ROW_HEIGHT,
            "witness: {measured:?}"
        );
    }

    /// The background of a composed row is at least `LIST_ROW_HEIGHT` tall,
    /// even if the measured row is shorter.
    #[test]
    fn the_reserved_background_imposes_the_minimum_height() {
        let context = context();
        let output = image(&context, |ui| {
            let background = ListRowBackground::reserve(ui);
            let row = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 18.0));
            background.paint(ui, row, true, false);
        });
        let rows = rects(&output, theme::list_row_selected_fill());
        assert_eq!(rows[0].height(), LIST_ROW_HEIGHT);
        let output = image(&context, |ui| {
            let background = ListRowBackground::reserve(ui);
            let row = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 40.0));
            background.paint(ui, row, true, false);
        });
        assert_eq!(
            rects(&output, theme::list_row_selected_fill())[0].height(),
            40.0,
            "witness: a taller row is not clipped"
        );
    }

    /// The reserved background of a composed row goes **under** its widgets and
    /// runs to the right edge; with no selection or hover, it paints nothing.
    #[test]
    fn the_reserved_background_goes_under_the_row_widgets() {
        let context = context();
        for selected in [true, false] {
            let output = image(&context, |ui| {
                let background = ListRowBackground::reserve(ui);
                let row = ui
                    .horizontal(|ui| {
                        ui.label("box");
                        ui.label(list_row_text("Object", selected));
                    })
                    .response
                    .rect;
                background.paint(ui, row, selected, false);
            });
            let fill = output.shapes.iter().position(|s| {
                matches!(&s.shape, egui::Shape::Rect(r) if r.fill == theme::list_row_selected_fill())
            });
            let text_index = output
                .shapes
                .iter()
                .position(|s| matches!(&s.shape, egui::Shape::Text(_)));
            if selected {
                let (fill, text_index) = (fill.expect("background"), text_index.expect("text"));
                assert!(
                    fill < text_index,
                    "the background is painted before the widgets"
                );
                let rows = rects(&output, theme::list_row_selected_fill());
                assert!(rows[0].right() >= 399.0, "up to the edge: {:?}", rows[0]);
                assert_eq!(rects(&output, theme::accent()).len(), 1);
            } else {
                assert!(fill.is_none(), "witness: nothing without selection");
                assert!(rects(&output, theme::accent()).is_empty());
            }
        }
    }
}
