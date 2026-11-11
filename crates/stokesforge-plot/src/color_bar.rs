//! The colour bar of a layer (issue #499, step C): a scale that gathers the
//! series groups coloured by a **value** (the x' row, the comments row…)
//! instead of by their rank, and replaces their entries in the legend.
//!
//! The model is data: segments, a value source, a scale and a placement. The
//! colour of a curve is **computed at display time** from its value and the
//! gradient its group retains ([`value_color`]); it is never written to the
//! series style, so an edit of the value follows at once and removing the
//! bar gives the curves their stored colors back.
//!
//! Every enum stores a stable key and keeps an `Unknown(String)` fallback, as
//! [`crate::LegendTextFrom`] does: a file written by a newer version opens
//! with a sensible reading and is saved back unchanged.

use crate::{gradient_at, LegendField, TextAnchor};

/// Declares an enum stored as a key, with an `Unknown` fallback that keeps
/// the stored key.
macro_rules! keyed_enum {
    (
        $(#[$meta:meta])*
        $name:ident { default $default:ident, $( $variant:ident => $key:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                #[doc = concat!("Stored as `", $key, "`.")]
                $variant,
            )+
            /// A choice written by a newer version: read as the default,
            /// kept as is when the file is saved again. Holds its key.
            Unknown(String),
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl $name {
            /// The choices of the interface, in display order.
            pub const CHOICES: &'static [Self] = &[ $( Self::$variant ),+ ];

            /// Stable key of the stored form (`.stf`, `pyst`).
            pub fn key(&self) -> &str {
                match self {
                    $( Self::$variant => $key, )+
                    Self::Unknown(key) => key,
                }
            }

            /// Choice of a stored key; an empty key is the default, an
            /// unknown one is kept as [`Self::Unknown`].
            pub fn from_key(key: &str) -> Self {
                match key {
                    "" => Self::$default,
                    $( $key => Self::$variant, )+
                    other => Self::Unknown(other.to_string()),
                }
            }

            /// The choice to act on: an unknown one reads as the default.
            pub fn effective(&self) -> Self {
                match self {
                    Self::Unknown(_) => Self::$default,
                    other => other.clone(),
                }
            }
        }
    };
}

keyed_enum! {
    /// Which header row (or cell) gives the value of a curve.
    ColorBarValue {
        default XPrime,
        XPrime => "x_prime",
        Comments => "comments",
        LongName => "long_name",
        ShortName => "short_name",
        Units => "units",
    }
}

impl ColorBarValue {
    /// The piece of column text this source reads.
    pub fn field(&self) -> LegendField {
        match self.effective() {
            Self::Comments => LegendField::Comments,
            Self::LongName => LegendField::LongName,
            Self::ShortName => LegendField::ShortName,
            Self::Units => LegendField::Units,
            _ => LegendField::XPrime,
        }
    }
}

keyed_enum! {
    /// How the value maps onto the gradient.
    ColorBarScale {
        default Linear,
        Linear => "linear",
        Log => "log",
    }
}

keyed_enum! {
    /// Direction of the drawn bar.
    ColorBarOrientation {
        default Horizontal,
        Vertical => "vertical",
        Horizontal => "horizontal",
    }
}

keyed_enum! {
    /// Where the bar sits: a corner of the plot area, or outside the frame.
    ColorBarPosition {
        default Below,
        TopLeft => "top_left",
        TopRight => "top_right",
        BottomLeft => "bottom_left",
        BottomRight => "bottom_right",
        Below => "below",
        Right => "right",
    }
}

impl ColorBarPosition {
    /// True for the two places outside the frame.
    pub fn is_outside(&self) -> bool {
        matches!(self.effective(), Self::Below | Self::Right)
    }
}

keyed_enum! {
    /// Which ticks the bar carries.
    ColorBarTicks {
        default Breaks,
        Breaks => "breaks",
        Auto => "auto",
    }
}

/// One segment of the bar: one series group.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorBarSegment {
    /// The series group (`SeriesSpec::group`) this segment colors.
    pub group: u32,
    /// Typed start of the range; `None` = the smallest value of the members.
    pub from: Option<f64>,
    /// Typed end of the range; `None` = the largest value of the members.
    pub to: Option<f64>,
    /// Typst label of the break at the end of the segment (empty = none).
    pub label: String,
}

impl ColorBarSegment {
    /// A segment of `group` with an automatic range and no label.
    pub fn new(group: u32) -> Self {
        Self {
            group,
            from: None,
            to: None,
            label: String::new(),
        }
    }
}

/// The colour bar of a layer (`PanelSpec::color_bar`).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorBarSpec {
    /// The segments, in bar order.
    pub segments: Vec<ColorBarSegment>,
    /// A segment starts where the previous one ends (group 1 0–57, group 2
    /// 0–25 is drawn 57–82). Changes the drawn scale only, never the color.
    pub chain: bool,
    /// Where the value of a curve is read.
    pub value: ColorBarValue,
    /// For a row series (#166): the column whose cell on that row is the
    /// value. `None` = such a series has no value.
    pub row_column: Option<stokesforge_id::NodeId>,
    /// Linear or logarithmic mapping.
    pub scale: ColorBarScale,
    /// Direction of the drawn bar.
    pub orientation: ColorBarOrientation,
    /// Placement.
    pub position: ColorBarPosition,
    /// Where the bar was dragged to; wins over `position`.
    pub anchor: Option<TextAnchor>,
    /// Ticks at the ends and breaks, or automatic.
    pub ticks: ColorBarTicks,
    /// Typst title of the bar (empty = none).
    pub title: String,
    /// Length of the strip in points (a bar inside the frame); `None` =
    /// automatic (a share of the frame). Outside the frame the length keeps
    /// following the frame and this value is only remembered.
    pub length: Option<f64>,
    /// Thickness of the strip in points; `None` = automatic.
    pub thickness: Option<f64>,
    /// False hides the bar **and** gives the curves their stored colors and
    /// legend entries back.
    pub visible: bool,
}

