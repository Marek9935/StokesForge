//! StokesForge's own visual theme.
//!
//! The application does **not** follow the system light/dark theme: it
//! imposes its own palette, derived from [`egui::Visuals::light`] (never from
//! the dark one — see `docs/application/theme-i18n-system.md` § "Theme"). Two reasons for this choice:
//!
//! - a scientific application is judged on its graphs, not on its chrome —
//!   `stokesforge-render`/`stokesforge-render-egui` draw the plots on an
//!   implicit light background (axes, grids, black text); letting the user
//!   switch to dark would break this contrast without any rendering code
//!   being informed;
//! - a theme that changes with the system is a theme that cannot be
//!   guaranteed in a screenshot or in support ("on my machine it is gray, on
//!   yours it is midnight blue"). Fixing the palette once and for all makes
//!   the application identical on every machine.
//!
//! [`install`] sets this palette once, when the window is built
//! (`main.rs`, before [`crate::app::StokesForgeApp::new`]) — never from
//! [`crate::app::StokesForgeApp::ui`](eframe::App::ui), which would otherwise run on every frame
//! for nothing.
//!
//! # Scroll bars: a single standard
//!
//! All the scroll bars of the application look the same
//! (width, color, contrast), fixed by [`scroll_style`] and set once
//! and for all by [`install`] on egui's global style — hence inherited by
//! every `egui::ScrollArea`, `TableBuilder` included, in all the
//! windows (floating and detached). To change the look of all the
//! bars at once, change the `SCROLL_BAR_*` constants below,
//! and nowhere else.
//!
//! Rule for future code: **never build or modify a
//! `ScrollStyle` outside this module** (`ScrollStyle::solid()`,
//! `spacing_mut().scroll = …`). An area that needs to know the room
//! taken by a bar (reserved band, offset bar) reads it through
//! [`scroll_bar_allocated_width`]. The test
//! `no_scroll_style_outside_the_theme` locks it.

use eframe::egui::{self, Color32, Context, CornerRadius, Theme};

/// Accent tint of the StokesForge brand: a forest green rather than egui's
/// default blue, used for selection, links and active elements — dark
/// enough to stay readable as text on a light background.
const ACCENT: Color32 = Color32::from_rgb(0x1E, 0x7A, 0x3C);

/// Text of the grid gutter (column letters, "#", row numbers, header field
/// labels) — clearly darker than [`ACCENT`]: the gutter already carries a
/// tinted background ([`header_letter_row_fill`] and the like), and a text of
/// the same saturation as its own background reads poorly there.
/// Deliberately independent of `Visuals::strong_text_color()` (which equals
/// `widgets.active.fg_stroke.color`, hence literally [`ACCENT`]) rather
/// than darkening that global tint: it also serves for the "active" text
/// of elements unrelated to the gutter (buttons, tabs).
const GUTTER_TEXT: Color32 = Color32::from_rgb(0x0F, 0x4A, 0x24);

/// Gutter text color — see [`GUTTER_TEXT`].
pub fn gutter_text_color() -> Color32 {
    GUTTER_TEXT
}

/// Weight of the interface bold (`wght` axis of Noto Sans Variable, which goes
/// from 100 to 900): the 600 "semi-bold" of the tabs mockup (#169).
const BOLD_WEIGHT: f32 = 600.0;

/// **The** interface bold: the active tab, the selected row of a
/// list, a form section title. `egui::RichText::strong` is
/// not a bold — it only changes the color, here the [`ACCENT`] —
/// and is therefore never used (`no_egui_strong_outside_the_theme`). The
/// weight is a variation coordinate of the font embedded by
/// `stokesforge_render::font_definitions`: with a font without a `wght` axis
/// (a Fira or Libertinus pair, a system font) the text stays at
/// normal weight, without error.
pub fn bold(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).variation("wght", BOLD_WEIGHT)
}

/// The **two** spacings of the interface (S-L4, #227): no
/// `ui.add_space(<number>)` is written outside this module
/// (`no_literal_add_space_outside_the_theme`, `tests/guards.rs`). The small
/// gap separates two elements of the same block (a note from its field, two
/// lines of a message); the section gap opens a new block
/// ([`section_title`], tab bands).
pub const SPACE_ITEM: f32 = 4.0;

