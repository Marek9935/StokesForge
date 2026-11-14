//! Shared bench of the tests of the import window: a state with one file,
//! the configuration folder redirected, and the helpers that paint the window
//! and read what it painted.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use eframe::egui;
use stokesforge_core::Project;
use stokesforge_session::Session;

use super::import_window;
use stokesforge_app_state::state::{Action, AppState};

use crate::app::test_support::{click, image, key, text_exact, texts, Text};

/// Serializes the tests that open the import: the configuration folder is
/// *process* state, and `cargo test` runs the tests in parallel.
static CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Redirects the configuration folder (preferences, templates) to a temporary
/// folder as long as the value lives, so that no test depends on the
/// workstation or writes to it.
pub(super) struct IsolatedConfig {
    before: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _directory: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl Drop for IsolatedConfig {
    fn drop(&mut self) {
        for (name, value) in self.before.iter_mut() {
            match value.take() {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn isolate_the_config() -> IsolatedConfig {
    let guard = CONFIG_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = tempfile::tempdir().unwrap();
    let names = [
        stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
        "XDG_CONFIG_HOME",
    ];
    let before: Vec<_> = names
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    for name in names {
        std::env::set_var(name, directory.path());
    }
    IsolatedConfig {
        before,
        _directory: directory,
        _guard: guard,
    }
}

/// A recognised JASCO V-770 file, two curves under an x' line-free header: no
/// doubt.
pub(super) const SURE: &str = "TITLE;S13\nORIGIN;JASCO\nSPECTROMETER/DATA SYSTEM;JASCO Corp., V-770, Rev. 1.00\nDATA TYPE;ULTRAVIOLET SPECTRUM\nXUNITS;NANOMETERS\nYUNITS;ABSORBANCE\nXYDATA\n800;1;2\n799;2;3\n798;3;4\n797;4;5\n";

/// Five curves under a line of times: the times and the single name of five
/// curves are the doubts.
pub(super) const DOUBTS: &str = "TITLE;S13\nORIGIN;JASCO\nSPECTROMETER/DATA SYSTEM;JASCO Corp., V-770, Rev. 1.00\nDATA TYPE;ULTRAVIOLET SPECTRUM\nXUNITS;NANOMETERS\nYUNITS;ABSORBANCE\nXYDATA\n;0;15;30;45;60\n800;1;2;3;4;5\n799;2;3;4;5;6\n798;3;4;5;6;7\n797;4;5;6;7;8\n";

/// A synthetic kinetic of the size of the real one: 630 lines, 241 columns (an
/// x column and 240 curves under a line of times).
pub(super) fn kinetic_630_by_241() -> String {
    let mut text = String::from(
        "TITLE;Kinetic\nORIGIN;JASCO\nSPECTROMETER/DATA SYSTEM;JASCO Corp., V-770, Rev. 1.00\nDATA TYPE;ULTRAVIOLET SPECTRUM\nXUNITS;NANOMETERS\nYUNITS;ABSORBANCE\nXYDATA\n",
    );
    text.push_str(&format!(
        ";{}\n",
        (0..240)
            .map(|k| (15 * k).to_string())
            .collect::<Vec<_>>()
            .join(";")
    ));
    for row in 0..(630 - 8) {
        text.push_str(&format!(
            "{};{}\n",
            900 - row,
            (0..240)
                .map(|k| format!("{:.4}", 0.1 + 0.001 * k as f32))
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    text
}

pub(super) struct Bench {
    pub(super) state: AppState,
    pub(super) files: Vec<PathBuf>,
    _session: Session,
    _directory: tempfile::TempDir,
    _config: IsolatedConfig,
}

/// The import open on one file with `content` (none when it is empty).
pub(super) fn bench(content: &str) -> Bench {
    let config = isolate_the_config();
    let session = Session::spawn(Project::new("Import").unwrap(), None).unwrap();
    let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut files: Vec<PathBuf> = Vec::new();
    if !content.is_empty() {
        let file = directory.path().join("jasco.csv");
        std::fs::write(&file, content).unwrap();
        files.push(file);
    }
    state.open_import(None::<stokesforge_core::NodeId>);
    state.set_import_paths(files.clone());
    Bench {
        state,
        files,
        _session: session,
        _directory: directory,
        _config: config,
    }
}

pub(super) fn paint(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    events: Vec<egui::Event>,
) -> Vec<Text> {
    texts(&image(ctx, events, |ui| {
        import_window(ui.ctx(), state, actions)
    }))
}

/// Paints until the layout settles (the guide is still open).
pub(super) fn settle(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) -> Vec<Text> {
    for _ in 0..3 {
        paint(ctx, state, actions, Vec::new());
    }
    paint(ctx, state, actions, Vec::new())
}

pub(super) fn click_text(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    wanted: &str,
) {
    let painted = paint(ctx, state, actions, Vec::new());
    let at = text_exact(&painted, wanted).center;
    click_at(ctx, state, actions, at);
}

pub(super) fn click_at(
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    at: egui::Pos2,
) {
    click(ctx, at, egui::PointerButton::Primary, |ui| {
        import_window(ui.ctx(), state, actions)
    });
}

pub(super) fn enter() -> egui::Event {
    key(egui::Key::Enter)
}

pub(super) fn escape() -> egui::Event {
    key(egui::Key::Escape)
}
