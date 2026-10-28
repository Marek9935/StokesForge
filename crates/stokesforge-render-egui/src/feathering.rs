//! The anti-aliasing lever — task R5.5 bis.
//!
//! # What measurement R0 0.3 found, and the plan had not foreseen
//!
//! *Feathering* — the one-pixel fringe that `epaint` adds to every stroke
//! to soften it — accounts for **78% of the tessellation cost**: 22.1 ms per
//! frame with it, **4.88 ms without**, on the same 300,000-point graph.
//! Turning it off therefore brings the document's load case back *under* the
//! 8 ms threshold, where LTTB would gain nothing there
//! ([`stokesforge_render::decimate`]). It is the most profitable lever of the
//! work package, and it was not in the list of §4.2 of `architecture-choisie.md`
//! before the measurement.
//!
//! **Decided on 2026-09-10**: it is turned off on entering mouse interaction
//! and restored on release. Aliasing is not visible on a moving image, and
//! it is exactly during the movement that the 22.1 ms hurt. It is therefore
//! not a setting exposed to the user, but a **rendering state**.
//!
//! # What R5 builds, and what R7.3 wired on top
//!
//! R5 provides **the lever**, not the event that triggers it: detecting a
//! drag was the job of phase R7 (Interaction), wired since.
//! The boundary stayed clean:
//!
//! - **R5 provides** [`Feathering`], [`apply`] and [`current`]: the means to
//!   say "draw without fringe" / "draw with", and to know where we stand;
//! - **R7.3 provides the call**, in `lib.rs`: on entering the drag
//!   (`Response::dragged()` — pan or zoom rectangle, both count),
//!   `apply(ctx, Feathering::Off)`; on release,
//!   `apply(ctx, Feathering::Full)`. Nothing else had to be written here, and
//!   this module has not changed since.
//!
//! # Why this is not in [`crate::cache`], nor in `stokesforge-render`
//!
//! Two boundaries, each for a different reason, and both
//! easy to cross by mistake:
//!
//! - **not in the cache key.** Turning off the fringe changes the
//!   *tessellation*, not the `Shape`s — and the tessellation happens on every
//!   frame anyway. Adding a term to the key
//!   `(revision, rect, pixels_per_point)` would rebuild the whole
//!   figure on every button press, that is, pay dearly
//!   at exactly the moment we wanted to lighten. `architecture-choisie.md`
//!   §4.2 says so explicitly; [`crate::cache`] says it again; and
//!   `tests/feathering.rs` checks it, because a third written reminder
//!   is not worth a check;
//! - **not in `stokesforge-render`.** [`egui::epaint::TessellationOptions`] is
//!   a setting of the egui stack, **downstream** of the collection of `Shape`s.
//!   `stokesforge-render` knows neither `egui` nor the `Context`, and
//!   `stokesforge-render-svg` has no tessellation at all: an SVG describes
//!   curves, it does not triangulate them. This is the only place of R5 where
//!   the rule "no feature in a single backend" does not apply,
//!   and it is not a breach: there is nothing to put on the other side.
//!
//! # Why this is a `Context` setting and not one of [`crate::GraphView`]
//!
//! `TessellationOptions` lives in the `Context` options, so for the whole
//! window. A `GraphView` that set it at paint time would set it
//! for all the other widgets of the frame, and two views would
//! set two contradictory values. The caller is the only one who knows
//! when an interaction starts; the function is therefore free, and takes the
//! `Context` as a parameter rather than fetching it.

/// The state of the anti-aliasing fringe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Feathering {
    /// Fringe on: full quality, the resting mode.
    #[default]
    Full,
    /// Fringe off: the interaction mode, ~4.5x cheaper to
    /// tessellate.
    Off,
}

impl Feathering {
    /// True if the fringe is on.
    pub fn is_on(self) -> bool {
        matches!(self, Feathering::Full)
    }

    /// The mode matching "an interaction is in progress".
    ///
    /// Serves the R7.3 wiring (`lib.rs`) in one line, without having to
    /// remember which way the boolean goes.
    pub fn during_drag(dragging: bool) -> Self {
        if dragging {
            Feathering::Off
        } else {
            Feathering::Full
        }
    }
}

/// Sets the anti-aliasing mode on an egui context.
///
/// Returns the **previous** mode, so that the caller can restore it without
/// having to remember it itself.
pub fn apply(context: &egui::Context, feathering: Feathering) -> Feathering {
    context.options_mut(|options| {
        let previous = from_flag(options.tessellation_options.feathering);
        options.tessellation_options.feathering = feathering.is_on();
        previous
    })
}

/// The mode in force on this context.
pub fn current(context: &egui::Context) -> Feathering {
    context.options(|options| from_flag(options.tessellation_options.feathering))
}

fn from_flag(on: bool) -> Feathering {
    if on {
        Feathering::Full
    } else {
        Feathering::Off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_mode_is_full_quality() {
        assert_eq!(Feathering::default(), Feathering::Full);
        assert!(Feathering::default().is_on());
    }

    #[test]
    fn the_direction_of_the_r7_trigger_is_written_once() {
        assert_eq!(Feathering::during_drag(true), Feathering::Off);
        assert_eq!(Feathering::during_drag(false), Feathering::Full);
    }
}
