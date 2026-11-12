//! Spring-loaded raise (#504): while columns are dragged, a graph window that
//! is not in front comes to the front once the pointer has stayed over it for
//! [`SPRING_LOAD_DELAY`], so the drop can land on it. Pure timing only, no
//! window, no model: the shell keeps `since` between frames and does the raise.

/// How long, in seconds, the pointer must stay over a window that is not in
/// front for it to come forward. The usual spring-loaded delays are about
/// 0.5 s to 1 s (macOS spring-loaded folders open after roughly that, the
/// Project Manager here waits 1 s before opening a folder under a dragged
/// row); 1 s is kept so that crossing a window on the way to another never
/// raises it.
pub const SPRING_LOAD_DELAY: f64 = 1.0;

/// One frame of the timer of one window. `since` is the moment the pointer
/// came over it (kept by the caller between frames), `hovered` is whether the
/// pointer is over it now and the drag goes on, `now` the time in seconds.
/// Returns `true` once, when the delay is up (the timer is then cleared).
/// Leaving the window, or the end of the drag, clears the timer, so coming
/// back starts again from zero.
pub fn spring_load_step(since: &mut Option<f64>, hovered: bool, now: f64) -> bool {
    if !hovered {
        *since = None;
        return false;
    }
    let start = *since.get_or_insert(now);
    if now - start >= SPRING_LOAD_DELAY {
        *since = None;
        true
    } else {
        false
    }
}

/// Seconds left before the raise, to schedule the repaint that fires it while
/// the pointer is still. `None` when no timer runs.
pub fn spring_load_remaining(since: Option<f64>, now: f64) -> Option<f64> {
    since.map(|start| (SPRING_LOAD_DELAY - (now - start)).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0.9 s over the window raises nothing, 1.0 s raises it, once.
    /// Witness: the comparison turned into `>` (1.0 s no longer raises) or the
    /// delay shortened (0.9 s raises).
    #[test]
    fn the_window_comes_forward_after_the_delay_and_only_once() {
        let mut since = None;
        assert!(!spring_load_step(&mut since, true, 10.0));
        assert!(!spring_load_step(&mut since, true, 10.9));
        assert!(spring_load_step(&mut since, true, 11.0));
        assert_eq!(since, None, "the timer is cleared by the raise");
    }

    /// Leaving the window clears the timer: coming back counts from zero.
    /// Witness: the `hovered == false` branch not clearing `since`.
    #[test]
    fn leaving_the_window_restarts_the_timer() {
        let mut since = None;
        spring_load_step(&mut since, true, 0.0);
        assert!(!spring_load_step(&mut since, true, 0.8));
        assert!(!spring_load_step(&mut since, false, 0.9));
        assert!(!spring_load_step(&mut since, true, 1.0));
        assert!(
            !spring_load_step(&mut since, true, 1.9),
            "0.9 s since return"
        );
        assert!(spring_load_step(&mut since, true, 2.0));
    }

    /// The repaint is scheduled for what is left of the delay.
    #[test]
    fn the_remaining_time_counts_down_to_zero() {
        assert_eq!(spring_load_remaining(None, 5.0), None);
        assert_eq!(spring_load_remaining(Some(5.0), 5.25), Some(0.75));
        assert_eq!(spring_load_remaining(Some(5.0), 9.0), Some(0.0));
    }
}
