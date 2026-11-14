//! Lot L4 of step 1 of #475: what the server thread's lock costs, measured on
//! the real application (`examples/server_bench.rs`) under a headless mutter.
//!
//! Manual, `#[ignore]`d. Run it in release (the debug build is far slower than
//! what a user has):
//! `cargo test --release -p stokesforge-app --test server_bench -- --ignored --nocapture --test-threads=1`
//! (`SERVER_BENCH_RUNS=3` runs per test by default). Each test prints
//! `server-bench: M<n> … PASS|FAIL` lines. Same environment as
//! `wayland_covered_root.rs`: `mutter` and `dbus-run-session`, GL backend,
//! the whole process group killed also on failure.
//!
//! Criteria (accepted thresholds of #475, step 1):
//! - M1 lock wait at the start of a root pass (`ui`, `logic`): p99 <= 0.5 ms,
//!   max <= 4 ms. The root repaints at once while the thread works.
//! - M2 lock held per tick of the thread: p99 <= 1 ms, max <= 8 ms; the tick
//!   that ends a recalculation and the import finish are reported apart.
//! - M3 autosave with every window hidden (a second maximised process, or the
//!   root minimised): the slot file's mtime advances at each deadline +-1 s.
//! - M4 end of a recalculation noticed with every window hidden: <= 150 ms.
//! - M5 at rest: no root pass in 5 s and no wake of the thread.
//! - M6 quit with an autosave write in flight: quit time <= save time + 100 ms.
//!
//! A criterion that cannot be measured prints `NOT RUN` and the reason.

#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/compositor.rs"]
mod compositor;
use compositor::Compositor;

/// One sample of `server_stats.rs`.
#[derive(Clone, Debug)]
struct Sample {
    kind: String,
    at_us: u64,
    dur_us: u64,
    flag: bool,
}

struct Run {
    out: String,
    /// `WAYLAND_DEBUG=client` log, when the run asked for it (`config.env`).
    wayland: String,
    samples: Vec<Sample>,
    finished: bool,
}

impl Run {
    /// `bench: <key> <value>` pairs: the value after `key` on any line.
    fn value(&self, key: &str) -> Option<u64> {
        self.out.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            while let Some(word) = words.next() {
                if word == key {
                    return words.next()?.parse().ok();
                }
            }
            None
        })
    }

    fn of(&self, kind: &str) -> Vec<&Sample> {
        self.samples.iter().filter(|s| s.kind == kind).collect()
    }
}

fn runs() -> usize {
    std::env::var("SERVER_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

fn release_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent()?.parent()?.file_name().map(|n| n == "release"))
        .unwrap_or(false)
}

fn exe_path() -> std::path::PathBuf {
    let mut exe = std::env::current_exe().expect("test path");
    exe.pop(); // deps/
    exe.pop(); // debug/ or release/
    exe.push("examples/server_bench");
    exe
}

fn build_example() {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args([
        "build",
        "-p",
        "stokesforge-app",
        "--example",
        "server_bench",
    ]);
    if release_build() {
        cmd.arg("--release");
    }
    assert!(
        cmd.status().expect("cargo runs").success(),
        "the bench builds"
    );
}

/// The native windows (xdg toplevels) of a `WAYLAND_DEBUG=client` log: `-> xdg_surface#N.
/// get_toplevel(new id xdg_toplevel#M)` creates one, `-> xdg_toplevel#M.destroy()`
/// ends one. Returns how many were created in all and how many were alive at
/// each `BENCH_STAGE <name>` marker the bench wrote to the same stream.
fn toplevels(log: &str) -> (usize, Vec<(String, i64)>) {
    let (mut created, mut live) = (0usize, 0i64);
    let mut stages = Vec::new();
    for line in log.lines() {
        if line.contains("-> xdg_surface#") && line.contains(".get_toplevel(") {
            created += 1;
            live += 1;
        } else if line.contains("-> xdg_toplevel#") && line.contains(".destroy(") {
            live -= 1;
        } else if let Some(name) = line.strip_prefix("BENCH_STAGE ") {
            stages.push((name.trim().to_string(), live));
        }
    }
    (created, stages)
}