/// The section gap: see [`SPACE_ITEM`].
pub const SPACE_SECTION: f32 = 6.0;

/// Width of a field **of a few digits** (S-L2, #237): an integer whose bound
/// is known and fits in three digits (color channel, percent, degree…),
/// without a value list (~38 px).
pub const FIELD_WIDTH_DIGITS: f32 = 38.0;

/// Width of a signed real field with three digits and one decimal, without a
/// list (rotation, "-180.5", #246).
pub const FIELD_WIDTH_SIGNED_DIGITS: f32 = 54.0;

/// Width of a field that shows **six significant digits** ("705.556",
/// "16.9333"): the size of a graph in any unit (~104 px).
pub const FIELD_WIDTH_VALUE: f32 = 104.0;

/// Width of a **short** field (S-L2, #227): a number, a bound, a
/// character (~80 px). This is the width of the "number" building block.
pub const FIELD_WIDTH_SHORT: f32 = 80.0;

/// Width of a **medium** field (S-L2): a name, a short value list
/// (~160 px).
pub const FIELD_WIDTH_MEDIUM: f32 = 160.0;

/// The four named widths of a form field (S-L2); the full one takes
/// all the room left in the field column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldWidth {
    /// [`FIELD_WIDTH_DIGITS`].
    Digits,
    /// [`FIELD_WIDTH_SHORT`].
    Short,
    /// [`FIELD_WIDTH_MEDIUM`].
    Medium,
    /// All the available width.
    Full,
}

impl FieldWidth {
    /// The width in points in `ui` (the full one reads the room left).
    pub fn points(self, ui: &egui::Ui) -> f32 {
        match self {
            Self::Digits => FIELD_WIDTH_DIGITS,
            Self::Short => FIELD_WIDTH_SHORT,
            Self::Medium => FIELD_WIDTH_MEDIUM,
            Self::Full => ui.available_width(),
        }
    }
}

/// The **three text levels** of the interface (#170, point 10); no other
/// is written by hand. Level 1 is the section title ([`section_heading`]),
/// level 2 the column header of a table ([`table_header`]), level 3
/// the secondary text ([`secondary_text`]). Combining `bold(` and `.small()` outside
/// this module is refused (`no_small_bold_outside_the_theme`, `tests/guards.rs`).
///
/// Level 1: the text of a section title, in [`bold`] at body size.
pub fn section_heading(text: impl Into<String>) -> egui::RichText {
    bold(text)
}

/// Level 2: the column header of a table (import preview, results,
/// formula memo), in [`bold`] and small text: it stands out from a section
/// title without competing with it.
pub fn table_header(text: impl Into<String>) -> egui::RichText {
    bold(text).small()
}

/// Level 3: the secondary text (detail, help), muted and in small text. The
/// usage rules are those of muted text (`docs/interface-standard.md`).
pub fn secondary_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).small().weak()
}

/// **The** section title of a form (graph properties, import wizard…):
/// a space, then the label at the [`section_heading`] level. The
/// title is enough to separate the groups: never a `ui.separator()` right before
/// (`no_separator_before_a_section_title`), never a lone rule between
/// two blocks (S-L3), and never an ordinary label standing in as a title.
pub fn section_title(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add_space(SPACE_SECTION);
    ui.label(section_heading(text))
}

/// The variations written by [`bold`], to measure or recognize a bold
/// (width reserved for a tab, tests).
pub fn bold_variations() -> egui::epaint::text::VariationCoords {
    egui::epaint::text::VariationCoords::new([("wght", BOLD_WEIGHT)])
}

/// The [`ACCENT`] itself: text and stroke of the active tab, rule of the
/// selected row of a list (#169).
pub fn accent() -> Color32 {
    ACCENT
}

/// Text placed on the solid [`ACCENT`] background: that of an active outlined
/// text (`form::toggle_texts`, #270).
pub fn text_on_accent() -> Color32 {
    Color32::WHITE
}

/// Rule that separates a tab band from its content (#169).
pub fn tab_rule_color() -> Color32 {
    Color32::from_gray(0xD6)
}

