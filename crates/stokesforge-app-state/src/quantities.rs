//! The table of quantities (#220, S-N8): for each numeric quantity of the
//! interface, a single definition of its bounds, its steps, its decimals and
//! the values offered.
//!
//! A numeric field never rewrites `0.0..=20.0` or `speed(0.1)`: it names its
//! quantity here and the building block `stokesforge_app::number_field` does the
//! rest. Changing the line width range means changing one line of this file,
//! for all the width fields at once.
//!
//! The **unit** is not in the table: it is stated in the field's label
//! ("Line width (pt)"), a catalog key (S-N10, `docs/ui-texts.md`).
//! A `▼` appears only if the quantity has [`Quantity::presets`]; an axis bound
//! or a fit parameter has none.
//!
//! The caps of an analysis setting (degree, iterations, peaks, components) are
//! not written here: they are read from `stokesforge_analysis::ops` (#260), the
//! source that `Operation::validate` also applies for `pyst` and the file.

use stokesforge_analysis::ops as limits;
use stokesforge_analysis::smooth as smoothing;

/// What a quantity does with the word `auto` and an emptied field (#221, S-C4/S-R10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoKind {
    /// No Auto: `auto` and an emptied field are rejected (S-N6).
    None,
    /// The value itself has an Auto: an emptied field or `auto` makes it
    /// automatic, and the field then shows "Auto (…)" in grey.
    Field,
    /// The Auto belongs to a linked group (axis Min/Max, "Auto range" box):
    /// `auto` enables it for the whole group, an emptied field is rejected.
    Group,
}

/// The definition of a numeric quantity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity {
    /// Smallest allowed value (`f64::NEG_INFINITY`: no bound).
    pub min: f64,
    /// Largest allowed value (`f64::INFINITY`: no bound).
    pub max: f64,
    /// One notch of arrow or wheel.
    pub step: f64,
    /// A big notch (Shift + arrow).
    pub big_step: f64,
    /// Most decimals displayed (the input may carry more).
    pub decimals: usize,
    /// Whole number: an input with decimals is rejected.
    pub integer: bool,
    /// Values offered by the `▼`, in display order.
    pub presets: &'static [f64],
    /// What the quantity does with `auto` and an emptied field.
    pub auto: AutoKind,
    /// The field is wide enough for six significant digits ("705.556",
    /// "2000"): a size shown in a unit with decimals.
    pub wide: bool,
}

impl Quantity {
    /// An unbounded real.
    pub const fn real(step: f64, big_step: f64, decimals: usize) -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step,
            big_step,
            decimals,
            integer: false,
            presets: &[],
            auto: AutoKind::None,
            wide: false,
        }
    }

    /// An unbounded integer, with step 1.
    pub const fn whole(big_step: f64) -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step: 1.0,
            big_step,
            decimals: 0,
            integer: true,
            presets: &[],
            auto: AutoKind::None,
            wide: false,
        }
    }

    /// The number is an integer, positive and fits in three digits, with no
    /// list of values: the field takes the "a few digits" width (S-L2, #237).
    pub fn is_compact(&self) -> bool {
        self.integer && self.min >= 0.0 && self.max < 1000.0 && self.presets.is_empty()
    }

    /// The number is a signed real bounded to three digits, with no list of
    /// values (rotation in degrees): "-180.5" fits in a field slightly wider
    /// than [`Self::is_compact`] (#246).
    pub fn is_compact_signed(&self) -> bool {
        !self.integer
            && self.min > -1000.0
            && self.max < 1000.0
            && self.presets.is_empty()
            && self.auto == AutoKind::None
            && !self.wide
    }

    /// The same quantity bounded to `min..=max`.
    pub const fn between(self, min: f64, max: f64) -> Self {
        Self { min, max, ..self }
    }

    /// The same quantity, with the given Auto.
    pub const fn with_auto(self, auto: AutoKind) -> Self {
        Self { auto, ..self }
    }

    /// The same quantity, with a field wide enough for six significant digits.
    pub const fn wide(self) -> Self {
        Self { wide: true, ..self }
    }

    /// The same quantity with its offered values.
    pub const fn with_presets(self, presets: &'static [f64]) -> Self {
        Self { presets, ..self }
    }
}

/// A free value: abscissa, ordinate, axis bound, fit parameter.
pub const FREE: Quantity = Quantity::real(0.1, 1.0, 8);

/// Minor ticks between two major ticks (#344): 1 to 9, or Auto.
pub const MINOR_TICK_COUNT: Quantity = Quantity::whole(4.0)
    .between(1.0, 9.0)
    .with_auto(AutoKind::Field);

/// One tick out of n is labeled (#344): 1 labels them all.
pub const LABEL_EVERY: Quantity = Quantity::whole(1.0).between(1.0, 99.0);

