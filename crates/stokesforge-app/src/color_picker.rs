//! Color picker of StokesForge: a swatch button that opens a popup with a
//! **palette** suited to what is being colored, the **saved colors**, a
//! **color area** (saturation × lightness square or **color wheel**), the
//! **opacity**, and entry as **RGB**, **HSL** and **hexadecimal**.
//!
//! # One palette per usage
//!
//! A curve, a text and a background do not need the same colors
//! ([`ColorUsage`]):
//!
//! - **curves**: a separate, fuller tool — a choice of named palettes
//!   ([`stokesforge_plot::CURVE_PALETTES`]: StokesForge, Tableau 10,
//!   Okabe-Ito (colorblind-safe), ColorBrewer, and the gradients
//!   Viridis, Plasma, grayscale), each with its darker and lighter shades.
//!   The palettes live in `stokesforge-plot` so that a future change
//!   can color several curves at once (palette or gradient) without
//!   moving them;
//! - **text**: readable inks — blacks, grays, white, dark tints;
//! - **backgrounds**: light tints, **dark** backgrounds (the automatic text
//!   then turns white, rule of `stokesforge_render::color::
//!   auto_ink`), and **"No color"**: a transparent background, to embed the
//!   exported figure in another presentation.
//!
//! # Saved colors
//!
//! "Save Color" keeps the current color **in the project** (by default: it
//! travels with the `.stf` and is undone by Ctrl+Z) or **for the user**
//! (on this machine, in all projects) — see
//! [`stokesforge_app_state::state::SaveTarget`]. A saved color reappears only
//! in the palette of its usage. Right-click on a saved color: remove
//! it. This module touches neither the model nor the disk: it drops
//! [`SavedColorRequest`]s into the [`PickerEnv`] supplied by the caller, which
//! turns them into actions.
//!
//! # "Automatic", "No color" and opacity
//!
//! In the model, a color is a `#RRGGBB` string (opaque) or
//! `#RRGGBBAA`; the empty string means "automatic" (ink contrasting
//! with the background, line color for a marker…). "No color" is a zero
//! opacity (`#ffffff00`). A transparent color is shown on a
//! checkerboard.
//!
//! # When the color is committed
//!
//! The string is modified live (immediate visual feedback in the
//! graph), and [`color_button`] returns `true` when the interaction is over:
//! click on a swatch, end of a drag in the color area, field
//! committed, popup closed. Same history anti-inflation rule as the rest
//! of the properties window (`crate::app::plot_settings`).
//!
//! The conversions (hexadecimal, RGB ↔ HSL, RGB ↔ HSV) and the palettes are
//! pure functions, tested without a window.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
pub use stokesforge_plot::ColorUsage;
use stokesforge_plot::{SavedColors, CURVE_PALETTES};

use crate::app::hover_help::help_text;
use crate::app::tab_strip::{tab_strip, TabStripSide};
use crate::number_field::number_field;
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{AppState, SaveTarget};
use stokesforge_app_state::tr;

/// A named row of swatches.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteSection {
    /// Title of the row.
    pub title: String,
    /// Colors, as RGBA (opacity included: "No color" is
    /// transparent).
    pub colors: Vec<[u8; 4]>,
}

/// The displayed name of the curve palette at `index` in [`CURVE_PALETTES`]
/// (clamped to the bounds). The ids of `stokesforge-plot` belong to the
/// renderer-neutral graph description: the interface writes the names through
/// the catalog (`color-palette-*` keys, in the order of the table).
pub fn curve_palette_name(index: usize) -> String {
    let index = index.min(CURVE_PALETTES.len() - 1);
    match index {
        0 => tr!("color-palette-stokesforge"),
        1 => tr!("color-palette-tableau"),
        2 => tr!("color-palette-okabe-ito"),
        3 => tr!("color-palette-brewer-set1"),
        4 => tr!("color-palette-brewer-dark2"),
        5 => tr!("color-palette-brewer-paired"),
        6 => tr!("color-palette-viridis"),
        7 => tr!("color-palette-plasma"),
        8 => tr!("color-palette-grayscale"),
        _ => CURVE_PALETTES[index].id.to_string(),
    }
}

/// Inks of the "text" palette.
const TEXT_INKS: [&str; 12] = [
    "#000000", "#202020", "#404040", "#6b6b6b", "#a0a0a0", "#ffffff", "#1f2a44", "#1f4e79",
    "#8b1a1a", "#1e5631", "#4b2a6b", "#5c3a1e",
];

/// Light backgrounds of the "backgrounds" palette, preceded by "No color"
/// ([`NO_COLOR`]), the default background of a graph.
const LIGHT_BACKGROUNDS: [&str; 9] = [
    NO_COLOR, "#ffffff", "#fafaf7", "#f5f0e1", "#f4ecd8", "#f0f0f0", "#e6e6e6", "#eef4fb",
    "#eef7ee",
];

/// Dark backgrounds of the "backgrounds" palette: automatic text in white.
const DARK_BACKGROUNDS: [&str; 6] = [
    "#000000", "#1e1e1e", "#2b2f36", "#101820", "#14213d", "#102a1f",
];

/// The string of a background with no color: white with zero opacity — the
/// default background of a graph (`stokesforge_plot::TRANSPARENT`).
pub const NO_COLOR: &str = stokesforge_plot::TRANSPARENT;

fn parse_all(colors: &[&str]) -> Vec<[u8; 4]> {
    colors.iter().filter_map(|hex| parse_hex(hex)).collect()
}

/// The rows of the palette for `usage`. For curves, those of the named
/// palette at index `curve_palette` in
/// [`stokesforge_plot::CURVE_PALETTES`] (clamped into bounds).
pub fn palette(usage: ColorUsage, curve_palette: usize) -> Vec<PaletteSection> {
    match usage {
        ColorUsage::Curves => {
            let named = CURVE_PALETTES[curve_palette.min(CURVE_PALETTES.len() - 1)];
            let base = parse_all(named.colors);
            let shade = |factor: fn(f32) -> f32| {
                base.iter()
                    .map(|&[r, g, b, a]| {
                        let (h, s, l) = rgb_to_hsl([r, g, b]);
                        let [r, g, b] = hsl_to_rgb(h, s, factor(l));
                        [r, g, b, a]
                    })
                    .collect()
            };
            vec![
                PaletteSection {
                    title: curve_palette_name(curve_palette),
                    colors: base.clone(),
                },
                PaletteSection {
                    title: tr!("color-shades-dark"),
                    colors: shade(|l| l * 0.62),
                },
                PaletteSection {
                    title: tr!("color-shades-light"),
                    colors: shade(|l| l + (100.0 - l) * 0.45),
                },
            ]
        }
        ColorUsage::Text => vec![PaletteSection {
            title: tr!("color-section-text"),
            colors: parse_all(&TEXT_INKS),
        }],
        ColorUsage::Background => vec![
            PaletteSection {
                title: tr!("color-section-light-backgrounds"),
                colors: parse_all(&LIGHT_BACKGROUNDS),
            },
            PaletteSection {
                title: tr!("color-section-dark-backgrounds"),
                colors: parse_all(&DARK_BACKGROUNDS),
            },
        ],
    }
}

