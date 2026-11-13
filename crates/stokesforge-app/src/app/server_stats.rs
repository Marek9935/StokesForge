//! Measurements of the server thread (#475, step 1, lot L4): what the shared
//! lock costs, read by the bench of `examples/server_bench.rs`.
//!
//! Off unless `STOKESFORGE_SERVER_STATS=<file>` is set when the application
//! starts: then every call below is a single load of a `OnceLock`. When on, a
//! sample is a wall-clock time, a duration in microseconds and a flag, pushed
//! to a vector, and written to the file as tab-separated lines when the
//! application exits (`kind`, `at_us` since the Unix epoch, `dur_us`, `flag`).
//! No behaviour changes and no user text.
//!
//! Kinds: `ui` and `logic` (wait for the lock at the start of the pass), `tick`
//! (lock held by one tick of the thread, flag = something the windows show
//! changed), `busy` (a tick that found the lock taken), `finish` (lock held by
//! the record that follows an autosave write or an import's commands), `write`
//! (the autosave write, lock released), `wake`, `exit`.

use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

struct Sample {
    kind: &'static str,
    at_us: u64,
    dur_us: u64,
    flag: bool,
}

struct Stats {
    path: String,
    samples: Mutex<Vec<Sample>>,
    write_started: AtomicU64,
    write_ended: AtomicU64,
}

/// Samples kept at most (a few MB); the rest is counted out.
const CAP: usize = 2_000_000;

fn stats() -> Option<&'static Stats> {
    static STATS: OnceLock<Option<Stats>> = OnceLock::new();
    STATS
        .get_or_init(|| {
            std::env::var("STOKESFORGE_SERVER_STATS")
                .ok()
                .filter(|path| !path.is_empty())
                .map(|path| Stats {
                    path,
                    samples: Mutex::new(Vec::new()),
                    write_started: AtomicU64::new(0),
                    write_ended: AtomicU64::new(0),
                })
        })
        .as_ref()
}

fn wall_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

/// A start mark, `None` when the measurements are off.
pub(super) fn start() -> Option<Instant> {
    stats().map(|_| Instant::now())
}

/// Records `kind` since `from` (the mark of [`start`]).
pub(super) fn since(kind: &'static str, from: Option<Instant>, flag: bool) {
    let (Some(stats), Some(from)) = (stats(), from) else {
        return;
    };
    let dur_us = from.elapsed().as_micros() as u64;
    let at_us = wall_us().saturating_sub(dur_us);
    let mut samples = stats.samples.lock();
    if samples.len() < CAP {
        samples.push(Sample {
            kind,
            at_us,
            dur_us,
            flag,
        });
    }
}

/// An instant event.
pub(super) fn event(kind: &'static str, flag: bool) {
    since(kind, start(), flag);
}

/// The autosave write begins / ends (also readable by the bench in process).
pub(super) fn write_began() {
    if let Some(stats) = stats() {
        stats.write_started.store(wall_us(), Ordering::SeqCst);
    }
}

pub(super) fn write_ended() {
    if let Some(stats) = stats() {
        stats.write_ended.store(wall_us(), Ordering::SeqCst);
    }
}

/// Is an autosave write in flight? For the bench; false when the
/// measurements are off.
#[doc(hidden)]
pub fn bench_write_in_flight() -> bool {
    stats().is_some_and(|s| {
        s.write_started.load(Ordering::SeqCst) > s.write_ended.load(Ordering::SeqCst)
    })
}

/// Writes the file: at exit, and for a bench that ends a hidden window by
/// itself.
#[doc(hidden)]
pub fn bench_flush_stats() {
    let Some(stats) = stats() else {
        return;
    };
    let samples = stats.samples.lock();
    let mut text = String::with_capacity(samples.len() * 32);
    for s in samples.iter() {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            s.kind,
            s.at_us,
            s.dur_us,
            u8::from(s.flag)
        ));
    }
    if let Ok(mut file) = std::fs::File::create(&stats.path) {
        let _ = file.write_all(text.as_bytes());
    }
}
