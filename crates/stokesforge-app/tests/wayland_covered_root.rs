//! Guard #457: a root window fully covered by a detached window keeps
//! processing what the detached window asks for, on native Wayland.
//!
//! Real compositor, so manual: `#[ignore]`d. Run it with
//! `cargo test -p stokesforge-app --test wayland_covered_root -- --ignored --nocapture`.
//! It starts a headless `mutter` on its own Wayland socket, runs the example
//! `wayland_covered_root` (a root, one maximised child that queues requests and
//! wakes the root, as a detached window does) with the GL backend, then kills the
//! compositor, its D-Bus session and the example, also when an assertion fails.
//! It skips with a message when `mutter` or `dbus-run-session` is missing.
//!
//! GL because under this headless mutter the Vulkan backend stops getting frame
//! callbacks for every window a few seconds in, covered or not.

#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// The compositor stack, killed by process group on drop.
struct Compositor {
    leader: Child,
    socket: String,
    /// `DISPLAY` of the Xwayland the compositor started, with `PROBE_X11=1` (#475).
    x_display: Option<String>,
    /// Its `XAUTHORITY`: mutter starts Xwayland with a cookie file.
    x_auth: Option<PathBuf>,
}

fn x11_run() -> bool {
    std::env::var_os("PROBE_X11").is_some_and(|v| v == "1")
}

