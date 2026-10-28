//! What to show at startup (#333): the decision, with no window or disk.
//!
//! The demo is not a permanent fixture. It is offered **once**, on the very
//! first launch: written to `demo.stf`, next to the preferences, then opened
//! from that file, which the user afterwards finds again through "Open". Later
//! launches start from an empty project, like File > New.
//!
//! Development keeps the demo on every launch through `STOKESFORGE_DEMO=1`
//! (screenshots, trials, measurements): the binary reads the environment and
//! passes the flag to [`plan`], which never touches it, so it can be tested
//! without `set_var`. This demo is disposable: the binary also sets
//! [`crate::state::AppState::set_quit_without_prompt`], and closing no longer
//! asks to save (New and Open… always ask).
//!
//! Rules, in order:
//! 1. an existing path is loaded, always;
//! 2. `STOKESFORGE_DEMO=1`: the in-memory demo, with no marker;
//! 3. a missing path is the target of a future save, on an empty project
//!    (it is not a "first launch");
//! 4. no argument: the demo if the first-launch marker is missing, otherwise
//!    an empty project.
//!
//! A `demo.stf` in the current folder is never read (#315): only the one in
//! the configuration folder counts.

use std::path::{Path, PathBuf};

use crate::preferences::Preferences;

/// What to build at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupSource {
    /// Load this existing file.
    Load(PathBuf),
    /// Write the demo to this file, then open it from there. If writing or
    /// reading fails, the binary keeps the demo in memory, with no path.
    DemoFile(PathBuf),
    /// The in-memory demo; the path is the target of a future save (empty if
    /// there is none).
    DemoMemory(PathBuf),
    /// An empty project; same target path.
    Empty(PathBuf),
}

/// The startup plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    /// The project to build.
    pub source: StartupSource,
    /// Pre-fill the script area with the example script: it reads
    /// `[Book1]Sheet1!A`, which exists only in the demo.
    pub example_script: bool,
    /// Set the "first launch done" marker once the window is built.
    pub mark_first_run: bool,
}

/// Decides what to open. `requested` is the command-line argument,
/// `requested_exists` says whether it names an existing file, `demo_forced`
/// comes from `STOKESFORGE_DEMO=1`.
pub fn plan(
    requested: Option<&Path>,
    requested_exists: bool,
    demo_forced: bool,
    preferences: &Preferences,
) -> Startup {
    let target = requested.map(Path::to_path_buf).unwrap_or_default();
    if let (Some(path), true) = (requested, requested_exists) {
        return Startup {
            source: StartupSource::Load(path.to_path_buf()),
            example_script: false,
            mark_first_run: false,
        };
    }
    if demo_forced {
        return Startup {
            source: StartupSource::DemoMemory(target),
            example_script: true,
            mark_first_run: false,
        };
    }
    if requested.is_some() || preferences.first_run_done() {
        return Startup {
            source: StartupSource::Empty(target),
            example_script: false,
            mark_first_run: false,
        };
    }
    // First launch: the file in the configuration folder, or, with no
    // configuration folder, the in-memory demo (the marker cannot be
    // written: it will come back).
    let source = match preferences.demo_file_path() {
        Some(file) => StartupSource::DemoFile(file),
        None => StartupSource::DemoMemory(PathBuf::new()),
    };
    Startup {
        source,
        example_script: true,
        mark_first_run: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preferences(done: bool) -> (tempfile::TempDir, Preferences) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            if done {
                r#"{"first_run_done":true}"#
            } else {
                "{}"
            },
        )
        .unwrap();
        (dir, Preferences::load_from(Some(path)))
    }

    #[test]
    fn without_argument_or_marker_the_demo_opens_from_its_file() {
        let (dir, prefs) = preferences(false);
        let startup = plan(None, false, false, &prefs);
        assert_eq!(
            startup.source,
            StartupSource::DemoFile(dir.path().join("demo.stf"))
        );
        assert!(startup.example_script);
        assert!(startup.mark_first_run);
    }

    #[test]
    fn with_the_marker_the_project_is_empty() {
        let (_dir, prefs) = preferences(true);
        let startup = plan(None, false, false, &prefs);
        assert_eq!(startup.source, StartupSource::Empty(PathBuf::new()));
        assert!(!startup.example_script);
        assert!(!startup.mark_first_run);
    }

    #[test]
    fn the_forced_demo_ignores_the_marker_and_sets_none() {
        let (_dir, prefs) = preferences(true);
        let startup = plan(None, false, true, &prefs);
        assert_eq!(startup.source, StartupSource::DemoMemory(PathBuf::new()));
        assert!(startup.example_script);
        assert!(!startup.mark_first_run);
    }

    #[test]
    fn without_a_configuration_folder_the_demo_stays_in_memory() {
        let prefs = Preferences::load_from(None);
        let startup = plan(None, false, false, &prefs);
        assert_eq!(startup.source, StartupSource::DemoMemory(PathBuf::new()));
        assert!(startup.example_script);
    }

    #[test]
    fn an_existing_path_is_always_loaded_even_if_the_demo_is_forced() {
        let (_dir, prefs) = preferences(false);
        let path = Path::new("project.stf");
        let startup = plan(Some(path), true, true, &prefs);
        assert_eq!(startup.source, StartupSource::Load(path.to_path_buf()));
        assert!(!startup.example_script);
        assert!(!startup.mark_first_run);
    }

    #[test]
    fn a_missing_path_gives_an_empty_project_targeting_it_with_no_marker() {
        let (_dir, prefs) = preferences(false);
        let path = Path::new("new.stf");
        let startup = plan(Some(path), false, false, &prefs);
        assert_eq!(startup.source, StartupSource::Empty(path.to_path_buf()));
        assert!(!startup.mark_first_run);
    }
}
