//! Task R4.1 — what locks the single source of measurement.
//!
//! Three properties, and only one of them was obvious in advance:
//!
//! 1. two distinct [`TextMeasurer`]s measure **identically**, because they
//!    start from the same `FontDefinitions`. This is precision 1 of the R0 0.1
//!    measurement, put back on file: the danger is not "two instances", it is
//!    "two font sets";
//! 2. measurement does not depend on `pixels_per_point`. If it did, the
//!    exported figure (at 1.0) and the figure drawn on screen (at 1.5 or 2.0
//!    depending on the user's display) would not have the same margins — and
//!    nothing would say so;
//! 3. measurement is **different** from Plotters' naive heuristic. Without
//!    this witness, the first two tests would still pass if both paths fell
//!    back on the trait's default, which is exactly the bug the phase must
//!    prevent.
//!
//! The set of strings is the one from the R0 spike: real ticks, units, and
//! non-ASCII where the byte-based heuristic is the most wrong.

use plotters_backend::{text_anchor::Pos, BackendColor, BackendTextStyle, FontFamily, FontStyle};
use stokesforge_render::text::{estimate, TextMeasurer};

/// A minimal text style, enough for `estimate_text_size`.
///
/// `BackendTextStyle` is implemented by `plotters::style::TextStyle`, but
/// building one would make this test depend on how the pipeline chooses its
/// fonts. We want to test the measurement, not the pipeline.
struct Style {
    family: &'static str,
    size: f64,
}

#[derive(Debug)]
struct NeverAnError;

impl std::fmt::Display for NeverAnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "never")
    }
}

impl std::error::Error for NeverAnError {}

impl BackendTextStyle for Style {
    type FontError = NeverAnError;

    fn color(&self) -> BackendColor {
        BackendColor {
            alpha: 1.0,
            rgb: (0, 0, 0),
        }
    }

    fn size(&self) -> f64 {
        self.size
    }

    fn style(&self) -> FontStyle {
        FontStyle::Normal
    }

    fn anchor(&self) -> Pos {
        Pos::default()
    }

    fn family(&self) -> FontFamily<'_> {
        FontFamily::from(self.family)
    }

    fn layout_box(&self, _text: &str) -> Result<((i32, i32), (i32, i32)), Self::FontError> {
        Err(NeverAnError)
    }

    fn draw<E, F: FnMut(i32, i32, BackendColor) -> Result<(), E>>(
        &self,
        _text: &str,
        _pos: (i32, i32),
        _draw: F,
    ) -> Result<Result<(), E>, Self::FontError> {
        Err(NeverAnError)
    }
}

/// The strings of the R0 spike: ticks, units, scientific notations.
const STRINGS: &[&str] = &[
    "0",
    "0,5",
    "111",
    "WWW",
    "-1,25e-9",
    "λ_max",
    "µm",
    "cm⁻¹",
    "Temps (s)",
    "Intensité (u.a.)",
    "1 000 000",
];

const SIZES: &[f64] = &[12.0, 14.0, 20.0, 30.0];
const FAMILIES: &[&str] = &["sans-serif", "serif", "monospace"];

fn all_measurements(measurer: &TextMeasurer) -> Vec<(u32, u32)> {
    let mut measurements = Vec::new();
    for family in FAMILIES {
        for size in SIZES {
            for text in STRINGS {
                measurements.push(estimate(
                    measurer,
                    text,
                    &Style {
                        family,
                        size: *size,
                    },
                ));
            }
        }
    }
    measurements
}

#[test]
fn two_font_instances_measure_identically() {
    let first = TextMeasurer::new(1.0);
    let second = TextMeasurer::new(1.0);
    assert_eq!(
        all_measurements(&first),
        all_measurements(&second),
        "two `Fonts` built on the same `FontDefinitions` must measure \
         identically — otherwise sharing the definition is not enough \
         and we must go back to a single `Mutex<Fonts>` (architecture §4.3)"
    );
}

#[test]
fn measurement_does_not_depend_on_pixels_per_point() {
    let reference = all_measurements(&TextMeasurer::new(1.0));
    for ppp in [1.25_f32, 1.5, 2.0, 3.0] {
        assert_eq!(
            all_measurements(&TextMeasurer::new(ppp)),
            reference,
            "measurement moved at pixels_per_point = {ppp}: the export (1.0) \
             and the screen (often 1.5 or 2.0) would no longer reserve the \
             same margins"
        );
    }
}

#[test]
fn measurement_is_not_plotters_naive_heuristic() {
    // The witness. Reproduces `plotters/src/style/font/naive.rs`:
    // `em = size / 1.24 / 1.24`, width ≈ `em * 0.7 * number of bytes`.
    fn naive(text: &str, size: f64) -> u32 {
        let em = size / 1.24 / 1.24;
        (em * 0.7 * text.len() as f64).round() as u32
    }

    let measurer = TextMeasurer::new(1.0);
    let measure = |text: &str| {
        estimate(
            &measurer,
            text,
            &Style {
                family: "sans-serif",
                size: 20.0,
            },
        )
        .0
    };

    // "111" and "WWW" have the same number of bytes: the naive heuristic
    // gives them the same width, a real measurement does not.
    assert_eq!(naive("111", 20.0), naive("WWW", 20.0));
    assert_ne!(
        measure("111"),
        measure("WWW"),
        "measurement treats \"111\" and \"WWW\" as monospace: the \
         estimate_text_size override is not in effect"
    );

    // "λ" is two bytes and a single glyph: the heuristic counts it double.
    assert!(
        measure("λ") < naive("λ", 20.0),
        "\"λ\" measured {} px, naive heuristic {} px — the measurement should be \
         clearly narrower",
        measure("λ"),
        naive("λ", 20.0)
    );
}

#[test]
fn height_does_not_depend_on_content_for_the_same_size() {
    // Property used by the pipeline: `DrawingArea::titled` lowers the
    // drawing area by the title height. If this height varied with the text,
    // two graphs with different titles would not have the same area — which
    // is *correct*, but you need to know it before writing an agreement test
    // that compares two figures with different titles. It does not vary:
    // epaint returns a line height.
    let measurer = TextMeasurer::new(1.0);
    let height = |text: &str| {
        estimate(
            &measurer,
            text,
            &Style {
                family: "sans-serif",
                size: 24.0,
            },
        )
        .1
    };
    let reference = height("Title");
    for text in STRINGS {
        assert_eq!(height(text), reference, "variable height on \"{text}\"");
    }
}