/// The components of a `#RRGGBB` (opaque) or `#RRGGBBAA` color —
/// case-insensitive, `#` optional, surrounding spaces ignored —, or
/// `None`. The syntax is that of [`stokesforge_plot::parse_hex`], the single
/// source shared with rendering (issue #25).
pub fn parse_hex(text: &str) -> Option<[u8; 4]> {
    stokesforge_plot::parse_hex(text)
}

/// The string of a color: `#rrggbb` if it is opaque, `#rrggbbaa`
/// otherwise.
pub fn to_hex([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// RGB → HSL: hue in degrees (0–360), saturation and lightness as a
/// percentage (0–100).
pub fn rgb_to_hsl([r, g, b]: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    (hue_of(r, g, b, max, delta), s * 100.0, l * 100.0)
}

/// HSL → RGB, inverse of [`rgb_to_hsl`]. Out-of-bounds inputs are
/// clamped into their bounds.
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0);
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    from_chroma(h, c, l - c / 2.0)
}

/// RGB → HSV: hue, saturation and value, each between 0 and 1.
pub fn rgb_to_hsv([r, g, b]: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let s = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    let h = if delta <= f32::EPSILON {
        0.0
    } else {
        hue_of(r, g, b, max, delta) / 360.0
    };
    (h, s, max)
}

/// HSV → RGB, inverse of [`rgb_to_hsv`].
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = (h * 360.0).rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    from_chroma(h, c, v - c)
}

fn hue_of(r: f32, g: f32, b: f32, max: f32, delta: f32) -> f32 {
    let h = if (max - r).abs() <= f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    h * 60.0
}

fn from_chroma(h: f32, c: f32, m: f32) -> [u8; 3] {
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let channel = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [channel(r), channel(g), channel(b)]
}

/// Point of a color wheel of radius 1 centered at the origin → (hue,
/// saturation), each between 0 and 1; hue 0 (red) is on the right and
/// turns counterclockwise, saturation grows toward the edge.
pub fn wheel_to_hue_saturation(dx: f32, dy: f32) -> (f32, f32) {
    let hue = (-dy).atan2(dx) / std::f32::consts::TAU;
    (hue.rem_euclid(1.0), (dx * dx + dy * dy).sqrt().min(1.0))
}

/// Inverse of [`wheel_to_hue_saturation`].
pub fn hue_saturation_to_wheel(hue: f32, saturation: f32) -> (f32, f32) {
    let angle = hue * std::f32::consts::TAU;
    (saturation * angle.cos(), -saturation * angle.sin())
}

/// Save or remove a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedColorEdit {
    /// Save.
    Save,
    /// Remove.
    Remove,
}

/// A request from the picker about saved colors, which the caller turns
/// into an action (`Action::SaveColor`/`RemoveSavedColor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedColorRequest {
    /// Save or remove.
    pub edit: SavedColorEdit,
    /// Usage.
    pub usage: ColorUsage,
    /// Project or user.
    pub target: SaveTarget,
    /// The color.
    pub color: String,
}

impl SavedColorRequest {
    /// The action that carries out the request.
    pub fn into_action(self) -> stokesforge_app_state::state::Action {
        use stokesforge_app_state::state::Action;
        match self.edit {
            SavedColorEdit::Save => Action::SaveColor {
                usage: self.usage,
                color: self.color,
                target: self.target,
            },
            SavedColorEdit::Remove => Action::RemoveSavedColor {
                usage: self.usage,
                color: self.color,
                target: self.target,
            },
        }
    }
}

/// The saved colors, published every frame by the root so that
/// the color picker of the in-place editor also offers them from
/// a detached window (which has no `AppState`).
pub(crate) fn saved_colors_id() -> egui::Id {
    egui::Id::new("stokesforge_saved_colors_for_graphs")
}

pub(crate) fn publish_saved_colors(ctx: &egui::Context, state: &AppState) {
    let colors = (
        state.saved_colors(SaveTarget::Project),
        state.saved_colors(SaveTarget::User),
    );
    ctx.data_mut(|data| data.insert_temp(saved_colors_id(), colors));
}

/// What the picker reads and writes besides the edited color: the saved
/// colors, and the requests that modify them.
#[derive(Debug, Clone, Default)]
pub struct PickerEnv {
    /// Colors saved in the project.
    pub project: SavedColors,
    /// Colors saved for the user.
    pub user: SavedColors,
    /// Requests dropped during this frame.
    pub requests: Vec<SavedColorRequest>,
}

/// A label followed by [`color_button`], on one line.
pub fn color_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        color_button(ui, label, value, usage, auto_label, env)
    })
    .inner
}

/// Side of the button's swatch: a small square (#89).
const SWATCH_SIDE: f32 = 12.0;
/// Room reserved for the button: the square, plus a little air on the sides.
const BUTTON_SWATCH: Vec2 = Vec2::new(14.5, SWATCH_SIDE);
/// The stroke across an undefined (automatic) color, the same red
/// as the `Danger` role of the icons.
const AUTO_SLASH: Color32 = Color32::from_rgb(0xe5, 0x45, 0x3b);
/// Size of a palette swatch.
const PALETTE_SWATCH: f32 = 18.0;
/// Side of the color-area square (and diameter of the wheel).
const PICKER_SIDE: f32 = 200.0;
/// Height of the bars (hue, value, opacity).
const BAR_HEIGHT: f32 = 16.0;
/// Fineness of the color-area meshes.
const MESH_STEPS: usize = 24;
/// Side of a cell of the checkerboard for transparent colors.
const CHECKER: f32 = 5.0;

/// Color area shown: square or wheel. Choice remembered for the session,
/// common to all pickers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum PickerShape {
    #[default]
    Square,
    Wheel,
}

/// State of the color area between two frames: the color as HSV, kept apart
/// from the string so as not to lose the hue on a gray (where it is not
/// defined) while dragging.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PickerMemory {
    /// The color `hsv` corresponds to; if the model changes (other
    /// field, undo), the memory is recomputed from the string.
    rgb: [u8; 3],
    hsv: (f32, f32, f32),
}

/// Height of the preview strip of a color gradient.
const GRADIENT_STRIP_HEIGHT: f32 = 14.0;
/// Width of the swatch of a color gradient in a list.
const GRADIENT_SWATCH_WIDTH: f32 = 64.0;

/// Preview of a color gradient (#178): a strip of slices, one per
/// `#RRGGBB` color of `colors`, over the whole available width. An unreadable
/// color leaves its slice empty; with no color (#213), the strip is a
/// grayed background.
pub(crate) fn gradient_strip(ui: &mut egui::Ui, colors: &[String]) {
    let width = ui.available_width();
    paint_gradient(ui, colors, width, egui::Sense::hover());
}