/// Number of bins of a histogram (#359): a whole number from 1 to
/// `MAX_HISTOGRAM_BINS`, read from the plot crate (the cap of `BinCount`).
pub const HISTOGRAM_BINS: Quantity =
    Quantity::whole(10.0).between(1.0, stokesforge_plot::MAX_HISTOGRAM_BINS as f64);

/// Width of the bars, in X units (#359): strictly positive, so the smallest
/// accepted value is 0.000001 (six decimals shown; typing 0 gives that
/// value in an orange frame, never a zero-width bar). Auto by default: the
/// width deduced from the data (`stokesforge_core::resolve::bar_width`).
pub const BAR_WIDTH: Quantity = Quantity::real(0.1, 1.0, 6)
    .between(0.000_001, 1.0e9)
    .with_auto(AutoKind::Field);

/// Length of a tick mark, in figure pixels (#344, "Advanced"): 0 to 50 (the
/// rendering cap, `MAX_TICK_LENGTH`), integer (tick marks are drawn on whole
/// pixels), Auto by default.
pub const TICK_LENGTH: Quantity = Quantity::whole(5.0)
    .between(0.0, 50.0)
    .with_auto(AutoKind::Field);

/// Thickness of tick marks, in points (#344, "Advanced"): integer (Plotters
/// draws in whole pixels), 1 to 20, Auto by default.
pub const TICK_WIDTH: Quantity = Quantity::whole(1.0)
    .between(1.0, 20.0)
    .with_auto(AutoKind::Field);

/// An axis bound: free. The word `auto` (or the "Auto" button of the From / To
/// row) gives both bounds back to the data; an emptied field is rejected (#344).
pub const AXIS_BOUND: Quantity = FREE.with_auto(AutoKind::Group);

/// Thickness of a series line, in points: 0 to 20, decimals typed.
pub const LINE_WIDTH: Quantity = Quantity::real(0.1, 1.0, 2)
    .between(0.0, 20.0)
    .with_presets(&[0.5, 1.0, 1.5, 2.0, 3.0, 4.0]);

/// Thickness of a graph shape's line, in points; values of the object format
/// bar.
pub const SHAPE_LINE_WIDTH: Quantity = Quantity::real(0.1, 1.0, 2)
    .between(0.0, 100.0)
    .with_presets(&[0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0]);

/// Size of a series symbol, in points.
pub const SYMBOL_SIZE: Quantity = Quantity::real(0.1, 1.0, 2)
    .between(0.0, 40.0)
    .with_presets(&[2.0, 4.0, 6.0, 8.0, 10.0, 12.0]);

/// Size of a graph text, in points; bounds of `AnnotationSpec::SIZE_RANGE`.
pub const TEXT_SIZE: Quantity = Quantity::real(0.5, 2.0, 1)
    .between(4.0, 200.0)
    .with_presets(&[8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 24.0, 36.0]);

/// Size offered by the text format bar: same bounds as [`TEXT_SIZE`], longer
/// list and an "Auto" entry (S-C4, S-N9).
pub const TEXT_FORMAT_SIZE: Quantity = TEXT_SIZE
    .with_presets(&[
        6.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0,
    ])
    .with_auto(AutoKind::Field);

/// Rotation of a text, in degrees.
pub const ROTATION: Quantity = Quantity::real(1.0, 15.0, 1).between(-180.0, 180.0);

/// Red, green or blue channel, from 0 to 255.
pub const COLOR_CHANNEL: Quantity = Quantity::whole(16.0).between(0.0, 255.0);

/// Opacity, in percent.
pub const OPACITY_PERCENT: Quantity = Quantity::whole(10.0).between(0.0, 100.0);

/// Hue, in degrees.
pub const HUE_DEGREES: Quantity = Quantity::whole(15.0).between(0.0, 360.0);

/// Saturation or lightness, in percent.
pub const PERCENT: Quantity = Quantity::whole(10.0).between(0.0, 100.0);

/// Maximum size of a `.stf` file on opening, in MB (A2, #316); bounds of
/// `preferences::{MIN,MAX}_MAX_FILE_MB`.
pub const MAX_FILE_MB: Quantity = Quantity::whole(64.0)
    .between(
        crate::preferences::MIN_MAX_FILE_MB as f64,
        crate::preferences::MAX_MAX_FILE_MB as f64,
    )
    .with_presets(&[64.0, 128.0, 256.0, 512.0, 1024.0, 2048.0, 4096.0]);

/// Global cell budget of a `.stf` on opening, in millions (B1, #316); bounds
/// of `preferences::{MIN,MAX}_MAX_CELLS_M`.
pub const MAX_CELLS_M: Quantity = Quantity::whole(100.0)
    .between(
        crate::preferences::MIN_MAX_CELLS_M as f64,
        crate::preferences::MAX_MAX_CELLS_M as f64,
    )
    .with_presets(&[10.0, 100.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0]);

