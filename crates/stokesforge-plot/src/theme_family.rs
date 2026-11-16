//! Graph themes by family (#514, #538): a theme is the whole **style** of a
//! graph, and applying it to another graph copies only the checked families.
//!
//! # Why a snapshot of a [`PlotSpec`]
//!
//! A theme is not a list of settings kept next to the spec: it is a
//! [`PlotSpec`] with every data reference removed ([`GraphTheme::capture`]).
//! Reusing the spec type means a new style field of the graph is a theme field
//! for free, and the apply code reads the theme with the same accessors the
//! renderer uses. What a theme **never** holds is stripped at capture and never
//! read at apply: columns of series, series names, free texts, shapes, dragged
//! positions, the group names, the row and segment values of a colour bar, the
//! stale warning.
//!
//! # The families
//!
//! Each style field belongs to exactly one [`ThemeFamily`], or to « never »
//! (data, identity, layout identity). The table lives in the classification
//! guard at the end of this file (`every_field_has_a_family`): it destructures
//! the spec types **without `..`**, so adding a field to [`PlotSpec`],
//! [`PanelSpec`], [`AxisSpec`], [`SeriesSpec`], [`LegendSpec`] or
//! [`ColorBarSpec`] does not compile until the field is classed there, and the
//! apply code below is then completed in the same place.
//!
//! # Shape mismatch
//!
//! The target rarely has the shape of the theme. The rules are fixed, so the
//! result is predictable:
//!
//! - **layers** (panels): the target layer `i` takes the theme layer
//!   `min(i, last)`: in order, the surplus takes the last. A theme with more
//!   layers than the target styles only the existing layers here; creating the
//!   missing ones (« Add missing layers ») needs the grid helpers of
//!   `stokesforge-core` and lives in `stokesforge-app-state`;
//! - **curves**: inside a layer, the curve `j` takes the curve `j % n` of the
//!   mapped theme layer (they cycle);
//! - **group gradients**: by rank of the group in the layer, the surplus takes
//!   the last;
//! - **axes**: a side maps to the same side;
//! - **grid sizes**: the width of column `i` is the theme's width of column
//!   `min(i, last)`, likewise for the heights of the rows;
//! - **free frames**: the ordinary layers, in order, take the frames of the
//!   theme's ordinary layers by the same `min(i, last)`, but a surplus layer is
//!   **offset** (never stacked on the frame of another one): the `j`-th surplus
//!   layer takes the last frame moved right by `j * (width + SURPLUS_GAP)`.
//!
//! Size and placement: this crate cannot measure text, so it never switches a
//! target between the regular grid and free placement. When the placements
//! differ the size family leaves the sizes alone; the application switches the
//! target first (`stokesforge_app_state::theme_apply`).
//!
//! # Light and Dark
//!
//! The built-in Light and Dark themes are **colour-only** (background, text
//! colour, axes colour, as before themes had families). A theme built by
//! [`GraphTheme::from_colors`] is flagged so, and its application writes only
//! those three colours, gated by the families they belong to (Background,
//! Title fonts, Axes), whatever else its style holds.

use std::collections::BTreeMap;

use crate::{
    AxisScale, AxisSide, AxisSpec, ColorBarSpec, FreeFrame, LayerPlacement, LegendSpec, LineStyle,
    PanelSpec, PlotSpec, SeriesSpec, SymbolShape, Theme, FALLBACK_FRAME, MAX_FRAME_SIZE,
    MIN_FRAME_SIZE,
};

/// Space, in points, left between a free frame and the one offset after it
/// when a theme has fewer layers than its target: wide enough for the tick
/// labels and the title of a layer, so the offset layers do not touch.
pub const SURPLUS_GAP: f64 = 72.0;

/// One group of style fields that a theme application copies as a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeFamily {
    /// Graph and layer backgrounds.
    Background,
    /// Text content: graph and layer titles, axis titles, legend text, colour
    /// bar title.
    Texts,
    /// Font, size, colour, alignment and visibility of the titles.
    TitleFonts,
    /// Scale, ticks, grid, mirror, tick colours, decimal separator.
    Axes,
    /// Axis bounds (automatic or not, From and To, inversion).
    AxisRanges,
    /// Frame sizes, placement and page layout.
    SizeAndPage,
    /// Legend look (not its anchor, not its text).
    Legend,
    /// Line and symbol style of the curves, group gradients.
    Curves,
    /// Colour bar look (not its content, anchor or title).
    ColorBar,
}

impl ThemeFamily {
    /// Every family, in the order the interface lists them.
    pub const ALL: [ThemeFamily; 9] = [
        ThemeFamily::Background,
        ThemeFamily::Texts,
        ThemeFamily::TitleFonts,
        ThemeFamily::Axes,
        ThemeFamily::AxisRanges,
        ThemeFamily::SizeAndPage,
        ThemeFamily::Legend,
        ThemeFamily::Curves,
        ThemeFamily::ColorBar,
    ];