/// The same strip as a fixed-width swatch, for a list entry (#213);
/// it receives the click, like the name that follows it.
pub(crate) fn gradient_swatch(ui: &mut egui::Ui, colors: &[String]) -> egui::Response {
    paint_gradient(ui, colors, GRADIENT_SWATCH_WIDTH, egui::Sense::click())
}

fn paint_gradient(
    ui: &mut egui::Ui,
    colors: &[String],
    width: f32,
    sense: egui::Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, GRADIENT_STRIP_HEIGHT), sense);
    if colors.is_empty() {
        let fill = ui.visuals().widgets.noninteractive.weak_bg_fill;
        ui.painter().rect_filled(rect, 0.0, fill);
        return response;
    }
    let step = rect.width() / colors.len() as f32;
    for (index, color) in colors.iter().enumerate() {
        if let Some(rgba) = parse_hex(color) {
            let min = rect.min + Vec2::new(index as f32 * step, 0.0);
            let slice =
                Rect::from_min_size(min, Vec2::new(step.ceil(), rect.height())).intersect(rect);
            paint_swatch(ui.painter(), slice, rgba);
        }
    }
    response
}

/// Paints `rgba` in `rect`, on a checkerboard if it is not opaque.
fn paint_swatch(painter: &egui::Painter, rect: Rect, [r, g, b, a]: [u8; 4]) {
    if a < 255 {
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        let columns = (rect.width() / CHECKER).ceil() as i32;
        let rows = (rect.height() / CHECKER).ceil() as i32;
        for row in 0..rows {
            for column in 0..columns {
                if (row + column) % 2 == 0 {
                    continue;
                }
                let min = rect.min + Vec2::new(column as f32 * CHECKER, row as f32 * CHECKER);
                let cell = Rect::from_min_size(min, Vec2::splat(CHECKER)).intersect(rect);
                painter.rect_filled(cell, 0.0, Color32::from_gray(0xcc));
            }
        }
    }
    painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(r, g, b, a));
}

/// The swatch button and its popup. `value` is the model string
/// (empty = automatic when `auto_label` is given). Returns `true`
/// when a gesture has just ended — see the module doc.
pub fn color_button(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    let current = parse_hex(value);
    let (cell, response) = ui.allocate_exact_size(BUTTON_SWATCH, Sense::click());
    let rect = Rect::from_center_size(cell.center(), Vec2::splat(SWATCH_SIDE));
    let response = response.on_hover_text(match current {
        Some([_, _, _, 0]) => tr!("color-none"),
        Some(rgba) => to_hex(rgba),
        None => auto_label.map_or_else(|| tr!("common-auto"), str::to_string),
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let stroke = ui.visuals().widgets.inactive.fg_stroke;
        match current {
            Some(rgba) => paint_swatch(painter, rect, rgba),
            // Undefined color = automatic: a light square crossed by a red
            // stroke (#89).
            None => {
                painter.rect_filled(rect, 2.0, Color32::WHITE);
                let inner = rect.shrink(1.5);
                painter.line_segment(
                    [inner.left_bottom(), inner.right_top()],
                    egui::Stroke::new(1.5, AUTO_SLASH),
                );
            }
        }
        painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    }

    let popup_id = ui.make_persistent_id(("stokesforge_color_popup", id_salt));
    let was_open = picker_is_open(ui.ctx(), popup_id);
    if response.clicked() {
        set_picker_open(ui.ctx(), popup_id, !was_open);
    }
    let mut commit = false;
    // Our own `Area`, not `egui::Popup`: egui keeps only one popup open
    // per viewport, so the list of a `ComboBox` placed in the picker would
    // close it (issue #9). Closing (Escape, click outside) is written
    // here, sparing the child popup layers.
    if picker_is_open(ui.ctx(), popup_id) {
        let area = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.left_bottom() + egui::vec2(0.0, 4.0))
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(PICKER_SIDE + 70.0);
                    commit |= picker_contents(ui, popup_id, value, usage, auto_label, env);
                });
            });
        // Escape is consumed: the dialog that contains the picker must not
        // close with it.
        let escape =
            ui.input_mut(|input| input.consume_shortcut(&crate::app::shortcuts::KEY_ESCAPE));
        let pressed_at = ui.input(|input| {
            input
                .pointer
                .any_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        let list_open = egui::Popup::is_any_open(ui.ctx());
        let outside = pressed_at.is_some_and(|pos| {
            !response.rect.contains(pos)
                && !ui.ctx().layer_id_at(pos).is_some_and(|layer| {
                    // A dropdown list (child popup layer) counts as
                    // "inside"; the dialog and its blocking layer are at
                    // the same order but are "outside".
                    layer == area.response.layer_id
                        || (list_open && layer.order == egui::Order::Foreground)
                })
        });
        if escape || outside {
            set_picker_open(ui.ctx(), popup_id, false);
        }
    }
    let is_open = picker_is_open(ui.ctx(), popup_id);
    // Closing the popup commits whatever may have been modified without an
    // explicit end gesture (field still being typed in).
    commit || (was_open && !is_open)
}

/// An icon that says *which* color is being set, followed by its compact
/// swatch: the form of the toolbars (#89). Forms keep a label in front
/// of the swatch (`color_field`). `tooltip` comes from `tr!`.
pub fn color_icon_button(
    ui: &mut egui::Ui,
    (icon, tooltip): (crate::icons::Icon, &str),
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add(
            crate::icons::icon_image(icon, crate::icons::IconRole::Neutral).sense(Sense::hover()),
        )
        .on_hover_text(tooltip);
        color_button(ui, id_salt, value, usage, auto_label, env)
    })
    .inner
}

fn picker_is_open(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false)
}

fn set_picker_open(ctx: &egui::Context, id: egui::Id, open: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(id, open);
        if !open {
            // The picker always reopens folded onto the palette (#10).
            data.remove::<bool>(custom_id(id));
        }
    });
}

/// Key of the "Custom color" expander of the picker `popup_id` (#10).
fn custom_id(popup_id: egui::Id) -> egui::Id {
    popup_id.with("custom")
}