/// Tint of the help icon shown on hovering a field (#197): the neutral icon,
/// faded to stay discreet next to the field it explains.
pub fn help_icon_tint() -> Color32 {
    Color32::from_white_alpha(150)
}

/// Stroke of a hovered tab, in place of that of the active tab (#169).
pub fn tab_hover_mark_color() -> Color32 {
    Color32::from_gray(0xC4)
}

/// Background of the selected row of a list (Project Manager, #169): paler
/// than `selection.bg_fill`, the [`ACCENT`] rule on the left carries the
/// selection.
pub fn list_row_selected_fill() -> Color32 {
    ACCENT.gamma_multiply(0.14)
}

/// Background of the Results pane: one step lighter than the panel background.
pub fn results_pane_fill(ui: &egui::Ui) -> Color32 {
    ui.visuals()
        .panel_fill
        .gamma_multiply(1.0)
        .lerp_to_gamma(Color32::WHITE, 0.45)
}

/// Background of the hovered row of a list (#169).
pub fn list_row_hover_fill() -> Color32 {
    Color32::from_gray(0xEC)
}

/// **Single** "drop here" stroke (#170, point 2; #386): a light blue, painted
/// on the **border of the window** that is the drop target (never inside its
/// content), for a system file dragged over the window (`app/file_drop.rs`)
/// as for columns or rows dragged to a graph (`app/graph_window.rs`). One look
/// says "the drop is possible here"; the nature of the gesture does not change
/// it. Not red: red is reserved for errors.
pub fn drop_target_stroke() -> egui::Stroke {
    egui::Stroke::new(2.5, Color32::from_rgb(70, 150, 230))
}

/// Edge bands of a graph while columns are dragged over it (#357): dropping
/// there creates a new cell beside the nearest one. The stroke outlines the
/// band; the fill marks the band under the pointer.
pub fn drop_edge_band_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, Color32::from_rgb(70, 150, 230))
}

/// Fill of the edge band under the pointer, see [`drop_edge_band_stroke`].
pub fn drop_edge_band_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(70, 150, 230, 60)
}

/// Import preview: recognized header line (name, sub-header, unit),
/// in the spirit of the color code of OriginLab's ASCII wizard.
pub fn import_header_line_color() -> Color32 {
    Color32::from_rgb(80, 140, 220)
}

/// Import preview: comment or x' sub-header.
pub fn import_comment_line_color() -> Color32 {
    Color32::from_rgb(200, 80, 200)
}

/// Tint of a computed column that is no longer up to date (header letter and
/// cells), #181.
pub fn stale_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(230, 170, 70)
    } else {
        Color32::from_rgb(170, 100, 0)
    }
}

/// Result curve of the analysis preview (fit, corrected signal,
/// sum of peaks), #181.
pub fn preview_result_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(255, 120, 90)
    } else {
        Color32::from_rgb(200, 50, 30)
    }
}

/// Helper curve of the analysis preview (baseline, background), #181.
pub fn preview_helper_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(120, 200, 255)
    } else {
        Color32::from_rgb(20, 100, 190)
    }
}

/// Marker placed on the analysis preview (anchor point, start peak), #181.
pub fn preview_marker_color() -> Color32 {
    Color32::from_rgb(230, 150, 20)
}

/// Translucent background of a zone or a range of the analysis preview, #181.
pub fn preview_zone_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(230, 150, 20, 48)
}

/// Color of peak `index` (starting at 1) in the analysis preview, #181.
pub fn preview_peak_color(index: usize) -> Color32 {
    const LIGHT: [(u8, u8, u8); 6] = [
        (31, 119, 180),
        (44, 160, 44),
        (148, 103, 189),
        (140, 86, 75),
        (227, 119, 194),
        (23, 190, 207),
    ];
    let (r, g, b) = LIGHT[index.saturating_sub(1) % LIGHT.len()];
    Color32::from_rgb(r, g, b)
}

/// Frame of a field whose input is **refused** (#219): unreadable text,
/// constraint between fields, Typst error. Red is reserved for this case, with one
/// **explicit** exception: [`r_squared_color`] (`docs/interface-standard.md`).
pub fn field_refused_color() -> Color32 {
    Color32::from_rgb(200, 30, 30)
}

