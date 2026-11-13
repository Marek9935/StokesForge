//! The covering window serves itself (#457, #475): on native Wayland a root
//! fully covered by a maximised window gets no frame callback, and the window on
//! top must not need it. Gestures are applied and painted by the window's own
//! passes with the root `ui` count flat; the window closes itself; at rest
//! nothing wakes the root. (The real application is measured by
//! `server_bench.rs`, scenarios `reattach` and `covering`.)
//!
//! Real compositor, so manual: `#[ignore]`d. Run it with
//! `cargo test -p stokesforge-app --test wayland_covered_root -- --ignored --nocapture`.
//! It starts a headless `mutter` on its own Wayland socket, runs the example
//! `wayland_covered_root` (a root, one maximised child that locks one shared state, applies the
//! gestures and paints them in its own pass) with the GL backend, then kills the
//! compositor, its D-Bus session and the example, also when an assertion fails.
//! It skips with a message when `mutter` or `dbus-run-session` is missing.
//!
//! GL because under this headless mutter the Vulkan backend stops getting frame
//! callbacks for every window a few seconds in, covered or not.

#![cfg(target_os = "linux")]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/compositor.rs"]
mod compositor;
use compositor::Compositor;

/// Builds and runs the probe under the compositor; returns (exit success, stdout).
/// `extra`: environment knobs of the probe (see its doc).
fn run_probe(compositor: &Compositor, idle: bool, extra: &[(&str, &str)]) -> (bool, String) {
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
fn a_covering_window_is_served_by_its_own_passes() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    // Root `ui` count flat, every gesture applied and painted by the child's own pass.
    let (ok, out) = run_probe(&compositor, false, &[]);
    println!("{out}");
    assert!(ok, "the covering child was not served alone:\n{out}");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn an_idle_covering_window_leaves_the_root_asleep() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    let (ok, out) = run_probe(&compositor, true, &[]);
    println!("{out}");
    assert!(ok, "the root woke up at rest:\n{out}");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn a_covering_window_closes_itself_with_no_root_pass() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    let (ok, out) = run_probe(&compositor, false, &[("PROBE_REATTACH", "1")]);
    println!("{out}");
    assert!(ok, "the child could not close itself:\n{out}");
}

#[test]
#[ignore = "manual: needs mutter and dbus-run-session, see the module doc"]
fn with_every_window_hidden_the_probe_reports_what_still_runs() {
    let Some(compositor) = Compositor::start() else {
        return;
    };
    // A second maximised process covers root and child: informative, the numbers
    // are the answer (printed with --nocapture); the test only fails if the probe
    // itself cannot run to its end.
    let (ok, out) = run_probe(&compositor, false, &[("PROBE_COVER", "1")]);
    println!("{out}");
    assert!(ok, "the hidden-case probe did not complete:\n{out}");
}