impl Default for ColorBarSpec {
    fn default() -> Self {
        Self {
            segments: Vec::new(),
            chain: false,
            value: ColorBarValue::default(),
            row_column: None,
            scale: ColorBarScale::default(),
            orientation: ColorBarOrientation::default(),
            position: ColorBarPosition::default(),
            anchor: None,
            ticks: ColorBarTicks::default(),
            title: String::new(),
            length: None,
            thickness: None,
            visible: true,
        }
    }
}

impl ColorBarSpec {
    /// Smallest strip length, in points, a resize or a stored value can give.
    pub const MIN_LENGTH: f64 = 20.0;
    /// Smallest strip thickness, in points.
    pub const MIN_THICKNESS: f64 = 4.0;
    /// Largest value of either, in points (a stored value beyond is clamped).
    pub const MAX_SIZE: f64 = 4000.0;

    /// A requested strip size bounded to what can be drawn; a non-finite
    /// value reads as automatic.
    pub fn clamp_size(value: Option<f64>, min: f64) -> Option<f64> {
        value
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(min, Self::MAX_SIZE))
    }

    /// The segment of `group`, if the bar has one.
    pub fn segment_of(&self, group: u32) -> Option<&ColorBarSegment> {
        self.segments.iter().find(|segment| segment.group == group)
    }

    /// Drops the segment of `group` (the group was dissolved or deleted).
    /// True if there was one.
    pub fn remove_group(&mut self, group: u32) -> bool {
        let before = self.segments.len();
        self.segments.retain(|segment| segment.group != group);
        self.segments.len() != before
    }

    /// The range `(from, to)` of a segment: the typed ends, completed by the
    /// smallest and largest of `values` (the values of its members). `None`
    /// if an end is missing and no value is left.
    pub fn natural_range(segment: &ColorBarSegment, values: &[f64]) -> Option<(f64, f64)> {
        let finite = || values.iter().copied().filter(|v| v.is_finite());
        let from = segment
            .from
            .filter(|v| v.is_finite())
            .or_else(|| finite().reduce(f64::min))?;
        let to = segment
            .to
            .filter(|v| v.is_finite())
            .or_else(|| finite().reduce(f64::max))?;
        Some((from, to))
    }

    /// The ranges **as drawn**, given the natural range of each segment. Without
    /// `chain` they are the natural ranges; with it each segment keeps its
    /// width but starts where the previous one ends.
    pub fn drawn_ranges(&self, natural: &[(f64, f64)]) -> Vec<(f64, f64)> {
        if !self.chain {
            return natural.to_vec();
        }
        let mut drawn = Vec::with_capacity(natural.len());
        let mut start: Option<f64> = None;
        for &(from, to) in natural {
            let (a, b) = match start {
                None => (from, to),
                Some(start) => (start, start + (to - from)),
            };
            drawn.push((a, b));
            start = Some(b);
        }
        drawn
    }
}