/// Tint of a low R² (#288), according to its level: orange below 0.95, red
/// from 0.8 (the dialog box, below 0.6, keeps this red). The only red allowed
/// outside field refusal: a **dedicated** token, used for nothing else. Color
/// is never the only signal (bold and tooltip, `exact_result_label`).
/// Without an alert level: the ordinary text color.
pub fn r_squared_color(
    level: stokesforge_app_state::r_squared::RSquaredLevel,
    dark: bool,
) -> Color32 {
    if level.is_red() {
        if dark {
            Color32::from_rgb(255, 110, 110)
        } else {
            Color32::from_rgb(190, 20, 40)
        }
    } else if level.is_warning() {
        if dark {
            Color32::from_rgb(255, 165, 50)
        } else {
            Color32::from_rgb(195, 95, 0)
        }
    } else if dark {
        Color32::from_gray(220)
    } else {
        Color32::from_gray(30)
    }
}

/// Frame of a field whose value was **clamped** to a bound (#219): warning
/// tint, distinct from the refusal red.
pub fn field_clamped_color() -> Color32 {
    Color32::from_rgb(220, 120, 0)
}

/// Dark and discreet red of the outline of a modal dialog that has just
/// refused a click ([`crate::app`], `modal_dialog`).
pub fn blocked_flash_color() -> Color32 {
    Color32::from_rgb(150, 28, 28)
}

/// The button of a destructive gesture (Delete, Discard Changes; S-R5, #229):
/// normal style at rest, **red** on hover and keyboard focus (egui paints
/// a focused widget with the `active` style). To be placed in a `scope` that
/// contains only this button.
pub fn destructive_button_visuals(visuals: &mut egui::Visuals) {
    let red = visuals.error_fg_color;
    for state in [&mut visuals.widgets.hovered, &mut visuals.widgets.active] {
        state.weak_bg_fill = red;
        state.bg_fill = red;
        state.bg_stroke.color = red;
        state.fg_stroke.color = Color32::WHITE;
    }
}

/// Resting background of the grid gutter (column letters, "#", row
/// numbers, header field labels) — visually distinguishes it
/// from the data itself. Derived from [`ACCENT`] like the rest of the
/// selection palette, but very diluted: a discreet tint rather than a
/// real full-color cell, which would overwhelm the rest of the grid.
pub fn header_letter_row_fill() -> Color32 {
    ACCENT.gamma_multiply(0.10)
}

/// Background of the gutter when it is *concerned* by the current selection
/// without being itself the target of the click — the column or row of one or
/// several selected data cells, for example. One step stronger than
/// [`header_letter_row_fill`] (the resting one), without reaching the
/// intensity of a direct selection of the gutter itself (see
/// [`header_letter_row_selected_fill`]) — three levels, like a spreadsheet:
/// nothing, "your row/column is concerned", "*you* are
/// selected".
pub fn header_letter_row_touched_fill() -> Color32 {
    ACCENT.gamma_multiply(0.24)
}

/// Background of the gutter when it is itself directly selected
/// (click on a column letter, a row number, a header field label,
/// or on "#" for the whole table) — the strongest level
/// of the three, see [`header_letter_row_touched_fill`].
pub fn header_letter_row_selected_fill() -> Color32 {
    ACCENT.gamma_multiply(0.42)
}

/// Thickness of all the scroll bars, in points. egui's default
/// (6 pt in solid bar, 2 pt in floating bar at rest) was hard to
/// hit and hard to see.
const SCROLL_BAR_WIDTH: f32 = 10.0;

/// Space left between the content and the bar, in points.
const SCROLL_BAR_MARGIN: f32 = 2.0;

/// Opacity of the handle at rest: the handle takes the color of the widgets'
/// text (dark gray, `widgets.inactive.fg_stroke`), muted — far
/// more contrasted than the very light gray (`widgets.inactive.bg_fill`) of
/// egui's solid bars, without being as heavy as text.
const SCROLL_BAR_HANDLE_OPACITY: f32 = 0.45;