/// Period of the autosave, in minutes (#317); bounds of
/// `preferences::{MIN,MAX}_AUTOSAVE_MINUTES`.
pub const AUTOSAVE_MINUTES: Quantity = Quantity::whole(1.0)
    .between(
        crate::preferences::MIN_AUTOSAVE_MINUTES as f64,
        crate::preferences::MAX_AUTOSAVE_MINUTES as f64,
    )
    .with_presets(&[1.0, 5.0, 10.0, 15.0, 30.0, 60.0]);

/// Degree of a polynomial.
pub const POLYNOMIAL_DEGREE: Quantity =
    Quantity::whole(1.0).between(1.0, limits::MAX_DEGREE as f64);

/// Minimum prominence of a peak, in percent of the signal.
pub const PROMINENCE_PERCENT: Quantity = Quantity::real(0.5, 5.0, 2).between(0.1, 100.0);

/// Maximum number of peaks searched.
pub const MAX_PEAKS: Quantity = Quantity::whole(5.0).between(1.0, limits::MAX_PEAKS as f64);

/// Number of components of a deconvolution.
pub const COMPONENTS: Quantity = Quantity::whole(1.0).between(1.0, limits::MAX_COMPONENTS as f64);

/// Number of iterations of a solver.
pub const ITERATIONS: Quantity = Quantity::whole(100.0).between(1.0, limits::MAX_ITERATIONS as f64);

/// Window of a smoothing, in points: odd, so the step is 2 (the setting
/// raises an even input, `SmoothSetup::set_points`).
pub const SMOOTH_WINDOW_POINTS: Quantity = Quantity {
    step: 2.0,
    ..Quantity::whole(10.0).between(smoothing::MIN_WINDOW as f64, smoothing::MAX_WINDOW as f64)
};

/// Width of a smoothing window, in X units: strictly positive.
pub const SMOOTH_WINDOW_X: Quantity =
    Quantity::real(0.1, 1.0, 6).between(f64::MIN_POSITIVE, f64::INFINITY);

/// Polynomial order of a Savitzky-Golay.
pub const SMOOTH_ORDER: Quantity =
    Quantity::whole(1.0).between(smoothing::MIN_ORDER as f64, smoothing::MAX_ORDER as f64);

/// A number of lines or characters to skip on import (0 and up).
pub const COUNT: Quantity = Quantity::whole(10.0).between(0.0, 9_007_199_254_740_991.0);

/// A line number of the import (1 and up).
pub const LINE_NUMBER: Quantity = Quantity::whole(10.0).between(1.0, 9_007_199_254_740_991.0);

/// The width (columns) or height (rows) of the frame, the plot area, of a grid
/// column or row of a graph, in `unit` (#394): bounded by the size a frame may
/// have ([`stokesforge_plot::MIN_FRAME_SIZE`],
/// [`stokesforge_plot::MAX_FRAME_SIZE`]), a notch and decimals suited to the
/// unit. The unit itself is in the label (S-N10).
pub fn frame_size(unit: crate::length_unit::LengthUnit) -> Quantity {
    Quantity::real(unit.step(), unit.step() * 10.0, unit.decimals())
        .between(
            unit.from_points(stokesforge_plot::MIN_FRAME_SIZE),
            unit.from_points(stokesforge_plot::MAX_FRAME_SIZE),
        )
        .wide()
}

/// Left or Top of the first plot area, from the edge of the image, in `unit`
/// (#405): Auto when empty, and never below `minimum_points`, what the axes and
/// the title need (a smaller value is raised to it). The unit is in the label.
pub fn figure_offset(unit: crate::length_unit::LengthUnit, minimum_points: f64) -> Quantity {
    Quantity::real(unit.step(), unit.step() * 10.0, unit.decimals())
        .between(
            unit.from_points(minimum_points.clamp(0.0, stokesforge_plot::MAX_FRAME_SIZE)),
            unit.from_points(stokesforge_plot::MAX_FRAME_SIZE),
        )
        .wide()
        .with_auto(AutoKind::Field)
}

/// Left or Top of a layer in free placement, a page coordinate in `unit`
/// (#398): signed, bounded by the position range a free frame keeps
/// (`stokesforge_plot::MAX_FIGURE_EXTENT / 2` each way). The unit is in the
/// label.
pub fn free_position(unit: crate::length_unit::LengthUnit) -> Quantity {
    let half = stokesforge_plot::MAX_FIGURE_EXTENT / 2.0;
    Quantity::real(unit.step(), unit.step() * 10.0, unit.decimals())
        .between(unit.from_points(-half), unit.from_points(half))
        .wide()
}