/// Content of the popup.
fn picker_contents(
    ui: &mut egui::Ui,
    popup_id: egui::Id,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    let mut close = false;
    let mut chosen: Option<[u8; 4]> = None;

    // -- Palette -------------------------------------------------------
    let curve_palette_id = egui::Id::new("stokesforge_curve_palette");
    let mut curve_palette: usize = ui
        .memory(|memory| memory.data.get_temp(curve_palette_id))
        .unwrap_or(0);
    if usage == ColorUsage::Curves {
        // The curves tool: a choice of named palettes.
        egui::ComboBox::from_id_salt(popup_id.with("curve_palette"))
            .width(PICKER_SIDE + 20.0)
            .selected_text(curve_palette_name(curve_palette))
            .show_ui(ui, |ui| {
                for index in 0..CURVE_PALETTES.len() {
                    ui.selectable_value(&mut curve_palette, index, curve_palette_name(index));
                }
            });
        ui.memory_mut(|memory| memory.data.insert_temp(curve_palette_id, curve_palette));
    }
    let current = parse_hex(value);
    for section in palette(usage, curve_palette) {
        ui.label(crate::theme::section_heading(section.title));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(3.0);
            for rgba in section.colors {
                let response = palette_swatch(ui, rgba, current == Some(rgba));
                if response.clicked() {
                    chosen = Some(rgba);
                    close = true;
                }
            }
        });
        ui.add_space(crate::theme::SPACE_ITEM);
    }
    // The color area, opacity, RGB/HSL and saving are hidden: the
    // "Custom color" checkbox at the end of the palette unfolds them (#10; a
    // checkbox since S-C5, #227).
    let mut custom = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(custom_id(popup_id)))
        .unwrap_or(false);
    ui.horizontal_wrapped(|ui| {
        if let Some(auto_label) = auto_label {
            if ui
                .selectable_label(value.trim().is_empty(), auto_label)
                .clicked()
            {
                value.clear();
                commit = true;
                close = true;
            }
        }
        // A toggle is a checkbox, label on the right (S-C5, #227).
        crate::app::hover_help::help_beside(ui, tr!("color-custom", "hint"), |ui| {
            ui.checkbox(&mut custom, tr!("color-custom"))
        });
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(custom_id(popup_id), custom));

    // -- Saved colors ---------------------------------------------------
    ui.separator();
    for (target, title) in [
        (SaveTarget::Project, tr!("color-saved-project")),
        (SaveTarget::User, tr!("color-saved-user")),
    ] {
        let saved = match target {
            SaveTarget::Project => env.project.get(usage).to_vec(),
            SaveTarget::User => env.user.get(usage).to_vec(),
        };
        if saved.is_empty() {
            continue;
        }
        ui.label(crate::theme::section_heading(title));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(3.0);
            for color in saved {
                let Some(rgba) = parse_hex(&color) else {
                    continue;
                };
                let response = palette_swatch(ui, rgba, current == Some(rgba))
                    .on_hover_text(tr!("color-saved-remove-hint"));
                if response.clicked() {
                    chosen = Some(rgba);
                    close = true;
                }
                if response.secondary_clicked() {
                    env.requests.push(SavedColorRequest {
                        edit: SavedColorEdit::Remove,
                        usage,
                        target,
                        color,
                    });
                }
            }
        });
    }
    if custom {
        let target_id = egui::Id::new("stokesforge_save_color_target");
        let mut target: SaveTarget = ui
            .memory(|memory| memory.data.get_temp(target_id))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(current.is_some(), egui::Button::new(tr!("color-save")))
                .on_hover_text(tr!("color-save", "hint"))
                .on_disabled_hover_text(tr!("disabled-color-invalid"))
                .clicked()
            {
                if let Some(rgba) = current {
                    env.requests.push(SavedColorRequest {
                        edit: SavedColorEdit::Save,
                        usage,
                        target,
                        color: to_hex(rgba),
                    });
                }
            }
            egui::ComboBox::from_id_salt(popup_id.with("save_target"))
                .selected_text(target.label())
                .show_ui(ui, |ui| {
                    for candidate in [SaveTarget::Project, SaveTarget::User] {
                        ui.selectable_value(&mut target, candidate, candidate.label());
                    }
                });
        });
        ui.memory_mut(|memory| memory.data.insert_temp(target_id, target));
        ui.separator();
    }

    // -- Color area ------------------------------------------------------
    let [r0, g0, b0, a0] = current.unwrap_or([0x80, 0x80, 0x80, 255]);
    let rgb = [r0, g0, b0];
    let memory_id = popup_id.with("hsv");
    let mut memory = ui
        .memory(|memory| memory.data.get_temp::<PickerMemory>(memory_id))
        .filter(|memory| memory.rgb == rgb)
        .unwrap_or(PickerMemory {
            rgb,
            hsv: rgb_to_hsv(rgb),
        });
    let mut new_rgb: Option<[u8; 3]> = None;
    let mut alpha = a0;
    if custom {
        let shape_id = egui::Id::new("stokesforge_picker_shape");
        let mut shape: PickerShape = ui
            .memory(|memory| memory.data.get_temp(shape_id))
            .unwrap_or_default();
        // Two views of the same color area: a small set of tabs, so the
        // common strip (#170, point 7), not selectable buttons.
        let mut chosen = shape;
        tab_strip(
            ui,
            TabStripSide::Top,
            &[
                (PickerShape::Square, tr!("color-shape-square")),
                (PickerShape::Wheel, tr!("color-shape-wheel")),
            ],
            shape,
            |picked| chosen = picked,
            None,
            || {},
        );
        shape = chosen;
        ui.memory_mut(|m| m.data.insert_temp(shape_id, shape));

        let (h, s, v) = &mut memory.hsv;
        let gestures = match shape {
            PickerShape::Square => [sv_square(ui, *h, s, v), hue_bar(ui, h)],
            PickerShape::Wheel => [color_wheel(ui, h, s, *v), value_bar(ui, *h, *s, v)],
        };
        if gestures.iter().any(|gesture| gesture.changed) {
            new_rgb = Some(hsv_to_rgb(memory.hsv.0, memory.hsv.1, memory.hsv.2));
        }
        commit |= gestures.iter().any(|gesture| gesture.finished);

        // -- Opacity ---------------------------------------------------------
        let mut opacity = (f32::from(a0) / 255.0 * 100.0).round() as i32;
        let opacity_changed = ui
            .horizontal(|ui| {
                ui.label(tr!("color-opacity"));
                number_field(
                    ui,
                    "color_opacity",
                    &mut opacity,
                    &quantities::OPACITY_PERCENT,
                )
                .changed
            })
            .inner;
        if opacity_changed {
            alpha = (opacity as f32 / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8;
            commit = true;
        }

        // -- RGB ---------------------------------------------------------------
        ui.add_space(crate::theme::SPACE_ITEM);
        let shown = new_rgb.unwrap_or(rgb);
        let mut channels = shown.map(i32::from);
        let mut rgb_changed = false;
        ui.horizontal(|ui| {
            ui.label(tr!("color-rgb"));
            let labels = [
                tr!("color-channel-red"),
                tr!("color-channel-green"),
                tr!("color-channel-blue"),
            ];
            for (index, (channel, label)) in channels.iter_mut().zip(labels).enumerate() {
                ui.label(label);
                rgb_changed |= number_field(
                    ui,
                    ("color_channel", index),
                    channel,
                    &quantities::COLOR_CHANNEL,
                )
                .changed;
            }
        });
        if rgb_changed {
            let edited = channels.map(|c| c.clamp(0, 255) as u8);
            new_rgb = Some(edited);
            memory.hsv = rgb_to_hsv(edited);
            commit = true;
        }

        // -- HSL ---------------------------------------------------------------
        let (th, ts, tl) = rgb_to_hsl(new_rgb.unwrap_or(rgb));
        let (mut th, mut ts, mut tl) = (th.round() as i32, ts.round() as i32, tl.round() as i32);
        let mut hsl_changed = false;
        ui.horizontal(|ui| {
            ui.label(tr!("color-hsl"));
            ui.label(tr!("color-channel-hue"));
            hsl_changed |= number_field(ui, "color_hue", &mut th, &quantities::HUE_DEGREES).changed;
            ui.label(tr!("color-channel-saturation"));
            hsl_changed |=
                number_field(ui, "color_saturation", &mut ts, &quantities::PERCENT).changed;
            ui.label(tr!("color-channel-lightness"));
            hsl_changed |=
                number_field(ui, "color_lightness", &mut tl, &quantities::PERCENT).changed;
        });
        if hsl_changed {
            let edited = hsl_to_rgb(th as f32, ts as f32, tl as f32);
            new_rgb = Some(edited);
            let (_, s, v) = rgb_to_hsv(edited);
            memory.hsv = (th as f32 / 360.0, s, v);
            commit = true;
        }
    }

    // -- Hexadecimal: applied only when readable, on commit ----------------
    let hex_id = popup_id.with("hex");
    let [r1, g1, b1] = new_rgb.unwrap_or(rgb);
    let mut hex_text: String = ui
        .memory(|memory| memory.data.get_temp(hex_id))
        .unwrap_or_else(|| to_hex([r1, g1, b1, alpha]));
    let hex_response = ui
        .horizontal(|ui| {
            ui.label(tr!("color-hex"));
            let response = ui.add(egui::TextEdit::singleline(&mut hex_text).desired_width(90.0));
            help_text(ui, tr!("color-hex", "hint"));
            response
        })
        .inner;
    if hex_response.has_focus() {
        ui.memory_mut(|memory| memory.data.insert_temp(hex_id, hex_text.clone()));
    } else {
        ui.memory_mut(|memory| memory.data.remove::<String>(hex_id));
    }
    if hex_response.lost_focus() {
        if let Some([r, g, b, a]) = parse_hex(&hex_text) {
            new_rgb = Some([r, g, b]);
            alpha = a;
            memory.hsv = rgb_to_hsv([r, g, b]);
            commit = true;
        }
    }

    // -- Apply -------------------------------------------------------------
    if let Some(rgba) = chosen {
        *value = to_hex(rgba);
        commit = true;
    } else if new_rgb.is_some() || alpha != a0 {
        let [r, g, b] = new_rgb.unwrap_or(rgb);
        *value = to_hex([r, g, b, alpha]);
        memory.rgb = [r, g, b];
    }
    ui.memory_mut(|m| m.data.insert_temp(memory_id, memory));
    if close {
        set_picker_open(ui.ctx(), popup_id, false);
    }
    commit
}