    /// A stable identifier (catalog keys, stored themes, tests). Never changes.
    pub const fn key(self) -> &'static str {
        match self {
            ThemeFamily::Background => "background",
            ThemeFamily::Texts => "texts",
            ThemeFamily::TitleFonts => "title-fonts",
            ThemeFamily::Axes => "axes",
            ThemeFamily::AxisRanges => "axis-ranges",
            ThemeFamily::SizeAndPage => "size-and-page",
            ThemeFamily::Legend => "legend",
            ThemeFamily::Curves => "curves",
            ThemeFamily::ColorBar => "color-bar",
        }
    }

    /// The family named by `key` ([`ThemeFamily::key`]), if any.
    pub fn from_key(key: &str) -> Option<ThemeFamily> {
        ThemeFamily::ALL
            .into_iter()
            .find(|family| family.key() == key)
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A set of [`ThemeFamily`]. The default is **all** families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThemeFamilies(u16);

impl Default for ThemeFamilies {
    fn default() -> Self {
        Self::all()
    }
}

impl ThemeFamilies {
    /// No family.
    pub const fn none() -> Self {
        Self(0)
    }

    /// Every family.
    pub const fn all() -> Self {
        Self((1 << ThemeFamily::ALL.len()) - 1)
    }

    /// Exactly one family.
    pub const fn only(family: ThemeFamily) -> Self {
        Self(family.bit())
    }

    /// True if `family` is in the set.
    pub const fn contains(self, family: ThemeFamily) -> bool {
        self.0 & family.bit() != 0
    }

    /// True if the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The set plus `family`.
    #[must_use]
    pub const fn with(self, family: ThemeFamily) -> Self {
        Self(self.0 | family.bit())
    }

    /// The set minus `family`.
    #[must_use]
    pub const fn without(self, family: ThemeFamily) -> Self {
        Self(self.0 & !family.bit())
    }

    /// The set with `family` checked or unchecked.
    #[must_use]
    pub const fn set(self, family: ThemeFamily, on: bool) -> Self {
        if on {
            self.with(family)
        } else {
            self.without(family)
        }
    }

    /// The families of the set, in [`ThemeFamily::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = ThemeFamily> {
        ThemeFamily::ALL
            .into_iter()
            .filter(move |family| self.contains(*family))
    }
}

/// The families a default theme gives to a new graph: all but the texts and
/// the axis ranges (a new graph keeps its own titles and its own ranges).
/// Scripts (`pyst.apply_theme`, #546) use the same set when none is named.
pub fn default_theme_families() -> ThemeFamilies {
    ThemeFamilies::all()
        .without(ThemeFamily::Texts)
        .without(ThemeFamily::AxisRanges)
}

/// Most themes one store (a project, the user's machine) keeps. A file that
/// carries more is cut at this count when read (invariant 10).
pub const MAX_THEMES: usize = 200;

/// Longest theme name, in characters. A longer name read from a file is cut.
pub const MAX_THEME_NAME_CHARS: usize = 80;

/// True when two theme names are the same name: spaces around them and letter
/// case are ignored. The rule of "a duplicate name inside one store is
/// refused" (#540); the same name in two stores is allowed.
pub fn same_theme_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// A list of themes as a store keeps it: nameless ones dropped, names trimmed
/// and cut at [`MAX_THEME_NAME_CHARS`], a later duplicate name dropped (case
/// ignored), the list cut at [`MAX_THEMES`] (the first kept).
pub fn bounded_themes(themes: Vec<GraphTheme>) -> Vec<GraphTheme> {
    let mut out: Vec<GraphTheme> = Vec::new();
    for mut theme in themes {
        theme.name = theme
            .name
            .trim()
            .chars()
            .take(MAX_THEME_NAME_CHARS)
            .collect();
        if theme.name.is_empty() || out.iter().any(|t| same_theme_name(&t.name, &theme.name)) {
            continue;
        }
        out.push(theme);
        if out.len() == MAX_THEMES {
            break;
        }
    }
    out
}

/// A named theme: the style of a whole graph, kept as a [`PlotSpec`] with no
/// data in it.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphTheme {
    /// Name shown in the interface.
    pub name: String,
    /// The style. Built by [`GraphTheme::capture`] (data stripped) or by
    /// [`GraphTheme::from_colors`].
    pub style: PlotSpec,
    /// True for a theme that defines three colours only (the built-in Light
    /// and Dark): its application writes background, text colour and axes
    /// colour and nothing else.
    pub colors_only: bool,
}

/// The placeholder column of a captured series: a spec cannot hold a series
/// without a Y column, and the theme must not name a real one.
const NO_COLUMN: stokesforge_id::NodeId = stokesforge_id::NodeId::from_raw(0);

impl GraphTheme {
    /// The theme of the graph `spec`: its style, with **every data reference
    /// removed**, so that two graphs differing only by data give equal themes.
    ///
    /// Removed: the columns and names of the series, their visibility and
    /// per-series settings that are not style (bar width, bins, row), the free
    /// texts and shapes, every dragged position, the legend and colour bar row
    /// columns, the colour bar segments, the group names, the stale warning.
    /// Kept: the layout of the layers (row, column, overlay) and the curves
    /// (type, group, line, symbol), which are the shape of the theme.
    pub fn capture(name: impl Into<String>, spec: &PlotSpec) -> Self {
        let mut style = spec.clone();
        style.stale_warning = false;
        style.title_anchor = None;
        for panel in style.panels.iter_mut() {
            strip_panel(panel);
        }
        Self {
            name: name.into(),
            style,
            colors_only: false,
        }
    }

    /// A colour-only theme (the built-in Light and Dark): `colors` written on
    /// an empty graph. Applying it changes only background, text colour and
    /// axes colour (see the module documentation).
    pub fn from_colors(name: impl Into<String>, colors: &Theme) -> Self {
        let mut style = PlotSpec::empty("");
        style.apply_theme(colors);
        Self {
            name: name.into(),
            style,
            colors_only: true,
        }
    }
}

fn strip_panel(panel: &mut PanelSpec) {
    panel.title_anchor = None;
    panel.annotations.clear();
    panel.shapes.clear();
    panel.group_names.clear();
    panel.legend.anchor = None;
    panel.legend.text.row_column = None;
    for side in SIDES {
        panel.axis_mut(side).title_anchor = None;
    }
    if let Some(bar) = &mut panel.color_bar {
        bar.segments.clear();
        bar.row_column = None;
        bar.anchor = None;
    }
    for series in &mut panel.series {
        let mut kept = SeriesSpec::new(NO_COLUMN);
        kept.plot_type = series.plot_type.clone();
        kept.group = series.group;
        kept.axis = series.axis;
        kept.line = series.line.clone();
        kept.symbol = series.symbol.clone();
        *series = kept;
    }
}

