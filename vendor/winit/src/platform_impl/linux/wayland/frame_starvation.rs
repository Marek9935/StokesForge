//! Transition #457: tell a window the compositor no longer serves from one that
//! is merely waiting for its next frame.
//!
//! winit withholds `RedrawRequested` while the callback of the last present is
//! outstanding. A window the compositor does not draw (fully covered by another,
//! on another workspace) never gets that callback, so the redraw is withheld for
//! good and, with it, everything the application does in the same pass. Wayland
//! has no occlusion event; this module infers one from the only signal there is:
//! a redraw is wanted, a callback has been outstanding for `STARVED_AFTER`.
//!
//! Pure: instants in, verdicts out, no Wayland type, so a test compiles it on
//! its own (`stokesforge-app/tests/wayland_frame_starvation.rs`).
//!
//! No periodic wake (rule #104): `wake_in` is a one-shot deadline, given only
//! while a redraw is pending and the verdict is still open. At rest, or once
//! starved, there is none.

use std::time::{Duration, Instant};

/// How long a callback may stay outstanding, with a redraw wanted, before the
/// window counts as not served. A visible window answers within a refresh.
pub const STARVED_AFTER: Duration = Duration::from_millis(100);

/// Verdict for one window at one instant.
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The compositor is not serving the window (report it occluded).
    pub starved: bool,
    /// Wake the loop in this long to ask again; `None` when nothing can change
    /// by waiting.
    pub wake_in: Option<Duration>,
}

/// `requested_at`: when the outstanding callback was asked for, `None` when none
/// is. `redraw_pending`: the application wants a frame. `was_starved`: this
/// window was already reported occluded.
pub fn assess(
    requested_at: Option<Instant>,
    redraw_pending: bool,
    was_starved: bool,
    now: Instant,
) -> Verdict {
    if was_starved {
        // Already said; only the callback ends it.
        return Verdict {
            starved: true,
            wake_in: None,
        };
    }
    match requested_at {
        Some(at) if redraw_pending => {
            let waited = now.saturating_duration_since(at);
            if waited >= STARVED_AFTER {
                Verdict {
                    starved: true,
                    wake_in: None,
                }
            } else {
                Verdict {
                    starved: false,
                    wake_in: Some(STARVED_AFTER - waited),
                }
            }
        }
        _ => Verdict {
            starved: false,
            wake_in: None,
        },
    }
}

/// What the frame callback of a window changes.
#[derive(Debug, PartialEq, Eq)]
pub struct OnCallback {
    /// Report the window visible again, before the redraw.
    pub occluded_false: bool,
    /// Emit `RedrawRequested` now.
    pub redraw: bool,
}

/// The callback arrived. A window that was reported occluded has to repaint at
/// once, whatever happened to the pending flag meanwhile: the buffer the
/// compositor shows is the one from before it was covered.
pub fn on_callback(was_starved: bool, redraw_pending: bool) -> OnCallback {
    OnCallback {
        occluded_false: was_starved,
        redraw: redraw_pending || was_starved,
    }
}