/// Opacity of the hovered or grabbed handle (it then takes the hover
/// color, then the [`ACCENT`] during the drag).
const SCROLL_BAR_HANDLE_INTERACT_OPACITY: f32 = 0.8;

/// Opacity of the track background (`extreme_bg_color`).
const SCROLL_BAR_TRACK_OPACITY: f32 = 1.0;

/// A vertical scroll area for a **page** (a settings tab, a form, a
/// list of help): same as `egui::ScrollArea::vertical()` plus an end
/// margin of [`SPACE_SECTION`] under the content, so the last control does
/// not touch the bottom edge once scrolled down (#347). Call sites keep
/// their own builders (`id_salt`, `auto_shrink`, `max_height`). Areas that
/// are not pages (trees, tables, mirror bars, canvas) stay raw and carry
/// `// RAW-SCROLL(#347): <why>` (guard in `tests/guards.rs`).
pub fn page_scroll_area() -> egui::ScrollArea {
    egui::ScrollArea::vertical().content_margin(egui::Margin {
        bottom: SPACE_SECTION as i8,
        ..egui::Margin::ZERO
    })
}

/// The style of **all** StokesForge scroll bars — see the
/// "Scroll bars" section at the top of the module.
///
/// Built in egui's "floating" mode, but frozen: constant width
/// (no widening on hover), reserved room equal to the bar plus its
/// margin (so it never covers the content), and non-zero opacities
/// at rest (it stays visible without hovering). This is the only egui mode that
/// lets one set the handle opacity; in "solid" mode, it is
/// always opaque and of the buttons' background color, which cannot be
/// darkened without darkening all the buttons too.
pub fn scroll_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle {
        floating: true,
        bar_width: SCROLL_BAR_WIDTH,
        floating_width: SCROLL_BAR_WIDTH,
        floating_allocated_width: SCROLL_BAR_WIDTH + SCROLL_BAR_MARGIN,
        bar_inner_margin: SCROLL_BAR_MARGIN,
        bar_outer_margin: 0.0,
        handle_min_length: 16.0,
        foreground_color: true,
        dormant_background_opacity: SCROLL_BAR_TRACK_OPACITY,
        active_background_opacity: SCROLL_BAR_TRACK_OPACITY,
        interact_background_opacity: SCROLL_BAR_TRACK_OPACITY,
        dormant_handle_opacity: SCROLL_BAR_HANDLE_OPACITY,
        active_handle_opacity: SCROLL_BAR_HANDLE_OPACITY,
        interact_handle_opacity: SCROLL_BAR_HANDLE_INTERACT_OPACITY,
        ..egui::style::ScrollStyle::floating()
    }
}

/// Room taken by a scroll bar of the [`scroll_style`] standard,
/// perpendicular to its direction (width of a vertical bar, height
/// of a horizontal bar), margin included.
pub fn scroll_bar_allocated_width() -> f32 {
    scroll_style().allocated_width()
}

/// Sets the StokesForge theme on `ctx`: locks the preference on
/// [`Theme::Light`] (prevents egui from following the system again at the
/// slightest `winit` notification), applies the house palette on top of
/// [`egui::Visuals::light`], then the scroll bar standard
/// ([`scroll_style`]).
pub fn install(ctx: &Context) {
    ctx.set_theme(Theme::Light);
    ctx.set_visuals(visuals());
    ctx.all_styles_mut(|style| style.spacing.scroll = scroll_style());
    // Image loaders (SVG icons, `icons.rs`).
    egui_extras::install_image_loaders(ctx);
}