const SIDES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];

/// Index of the theme item that the target item `index` takes: the surplus
/// takes the last. `None` if the theme has no item.
fn mapped(index: usize, len: usize) -> Option<usize> {
    len.checked_sub(1).map(|last| index.min(last))
}

/// `values` resized to `count` entries by the shape-mismatch rule.
fn resized(values: &[f64], count: usize) -> Vec<f64> {
    (0..count)
        .filter_map(|i| mapped(i, values.len()).map(|j| values[j]))
        .collect()
}

/// The stored frame sizes `stored` of a theme resized to `count` entries by the
/// shape-mismatch rule, each brought into bounds (a hostile entry reads the
/// fallback size). Empty stays empty: the theme has no stored frame (legacy).
fn resized_sizes(stored: &[f64], count: usize, fallback: f64) -> Vec<f64> {
    let usable: Vec<f64> = stored
        .iter()
        .map(|v| {
            if v.is_finite() && *v > 0.0 {
                v.clamp(MIN_FRAME_SIZE, MAX_FRAME_SIZE)
            } else {
                fallback
            }
        })
        .collect();
    resized(&usable, count)
}

fn apply_axis(target: &mut AxisSpec, theme: &AxisSpec, families: ThemeFamilies) {
    if families.contains(ThemeFamily::Texts) {
        target.label = theme.label.clone();
    }
    if families.contains(ThemeFamily::TitleFonts) {
        target.title_font = theme.title_font.clone();
        target.title_color = theme.title_color.clone();
        target.title_visible = theme.title_visible;
    }
    if families.contains(ThemeFamily::Axes) {
        target.scale = theme.scale;
        target.visible = theme.visible;
        target.mirror = theme.mirror;
        target.tick_labels = theme.tick_labels;
        target.ticks = theme.ticks;
        target.tick_direction = theme.tick_direction;
        target.minor_ticks = theme.minor_ticks;
        target.minor_tick_count = theme.minor_tick_count;
        target.label_every = theme.label_every;
        target.tick_length = theme.tick_length;
        target.minor_tick_length = theme.minor_tick_length;
        target.tick_width = theme.tick_width;
        target.tick_color = theme.tick_color.clone();
        target.tick_mark_color = theme.tick_mark_color.clone();
        target.grid = theme.grid;
    }
    if families.contains(ThemeFamily::AxisRanges) {
        target.auto_range = theme.auto_range;
        target.range_min = theme.range_min;
        target.range_max = theme.range_max;
        target.inverted = theme.inverted;
    }
    if families.contains(ThemeFamily::Axes) || families.contains(ThemeFamily::AxisRanges) {
        automatic_if_impossible_on_log(target);
    }
}

/// A log axis cannot show a bound that is not strictly positive: a scale
/// copied onto a manual range (or a range copied onto a log axis) that makes
/// it impossible sends the axis back to **automatic**, so the application
/// freezes a drawable range on the next write instead of drawing nothing.
fn automatic_if_impossible_on_log(axis: &mut AxisSpec) {
    if axis.scale != AxisScale::Log || axis.auto_range {
        return;
    }
    let impossible = |bound: Option<f64>| bound.is_some_and(|v| v.is_nan() || v <= 0.0);
    if impossible(axis.range_min) || impossible(axis.range_max) {
        axis.auto_range = true;
        axis.range_min = None;
        axis.range_max = None;
    }
}

fn apply_gradients(target: &mut BTreeMap<u32, Vec<String>>, theme: &BTreeMap<u32, Vec<String>>) {
    let theme: Vec<&Vec<String>> = theme.values().collect();
    for (rank, gradient) in target.values_mut().enumerate() {
        if let Some(source) = mapped(rank, theme.len()) {
            gradient.clone_from(theme[source]);
        }
    }
}

fn apply_legend(to: &mut LegendSpec, from: &LegendSpec) {
    to.visible = from.visible;
    to.position = from.position;
    to.font = from.font.clone();
    to.color = from.color.clone();
    to.frame = from.frame;
    to.frame_color = from.frame_color.clone();
    to.frame_width = from.frame_width;
    to.background = from.background.clone();
    to.columns = from.columns;
    to.row_gap = from.row_gap;
    to.column_gap = from.column_gap;
    to.swatch_length = from.swatch_length;
}

fn apply_color_bar(to: &mut ColorBarSpec, from: &ColorBarSpec) {
    to.orientation = from.orientation.clone();
    to.position = from.position.clone();
    to.ticks = from.ticks.clone();
    to.scale = from.scale.clone();
    to.length = from.length;
    to.thickness = from.thickness;
    to.visible = from.visible;
}

fn apply_panel(target: &mut PanelSpec, theme: &PanelSpec, families: ThemeFamilies) {
    if families.contains(ThemeFamily::Background) {
        target.layer_background = theme.layer_background.clone();
    }
    if families.contains(ThemeFamily::Texts) {
        target.title = theme.title.clone();
        target.legend.text.from = theme.legend.text.from.clone();
        target.legend.text.template = theme.legend.text.template.clone();
        if let (Some(bar), Some(source)) = (&mut target.color_bar, &theme.color_bar) {
            bar.title = source.title.clone();
        }
    }
    if families.contains(ThemeFamily::TitleFonts) {
        target.title_font = theme.title_font.clone();
        target.title_color = theme.title_color.clone();
        target.title_visible = theme.title_visible;
    }
    for side in SIDES {
        apply_axis(target.axis_mut(side), theme.axis(side), families);
    }
    if families.contains(ThemeFamily::Legend) {
        apply_legend(&mut target.legend, &theme.legend);
    }
    if families.contains(ThemeFamily::Curves) {
        if !theme.series.is_empty() {
            for (j, series) in target.series.iter_mut().enumerate() {
                let source = &theme.series[j % theme.series.len()];
                // The plot type is never changed, so a theme that hides what
                // this type draws (points only on a line, no marker on a
                // scatter) leaves the target's own, or the curve would vanish.
                let hides_line = series.plot_type.draws_line()
                    && !series.plot_type.draws_symbols()
                    && source.line.style == LineStyle::None;
                let hides_symbol = series.plot_type.draws_symbols()
                    && !series.plot_type.draws_line()
                    && source.symbol.shape == SymbolShape::None;
                if !hides_line {
                    series.line = source.line.clone();
                }
                if !hides_symbol {
                    series.symbol = source.symbol.clone();
                }
            }
        }
        apply_gradients(&mut target.group_gradients, &theme.group_gradients);
    }
    if families.contains(ThemeFamily::ColorBar) {
        if let (Some(to), Some(from)) = (&mut target.color_bar, &theme.color_bar) {
            apply_color_bar(to, from);
        }
    }
}