fn x_auth_files(runtime: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(runtime)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(".mutter-Xwaylandauth.")
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn x_sockets() -> Vec<String> {
    std::fs::read_dir("/tmp/.X11-unix")
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

impl Compositor {
    fn start() -> Option<Self> {
        for tool in ["mutter", "dbus-run-session"] {
            if Command::new(tool)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_err()
            {
                eprintln!("skipped: `{tool}` is not installed");
                return None;
            }
        }
        let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
        // One socket per test: the tests of this binary share a pid and run in
        // parallel, and two probes on one compositor would cover each other.
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let socket = format!(
            "sf-457-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let before = x_sockets();
        let before_auth = x_auth_files(&runtime);
        let mut cmd = Command::new("dbus-run-session");
        cmd.args(["--", "mutter", "--headless"])
            .args(if x11_run() { None } else { Some("--no-x11") })
            .arg("--wayland-display")
            .arg(&socket)
            .args(["--virtual-monitor", "1400x900"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Own process group: one kill takes the bus and the compositor.
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let leader = cmd.spawn().ok()?;
        let mut compositor = Self {
            leader,
            socket,
            x_display: None,
            x_auth: None,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        while !runtime.join(&compositor.socket).exists() {
            if Instant::now() > deadline {
                eprintln!("skipped: the headless compositor opened no socket");
                return None;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if x11_run() {
            // mutter opens its Xwayland sockets and cookie file with its Wayland one;
            // of the new sockets the lowest number is the display.
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let mut new: Vec<String> = x_sockets()
                    .into_iter()
                    .filter(|n| !before.contains(n))
                    .collect();
                new.sort();
                let auth = x_auth_files(&runtime)
                    .into_iter()
                    .find(|p| !before_auth.contains(p));
                if let (Some(first), Some(auth)) = (new.first(), auth) {
                    compositor.x_display = Some(format!(":{}", first.trim_start_matches('X')));
                    compositor.x_auth = Some(auth);
                    break;
                }
                if Instant::now() > deadline {
                    eprintln!("skipped: the compositor opened no X socket");
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Some(compositor)
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        let group = format!("-{}", self.leader.id());
        let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
        let _ = self.leader.wait();
        // Its Xwayland dies with it; the cookie file and the socket stay behind.
        if let Some(auth) = &self.x_auth {
            let _ = std::fs::remove_file(auth);
        }
        if let Some(display) = &self.x_display {
            let _ = std::fs::remove_file(format!(
                "/tmp/.X11-unix/X{}",
                display.trim_start_matches(':')
            ));
        }
    }
}

/// Builds and runs the probe under the compositor; returns (exit success, stdout).
fn run_probe(compositor: &Compositor, idle: bool, two: bool) -> (bool, String) {
    run_probe_env(compositor, idle, two, &[])
}

/// Same with extra environment variables (the single-state modes of #475).
fn run_probe_env(
    compositor: &Compositor,
    idle: bool,
    two: bool,
    extra: &[(&str, &str)],
) -> (bool, String) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let built = Command::new(cargo)
        .args([
            "build",
            "-p",
            "stokesforge-app",
            "--example",
            "wayland_covered_root",
        ])
        .status()
        .expect("cargo runs");
    assert!(built.success(), "the probe example builds");
    let mut exe = std::env::current_exe().expect("test path");
    exe.pop(); // deps/
    exe.pop(); // debug/ or release/
    exe.push("examples/wayland_covered_root");
    let mut cmd = Command::new(exe);
    cmd.env_remove("DISPLAY")
        .env_remove("STOKESFORGE_X11")
        .env("WAYLAND_DISPLAY", &compositor.socket)
        .env("WGPU_BACKEND", "gl")
        .env("PROBE_IDLE", if idle { "1" } else { "0" })
        .env("PROBE_TWO", if two { "1" } else { "0" })
        .envs(extra.iter().copied())
        .stdout(Stdio::piped())
        .stderr(if std::env::var_os("PROBE_STDERR").is_some() {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
    if let Some(display) = &compositor.x_display {
        // X11 through Xwayland: no Wayland socket for the probe, so winit picks X11.
        cmd.env_remove("WAYLAND_DISPLAY").env("DISPLAY", display);
        if let Some(auth) = &compositor.x_auth {
            cmd.env("XAUTHORITY", auth);
        }
    }
    let mut child = cmd.spawn().expect("probe starts");
    let deadline = Instant::now() + Duration::from_secs(40);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break Some(status);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut out = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        std::io::Read::read_to_string(&mut pipe, &mut out).ok();
    }
    let _ = child.wait();
    (status.is_some_and(|s| s.success()), out)
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn a_covered_root_drains_the_requests_of_the_window_covering_it() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    let (ok, out) = run_probe(&compositor, false, false);
    println!("{out}");
    assert!(ok, "the covered root stalled:\n{out}");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn an_idle_covered_root_stays_asleep() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    let (ok, out) = run_probe(&compositor, true, false);
    println!("{out}");
    assert!(ok, "the idle root woke up periodically:\n{out}");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn a_covered_detached_window_is_not_reported_occluded() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    // Child A (small, repaints itself) is fully covered by child B (maximised).
    // Reported occluded, A would get no pass and lose its input (#457): only the
    // root asks for the report.
    let (ok, out) = run_probe(&compositor, false, true);
    println!("{out}");
    assert!(
        ok,
        "a covered child was reported occluded, or the root stalled:\n{out}"
    );
}

// #475 probe: one state behind one lock, no `set_reports_occlusion`. The three
// tests below say whether the winit frame-starvation patch and the fork can go.

#[test]
#[ignore = "manual: #475 probe, needs mutter and dbus-run-session, see the module doc"]
fn single_state_without_opt_in_keeps_the_covering_window_served() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    // Root `ui` count flat, every gesture applied and painted by the child's own pass.
    let (ok, out) = run_probe_env(&compositor, false, false, &[("PROBE_MODE", "single")]);
    println!("{out}");
    assert!(ok, "the single-state child was not served alone:\n{out}");
}

#[test]
#[ignore = "manual: #475 probe, needs mutter and dbus-run-session, see the module doc"]
fn single_state_child_closes_itself_with_no_root_pass() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    let (ok, out) = run_probe_env(
        &compositor,
        false,
        false,
        &[("PROBE_MODE", "single"), ("PROBE_REATTACH", "1")],
    );
    println!("{out}");
    assert!(ok, "the child could not close itself:\n{out}");
}

#[test]
#[ignore = "manual: #475 probe, needs mutter and dbus-run-session, see the module doc"]
fn single_state_with_every_window_hidden_reports_what_still_runs() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    // A second maximised process covers root and child: informative, the numbers
    // are the answer (printed with --nocapture); the test only fails if the probe
    // itself cannot run to its end.
    let (ok, out) = run_probe_env(
        &compositor,
        false,
        false,
        &[("PROBE_MODE", "single"), ("PROBE_COVER", "1")],
    );
    println!("{out}");
    assert!(ok, "the hidden-case probe did not complete:\n{out}");
}
