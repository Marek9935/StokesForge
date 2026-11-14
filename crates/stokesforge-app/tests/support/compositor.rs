//! The headless compositor of the manual Wayland tests (#457, #475): `mutter
//! --headless` under `dbus-run-session`, on its own socket, killed by process
//! group on drop (also when an assertion fails). Shared by
//! `wayland_covered_root.rs` and `server_bench.rs` through `#[path]`.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// The compositor stack, killed by process group on drop.
pub struct Compositor {
    pub leader: Child,
    pub socket: String,
    /// `DISPLAY` of the Xwayland the compositor started, with `PROBE_X11=1` (#475).
    pub x_display: Option<String>,
    /// Its `XAUTHORITY`: mutter starts Xwayland with a cookie file.
    pub x_auth: Option<PathBuf>,
}

pub fn x11_run() -> bool {
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
    pub fn start() -> Option<Self> {
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
            "sf-wayland-test-{}-{}",
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