fn read_samples(path: &Path) -> Vec<Sample> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\t');
            Some(Sample {
                kind: f.next()?.to_string(),
                at_us: f.next()?.parse().ok()?,
                dur_us: f.next()?.parse().ok()?,
                flag: f.next()? == "1",
            })
        })
        .collect()
}

struct Config<'a> {
    scenario: &'a str,
    big: bool,
    secs: u32,
    env: &'a [(&'a str, &'a str)],
}

/// One run of the bench under a fresh compositor. `None`: the stack is absent.
fn run_once(config: &Config) -> Option<Run> {
    let compositor = Compositor::start()?;
    let dir = tempfile::tempdir().expect("scratch folder");
    let stats = dir.path().join("stats.tsv");
    let wayland_log = dir.path().join("wayland.log");
    let wants_wayland_log = config.env.iter().any(|(key, _)| *key == "WAYLAND_DEBUG");
    let mut cmd = Command::new(exe_path());
    cmd.env_remove("DISPLAY")
        .env_remove("STOKESFORGE_X11")
        .env("WAYLAND_DISPLAY", &compositor.socket)
        .env("WGPU_BACKEND", "gl")
        .env("BENCH_SCENARIO", config.scenario)
        .env("BENCH_SIZE", if config.big { "big" } else { "small" })
        .env("BENCH_SECS", config.secs.to_string())
        .env("BENCH_DIR", dir.path())
        .env("STOKESFORGE_SERVER_STATS", &stats)
        .env("STOKESFORGE_CONFIG_DIR", dir.path().join("config"))
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .envs(config.env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(if wants_wayland_log {
            Stdio::from(std::fs::File::create(&wayland_log).expect("log file"))
        } else if std::env::var_os("PROBE_STDERR").is_some() {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
    let mut child = cmd.spawn().expect("the bench starts");
    // The pipe is read by a thread: a full pipe would stall the bench.
    let mut pipe = child.stdout.take().expect("stdout");
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        std::io::Read::read_to_string(&mut pipe, &mut out).ok();
        out
    });
    let deadline = Instant::now() + Duration::from_secs(u64::from(config.secs) + 120);
    let finished = loop {
        if child.try_wait().expect("wait").is_some() {
            break true;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break false;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let out = reader.join().unwrap_or_default();
    Some(Run {
        out,
        wayland: std::fs::read_to_string(&wayland_log).unwrap_or_default(),
        samples: read_samples(&stats),
        finished,
    })
}

/// Nearest-rank percentile, in milliseconds.
fn pct(values: &[u64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let rank = ((v.len() as f64) * p).ceil().max(1.0) as usize;
    v[rank.min(v.len()) - 1] as f64 / 1000.0
}

fn summary(values: &[u64]) -> String {
    format!(
        "n={} p50={:.3} p99={:.3} max={:.3} ms",
        values.len(),
        pct(values, 0.5),
        pct(values, 0.99),
        pct(values, 1.0)
    )
}

fn verdict(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

fn size_name(big: bool) -> &'static str {
    if big {
        "big"
    } else {
        "small"
    }
}

fn skip() {
    eprintln!("skipped: no headless compositor stack");
}

/// M1 and M2: the root repaints at once while a recalculation and an import
/// go through the thread.
fn lock_cost(big: bool) {
    build_example();
    let secs = if big { 45 } else { 30 };
    let mut all_ok = true;
    for run in 1..=runs() {
        let Some(r) = run_once(&Config {
            scenario: "lock",
            big,
            secs,
            env: &[],
        }) else {
            return skip();
        };
        assert!(r.finished, "the bench did not end:\n{}", r.out);
        let waits: Vec<u64> = r
            .samples
            .iter()
            .filter(|s| s.kind == "ui" || s.kind == "logic")
            .map(|s| s.dur_us)
            .collect();
        let ticks: Vec<u64> = r.of("tick").iter().map(|s| s.dur_us).collect();
        let changed: Vec<u64> = r
            .of("tick")
            .iter()
            .filter(|s| s.flag)
            .map(|s| s.dur_us)
            .collect();
        let finish: Vec<u64> = r.of("finish").iter().map(|s| s.dur_us).collect();
        let busy = r.of("busy").len();
        let dispose: Vec<u64> = r.of("dispose").iter().map(|s| s.dur_us).collect();
        println!(
            "server-bench: M2 {} run={run} dropped after unlocking: {}",
            size_name(big),
            summary(&dispose)
        );
        let m1 = pct(&waits, 0.99) <= 0.5 && pct(&waits, 1.0) <= 4.0;
        let m2 = pct(&ticks, 0.99) <= 1.0 && pct(&ticks, 1.0) <= 8.0;
        let size = size_name(big);
        println!(
            "server-bench: M1 {size} run={run} lock_wait {} {}",
            summary(&waits),
            verdict(m1)
        );
        println!(
            "server-bench: M2 {size} run={run} tick_hold {} {}",
            summary(&ticks),
            verdict(m2)
        );
        println!(
            "server-bench: M2 {size} run={run} detail changed_ticks {} | finish {} | busy_ticks={busy}",
            summary(&changed),
            summary(&finish)
        );
        for line in r.out.lines().filter(|l| l.contains("gesture")) {
            println!("server-bench: {size} run={run} {line}");
        }
        all_ok &= m1 && m2;
    }
    println!(
        "server-bench: M1+M2 {} ALL RUNS {}",
        size_name(big),
        verdict(all_ok)
    );
    assert!(all_ok, "M1 or M2 over its threshold");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn m1_m2_lock_cost_small_project() {
    lock_cost(false);
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn m1_m2_lock_cost_big_project() {
    lock_cost(true);
}

/// M5: at rest, the root is idle and the thread is not woken.
fn at_rest(big: bool) {
    build_example();
    let mut all_ok = true;
    for run in 1..=runs() {
        let Some(r) = run_once(&Config {
            scenario: "rest",
            big,
            secs: 14,
            env: &[],
        }) else {
            return skip();
        };
        assert!(r.finished, "the bench did not end:\n{}", r.out);
        let (Some(from), Some(to), Some(passes)) = (
            r.value("rest_window_start_us"),
            r.value("rest_window_end_us"),
            r.value("root_passes_in_window"),
        ) else {
            panic!("no rest window in:\n{}", r.out);
        };
        let wakes = r
            .samples
            .iter()
            .filter(|s| (s.kind == "tick" || s.kind == "busy") && s.at_us >= from && s.at_us <= to)
            .count();
        let ok = passes == 0 && wakes <= 1;
        println!(
            "server-bench: M5 {} run={run} root_passes_in_5s={passes} thread_wakes_in_5s={wakes} (autosave period 60 s) {}",
            size_name(big),
            verdict(ok)
        );
        all_ok &= ok;
    }
    assert!(all_ok, "M5 failed");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn m5_at_rest_small_project() {
    at_rest(false);
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn m5_at_rest_big_project() {
    at_rest(true);
}

/// M3, M4, M5 with every window hidden. `minimise`: the root minimises itself
/// instead of being covered by a second process.
fn hidden(big: bool, minimise: bool) {
    build_example();
    let how = if minimise { "minimised" } else { "covered" };
    let size = size_name(big);
    let mut all_ok = true;
    for run in 1..=runs() {
        let Some(r) = run_once(&Config {
            scenario: "hidden",
            big,
            secs: 135,
            env: &[
                ("BENCH_EDIT_AT", "30,90"),
                ("BENCH_RECALC_AT", "20"),
                ("BENCH_IMPORT_AT", "40"),
                ("BENCH_MINIMISE", if minimise { "1" } else { "0" }),
            ],
        }) else {
            return skip();
        };
        assert!(r.finished, "the bench did not end:\n{}", r.out);
        let tag = format!("{size} {how} run={run}");
        // Root passes while hidden.
        let at_hidden = r.value("root_passes_at_hidden").unwrap_or(0);
        let at_end = r.value("root_passes_at_end").unwrap_or(0);
        let hidden_passes = at_end.saturating_sub(at_hidden);
        let hidden_from = r.value("hidden_from_us").unwrap_or(0);
        let late: Vec<f64> = r
            .of("ui")
            .iter()
            .filter(|s| s.at_us >= hidden_from)
            .map(|s| (s.at_us - hidden_from) as f64 / 1e6)
            .collect();
        println!(
            "server-bench: hidden {tag} root_ui_passes_while_hidden={hidden_passes} (counter {at_hidden} to {at_end}; {} samples) at s after hidden_from {:?}; wakes={}",
            late.len(),
            &late[..late.len().min(12)],
            r.of("wake").len()
        );
        // M3: the mtime of the slot file against the deadlines.
        let first_tick = r.of("tick").first().map_or(0, |s| s.at_us);
        let mtimes: Vec<u64> = r
            .out
            .lines()
            .filter_map(|l| l.split("mtime_us ").nth(1)?.trim().parse().ok())
            .collect();
        let writes = r.of("write");
        let mut m3 = !mtimes.is_empty() && mtimes.len() >= 2;
        for (k, mtime) in mtimes.iter().enumerate() {
            let expected = first_tick + 60_000_000 * (k as u64 + 1);
            let off = *mtime as i64 - expected as i64;
            m3 &= off.abs() <= 1_000_000;
            println!(
                "server-bench: M3 {tag} write #{} mtime_minus_deadline={:.3} s (write took {:.0} ms)",
                k + 1,
                off as f64 / 1e6,
                writes.get(k).map_or(f64::NAN, |w| w.dur_us as f64 / 1000.0)
            );
        }
        println!(
            "server-bench: M3 {tag} writes={} {}",
            mtimes.len(),
            verdict(m3)
        );
        all_ok &= m3;
        // M4: end of the recalculation against its true end.
        if let Some(end) = r.value("recalc_true_end_us") {
            let noticed = r
                .of("tick")
                .into_iter()
                .filter(|s| s.flag && s.at_us + s.dur_us >= end.saturating_sub(2_000))
                .map(|s| s.at_us + s.dur_us)
                .min();
            match noticed {
                Some(at) => {
                    let ms = (at as f64 - end as f64) / 1000.0;
                    let hold = r
                        .of("tick")
                        .into_iter()
                        .find(|s| s.flag && s.at_us + s.dur_us == at)
                        .map_or(0, |s| s.dur_us);
                    println!(
                        "server-bench: M4 {tag} recalc end noticed after {ms:.1} ms (that tick held the lock {:.3} ms) {}",
                        hold as f64 / 1000.0,
                        verdict(ms <= 150.0)
                    );
                    all_ok &= ms <= 150.0;
                }
                None => println!("server-bench: M4 {tag} NOT RUN (no tick after the end)"),
            }
        } else {
            println!(
                "server-bench: M4 {tag} NOT RUN (recalculation not observed: {})",
                r.out
                    .lines()
                    .find(|l| l.contains("recalc_not_seen"))
                    .unwrap_or("no line")
            );
        }
        // M2 detail: finish holds (autosave record, import) and the longest ticks.
        let mut ticks: Vec<&Sample> = r.of("tick");
        ticks.sort_by_key(|s| std::cmp::Reverse(s.dur_us));
        let finish: Vec<u64> = r.of("finish").iter().map(|s| s.dur_us).collect();
        println!(
            "server-bench: M2 {tag} hidden tick_hold {} | finish {} | longest ticks ms {:?}",
            summary(&ticks.iter().map(|s| s.dur_us).collect::<Vec<_>>()),
            summary(&finish),
            ticks
                .iter()
                .take(3)
                .map(|s| s.dur_us as f64 / 1000.0)
                .collect::<Vec<_>>()
        );
        // M5 while hidden: wakes of the thread in the quiet stretch after the
        // second edit's write (no activity from the bench).
        if let (Some(w2), Some(_)) = (writes.get(1), mtimes.get(1)) {
            let from = w2.at_us + w2.dur_us + 2_000_000;
            let to = from + 40_000_000;
            let wakes = r
                .samples
                .iter()
                .filter(|s| {
                    (s.kind == "tick" || s.kind == "busy") && s.at_us >= from && s.at_us <= to
                })
                .count();
            println!(
                "server-bench: M5 {tag} thread wakes in a quiet 40 s after the second write: {wakes} {}",
                verdict(wakes <= 1)
            );
            all_ok &= wakes <= 1;
        }
    }
    println!(
        "server-bench: hidden {size} {how} ALL RUNS {}",
        verdict(all_ok)
    );
    assert!(all_ok, "a hidden-window criterion failed");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, ~8 min, see the module doc"]
fn m3_m4_hidden_covered_big_project() {
    hidden(true, false);
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, ~8 min, see the module doc"]
fn m3_m4_hidden_covered_small_project() {
    hidden(false, false);
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, ~8 min, see the module doc"]
fn m3_hidden_minimised_big_project() {
    hidden(true, true);
}

/// Quit with no write in flight: the cost of the exit itself, to read M6 by.
fn idle_quit_ms() -> Option<f64> {
    let r = run_once(&Config {
        scenario: "quit_idle",
        big: true,
        secs: 60,
        env: &[],
    })?;
    let (asked, closed) = (r.value("quit_request_us")?, r.value("window_closed_us")?);
    Some((closed - asked) as f64 / 1000.0)
}

/// M6: Quit while the autosave write is in flight waits for it, no longer.
#[test]
#[ignore = "manual: needs mutter and dbus-run-session, ~4 min, see the module doc"]
fn m6_quit_during_autosave_big_project() {
    build_example();
    let mut all_ok = true;
    for run in 1..=runs() {
        let Some(r) = run_once(&Config {
            scenario: "quit",
            big: true,
            secs: 100,
            env: &[],
        }) else {
            return skip();
        };
        assert!(r.finished, "the bench did not end:\n{}", r.out);
        let (Some(asked), Some(closed)) = (r.value("quit_request_us"), r.value("window_closed_us"))
        else {
            panic!("no quit measured:\n{}", r.out);
        };
        let Some(write) = r.of("write").into_iter().next() else {
            panic!("no autosave write:\n{}", r.out);
        };
        let quit_ms = (closed - asked) as f64 / 1000.0;
        let save_ms = write.dur_us as f64 / 1000.0;
        let remaining_ms = (write.at_us + write.dur_us).saturating_sub(asked) as f64 / 1000.0;
        let ok = quit_ms <= save_ms + 100.0;
        println!(
            "server-bench: M6 run={run} quit_time={quit_ms:.0} ms save_time={save_ms:.0} ms (write remaining at the request {remaining_ms:.0} ms; quit minus remaining {:.0} ms) {}",
            quit_ms - remaining_ms,
            verdict(ok)
        );
        all_ok &= ok;
        // Timeline after the request (ms): where the time goes.
        let rel = |at: u64| (at as f64 - asked as f64) / 1000.0;
        let write_end = write.at_us + write.dur_us;
        let after = |kind: &str| {
            r.of(kind)
                .into_iter()
                .filter(|s| s.at_us + s.dur_us >= write_end)
                .map(|s| rel(s.at_us))
                .take(3)
                .collect::<Vec<_>>()
        };
        println!(
            "server-bench: M6 run={run} timeline ms after the request: write_end={:.0} finish_at={:?} wake_at={:?} ui_passes_at={:?} exit_event={:?} window_closed={:.0}",
            rel(write_end),
            after("finish"),
            after("wake"),
            after("ui"),
            r.of("exit").iter().map(|s| rel(s.at_us)).collect::<Vec<_>>(),
            rel(closed)
        );
        let everything: Vec<String> = r
            .samples
            .iter()
            .filter(|s| s.at_us + 50_000 >= write_end && s.at_us <= closed)
            .map(|s| {
                format!(
                    "{}@{:.1}+{:.3}",
                    s.kind,
                    rel(s.at_us),
                    s.dur_us as f64 / 1000.0
                )
            })
            .collect();
        println!("server-bench: M6 run={run} samples from the write end: {everything:?}");
    }
    if let Some(idle) = idle_quit_ms() {
        println!("server-bench: M6 reference: quit with no write in flight takes {idle:.0} ms");
    }
    assert!(all_ok, "M6 failed");
}

/// #475, step 2, L4 (native Wayland): a detached graph maximised over the root
/// reattaches itself from its own pass. Real application, GL, headless mutter.
/// The window is gone from the application's list within 500 ms of the click
/// and the tab floats in the main window; the root, covered, runs at most one
/// pass until then and paints the floating tab once uncovered.
#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn a_covering_detached_graph_reattaches_itself() {
    build_example();
    let Some(r) = run_once(&Config {
        scenario: "reattach",
        big: false,
        secs: 14,
        env: &[],
    }) else {
        return skip();
    };
    println!("{}", r.out);
    assert!(r.finished, "the bench did not end:\n{}", r.out);
    let line = |key: &str| {
        r.out
            .lines()
            .find(|line| line.contains(key))
            .unwrap_or_else(|| panic!("no `{key}` line:\n{}", r.out))
            .to_string()
    };
    let clicked = line("click_to_gone_us");
    let gone_us: i64 = clicked
        .split("click_to_gone_us ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("a time");
    let delta: u64 = clicked
        .rsplit("root_ui_delta_until_gone ")
        .next()
        .and_then(|value| value.trim().parse().ok())
        .expect("a count");
    let after = line("reattach after");
    let floats = after.contains("(false, true)");
    let after_delta: u64 = after
        .rsplit("root_ui_delta_after_uncovered ")
        .next()
        .and_then(|value| value.trim().parse().ok())
        .expect("a count");
    let ok = (0..500_000).contains(&gone_us) && delta <= 1 && floats && after_delta >= 1;
    println!(
        "server-bench: L4 reattach click_to_gone={:.1} ms root_ui_delta_until_gone={delta} floats={floats} root_ui_after_uncovered={after_delta} {}",
        gone_us as f64 / 1000.0,
        verdict(ok)
    );
    assert!(
        ok,
        "the covering window did not reattach itself:\n{}",
        r.out
    );
}

/// #475, step 2, L5: with the starvation patch of #457 gone, a graph, then a
/// workbook, then the Project Manager, each maximised over the root, serve
/// themselves: the Reattach click is applied by the window's own pass (gone
/// within 500 ms) while the covered root runs no pass at all.
#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn covering_windows_serve_themselves_with_the_root_left_alone() {
    build_example();
    let Some(r) = run_once(&Config {
        scenario: "covering",
        big: false,
        secs: 20,
        env: &[],
    }) else {
        return skip();
    };
    println!("{}", r.out);
    assert!(r.finished, "the bench did not end:\n{}", r.out);
    let mut all_ok = true;
    for name in ["graph", "book", "project_manager"] {
        let key = format!("covering {name} ");
        let line = r
            .out
            .lines()
            .find(|line| line.contains(&key))
            .unwrap_or_else(|| panic!("no `{key}` line:\n{}", r.out));
        let gone_us: i64 = line
            .split("click_to_gone_us ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse().ok())
            .expect("a time");
        let delta: u64 = line
            .rsplit("root_ui_delta_until_gone ")
            .next()
            .and_then(|value| value.trim().parse().ok())
            .expect("a count");
        let ok = (0..500_000).contains(&gone_us) && delta == 0;
        all_ok &= ok;
        println!(
            "server-bench: L5 covering {name} click_to_gone={:.1} ms root_ui_delta_until_gone={delta} {}",
            gone_us as f64 / 1000.0,
            verdict(ok)
        );
    }
    assert!(all_ok, "a covering window needed the root:\n{}", r.out);
}

/// #475, after the GNOME trial: a reattached window leaves no native window
/// behind. The `covering` scenario detaches and reattaches a graph, a
/// workbook and the Project Manager, one after the other. Counted on the
/// Wayland wire, the application creates four toplevels in all (the main
/// window and the three it detached) and no more: egui listing a viewport
/// that no window of ours owns makes eframe create an empty "egui window"
/// (default title), which stays until the application quits; the windows alive after each reattachment are counted too.
#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn reattached_windows_leave_no_native_window_behind() {
    build_example();
    let Some(r) = run_once(&Config {
        scenario: "covering",
        big: false,
        secs: 20,
        env: &[("WAYLAND_DEBUG", "client")],
    }) else {
        return skip();
    };
    assert!(r.finished, "the bench did not end:\n{}", r.out);
    let (created, stages) = toplevels(&r.wayland);
    // The scenario opens the three windows, then reattaches them one by one: the
    // main window and the windows still detached are all that is alive.
    let alive: Vec<i64> = stages.iter().map(|(_, live)| *live).collect();
    let ok = created == 4 && alive == [3, 2, 1];
    println!(
        "server-bench: toplevels created={created} (expected 4) alive after each stage {stages:?} (expected [3, 2, 1]) {}",
        verdict(ok)
    );
    assert!(
        ok,
        "ghost windows: created {created}, alive per stage {stages:?}"
    );
}