impl PlotSpec {
    /// Copies the style of `theme` onto this graph, family by family; see the
    /// module documentation for what a family holds, how a different shape
    /// maps, and why the placement (grid or free) is never switched here.
    ///
    /// Never panics whatever the theme holds (invariant 10): a hostile size is
    /// brought into bounds when read, a theme without layers or curves copies
    /// nothing.
    pub fn apply_theme_families(&mut self, theme: &GraphTheme, families: ThemeFamilies) {
        if theme.colors_only {
            self.apply_theme_colors(&theme.style, families);
            return;
        }
        let style = &theme.style;
        if families.contains(ThemeFamily::Background) {
            self.background = style.background.clone();
        }
        if families.contains(ThemeFamily::Texts) {
            self.title = style.title.clone();
        }
        if families.contains(ThemeFamily::TitleFonts) {
            self.title_font = style.title_font.clone();
            self.title_color = style.title_color.clone();
            self.title_visible = style.title_visible;
            self.text_font = style.text_font.clone();
            self.text_scale = style.text_scale;
        }
        if families.contains(ThemeFamily::Axes) {
            self.decimal_comma = style.decimal_comma;
        }
        if families.contains(ThemeFamily::SizeAndPage) {
            self.apply_size_and_page(style);
        }
        let count = style.panels.len();
        for (i, panel) in self.panels.iter_mut().enumerate() {
            if let Some(source) = mapped(i, count) {
                apply_panel(panel, &style.panels[source], families);
            }
        }
    }

    /// The colour-only application of Light and Dark: the three colours of
    /// `theme` (read as the « Theme » tab reads them), each behind the family
    /// it belongs to.
    fn apply_theme_colors(&mut self, theme: &PlotSpec, families: ThemeFamilies) {
        if families.contains(ThemeFamily::Background) {
            self.background = theme.background.clone();
        }
        if families.contains(ThemeFamily::TitleFonts) {
            self.set_text_color(theme.theme_text_color());
        }
        if families.contains(ThemeFamily::Axes) {
            self.set_axes_color(theme.theme_axes_color());
        }
    }

    /// The Size and page family. Only what the **current placement** of this
    /// graph uses is copied, and only from a theme in the same placement (the
    /// application switches the target first); `fixed_figure` and the page of
    /// a Free graph follow their own placement.
    fn apply_size_and_page(&mut self, theme: &PlotSpec) {
        if theme.layer_placement != self.layer_placement {
            return;
        }
        match self.layer_placement {
            LayerPlacement::Grid => {
                self.frame_widths =
                    resized_sizes(&theme.frame_widths, self.columns(), FALLBACK_FRAME.0);
                self.frame_heights =
                    resized_sizes(&theme.frame_heights, self.rows(), FALLBACK_FRAME.1);
                self.fixed_figure = theme.fixed_figure;
                self.figure_left = theme.sanitized_figure_left();
                self.figure_top = theme.sanitized_figure_top();
                self.grid_slack = theme.sanitized_grid_slack();
            }
            LayerPlacement::Free => {
                self.free_page = theme.sanitized_free_page();
                self.apply_free_frames(theme);
            }
        }
    }

    /// Free frames: the ordinary layers, in order, take those of the theme's
    /// ordinary layers; a surplus layer is offset to the right of the last
    /// frame (module documentation), so no two layers share a frame.
    fn apply_free_frames(&mut self, theme: &PlotSpec) {
        let frames: Vec<FreeFrame> = theme
            .panels
            .iter()
            .filter(|panel| panel.overlay.is_none())
            .map(PanelSpec::sanitized_free_frame)
            .collect();
        let ordinary = self.panels.iter_mut().filter(|p| p.overlay.is_none());
        for (ordinal, panel) in ordinary.enumerate() {
            let Some(source) = mapped(ordinal, frames.len()) else {
                return;
            };
            let surplus = ordinal - source;
            let mut frame = frames[source];
            if surplus > 0 {
                frame.left += surplus as f64 * (frame.width + SURPLUS_GAP);
                frame = frame.sanitized();
            }
            panel.free_frame = Some(frame);
        }
    }
}

