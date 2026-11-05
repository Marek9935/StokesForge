//! Guard #457: the pure rule that infers "occluded" on Wayland from a frame
//! callback that does not arrive, without a compositor.

#![cfg(target_os = "linux")]
#[path = "../../../vendor/winit/src/platform_impl/linux/wayland/frame_starvation.rs"]
mod starvation;

use std::time::{Duration, Instant};

use starvation::{assess, on_callback, Verdict};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn a_callback_outstanding_for_150_ms_with_a_redraw_pending_is_starved() {
    let now = Instant::now();
    let verdict = assess(Some(now - ms(150)), true, false, true, now);
    assert!(verdict.starved);
}

#[test]
fn a_callback_outstanding_for_50_ms_is_still_waited_for_with_a_50_ms_deadline() {
    let now = Instant::now();
    let verdict = assess(Some(now - ms(50)), true, false, true, now);
    assert_eq!(
        verdict,
        Verdict {
            starved: false,
            wake_in: Some(ms(50))
        }
    );
}

#[test]
fn the_threshold_is_one_hundred_milliseconds_inclusive() {
    let now = Instant::now();
    assert!(assess(Some(now - ms(100)), true, false, true, now).starved);
    assert!(!assess(Some(now - ms(99)), true, false, true, now).starved);
}

#[test]
fn an_idle_window_is_never_starved_and_never_wakes_the_loop() {
    let now = Instant::now();
    let verdict = assess(Some(now - ms(10_000)), false, false, true, now);
    assert_eq!(
        verdict,
        Verdict {
            starved: false,
            wake_in: None
        }
    );
    assert_eq!(
        assess(None, false, false, true, now),
        Verdict {
            starved: false,
            wake_in: None
        }
    );
}

#[test]
fn a_redraw_pending_without_an_outstanding_callback_is_not_starved() {
    let now = Instant::now();
    assert_eq!(
        assess(None, true, false, true, now),
        Verdict {
            starved: false,
            wake_in: None
        }
    );
}

#[test]
fn a_starved_window_asks_for_no_further_wake() {
    let now = Instant::now();
    let first = assess(Some(now - ms(150)), true, false, true, now);
    assert_eq!(first.wake_in, None);
    // Reported already: stays so, with no deadline, whatever the clock says.
    let later = assess(Some(now - ms(10)), true, true, true, now);
    assert_eq!(
        later,
        Verdict {
            starved: true,
            wake_in: None
        }
    );
}

#[test]
fn the_callback_of_a_starved_window_reports_it_visible_and_keeps_the_redraw() {
    let both = on_callback(true, true);
    assert!(both.occluded_false && both.redraw);
    // The pending flag was lost meanwhile: the stale buffer still repaints.
    let flag_lost = on_callback(true, false);
    assert!(flag_lost.occluded_false && flag_lost.redraw);
}

#[test]
fn the_callback_of_a_served_window_changes_nothing_but_the_pending_redraw() {
    assert!(on_callback(false, true).redraw);
    let idle = on_callback(false, false);
    assert!(!idle.occluded_false && !idle.redraw);
    assert!(!on_callback(false, true).occluded_false);
}

#[test]
fn a_window_that_did_not_ask_for_the_report_is_never_starved_and_never_wakes_the_loop() {
    let now = Instant::now();
    let verdict = assess(Some(now - ms(10_000)), true, false, false, now);
    assert_eq!(
        verdict,
        Verdict {
            starved: false,
            wake_in: None
        }
    );
}