/// Builds the StokesForge palette: a standard `Visuals::light()`, with just
/// the tints that carry the identity of the application (selection, links, outline
/// of active widgets) redefined — the rest (background grays, text
/// contrasts, sizes) comes as is from egui, to avoid reinventing what the
/// library has already calibrated for legibility.
fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();

    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke.color = ACCENT;

    // Outline of active buttons/tabs (including the focused floating
    // window): takes the accent rather than the default blue, so that
    // the layout resembles the rest of the application.
    visuals.widgets.active.bg_stroke.color = ACCENT;
    visuals.widgets.active.fg_stroke.color = ACCENT;
    visuals.widgets.hovered.bg_stroke.color = ACCENT;

    // Corners slightly less rounded than egui's default (2px): closer
    // to the dense and tabular look of a data analysis tool
    // than to a mobile app.
    let corner_radius = CornerRadius::same(3);
    visuals.window_corner_radius = corner_radius;
    visuals.menu_corner_radius = corner_radius;
    visuals.widgets.noninteractive.corner_radius = corner_radius;
    visuals.widgets.inactive.corner_radius = corner_radius;
    visuals.widgets.hovered.corner_radius = corner_radius;
    visuals.widgets.active.corner_radius = corner_radius;
    visuals.widgets.open.corner_radius = corner_radius;

    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard: if someone one day derives [`visuals`] from
    /// `Visuals::dark()` by mistake (copy-paste, refactor...), this test
    /// fails rather than letting the theme go dark silently.
    #[test]
    fn base_theme_is_light() {
        assert!(
            !visuals().dark_mode,
            "the StokesForge theme must stay light"
        );
    }

    /// #347: the content of a page scroll area ends `SPACE_SECTION` below
    /// its last widget, and the width is untouched.
    #[test]
    fn a_page_scroll_area_adds_an_end_margin() {
        let ctx = egui::Context::default();
        let mut content = egui::Vec2::ZERO;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let out = page_scroll_area().show(ui, |ui| {
                ui.allocate_space(egui::vec2(50.0, 40.0));
            });
            content = out.content_size;
        });
        output.textures_delta.clear();
        assert_eq!(content, egui::vec2(50.0, 40.0 + SPACE_SECTION));
    }

    /// The drop stroke (#386) is a light blue: blue dominant, neither the
    /// accent green nor one of the error reds.
    #[test]
    fn the_drop_target_stroke_is_a_light_blue() {
        let stroke = drop_target_stroke();
        let color = stroke.color;
        assert!(
            color.b() > color.r() && color.b() > color.g(),
            "blue dominant"
        );
        assert_ne!(color, ACCENT);
        assert_ne!(color, field_refused_color());
        assert_ne!(
            color,
            r_squared_color(stokesforge_app_state::r_squared::RSquaredLevel::Red, false)
        );
    }

    /// The R² red (#288) is a token apart: distinct from that of field
    /// refusal, and each level has its tint, in light as in dark.
    #[test]
    fn the_r_squared_tint_has_its_own_token_per_level() {
        use stokesforge_app_state::r_squared::RSquaredLevel::{Dialog, Good, Orange, Red};
        for dark in [false, true] {
            let orange = r_squared_color(Orange, dark);
            let red = r_squared_color(Red, dark);
            assert_ne!(orange, red);
            assert_eq!(
                red,
                r_squared_color(Dialog, dark),
                "the dialog keeps the red"
            );
            assert_ne!(red, field_refused_color());
            assert_ne!(orange, r_squared_color(Good, dark));
        }
        assert_ne!(r_squared_color(Red, true), r_squared_color(Red, false));
    }

    /// All the levels, all the tints: `Good` keeps the ordinary text color
    /// (neither orange nor red), and no alert tint equals the field refusal
    /// red.
    #[test]
    fn the_r_squared_tint_distinguishes_all_levels() {
        use stokesforge_app_state::r_squared::RSquaredLevel::{Dialog, Good, Orange, Red};
        for dark in [false, true] {
            let tints = [
                r_squared_color(Good, dark),
                r_squared_color(Orange, dark),
                r_squared_color(Red, dark),
            ];
            for (i, a) in tints.iter().enumerate() {
                for b in &tints[i + 1..] {
                    assert_ne!(a, b, "dark = {dark}");
                }
                assert_ne!(*a, field_refused_color(), "dark = {dark}");
                assert_ne!(*a, field_clamped_color(), "dark = {dark}");
            }
            assert_eq!(r_squared_color(Dialog, dark), tints[2]);
        }
        assert_ne!(
            r_squared_color(Orange, true),
            r_squared_color(Orange, false)
        );
        assert_ne!(r_squared_color(Good, true), r_squared_color(Good, false));
    }

    /// Minimal WCAG contrast 3:1 (bold text) of the alert tints on the light
    /// (white) and dark (gray 27, egui's dark background) backgrounds.
    #[test]
    fn the_r_squared_tint_contrasts_on_light_and_dark_backgrounds() {
        use stokesforge_app_state::r_squared::RSquaredLevel::{Good, Orange, Red};
        use stokesforge_render::color::contrast_ratio;
        let rgb = |c: Color32| (c.r(), c.g(), c.b());
        for (dark, background) in [(false, (255, 255, 255)), (true, (27, 27, 27))] {
            for level in [Good, Orange, Red] {
                let ratio = contrast_ratio(rgb(r_squared_color(level, dark)), background);
                assert!(
                    ratio >= 3.0,
                    "{level:?} dark = {dark}: contrast {ratio:.2} < 3"
                );
            }
        }
    }

    #[test]
    fn install_sets_the_scrollbar_standard() {
        let ctx = Context::default();
        install(&ctx);
        assert_eq!(ctx.style_of(Theme::Light).spacing.scroll, scroll_style());
        assert_eq!(
            scroll_bar_allocated_width(),
            SCROLL_BAR_WIDTH + SCROLL_BAR_MARGIN
        );
    }

    /// Guard of the standard: no file of `stokesforge-app` outside this
    /// module builds or modifies a `ScrollStyle`. A bar with a local style
    /// would escape the single setting of the `SCROLL_BAR_*` constants.
    #[test]
    fn no_scroll_style_outside_the_theme() {
        let offenders = scroll_styles_outside_the_theme(&crate::sources::sources());
        assert!(
            offenders.is_empty(),
            "scroll bar style built outside theme.rs — use \
             theme::scroll_style / theme::scroll_bar_allocated_width: {offenders:?}"
        );
    }

    /// The lines that touch a `ScrollStyle` outside `theme.rs`,
    /// `src/path:line` (comments excluded, tests included). All the
    /// sources of `src/`, subdirectories included (#116).
    fn scroll_styles_outside_the_theme(sources: &[crate::sources::SourceFile]) -> Vec<String> {
        let mut offenders = Vec::new();
        for file in sources {
            if file.path == "theme.rs" {
                continue;
            }
            for (index, line) in file.code().lines().enumerate() {
                if line.contains("ScrollStyle") || line.contains(".scroll =") {
                    offenders.push(format!("src/{}:{}", file.path, index + 1));
                }
            }
        }
        offenders
    }

    /// Guard of the bold: egui's `RichText::strong` / `Ui::strong` embolden
    /// nothing (another color only); every bold goes through
    /// [`bold`]. All the sources of `src/`, comments excluded.
    #[test]
    fn no_egui_strong_outside_the_theme() {
        let offenders = egui_strong(&crate::sources::sources());
        assert!(
            offenders.is_empty(),
            "egui's `strong()` does not embolden — use theme::bold: {offenders:?}"
        );
    }

    /// The lines that call `.strong()` outside `theme.rs`.
    fn egui_strong(sources: &[crate::sources::SourceFile]) -> Vec<String> {
        let mut offenders = Vec::new();
        for file in sources {
            if file.path == "theme.rs" {
                continue;
            }
            for (index, line) in file.code().lines().enumerate() {
                if line.contains(".strong()") || line.contains("ui.strong(") {
                    offenders.push(format!("src/{}:{}", file.path, index + 1));
                }
            }
        }
        offenders
    }

    /// The witness: a `.strong()` outside `theme.rs` is seen, [`bold`] is not.
    #[test]
    fn the_bold_guard_sees_a_strong() {
        let file = |code: &str| crate::sources::SourceFile {
            path: "app/x.rs".to_string(),
            text: code.to_string(),
            is_test: false,
        };
        assert_eq!(
            egui_strong(&[file("let t = RichText::new(a).strong();")]),
            ["src/app/x.rs:1"]
        );
        assert_eq!(egui_strong(&[file("ui.strong(a);")]), ["src/app/x.rs:1"]);
        assert!(egui_strong(&[file("ui.label(theme::bold(a));")]).is_empty());
    }

    /// [`bold`] sets the weight on the text, and nothing else does.
    #[test]
    fn bold_sets_the_weight() {
        let job = egui::WidgetText::from(bold("x")).into_layout_job(
            &egui::Style::default(),
            egui::FontSelection::Default,
            egui::Align::LEFT,
        );
        assert_eq!(job.sections[0].format.coords, bold_variations());
        let job = egui::WidgetText::from(egui::RichText::new("x")).into_layout_job(
            &egui::Style::default(),
            egui::FontSelection::Default,
            egui::Align::LEFT,
        );
        assert_ne!(job.sections[0].format.coords, bold_variations());
    }

    /// Each text level sets its weight and size: the section title
    /// and the table header are bold, the secondary is not; only the header
    /// and the secondary are in small text, and the secondary is muted.
    #[test]
    fn each_text_level_sets_weight_and_size() {
        let job = |text: egui::RichText| {
            egui::WidgetText::from(text).into_layout_job(
                &egui::Style::default(),
                egui::FontSelection::Default,
                egui::Align::LEFT,
            )
        };
        let body = job(egui::RichText::new("x")).sections[0]
            .format
            .font_id
            .size;
        let section = job(section_heading("x"));
        let header = job(table_header("x"));
        let secondary = job(secondary_text("x"));
        assert_eq!(section.sections[0].format.coords, bold_variations());
        assert_eq!(header.sections[0].format.coords, bold_variations());
        assert_ne!(secondary.sections[0].format.coords, bold_variations());
        assert_eq!(section.sections[0].format.font_id.size, body);
        assert!(header.sections[0].format.font_id.size < body);
        assert!(secondary.sections[0].format.font_id.size < body);
        // Only the secondary is muted: the other two keep the fallback
        // color (`PLACEHOLDER`), resolved at paint time.
        assert_eq!(section.sections[0].format.color, Color32::PLACEHOLDER);
        assert_eq!(header.sections[0].format.color, Color32::PLACEHOLDER);
        assert_ne!(secondary.sections[0].format.color, Color32::PLACEHOLDER);
    }

    /// `section_title` relies on level 1: same weight, same size.
    #[test]
    fn section_title_paints_the_section_heading_level() {
        let ctx = Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            section_title(ui, "Title");
        });
        out.textures_delta.clear();
        let text = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.clone()),
                _ => None,
            })
            .expect("a painted text");
        let coords: Vec<_> = text
            .galley
            .job
            .sections
            .iter()
            .map(|s| s.format.coords.clone())
            .collect();
        assert_eq!(coords, [bold_variations()]);
    }

    /// With the embedded fonts, [`bold`] really emboldens the glyphs
    /// (text wider than at normal weight) — not just a
    /// coordinate set in a void: this is what proves that the project's font
    /// has a `wght` axis that epaint can follow.
    #[test]
    fn bold_really_widens_text_with_the_embedded_fonts() {
        let ctx = Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        install(&ctx);
        let mut widths = (0.0, 0.0);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            widths = (
                ui.label(egui::RichText::new("Plot area")).rect.width(),
                ui.label(bold("Plot area")).rect.width(),
            );
        });
        out.textures_delta.clear();
        assert!(
            widths.1 > widths.0 + 1.0,
            "bold {} versus normal {}",
            widths.1,
            widths.0
        );
    }

    /// The witness: a scroll style placed in a module of `src/app/` is
    /// seen; a comment and `theme.rs` itself pass.
    #[test]
    fn the_scroll_guard_sees_a_style_outside_app_rs() {
        let file = |path: &str, text: &str| crate::sources::SourceFile {
            path: path.to_string(),
            text: text.to_string(),
            is_test: false,
        };
        let sources = [
            file(
                "app/grid.rs",
                "// ScrollStyle\nfn f(s: &mut Style) {\n    s.spacing.scroll = ScrollStyle::thin();\n}",
            ),
            file("app/notes.rs", "fn f(s: &mut Spacing) { s.scroll = x; }"),
            file("theme.rs", "fn scroll_style() -> ScrollStyle { todo!() }"),
        ];
        assert_eq!(
            scroll_styles_outside_the_theme(&sources),
            ["src/app/grid.rs:3", "src/app/notes.rs:1"]
        );
    }
}
