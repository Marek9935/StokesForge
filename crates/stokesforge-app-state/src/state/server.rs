//! The pure part of the server thread (#475): one tick of the background
//! follow-up (scripts, recalculations, imports, autosave), with an injected
//! clock.
//!
//! The thread itself lives in `stokesforge-app` (`app/server.rs`); this module
//! only decides. A tick runs under the lock and does no heavy work: the
//! blocking write of the autosave is handed back as a job
//! ([`AutosaveJob`](super::AutosaveJob)) that the thread runs with the lock
//! released. A tick executes no command (invariant 1) and drops no script
//! engine (invariant 9: a finished run only moves its engine into the state).

use std::time::{Duration, Instant};

use super::autosave::{AutosaveStep, ServerTick};
use super::AppState;

/// While a script runs, the thread looks again every 50 ms.
pub const SCRIPT_LOOK: Duration = Duration::from_millis(50);
/// During a recalculation or an import, every 100 ms.
pub const WORK_LOOK: Duration = Duration::from_millis(100);

/// What the windows show of the background work. Focus plays no part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ServerStamp {
    epoch: u64,
    revision: u64,
    geometry_revision: u64,
    recorded: u64,
    alert: bool,
    script: bool,
    recalc: bool,
    import: bool,
    import_ready: bool,
    lost: bool,
}

impl AppState {
    fn server_stamp(&self) -> ServerStamp {
        ServerStamp {
            epoch: self.snapshot.epoch,
            revision: self.snapshot.revision,
            geometry_revision: self.snapshot.geometry_revision,
            recorded: self.recorded_count,
            alert: self.alert.is_some(),
            script: self.script_running(),
            recalc: self.recalc_since.is_some(),
            import: self.import_job.is_some(),
            import_ready: self.import_job.as_ref().is_some_and(|j| j.ready.is_some()),
            lost: self.session_lost,
        }
    }

    /// One tick of the server thread, under the lock (#475): polls the script
    /// run and the recalculation (collecting what ended, as the paint pass used
    /// to), the progress of an import (applying a finished import stays in the
    /// paint pass), and decides the autosave. `changed` is true when the stamp
    /// of what the windows show moved during the tick.
    pub fn server_tick(&mut self, now: Instant) -> ServerTick {
        let before = self.server_stamp();
        self.poll_script();
        self.poll_recalc();
        self.background(|state| {
            state.poll_import_progress();
        });
        let step = self.background(|state| state.autosave_decide(now));
        let changed = self.server_stamp() != before;
        let (autosave, auto_next) = match step {
            AutosaveStep::Wait(next) => (None, next),
            // A job is run at once: look again straight after.
            AutosaveStep::Job(job) => (Some(job), Some(Duration::ZERO)),
        };
        let mut next = auto_next;
        let mut sooner = |look: Duration| {
            next = Some(next.map_or(look, |current| current.min(look)));
        };
        if self.script_running() {
            sooner(SCRIPT_LOOK);
        }
        if self.recalc_since.is_some() || self.import_job.is_some() {
            sooner(WORK_LOOK);
        }
        ServerTick {
            changed,
            next,
            autosave,
        }
    }
}
