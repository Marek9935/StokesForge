//! `stokesforge-app --version` (#382): prints the version and exits without
//! opening a window or touching the configuration folder.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn both_flags_print_the_version_and_exit_before_any_window() {
    let config = tempfile::tempdir().unwrap();
    for flag in ["--version", "-V"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_stokesforge-app"))
            .arg(flag)
            .env("STOKESFORGE_CONFIG_DIR", config.path())
            .env("XDG_CONFIG_HOME", config.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
                panic!("{flag}: the application did not exit (a window opened?)");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(status.success(), "{flag}");
        let mut stdout = String::new();
        std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut stdout).unwrap();
        assert_eq!(
            stdout,
            format!("stokesforge-app {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
        assert_eq!(
            std::fs::read_dir(config.path()).unwrap().count(),
            0,
            "{flag}: the configuration folder must stay untouched"
        );
    }
}