/// Compile-time guard of the family table (#538): every field of the spec
/// types is named here, with the family that copies it or `never`. The
/// destructurings have **no `..`**, so a field added to one of these types
/// stops this file from compiling until it is classed: put it in a family
/// (and copy it in `apply_*` above) or in « never » with the reason.
///
/// Never called: it exists to be compiled.
#[allow(dead_code, clippy::needless_pass_by_value)]
fn every_field_has_a_family(
    plot: &PlotSpec,
    panel: &PanelSpec,
    axis: &AxisSpec,
    series: &SeriesSpec,
    legend: &LegendSpec,
    bar: &ColorBarSpec,
) {
    let PlotSpec {
        title: _,           // Texts
        title_visible: _,   // TitleFonts
        panels: _,          // per layer, below
        background: _,      // Background
        title_color: _,     // TitleFonts
        title_anchor: _,    // never: dragged position
        title_font: _,      // TitleFonts
        stale_warning: _,   // never: state of the data
        decimal_comma: _,   // Axes
        frame_widths: _,    // SizeAndPage (grid)
        frame_heights: _,   // SizeAndPage (grid)
        fixed_figure: _,    // SizeAndPage (grid)
        figure_left: _,     // SizeAndPage (grid)
        figure_top: _,      // SizeAndPage (grid)
        layer_placement: _, // SizeAndPage (the application switches it)
        free_page: _,       // SizeAndPage (free)
        grid_slack: _,      // SizeAndPage (grid)
        text_font: _,       // TitleFonts (#545)
        text_scale: _,      // TitleFonts (#545)
    } = plot;
    let PanelSpec {
        title: _,         // Texts
        title_visible: _, // TitleFonts
        title_anchor: _,  // never: dragged position
        title_font: _,    // TitleFonts
        title_color: _,   // TitleFonts
        series: _,        // Curves (line, symbol); the rest is data
        bottom: _,        // the four axes, below
        left: _,
        right: _,
        top: _,
        legend: _,           // below
        overlay: _,          // never: identity of the layer
        row: _,              // never: identity of the layer
        column: _,           // never: identity of the layer
        annotations: _,      // never: free texts
        shapes: _,           // never: free shapes
        group_names: _,      // never: data names
        group_gradients: _,  // Curves
        color_bar: _,        // below
        free_frame: _,       // SizeAndPage (free)
        layer_background: _, // Background
    } = panel;
    let AxisSpec {
        label: _,             // Texts
        title_visible: _,     // TitleFonts
        unit: _,              // never: legacy, empty after a load
        scale: _,             // Axes
        auto_range: _,        // AxisRanges
        range_min: _,         // AxisRanges
        range_max: _,         // AxisRanges
        grid: _,              // Axes
        visible: _,           // Axes
        inverted: _,          // AxisRanges
        title_color: _,       // TitleFonts
        title_anchor: _,      // never: dragged position
        title_font: _,        // TitleFonts
        tick_color: _,        // Axes
        mirror: _,            // Axes
        tick_labels: _,       // Axes
        ticks: _,             // Axes
        tick_direction: _,    // Axes
        minor_ticks: _,       // Axes
        minor_tick_count: _,  // Axes
        label_every: _,       // Axes
        tick_length: _,       // Axes
        minor_tick_length: _, // Axes
        tick_width: _,        // Axes
        tick_mark_color: _,   // Axes
    } = axis;
    let SeriesSpec {
        y: _,              // never: data
        x: _,              // never: data
        x_error: _,        // never: data
        y_error: _,        // never: data
        plot_type: _,      // never: what is plotted
        name: _,           // never: data
        line: _,           // Curves
        symbol: _,         // Curves
        axis: _,           // never: which axis the data uses
        visible: _,        // never: state of the data
        show_in_legend: _, // never: state of the data
        bar_width: _,      // never: tied to the data
        histogram_bins: _, // never: tied to the data
        row: _,            // never: data
        group: _,          // never: data (gradients are Curves)
    } = series;
    let LegendSpec {
        visible: _,       // Legend
        position: _,      // Legend
        anchor: _,        // never: dragged position
        font: _,          // Legend
        color: _,         // Legend
        text: _,          // Texts (from, template); row_column is data
        frame: _,         // Legend
        frame_color: _,   // Legend
        frame_width: _,   // Legend
        background: _,    // Legend
        columns: _,       // Legend
        row_gap: _,       // Legend
        column_gap: _,    // Legend
        swatch_length: _, // Legend
    } = legend;
    let crate::LegendText {
        from: _,       // Texts
        template: _,   // Texts
        row_column: _, // never: data
    } = &legend.text;
    let ColorBarSpec {
        segments: _,    // never: data values
        chain: _,       // never: tied to the segments
        value: _,       // never: where the data is read
        row_column: _,  // never: data
        scale: _,       // ColorBar
        orientation: _, // ColorBar
        position: _,    // ColorBar
        anchor: _,      // never: dragged position
        ticks: _,       // ColorBar
        title: _,       // Texts
        length: _,      // ColorBar
        thickness: _,   // ColorBar
        visible: _,     // ColorBar
    } = bar;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TextAnchor, THEMES};
    use stokesforge_id::NodeId;

    fn series(n: u64, color: &str) -> SeriesSpec {
        let mut s = SeriesSpec::new(NodeId::from_raw(n));
        s.line.color = color.to_string();
        s.symbol.color = color.to_string();
        s
    }

    fn graph(panels: &[&[&str]]) -> PlotSpec {
        let mut spec = PlotSpec::empty("t");
        let built: Vec<PanelSpec> = panels
            .iter()
            .enumerate()
            .map(|(i, colors)| {
                let mut p = PanelSpec::at(0, i);
                p.series = colors
                    .iter()
                    .enumerate()
                    .map(|(j, c)| series(100 * i as u64 + j as u64 + 1, c))
                    .collect();
                p
            })
            .collect();
        spec.panels = built.into();
        spec
    }

    fn theme_of(spec: &PlotSpec) -> GraphTheme {
        GraphTheme::capture("theme", spec)
    }

    fn styled() -> PlotSpec {
        let mut theme = graph(&[&["#111111"]]);
        theme.background = "#222222".into();
        theme.title = "Theme title".into();
        theme.panels.get_mut(0).unwrap().bottom.label = "Theme axis".into();
        theme.panels.get_mut(0).unwrap().bottom.scale = AxisScale::Log;
        theme.panels.get_mut(0).unwrap().bottom.auto_range = false;
        theme.panels.get_mut(0).unwrap().bottom.range_min = Some(1.0);
        theme.panels.get_mut(0).unwrap().bottom.range_max = Some(9.0);
        theme
    }

    fn panel(spec: &PlotSpec, i: usize) -> &PanelSpec {
        &spec.panels.as_slice()[i]
    }

    fn line_colors(spec: &PlotSpec, i: usize) -> Vec<String> {
        panel(spec, i)
            .series
            .iter()
            .map(|s| s.line.color.clone())
            .collect()
    }

    #[test]
    fn family_keys_are_stable_and_round_trip() {
        for family in ThemeFamily::ALL {
            assert_eq!(ThemeFamily::from_key(family.key()), Some(family));
        }
        assert_eq!(ThemeFamily::SizeAndPage.key(), "size-and-page");
        assert_eq!(ThemeFamily::from_key("nope"), None);
    }

    #[test]
    fn the_family_set_adds_and_removes() {
        let set = ThemeFamilies::none().with(ThemeFamily::Axes);
        assert!(set.contains(ThemeFamily::Axes) && !set.contains(ThemeFamily::Texts));
        assert!(set.without(ThemeFamily::Axes).is_empty());
        assert_eq!(ThemeFamilies::all().iter().count(), 9);
        assert_eq!(ThemeFamilies::default(), ThemeFamilies::all());
    }

    #[test]
    fn one_family_copies_only_its_fields() {
        let theme = theme_of(&styled());
        let mut target = graph(&[&["#aaaaaa"]]);
        let before = target.clone();
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::Background));
        let mut expected = before;
        expected.background = "#222222".into();
        assert_eq!(target, expected);
    }

    #[test]
    fn the_texts_family_copies_the_titles_only() {
        let theme = theme_of(&styled());
        let mut target = graph(&[&["#aaaaaa"]]);
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::Texts));
        assert_eq!(panel(&target, 0).bottom.label, "Theme axis");
        assert_eq!(target.title, "Theme title");
        assert_eq!(target.background, PlotSpec::default().background);
        assert_eq!(panel(&target, 0).bottom.scale, AxisScale::Linear);
    }

    #[test]
    fn three_curves_onto_seven_cycle_inside_the_layer() {
        let theme = theme_of(&graph(&[&["#c1", "#c2", "#c3"]]));
        let mut target = graph(&[&["#0", "#1", "#2", "#3", "#4", "#5", "#6"]]);
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::Curves));
        assert_eq!(
            line_colors(&target, 0),
            ["#c1", "#c2", "#c3", "#c1", "#c2", "#c3", "#c1"]
        );
        // The data of the target series is untouched.
        assert_eq!(panel(&target, 0).series[6].y, NodeId::from_raw(7));
    }

    #[test]
    fn a_theme_never_hides_what_the_plot_type_draws() {
        // A points-only theme onto a line curve keeps the line; a theme
        // without markers onto a scatter keeps the markers (#542).
        let mut source = graph(&[&["#c1"]]);
        let theme_series = &mut source.panels.get_mut(0).unwrap().series[0];
        theme_series.line.style = LineStyle::None;
        theme_series.symbol.shape = SymbolShape::None;
        let theme = theme_of(&source);
        let mut target = graph(&[&["#a", "#b"]]);
        target.panels.get_mut(0).unwrap().series[0].plot_type = crate::PlotType::Line;
        target.panels.get_mut(0).unwrap().series[0].line.style = LineStyle::Dash;
        target.panels.get_mut(0).unwrap().series[1].plot_type = crate::PlotType::Scatter;
        target.panels.get_mut(0).unwrap().series[1].symbol.shape = SymbolShape::Square;
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::Curves));
        assert_eq!(target.panels[0].series[0].line.style, LineStyle::Dash);
        assert_eq!(target.panels[0].series[1].symbol.shape, SymbolShape::Square);
    }

    #[test]
    fn a_surplus_layer_takes_the_last_theme_layer() {
        let theme = theme_of(&graph(&[&["#c1", "#c2"], &["#d1"]]));
        let mut target = graph(&[&["#a", "#b", "#c"], &["#e", "#f", "#g", "#h"], &["#x"]]);
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::Curves));
        assert_eq!(line_colors(&target, 0), ["#c1", "#c2", "#c1"]);
        assert_eq!(line_colors(&target, 1), ["#d1", "#d1", "#d1", "#d1"]);
        assert_eq!(line_colors(&target, 2), ["#d1"]);
    }

    #[test]
    fn axis_sides_map_side_to_side() {
        let mut source = graph(&[&[]]);
        source.panels.get_mut(0).unwrap().right.label = "right".into();
        source.panels.get_mut(0).unwrap().top.label = "top".into();
        let mut target = graph(&[&[]]);
        target.apply_theme_families(&theme_of(&source), ThemeFamilies::only(ThemeFamily::Texts));
        assert_eq!(panel(&target, 0).right.label, "right");
        assert_eq!(panel(&target, 0).top.label, "top");
        assert_eq!(panel(&target, 0).left.label, "");
    }

    #[test]
    fn ranges_family_off_leaves_the_bounds() {
        let theme = theme_of(&styled());
        let mut target = graph(&[&[]]);
        target.apply_theme_families(
            &theme,
            ThemeFamilies::all().without(ThemeFamily::AxisRanges),
        );
        let axis = &panel(&target, 0).bottom;
        assert!(axis.auto_range && axis.range_min.is_none() && axis.range_max.is_none());
        assert_eq!(axis.scale, AxisScale::Log);
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::AxisRanges));
        assert!(!panel(&target, 0).bottom.auto_range);
        assert_eq!(panel(&target, 0).bottom.range_max, Some(9.0));
    }

    #[test]
    fn a_log_axis_with_an_impossible_range_goes_back_to_automatic() {
        // The theme is log, the target has a manual range through zero.
        let theme = theme_of(&styled());
        let mut target = graph(&[&[]]);
        let axis = &mut target.panels.get_mut(0).unwrap().bottom;
        axis.auto_range = false;
        axis.range_min = Some(-5.0);
        axis.range_max = Some(5.0);
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::Axes));
        let axis = &panel(&target, 0).bottom;
        assert_eq!(axis.scale, AxisScale::Log);
        assert!(axis.auto_range && axis.range_min.is_none() && axis.range_max.is_none());
        // The other way round: a range through zero copied onto a log axis.
        let mut source = graph(&[&[]]);
        let from = &mut source.panels.get_mut(0).unwrap().bottom;
        from.auto_range = false;
        from.range_min = Some(0.0);
        from.range_max = Some(10.0);
        let mut target = graph(&[&[]]);
        target.panels.get_mut(0).unwrap().bottom.scale = AxisScale::Log;
        target.apply_theme_families(
            &theme_of(&source),
            ThemeFamilies::only(ThemeFamily::AxisRanges),
        );
        assert!(panel(&target, 0).bottom.auto_range);
        // A possible range on a log axis is kept.
        let mut target = graph(&[&[]]);
        target.apply_theme_families(&theme, ThemeFamilies::all());
        assert!(!panel(&target, 0).bottom.auto_range);
        assert_eq!(panel(&target, 0).bottom.range_min, Some(1.0));
    }

    #[test]
    fn data_and_anchors_are_never_in_a_theme() {
        let mut source = styled();
        let tp = source.panels.get_mut(0).unwrap();
        tp.series[0].name = "theme name".into();
        tp.series[0].visible = false;
        tp.series[0].show_in_legend = false;
        tp.legend.anchor = Some(TextAnchor::new(1.0, 2.0));
        tp.row = 5;
        tp.column = 6;
        let mut target = graph(&[&["#aaaaaa"]]);
        let before = target.clone();
        target.apply_theme_families(&theme_of(&source), ThemeFamilies::all());
        let (a, b) = (panel(&target, 0), panel(&before, 0));
        assert_eq!(a.series[0].y, b.series[0].y);
        assert_eq!(a.series[0].name, b.series[0].name);
        assert!(a.series[0].visible && a.series[0].show_in_legend);
        assert_eq!(a.legend.anchor, None);
        assert_eq!((a.row, a.column), (b.row, b.column));
        assert_eq!(a.series[0].line.color, "#111111");
    }

    #[test]
    fn capture_strips_every_data_reference() {
        let mut spec = styled();
        spec.stale_warning = true;
        spec.title_anchor = Some(TextAnchor::new(1.0, 1.0));
        let p = spec.panels.get_mut(0).unwrap();
        p.title_anchor = Some(TextAnchor::new(1.0, 1.0));
        p.bottom.title_anchor = Some(TextAnchor::new(2.0, 2.0));
        p.legend.anchor = Some(TextAnchor::new(3.0, 3.0));
        p.legend.text.row_column = Some(NodeId::from_raw(9));
        p.group_names.insert(1, "group".into());
        p.annotations
            .push(crate::AnnotationSpec::new("note", 1.0, 1.0));
        p.shapes.push(crate::ShapeSpec::new(
            crate::ShapeKind::Line,
            0.0,
            0.0,
            1.0,
            1.0,
        ));
        let s = &mut p.series[0];
        s.x = Some(NodeId::from_raw(2));
        s.x_error = Some(NodeId::from_raw(3));
        s.y_error = Some(NodeId::from_raw(4));
        s.name = "name".into();
        s.row = Some(1);
        p.color_bar = Some(ColorBarSpec {
            segments: vec![crate::ColorBarSegment::new(1)],
            row_column: Some(NodeId::from_raw(5)),
            anchor: Some(TextAnchor::new(4.0, 4.0)),
            ..ColorBarSpec::default()
        });
        let theme = theme_of(&spec);
        let kept = &theme.style;
        assert!(!kept.stale_warning && kept.title_anchor.is_none());
        let p = panel(kept, 0);
        assert!(p.title_anchor.is_none() && p.bottom.title_anchor.is_none());
        assert!(p.legend.anchor.is_none() && p.legend.text.row_column.is_none());
        assert!(p.group_names.is_empty() && p.annotations.is_empty() && p.shapes.is_empty());
        let s = &p.series[0];
        assert_eq!(s.y, NO_COLUMN);
        assert!(s.x.is_none() && s.x_error.is_none() && s.y_error.is_none());
        assert!(s.name.is_empty() && s.row.is_none());
        let bar = p.color_bar.as_ref().unwrap();
        assert!(bar.segments.is_empty() && bar.row_column.is_none() && bar.anchor.is_none());
        // The style survives.
        assert_eq!(s.line.color, "#111111");
        assert_eq!(kept.title, "Theme title");
    }

    #[test]
    fn two_captures_differing_only_by_data_are_equal_and_by_style_are_not() {
        let mut a = graph(&[&["#111111", "#222222"]]);
        let mut b = graph(&[&["#111111", "#222222"]]);
        b.panels.get_mut(0).unwrap().series[0].y = NodeId::from_raw(77);
        b.panels.get_mut(0).unwrap().series[1].name = "other".into();
        b.panels
            .get_mut(0)
            .unwrap()
            .annotations
            .push(crate::AnnotationSpec::new("note", 0.0, 0.0));
        b.panels.get_mut(0).unwrap().legend.anchor = Some(TextAnchor::new(5.0, 5.0));
        b.stale_warning = true;
        assert_eq!(theme_of(&a), theme_of(&b));
        for edit in [
            (|s: &mut PlotSpec| s.background = "#123456".into()) as fn(&mut PlotSpec),
            |s| s.panels.get_mut(0).unwrap().series[1].line.color = "#abcdef".into(),
            |s| s.panels.get_mut(0).unwrap().left.grid = true,
            |s| s.panels.get_mut(0).unwrap().legend.columns = 3,
        ] {
            let mut styled = b.clone();
            edit(&mut styled);
            assert_ne!(theme_of(&a), theme_of(&styled));
        }
        a.title = "x".into();
        assert_ne!(theme_of(&a), theme_of(&b));
    }

    #[test]
    fn grid_sizes_follow_the_last_entry() {
        let mut source = graph(&[&[]]);
        source.frame_widths = vec![100.0, 200.0];
        source.frame_heights = vec![150.0];
        let mut target = graph(&[&[], &[], &[]]);
        target.apply_theme_families(
            &theme_of(&source),
            ThemeFamilies::only(ThemeFamily::SizeAndPage),
        );
        assert_eq!(target.frame_widths, [100.0, 200.0, 200.0]);
        assert_eq!(target.frame_heights, [150.0]);
    }

    fn free_graph(frames: &[(f64, f64)]) -> PlotSpec {
        let mut spec = graph(&vec![&[][..]; frames.len()]);
        spec.layer_placement = LayerPlacement::Free;
        for (panel, (left, width)) in spec.panels.iter_mut().zip(frames) {
            panel.free_frame = Some(FreeFrame {
                left: *left,
                top: 10.0,
                width: *width,
                height: 100.0,
                stack: 0,
            });
        }
        spec
    }

    fn lefts(spec: &PlotSpec) -> Vec<f64> {
        spec.panels
            .iter()
            .map(|p| p.free_frame.unwrap().left)
            .collect()
    }

    #[test]
    fn free_frames_follow_by_index_and_surplus_layers_are_offset() {
        let theme = theme_of(&free_graph(&[(0.0, 200.0), (300.0, 150.0)]));
        // All three layers of the target sit on the same frame.
        let mut target = free_graph(&[(5.0, 80.0), (5.0, 80.0), (5.0, 80.0), (5.0, 80.0)]);
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::SizeAndPage));
        let left = lefts(&target);
        assert_eq!(&left[..2], &[0.0, 300.0]);
        assert_eq!(left[2], 300.0 + (150.0 + SURPLUS_GAP));
        assert_eq!(left[3], 300.0 + 2.0 * (150.0 + SURPLUS_GAP));
        for (i, a) in left.iter().enumerate() {
            for b in &left[i + 1..] {
                assert_ne!(a, b, "two layers share a frame");
            }
        }
    }

    #[test]
    fn size_is_left_alone_when_the_placements_differ() {
        let theme = theme_of(&free_graph(&[(0.0, 200.0)]));
        let mut target = graph(&[&[]]);
        target.frame_widths = vec![111.0];
        let before = target.clone();
        target.apply_theme_families(&theme, ThemeFamilies::only(ThemeFamily::SizeAndPage));
        assert_eq!(target, before);
    }

    #[test]
    fn light_and_dark_change_only_the_three_colours() {
        let mut target = styled();
        target.panels.get_mut(0).unwrap().left.grid = true;
        target.panels.get_mut(0).unwrap().legend.columns = 2;
        target.title_color = "#ff0000".into();
        let before = target.clone();
        let dark = GraphTheme::from_colors("Dark", &THEMES[1]);
        target.apply_theme_families(&dark, ThemeFamilies::all());
        // The same result as today's whole-theme path.
        let mut expected = before.clone();
        expected.apply_theme(&THEMES[1]);
        assert_eq!(target, expected);
        // And nothing but colours moved.
        let mut restored = target.clone();
        restored.background = before.background.clone();
        restored.title_color = before.title_color.clone();
        for side in SIDES {
            let (to, from) = (
                restored.panels.get_mut(0).unwrap().axis_mut(side),
                panel(&before, 0).axis(side),
            );
            to.title_color = from.title_color.clone();
            to.tick_color = from.tick_color.clone();
        }
        assert_eq!(restored, before);
        // Unticking a family leaves that colour.
        let mut only_background = before.clone();
        only_background.apply_theme_families(&dark, ThemeFamilies::only(ThemeFamily::Background));
        assert_eq!(only_background.background, "#1e1e1e");
        assert_eq!(only_background.title_color, "#ff0000");
    }

    #[test]
    fn a_hostile_theme_never_panics() {
        let mut hostile = free_graph(&[(f64::NAN, f64::NAN), (f64::INFINITY, -1.0)]);
        hostile.frame_widths = vec![f64::NAN, f64::INFINITY, -3.0];
        hostile.frame_heights = vec![f64::NAN];
        hostile.figure_left = Some(f64::NAN);
        hostile.free_page = Some(crate::FreePage {
            left: f64::NAN,
            top: 0.0,
            width: 1.0,
            height: 1.0,
        });
        let theme = theme_of(&hostile);
        let mut free = free_graph(&[(0.0, 100.0), (0.0, 100.0), (0.0, 100.0)]);
        free.apply_theme_families(&theme, ThemeFamilies::all());
        assert!(free.panels.iter().all(|p| {
            let f = p.free_frame.unwrap();
            f.left.is_finite() && f.width.is_finite()
        }));
        let mut grid_theme = theme.clone();
        grid_theme.style.layer_placement = LayerPlacement::Grid;
        let mut grid = graph(&[&[], &[]]);
        grid.apply_theme_families(&grid_theme, ThemeFamilies::all());
        assert!(grid.frame_widths.iter().all(|w| w.is_finite()));
        // A theme with no curve and an empty layer copies nothing and stays calm.
        let empty = theme_of(&graph(&[&[]]));
        let mut target = graph(&[&["#a"], &["#b"]]);
        target.apply_theme_families(&empty, ThemeFamilies::all());
        assert_eq!(target.panels.len(), 2);
    }
}
