//! Gauge of the interruptible recalculation (#239): the only state shared
//! between the actor and its clients that does **not** go through the FIFO
//! channel.
//!
//! Why outside the channel: during a heavy recalculation the actor no longer
//! reads its requests, so a "Stop" sent through the channel would wait for the
//! end of the recalculation it wants to interrupt, and a snapshot requested
//! to paint the indicator would block the interface. The gauge is read and
//! written without the actor: a mutex for the state (with a condition to wait
//! for the end), an atomic for the Stop, read by the recalculation between
//! two columns.
//!
//! A recalculation carries a number: a late Stop — requested just after the
//! end of the recalculation it targeted — cannot stop the next one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// How an interruptible recalculation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecalcEnd {
    /// Ran to completion.
    Done,
    /// Interrupted by Stop: the command that triggered it was undone (as by
    /// Ctrl+Z) and what had changed was recalculated.
    Stopped,
}

#[derive(Debug, Default)]
struct State {
    /// Number of the last recalculation started (0 is reserved: "none"; the
    /// first one is 1).
    last: u64,
    /// The recalculation in progress.
    running: Option<u64>,
    /// The end of the last recalculation, until somebody collects it.
    finished: Option<RecalcEnd>,
    /// The actor's thread left its loop (normal stop **or** panic).
    gone: bool,
}

/// See the module docs.
#[derive(Debug, Default)]
pub(crate) struct RecalcGauge {
    state: Mutex<State>,
    idle: Condvar,
    stop: AtomicU64,
    /// Tests only: the next stop point of the recalculation panics.
    #[cfg(test)]
    panic_in_recalc: std::sync::atomic::AtomicBool,
}

impl RecalcGauge {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a recalculation and returns its number.
    pub(crate) fn begin(&self) -> u64 {
        let mut state = self.lock();
        state.last += 1;
        state.running = Some(state.last);
        state.finished = None;
        state.last
    }

    /// Closes recalculation `id` and wakes those who wait.
    pub(crate) fn finish(&self, id: u64, end: RecalcEnd) {
        let mut state = self.lock();
        if state.running == Some(id) {
            state.running = None;
            state.finished = Some(end);
        }
        self.idle.notify_all();
    }

    pub(crate) fn running(&self) -> Option<u64> {
        self.lock().running
    }

    /// The actor is gone: no recalculation can finish or be waited for any
    /// more. Resets `running` to `None` (otherwise a panic in the middle of a
    /// recalculation would leave an "in progress" indicator forever) and
    /// wakes those who wait.
    pub(crate) fn abandon(&self) {
        let mut state = self.lock();
        state.gone = true;
        state.running = None;
        self.idle.notify_all();
    }

    /// Is the actor's thread still there? Does not go through the actor.
    pub(crate) fn actor_alive(&self) -> bool {
        !self.lock().gone
    }

    /// Requests the stop of recalculation `id`; false if it is no longer running.
    pub(crate) fn request_stop(&self, id: u64) -> bool {
        let state = self.lock();
        if state.running == Some(id) {
            self.stop.store(id, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Read by the recalculation between two columns.
    pub(crate) fn stop_requested(&self, id: u64) -> bool {
        #[cfg(test)]
        assert!(
            !self.panic_in_recalc.load(Ordering::Acquire),
            "test panic during a recalculation"
        );
        self.stop.load(Ordering::Acquire) == id
    }

    /// Waits for the end of the recalculation in progress, at most `timeout`;
    /// true if there is (no longer) any.
    pub(crate) fn wait_idle(&self, timeout: Duration) -> bool {
        let state = self.lock();
        let (state, _) = self
            .idle
            .wait_timeout_while(state, timeout, |state| state.running.is_some())
            .unwrap_or_else(PoisonError::into_inner);
        state.running.is_none()
    }

    /// Collects the end of the last recalculation (only once).
    pub(crate) fn take_finished(&self) -> Option<RecalcEnd> {
        self.lock().finished.take()
    }
}

#[cfg(test)]
impl RecalcGauge {
    /// Sets the stop on the next recalculation: its first stop point sees
    /// it, which makes deterministic a test that would otherwise have to race
    /// the recalculation.
    pub(crate) fn panic_in_next_recalc_for_test(&self) {
        self.panic_in_recalc.store(true, Ordering::Release);
    }

    pub(crate) fn stop_next_for_test(&self) {
        let next = self.lock().last + 1;
        self.stop.store(next, Ordering::Release);
    }
}