/// The color of `value` in a segment of range `range` whose group retains
/// `stops`, on the scale of `bar` (#499, C1/C8/C9).
///
/// - a value outside the range takes the color of the nearer end;
/// - a range of one point (or all values equal) gives the start color;
/// - on a logarithmic scale a value `<= 0`, or a range with an end `<= 0`,
///   has no color (`None`: the curve keeps its own);
/// - a `NaN` value, or no readable stop, gives `None`.
pub fn value_color<S: AsRef<str>>(
    bar: &ColorBarSpec,
    stops: &[S],
    range: (f64, f64),
    value: f64,
) -> Option<String> {
    if !value.is_finite() || !range.0.is_finite() || !range.1.is_finite() {
        return None;
    }
    let (from, to) = range;
    let t = match bar.scale.effective() {
        ColorBarScale::Log => {
            if value <= 0.0 || from <= 0.0 || to <= 0.0 {
                return None;
            }
            let span = to.ln() - from.ln();
            if span == 0.0 {
                0.0
            } else {
                (value.ln() - from.ln()) / span
            }
        }
        _ => {
            let span = to - from;
            if span == 0.0 {
                0.0
            } else {
                (value - from) / span
            }
        }
    };
    // `gradient_at` clamps the position to the ends.
    gradient_at(stops, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK_RED: [&str; 2] = ["#000000", "#ff0000"];

    #[test]
    fn a_value_takes_its_place_on_the_gradient_and_the_ends_clamp() {
        let bar = ColorBarSpec::default();
        let at = |v| value_color(&bar, &BLACK_RED, (0.0, 57.0), v);
        assert_eq!(at(28.5).as_deref(), Some("#800000"));
        assert_eq!(at(100.0).as_deref(), Some("#ff0000"));
        assert_eq!(at(-5.0).as_deref(), Some("#000000"));
        assert_eq!(at(f64::NAN), None);
        // A gradient that does not saturate shows the clamp: past the end is
        // the end, not an extrapolation.
        let soft = ["#102030", "#506070"];
        let past = |v| value_color(&bar, &soft, (0.0, 10.0), v);
        assert_eq!(past(99.0).as_deref(), Some("#506070"));
        assert_eq!(past(-99.0).as_deref(), Some("#102030"));
    }

    #[test]
    fn equal_values_give_the_start_color() {
        let bar = ColorBarSpec::default();
        assert_eq!(
            value_color(&bar, &BLACK_RED, (3.0, 3.0), 3.0).as_deref(),
            Some("#000000")
        );
    }

    #[test]
    fn a_log_scale_puts_ten_in_the_middle_of_one_to_a_hundred() {
        let bar = ColorBarSpec {
            scale: ColorBarScale::Log,
            ..ColorBarSpec::default()
        };
        let at = |v| value_color(&bar, &BLACK_RED, (1.0, 100.0), v);
        assert_eq!(at(10.0).as_deref(), Some("#800000"));
        assert_eq!(at(0.0), None);
        assert_eq!(at(-2.0), None);
        // A range that cannot be logarithmic colors nothing.
        assert_eq!(value_color(&bar, &BLACK_RED, (0.0, 100.0), 10.0), None);
    }

    #[test]
    fn a_chain_offsets_the_drawn_ranges_but_not_the_colors() {
        let g1 = ColorBarSegment::new(1);
        let g2 = ColorBarSegment::new(2);
        let bar = ColorBarSpec {
            segments: vec![g1.clone(), g2.clone()],
            chain: true,
            ..ColorBarSpec::default()
        };
        let natural = [
            ColorBarSpec::natural_range(&g1, &[0.0, 30.0, 57.0]).unwrap(),
            ColorBarSpec::natural_range(&g2, &[0.0, 25.0]).unwrap(),
        ];
        assert_eq!(bar.drawn_ranges(&natural), [(0.0, 57.0), (57.0, 82.0)]);
        // The color of 25 in group 2 is the end color, whatever is drawn.
        assert_eq!(
            value_color(&bar, &BLACK_RED, natural[1], 25.0).as_deref(),
            Some("#ff0000")
        );
        let unchained = ColorBarSpec {
            chain: false,
            ..bar
        };
        assert_eq!(unchained.drawn_ranges(&natural), natural);
    }

    #[test]
    fn typed_ends_win_over_the_members() {
        let segment = ColorBarSegment {
            from: Some(0.0),
            ..ColorBarSegment::new(1)
        };
        assert_eq!(
            ColorBarSpec::natural_range(&segment, &[4.0, 9.0]),
            Some((0.0, 9.0))
        );
        assert_eq!(
            ColorBarSpec::natural_range(&ColorBarSegment::new(1), &[]),
            None
        );
    }

    #[test]
    fn a_stored_key_round_trips_and_an_unknown_one_is_kept() {
        for position in ColorBarPosition::CHOICES {
            assert_eq!(&ColorBarPosition::from_key(position.key()), position);
        }
        let future = ColorBarOrientation::from_key("diagonal");
        assert_eq!(future.key(), "diagonal");
        assert_eq!(future.effective(), ColorBarOrientation::Horizontal);
        assert_eq!(ColorBarScale::from_key(""), ColorBarScale::Linear);
    }

    #[test]
    fn a_new_bar_is_horizontal_below_the_frame() {
        let bar = ColorBarSpec::default();
        assert_eq!(bar.orientation, ColorBarOrientation::Horizontal);
        assert_eq!(bar.position, ColorBarPosition::Below);
        assert!(bar.position.is_outside());
    }

    #[test]
    fn removing_a_group_drops_its_segment() {
        let mut bar = ColorBarSpec {
            segments: vec![ColorBarSegment::new(1), ColorBarSegment::new(2)],
            ..ColorBarSpec::default()
        };
        assert!(bar.remove_group(1));
        assert!(!bar.remove_group(1));
        assert_eq!(bar.segments.len(), 1);
    }
}
