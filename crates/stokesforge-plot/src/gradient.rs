//! Color gradients (#178): gradients spread over the curves
//! of a group (#177).
//!
//! A color gradient is a list of `#RRGGBB` **stops** (at least one); the
//! colors of the `n` curves of a group are taken at regular intervals
//! between the first and the last stop, whatever `n` is, by linear
//! interpolation of the RGB components. The provided color gradients are data
//! (`GRADIENT_PRESETS`); their displayed name is a key of the application
//! catalog, never a text from here.

use crate::style::parse_hex;

/// A provided color gradient: a stable identifier (label key of the
/// application) and its stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradientPreset {
    /// Stable identifier, in lowercase: `blue-red`.
    pub id: &'static str,
    /// `#RRGGBB` stops, from first to last.
    pub stops: &'static [&'static str],
}

/// The provided color gradients.
pub const GRADIENT_PRESETS: [GradientPreset; 8] = [
    GradientPreset {
        id: "blue-red",
        stops: &["#0000ff", "#ff0000"],
    },
    GradientPreset {
        id: "black-red",
        stops: &["#000000", "#ff0000"],
    },
    GradientPreset {
        id: "black-blue",
        stops: &["#000000", "#0000ff"],
    },
    GradientPreset {
        id: "blue-yellow",
        stops: &["#0000ff", "#ffd700"],
    },
    GradientPreset {
        id: "green-red",
        stops: &["#008000", "#ff0000"],
    },
    GradientPreset {
        id: "blue-white-red",
        stops: &["#0000ff", "#ffffff", "#ff0000"],
    },
    GradientPreset {
        id: "viridis",
        stops: &["#440154", "#3e4989", "#26828e", "#35b779", "#fde725"],
    },
    GradientPreset {
        id: "plasma",
        stops: &["#0d0887", "#9c179e", "#ed7953", "#f0f921"],
    },
];

/// The `count` `#RRGGBB` colors of a color gradient with stops `stops`, evenly
/// spaced between the first and the last stop.
///
/// - `count == 0`: empty list; `count == 1`: the first stop;
/// - an unreadable stop is ignored; with no readable stop: empty list
///   (the caller then changes no color);
/// - the opacity of a `#RRGGBBAA` stop is ignored: a color gradient colors, it does not
///   make transparent.
pub fn gradient_colors<S: AsRef<str>>(stops: &[S], count: usize) -> Vec<String> {
    let stops = readable_stops(stops);
    if stops.is_empty() {
        return Vec::new();
    }
    (0..count)
        .map(|index| {
            if count == 1 {
                hex(stops[0])
            } else {
                hex(rgb_at(&stops, index as f64 / (count - 1) as f64))
            }
        })
        .collect()
}

/// The color of a color gradient at the position `t` (0 = first stop, 1 =
/// last stop), by linear interpolation of the RGB components (#499). `t` is
/// clamped to `[0, 1]`; a `NaN` reads as 0. `None` if no stop is readable.
///
/// [`gradient_colors`] is this function at `i / (count - 1)`: the colors
/// painted by position and by value are the same.
pub fn gradient_at<S: AsRef<str>>(stops: &[S], t: f64) -> Option<String> {
    let stops = readable_stops(stops);
    if stops.is_empty() {
        return None;
    }
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    Some(hex(rgb_at(&stops, t)))
}

fn readable_stops<S: AsRef<str>>(stops: &[S]) -> Vec<[u8; 3]> {
    stops
        .iter()
        .filter_map(|stop| parse_hex(stop.as_ref()))
        .map(|[r, g, b, _]| [r, g, b])
        .collect()
}

/// The color at `t` in `[0, 1]` of a non-empty list of stops.
fn rgb_at(stops: &[[u8; 3]], t: f64) -> [u8; 3] {
    if stops.len() == 1 {
        return stops[0];
    }
    let position = t * (stops.len() - 1) as f64;
    let lower = (position.floor() as usize).min(stops.len() - 2);
    let fraction = position - lower as f64;
    let mix = |channel: usize| {
        let (a, b) = (
            f64::from(stops[lower][channel]),
            f64::from(stops[lower + 1][channel]),
        );
        (a + (b - a) * fraction).round().clamp(0.0, 255.0) as u8
    };
    [mix(0), mix(1), mix(2)]
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ends_are_the_first_and_last_stops() {
        let colors = gradient_colors(&["#0000ff", "#ff0000"], 5);
        assert_eq!(colors.len(), 5);
        assert_eq!(colors[0], "#0000ff");
        assert_eq!(colors[4], "#ff0000");
        assert_eq!(colors[2], "#800080");
    }

    #[test]
    fn an_intermediate_stop_is_reached_exactly() {
        let colors = gradient_colors(&["#000000", "#ffffff", "#ff0000"], 3);
        assert_eq!(colors, ["#000000", "#ffffff", "#ff0000"]);
    }

    /// Position and value share one interpolation: no preset drifts.
    #[test]
    fn gradient_at_agrees_with_gradient_colors_for_every_preset() {
        for preset in GRADIENT_PRESETS {
            for count in 2..=11 {
                let by_rank = gradient_colors(preset.stops, count);
                for (index, expected) in by_rank.iter().enumerate() {
                    let t = index as f64 / (count - 1) as f64;
                    assert_eq!(
                        gradient_at(preset.stops, t).as_deref(),
                        Some(expected.as_str()),
                        "{} {index}/{count}",
                        preset.id
                    );
                }
            }
        }
    }

    #[test]
    fn gradient_at_clamps_and_refuses_unreadable_stops() {
        let soft = ["#102030", "#506070"];
        assert_eq!(gradient_at(&soft, 9.0).as_deref(), Some("#506070"));
        assert_eq!(gradient_at(&soft, -9.0).as_deref(), Some("#102030"));
        assert_eq!(
            gradient_at(&["#000000", "#ff0000"], -3.0).as_deref(),
            Some("#000000")
        );
        assert_eq!(
            gradient_at(&["#000000", "#ff0000"], 9.0).as_deref(),
            Some("#ff0000")
        );
        assert_eq!(
            gradient_at(&["#000000", "#ff0000"], f64::NAN).as_deref(),
            Some("#000000")
        );
        assert_eq!(gradient_at(&["#102030"], 0.7).as_deref(), Some("#102030"));
        assert_eq!(gradient_at(&["nope"], 0.5), None);
    }

    #[test]
    fn one_or_zero_curves() {
        assert_eq!(gradient_colors(&["#102030", "#ffffff"], 1), ["#102030"]);
        assert!(gradient_colors(&["#102030", "#ffffff"], 0).is_empty());
    }

    #[test]
    fn unreadable_stops_are_ignored_and_opacity_is_dropped() {
        assert!(gradient_colors(&["nope"], 3).is_empty());
        assert_eq!(
            gradient_colors(&["nope", "#11223380"], 2),
            ["#112233", "#112233"]
        );
    }

    #[test]
    fn every_provided_gradient_is_readable() {
        for preset in GRADIENT_PRESETS {
            assert!(preset.stops.len() >= 2, "{}", preset.id);
            assert_eq!(gradient_colors(preset.stops, 4).len(), 4, "{}", preset.id);
        }
    }
}