/// Tooltip of a swatch: its code, or "No color" if it is
/// fully transparent.
fn swatch_label(rgba: [u8; 4]) -> String {
    if rgba[3] == 0 {
        tr!("color-none-swatch")
    } else {
        to_hex(rgba)
    }
}

/// A clickable palette swatch.
fn palette_swatch(ui: &mut egui::Ui, rgba: [u8; 4], selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(PALETTE_SWATCH), Sense::click());
    let response = response.on_hover_text(swatch_label(rgba));
    let painter = ui.painter();
    paint_swatch(painter, rect, rgba);
    let stroke = if selected {
        Stroke::new(2.0, ui.visuals().selection.stroke.color)
    } else if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke
    } else {
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    response
}

/// Result of a color-area element this frame.
struct PickerGesture {
    changed: bool,
    finished: bool,
}

/// Pointer position during a press or click on `response`.
fn pressed_pos(response: &egui::Response) -> Option<Pos2> {
    response
        .interact_pointer_pos()
        .filter(|_| response.is_pointer_button_down_on() || response.clicked())
}

fn gesture(response: &egui::Response) -> PickerGesture {
    PickerGesture {
        changed: false,
        finished: response.drag_stopped() || response.clicked(),
    }
}

/// The saturation (horizontal) × value (vertical) square at hue `h`.
fn sv_square(ui: &mut egui::Ui, h: f32, s: &mut f32, v: &mut f32) -> PickerGesture {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(PICKER_SIDE), Sense::click_and_drag());
    let mut result = gesture(&response);
    if let Some(pos) = pressed_pos(&response) {
        let new_s = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let new_v = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        if (new_s, new_v) != (*s, *v) {
            (*s, *v) = (new_s, new_v);
            result.changed = true;
        }
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        for row in 0..=MESH_STEPS {
            for column in 0..=MESH_STEPS {
                let fs = column as f32 / MESH_STEPS as f32;
                let fv = 1.0 - row as f32 / MESH_STEPS as f32;
                let [r, g, b] = hsv_to_rgb(h, fs, fv);
                let pos = Pos2::new(
                    rect.left() + fs * rect.width(),
                    rect.top() + (1.0 - fv) * rect.height(),
                );
                mesh.colored_vertex(pos, Color32::from_rgb(r, g, b));
                if row < MESH_STEPS && column < MESH_STEPS {
                    let i = (row * (MESH_STEPS + 1) + column) as u32;
                    let next_row = i + MESH_STEPS as u32 + 1;
                    mesh.add_triangle(i, i + 1, next_row);
                    mesh.add_triangle(i + 1, next_row + 1, next_row);
                }
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let marker = Pos2::new(
            rect.left() + *s * rect.width(),
            rect.top() + (1.0 - *v) * rect.height(),
        );
        let contrast = if *v > 0.5 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        painter.circle_stroke(marker, 5.0, Stroke::new(1.5, contrast));
        painter.rect_stroke(
            rect,
            0.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }
    result
}

/// The color wheel: hue by angle, saturation by distance
/// from the center, at value `v`.
fn color_wheel(ui: &mut egui::Ui, h: &mut f32, s: &mut f32, v: f32) -> PickerGesture {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(PICKER_SIDE), Sense::click_and_drag());
    let center = rect.center();
    let radius = rect.width() / 2.0;
    let mut result = gesture(&response);
    if let Some(pos) = pressed_pos(&response) {
        let (new_h, new_s) =
            wheel_to_hue_saturation((pos.x - center.x) / radius, (pos.y - center.y) / radius);
        if (new_h, new_s) != (*h, *s) {
            (*h, *s) = (new_h, new_s);
            result.changed = true;
        }
    }
    if ui.is_rect_visible(rect) {
        const RINGS: usize = 12;
        const SEGMENTS: usize = 72;
        let mut mesh = egui::Mesh::default();
        let [r, g, b] = hsv_to_rgb(0.0, 0.0, v);
        mesh.colored_vertex(center, Color32::from_rgb(r, g, b));
        for ring in 1..=RINGS {
            let saturation = ring as f32 / RINGS as f32;
            for segment in 0..SEGMENTS {
                let hue = segment as f32 / SEGMENTS as f32;
                let (dx, dy) = hue_saturation_to_wheel(hue, saturation);
                let [r, g, b] = hsv_to_rgb(hue, saturation, v);
                mesh.colored_vertex(
                    center + Vec2::new(dx, dy) * radius,
                    Color32::from_rgb(r, g, b),
                );
            }
        }
        let vertex = |ring: usize, segment: usize| -> u32 {
            if ring == 0 {
                0
            } else {
                (1 + (ring - 1) * SEGMENTS + segment % SEGMENTS) as u32
            }
        };
        for segment in 0..SEGMENTS {
            mesh.add_triangle(0, vertex(1, segment), vertex(1, segment + 1));
            for ring in 2..=RINGS {
                let (a, b) = (vertex(ring - 1, segment), vertex(ring - 1, segment + 1));
                let (c, d) = (vertex(ring, segment), vertex(ring, segment + 1));
                mesh.add_triangle(a, c, d);
                mesh.add_triangle(a, d, b);
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let (dx, dy) = hue_saturation_to_wheel(*h, *s);
        let marker = center + Vec2::new(dx, dy) * radius;
        let contrast = if v > 0.5 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        painter.circle_stroke(marker, 5.0, Stroke::new(1.5, contrast));
        painter.circle_stroke(
            center,
            radius,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    }
    result
}

/// A horizontal bar painted with a gradient `color(f)`, f from 0 to 1, with
/// a cursor at `position`. Returns the new position if it was moved.
fn gradient_bar(
    ui: &mut egui::Ui,
    position: f32,
    color: impl Fn(f32) -> Color32,
) -> (Option<f32>, PickerGesture) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(PICKER_SIDE, BAR_HEIGHT), Sense::click_and_drag());
    let result = gesture(&response);
    let moved = pressed_pos(&response)
        .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
        .filter(|&f| f != position);
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        for step in 0..=MESH_STEPS {
            let f = step as f32 / MESH_STEPS as f32;
            let x = rect.left() + f * rect.width();
            mesh.colored_vertex(Pos2::new(x, rect.top()), color(f));
            mesh.colored_vertex(Pos2::new(x, rect.bottom()), color(f));
            if step < MESH_STEPS {
                let i = (step * 2) as u32;
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i + 1, i + 3, i + 2);
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let x = rect.left() + moved.unwrap_or(position) * rect.width();
        painter.rect_stroke(
            Rect::from_center_size(
                Pos2::new(x, rect.center().y),
                Vec2::new(4.0, rect.height() + 2.0),
            ),
            1.0,
            Stroke::new(1.5, Color32::BLACK),
            egui::StrokeKind::Outside,
        );
    }
    (moved, result)
}

/// The hue bar, from 0 to 360°.
fn hue_bar(ui: &mut egui::Ui, h: &mut f32) -> PickerGesture {
    let (moved, mut result) = gradient_bar(ui, *h, |f| {
        let [r, g, b] = hsv_to_rgb(f, 1.0, 1.0);
        Color32::from_rgb(r, g, b)
    });
    if let Some(new_h) = moved {
        *h = new_h;
        result.changed = true;
    }
    result
}

/// The value bar (from black to the full color), under the wheel.
fn value_bar(ui: &mut egui::Ui, h: f32, s: f32, v: &mut f32) -> PickerGesture {
    let (moved, mut result) = gradient_bar(ui, *v, |f| {
        let [r, g, b] = hsv_to_rgb(h, s, f);
        Color32::from_rgb(r, g, b)
    });
    if let Some(new_v) = moved {
        *v = new_v;
        result.changed = true;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexadecimal_reads_and_writes_with_opacity() {
        assert_eq!(parse_hex("#4C78a8"), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex(" 4c78a8 "), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex("#4c78a880"), Some([0x4c, 0x78, 0xa8, 0x80]));
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#abc"), None);
        assert_eq!(parse_hex("#12345g"), None);
        assert_eq!(
            to_hex([0x4c, 0x78, 0xa8, 255]),
            "#4c78a8",
            "opaque: 6 digits"
        );
        assert_eq!(to_hex([0x4c, 0x78, 0xa8, 0x80]), "#4c78a880");
        assert_eq!(
            parse_hex(NO_COLOR).map(|c| c[3]),
            Some(0),
            "\"No color\" is transparent"
        );
    }

    #[test]
    fn hsl_of_reference_colors() {
        assert_eq!(rgb_to_hsl([255, 0, 0]), (0.0, 100.0, 50.0));
        assert_eq!(rgb_to_hsl([255, 255, 255]), (0.0, 0.0, 100.0));
        assert_eq!(hsl_to_rgb(120.0, 100.0, 50.0), [0, 255, 0]);
        assert_eq!(hsl_to_rgb(240.0, 100.0, 25.0), [0, 0, 128]);
        assert_eq!(hsv_to_rgb(0.0, 0.0, 0.0), [0, 0, 0]);
    }

    /// A round trip RGB → HSL → RGB and RGB → HSV → RGB gives back the
    /// starting color, over a sample of the whole cube.
    #[test]
    fn conversions_round_trip() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(85) {
                    let rgb = [r as u8, g as u8, b as u8];
                    let (h, s, l) = rgb_to_hsl(rgb);
                    assert_eq!(hsl_to_rgb(h, s, l), rgb, "HSL of {rgb:?}");
                    let (h, s, v) = rgb_to_hsv(rgb);
                    assert_eq!(hsv_to_rgb(h, s, v), rgb, "HSV of {rgb:?}");
                }
            }
        }
    }

    /// The wheel: red on the right, green (1/3) at the top left, zero
    /// saturation at the center and full at the edge — and the round trip holds.
    #[test]
    fn the_wheel_places_hues_as_expected() {
        assert_eq!(wheel_to_hue_saturation(1.0, 0.0), (0.0, 1.0));
        let (h, s) = wheel_to_hue_saturation(0.0, -1.0);
        assert!(
            (h - 0.25).abs() < 1e-6 && (s - 1.0).abs() < 1e-6,
            "top: 90°"
        );
        assert_eq!(wheel_to_hue_saturation(0.0, 0.0).1, 0.0);
        assert_eq!(
            wheel_to_hue_saturation(3.0, 4.0).1,
            1.0,
            "capped at the edge"
        );
        for (hue, saturation) in [(0.1, 0.5), (0.6, 0.9), (0.95, 0.2)] {
            let (x, y) = hue_saturation_to_wheel(hue, saturation);
            let (h, s) = wheel_to_hue_saturation(x, y);
            assert!((h - hue).abs() < 1e-5 && (s - saturation).abs() < 1e-5);
        }
    }

    /// Draws, without a window, an open picker — for each usage and
    /// each shape of color area — and checks that it paints its content
    /// (saved colors included) without panicking. The screenshot cannot
    /// open a popup; this test is the safety net.
    #[test]
    fn an_open_picker_draws_for_each_usage_and_each_color_area() {
        for shape in [PickerShape::Square, PickerShape::Wheel] {
            for usage in [ColorUsage::Curves, ColorUsage::Text, ColorUsage::Background] {
                let context = egui::Context::default();
                context.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(egui::Id::new("stokesforge_picker_shape"), shape)
                });
                let mut env = PickerEnv {
                    project: SavedColors::default().with_added(usage, "#123456"),
                    user: SavedColors::default().with_added(usage, "#22aa4480"),
                    requests: Vec::new(),
                };
                let mut value = "#ff000080".to_string();
                let mut shapes = 0;
                for frame in 0..3 {
                    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                        let popup_id = ui.make_persistent_id(("stokesforge_color_popup", "Color"));
                        if frame == 0 {
                            set_picker_open(ui.ctx(), popup_id, true);
                            // The color area is folded by default (#10): unfold
                            // it to exercise the whole content.
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(custom_id(popup_id), true));
                        }
                        color_button(ui, "Color", &mut value, usage, Some("Auto"), &mut env);
                    });
                    output.textures_delta.clear();
                    shapes = output.shapes.len();
                }
                assert!(shapes > 40, "{usage:?}/{shape:?}: only {shapes} shapes");
                assert_eq!(value, "#ff000080", "nothing changed without a gesture");
                assert!(env.requests.is_empty());
            }
        }
    }

    /// Position of the text `needle` among the painted shapes.
    fn find_text(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text().contains(needle) => {
                    Some(text.visual_bounding_rect().center())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, needle)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|clipped| walk(&clipped.shape, needle))
    }

    /// #170, point 7: "Square | Wheel" is a set of underlined tabs, like
    /// the rest of the application, not two selectable buttons: the accent
    /// stroke is under the active tab, no `selection.bg_fill` background
    /// covers a label, and a click on "Wheel" changes view (and keeps it
    /// in memory for the following pickers).
    #[test]
    fn square_and_wheel_are_underlined_tabs() {
        let context = egui::Context::default();
        let mut env = PickerEnv {
            project: SavedColors::default(),
            user: SavedColors::default(),
            requests: Vec::new(),
        };
        let mut value = "#ff0000".to_string();
        let selection_fill = std::cell::Cell::new(egui::Color32::TRANSPARENT);
        let mut clock = 0.0_f64;
        let mut run = |events: Vec<egui::Event>, open: bool| {
            // Time advances by a tenth of a second per frame: the popup
            // fades in, its tint must be full to compare it.
            clock += 0.1;
            let input = egui::RawInput {
                events,
                time: Some(clock),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                selection_fill.set(ui.visuals().selection.bg_fill);
                let popup_id = ui.make_persistent_id(("stokesforge_color_popup", "Color"));
                if open {
                    set_picker_open(ui.ctx(), popup_id, true);
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(custom_id(popup_id), true));
                }
                color_button(
                    ui,
                    "Color",
                    &mut value,
                    ColorUsage::Background,
                    Some("Auto"),
                    &mut env,
                );
            });
            output.textures_delta.clear();
            output
        };
        run(vec![], true);
        run(vec![], false);
        let mut output = run(vec![], false);
        fn flat_rects(output: &egui::FullOutput) -> Vec<egui::epaint::RectShape> {
            fn walk(shape: &egui::Shape, into: &mut Vec<egui::epaint::RectShape>) {
                match shape {
                    egui::Shape::Rect(rect) => into.push(rect.clone()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, into)),
                    _ => {}
                }
            }
            let mut found = Vec::new();
            for painted in &output.shapes {
                walk(&painted.shape, &mut found);
            }
            found
        }
        let marks = |output: &egui::FullOutput| -> Vec<egui::Rect> {
            flat_rects(output)
                .into_iter()
                .filter(|rect| rect.fill == crate::theme::accent())
                .map(|rect| rect.rect)
                .collect()
        };
        let under = |output: &egui::FullOutput, label: &str| -> bool {
            let at = find_text(output, label).expect("tab painted");
            marks(output).iter().any(|mark| {
                mark.height() <= 2.5 && mark.x_range().contains(at.x) && mark.top() > at.y
            })
        };
        assert!(
            under(&output, &tr!("color-shape-square")),
            "stroke under Square"
        );
        assert!(
            !under(&output, &tr!("color-shape-wheel")),
            "witness: no stroke under the inactive tab"
        );
        let filled_label = |output: &egui::FullOutput| {
            let fill = selection_fill.get();
            [tr!("color-shape-square"), tr!("color-shape-wheel")]
                .iter()
                .any(|label| {
                    let at = find_text(output, label).expect("tab painted");
                    flat_rects(output)
                        .iter()
                        .any(|rect| rect.fill == fill && rect.rect.contains(at))
                })
        };
        assert!(
            !filled_label(&output),
            "no selection background behind a label: tabs, not buttons"
        );
        let click = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let wheel = find_text(&output, &tr!("color-shape-wheel")).unwrap();
        run(vec![egui::Event::PointerMoved(wheel)], false);
        run(vec![click(wheel, true)], false);
        run(vec![click(wheel, false)], false);
        output = run(vec![], false);
        assert!(
            under(&output, &tr!("color-shape-wheel")),
            "a click on Wheel moves the stroke there"
        );
        assert!(!under(&output, &tr!("color-shape-square")));
        let kept: PickerShape = context
            .memory(|memory| {
                memory
                    .data
                    .get_temp(egui::Id::new("stokesforge_picker_shape"))
            })
            .unwrap();
        assert_eq!(kept, PickerShape::Wheel, "the choice stays in memory");
    }

    /// Bug #9: the dropdown for the destination of "Save Color" opens
    /// without closing the picker, "user" is chosen in it, and a click
    /// truly outside does close the picker.
    #[test]
    fn a_picker_dropdown_does_not_close_it() {
        let context = egui::Context::default();
        let mut env = PickerEnv {
            project: SavedColors::default(),
            user: SavedColors::default(),
            requests: Vec::new(),
        };
        let mut value = "#ff0000".to_string();
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                color_button(
                    ui,
                    "Color",
                    &mut value,
                    ColorUsage::Background,
                    Some("Auto"),
                    &mut env,
                );
            });
            output.textures_delta.clear();
            output
        };
        // Open the picker, then let the layout settle.
        let mut output = run(vec![]);
        let button = egui::pos2(14.0, 14.0);
        let click = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(vec![egui::Event::PointerMoved(button)]);
        run(vec![click(button, true)]);
        run(vec![click(button, false)]);
        for _ in 0..2 {
            output = run(vec![]);
        }
        let plus =
            find_text(&output, &tr!("color-custom")).expect("\"Custom color\" checkbox painted");
        run(vec![egui::Event::PointerMoved(plus)]);
        run(vec![click(plus, true)]);
        run(vec![click(plus, false)]);
        for _ in 0..2 {
            output = run(vec![]);
        }
        let combo = find_text(&output, &tr!("color-target-project")).expect("dropdown painted");
        run(vec![egui::Event::PointerMoved(combo)]);
        run(vec![click(combo, true)]);
        run(vec![click(combo, false)]);
        output = run(vec![]);
        let entry = find_text(&output, &tr!("color-target-user")).expect("menu list open");
        run(vec![egui::Event::PointerMoved(entry)]);
        run(vec![click(entry, true)]);
        run(vec![click(entry, false)]);
        output = run(vec![]);
        assert!(
            find_text(&output, &tr!("color-save")).is_some(),
            "the picker stayed open after the choice in the list"
        );
        let away = egui::pos2(700.0, 500.0);
        run(vec![egui::Event::PointerMoved(away)]);
        run(vec![click(away, true)]);
        run(vec![click(away, false)]);
        output = run(vec![]);
        assert!(
            find_text(&output, &tr!("color-save")).is_none(),
            "a click outside closes the picker"
        );
    }

    /// #10: the picker opens folded onto the palette (with the hexadecimal);
    /// the "Custom color" expander shows color area, opacity, RGB/HSL and
    /// saving; closed, it reopens folded.
    #[test]
    fn the_picker_is_folded_by_default_and_unfolds_on_demand() {
        let context = egui::Context::default();
        let mut env = PickerEnv {
            project: SavedColors::default(),
            user: SavedColors::default(),
            requests: Vec::new(),
        };
        let mut value = "#ff0000".to_string();
        let popup_id = std::cell::Cell::new(egui::Id::NULL);
        let mut run = |open: Option<bool>, custom: Option<bool>| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                popup_id.set(ui.make_persistent_id(("stokesforge_color_popup", "Color")));
                if let Some(open) = open {
                    set_picker_open(ui.ctx(), popup_id.get(), open);
                }
                if let Some(custom) = custom {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(custom_id(popup_id.get()), custom));
                }
                color_button(
                    ui,
                    "Color",
                    &mut value,
                    ColorUsage::Background,
                    Some("Auto"),
                    &mut env,
                );
            });
            output.textures_delta.clear();
            output
        };
        let settle = |run: &mut dyn FnMut(Option<bool>, Option<bool>) -> egui::FullOutput| {
            run(None, None);
            run(None, None)
        };
        run(Some(true), None);
        let folded = settle(&mut run);
        assert!(
            find_text(&folded, &tr!("color-hex")).is_some(),
            "hex always visible"
        );
        assert!(find_text(&folded, &tr!("color-custom")).is_some());
        let behind_the_checkbox = [
            tr!("color-opacity"),
            tr!("color-rgb"),
            tr!("color-hsl"),
            tr!("color-save"),
            tr!("color-shape-square"),
        ];
        for hidden in &behind_the_checkbox {
            assert!(find_text(&folded, hidden).is_none(), "{hidden} is folded");
        }
        run(None, Some(true));
        let unfolded = settle(&mut run);
        for shown in std::iter::once(&tr!("color-hex")).chain(&behind_the_checkbox) {
            assert!(find_text(&unfolded, shown).is_some(), "{shown} is unfolded");
        }
        // Closed then reopened: folded again.
        run(Some(false), None);
        run(Some(true), None);
        let reopened = settle(&mut run);
        assert!(find_text(&reopened, &tr!("color-opacity")).is_none());
    }

    fn luminance([r, g, b]: [u8; 3]) -> f32 {
        (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
    }

    /// The palettes are distinct per usage; the curves one follows the
    /// chosen named palette, with its shades; the backgrounds one separates
    /// light and dark.
    #[test]
    fn each_curve_palette_has_its_name_in_the_catalog() {
        // One `color-palette-*` key per palette of `stokesforge-plot`, in order:
        // a palette added to the table without its key would fall back on the
        // name from `stokesforge-plot`, which is not translated.
        assert_eq!(
            CURVE_PALETTES.len(),
            9,
            "`curve_palette_name` writes the nine palettes: adding one requires its key              `color-palette-*` and its branch"
        );
        for index in 0..CURVE_PALETTES.len() {
            assert!(!curve_palette_name(index).is_empty());
        }
    }

    #[test]
    fn each_usage_has_its_palette() {
        let curves = palette(ColorUsage::Curves, 0);
        let text = palette(ColorUsage::Text, 0);
        let backgrounds = palette(ColorUsage::Background, 0);
        assert_ne!(curves, text);
        assert_ne!(text, backgrounds);
        assert_eq!(
            curves[0].colors,
            parse_all(&stokesforge_plot::DEFAULT_PALETTE)
        );
        assert_eq!(curves.len(), 3, "hues, darker, lighter");
        let okabe = palette(ColorUsage::Curves, 2);
        assert_eq!(okabe[0].colors, parse_all(CURVE_PALETTES[2].colors));
        assert_eq!(
            palette(ColorUsage::Curves, 999)[0].title,
            curve_palette_name(CURVE_PALETTES.len() - 1),
            "index clamped into bounds"
        );
        assert_eq!(
            backgrounds[0].colors[0],
            [0xff, 0xff, 0xff, 0],
            "\"No color\" first"
        );
        assert!(
            backgrounds[0].colors[1..]
                .iter()
                .all(|&[r, g, b, _]| luminance([r, g, b]) > 0.85),
            "light backgrounds"
        );
        assert!(
            backgrounds[1]
                .colors
                .iter()
                .all(|&[r, g, b, _]| luminance([r, g, b]) < 0.2),
            "dark backgrounds"
        );
    }

    /// The saved colors published by the root into egui's memory
    /// are read back, under [`saved_colors_id`], as the detached
    /// windows read them (`(project, user)`): the project stays the
    /// project, the user the user. The witness: the two sets
    /// differ (one color each), a swap would show. A
    /// new publication replaces the old one. The user's colors
    /// go to a temporary file, never into the machine's
    /// preferences.
    #[test]
    fn published_colors_read_back_with_project_and_user_in_place() {
        use stokesforge_app_state::state::Action;
        use stokesforge_core::Project;
        use stokesforge_session::Session;

        let dir = tempfile::tempdir().expect("temporary directory");
        let session = Session::spawn(Project::new("Colors").unwrap(), None).unwrap();
        let mut state = AppState::new(session.handle(), String::new()).unwrap();
        state.use_user_colors_file(dir.path().join("colors.json"));
        for (color, target) in [
            ("#aa0000", SaveTarget::Project),
            ("#00bb00", SaveTarget::User),
        ] {
            state.dispatch(Action::SaveColor {
                usage: ColorUsage::Curves,
                color: color.to_string(),
                target,
            });
        }
        let project = state.saved_colors(SaveTarget::Project);
        let user = state.saved_colors(SaveTarget::User);
        assert_eq!(project.get(ColorUsage::Curves), ["#aa0000"]);
        assert_eq!(user.get(ColorUsage::Curves), ["#00bb00"]);

        let context = egui::Context::default();
        let read_back = |context: &egui::Context| -> Option<(SavedColors, SavedColors)> {
            context.data(|data| data.get_temp(saved_colors_id()))
        };
        assert_eq!(read_back(&context), None, "nothing before the publication");
        publish_saved_colors(&context, &state);
        assert_eq!(read_back(&context), Some((project, user.clone())));

        state.dispatch(Action::SaveColor {
            usage: ColorUsage::Text,
            color: "#0000cc".to_string(),
            target: SaveTarget::Project,
        });
        publish_saved_colors(&context, &state);
        let (project, read_user) = read_back(&context).expect("republished");
        assert_eq!(project.get(ColorUsage::Text), ["#0000cc"], "republished");
        assert_eq!(read_user, user);
    }
}
