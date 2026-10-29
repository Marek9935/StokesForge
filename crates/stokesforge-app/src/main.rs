//! Binary of the test bench: starts the session actor, builds the state and
//! opens the window.
//!
//! Usage: `stokesforge-app [path]`, or `stokesforge-app --version` (`-V`),
//! which prints `stokesforge-app X.Y.Z` and exits (#382). If `path` exists, the
//! project is loaded from that file (and marked saved); otherwise an empty project is built and
//! `path` simply becomes the target of a future save.
//!
//! Without argument (#333): at the **very first launch**, the demo is written
//! to `demo.stf`, next to the preferences, and opened from that file; then an
//! empty project, like File > New. The decision is
//! `stokesforge_app_state::startup::plan`. `STOKESFORGE_DEMO=1` keeps the demo
//! in memory, without a path, at every launch (development, captures): the
//! first Ctrl+S then opens the picker (#315).
//!
//! The demo project lives in the [`demo`] module (code, not a file): `Book1`
//! and `Graph1` for the example script, then workbooks of column types, a
//! folder of analyses (with a few calculations already done) and six graphs
//! that exercise plotting. Its formula columns get their values only after the
//! recalculation of `demo_session`.

// Windows, release: windowed subsystem, no black console behind the window
// (#332). `console_windows::attach_parent` gives the messages back to the
// terminal that launched the application, if there is one.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(all(windows, not(debug_assertions)))]
mod console_windows;
mod demo;

use std::path::PathBuf;

use anyhow::Context as _;
use stokesforge_app::app::StokesForgeApp;
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::startup::{self, StartupSource};
use stokesforge_app_state::state::AppState;
use stokesforge_core::Project;
use stokesforge_session::{LoadReport, Session};

/// Example script that pre-fills the script area: it works as is on the demo
/// project ([`demo::demo_project`]) and illustrates the end criterion of the
/// foundation (copy-free read of a column). Shown even without the `python`
/// feature: a test bench must show what is missing, not hide it.
const EXAMPLE_SCRIPT: &str = "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
v = pyst.column(a)\n\
print(\"length:\", len(v), \"first:\", v[0])\n";

fn main() -> anyhow::Result<()> {
    #[cfg(all(windows, not(debug_assertions)))]
    console_windows::attach_parent();
    // Before anything else: asking for the version opens no window and, above
    // all, does not write the first-launch demo (#382).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(line) = startup::version_request(&args) {
        println!("{line}");
        return Ok(());
    }
    let requested = std::env::args().nth(1).map(PathBuf::from);
    let mut preferences = Preferences::load();
    let demo_forced = std::env::var_os("STOKESFORGE_DEMO").is_some_and(|value| value == "1");
    let startup = startup::plan(
        requested.as_deref(),
        requested.as_ref().is_some_and(|path| path.exists()),
        demo_forced,
        &preferences,
    );
    let (session, path, report) = session_for_startup(&startup.source, preferences.load_limits())?;
    let mut state = AppState::new(session.handle(), path.display().to_string())
        .context("first snapshot of the project")?;
    if let Some(report) = &report {
        state.mark_loaded(report);
    }
    // Backup copies of vanished processes (#317): only the root window offers
    // them.
    state.scan_recovery();
    // The forced demo is disposable: no question on close. An existing project
    // passed as argument keeps the question.
    state.set_quit_without_prompt(
        demo_forced && matches!(startup.source, StartupSource::DemoMemory(_)),
    );
    if startup.example_script {
        state.set_script_source(EXAMPLE_SCRIPT.to_string());
    }
    configure_script_engine(&mut state);
    if startup.mark_first_run {
        // A failure (no configuration folder, two first launches at the same
        // time) is harmless: the demo will come back.
        let _ = preferences.save_first_run_done();
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(state.window_title())
            .with_inner_size([1100.0, 700.0]),
        event_loop_builder: Some(Box::new(configure_window_backend)),
        // egui-wgpu queues two frames by default (maximum throughput); a
        // single one (low latency) removes a frame of delay between the
        // pointer and what is painted — noticeable when moving a window, which
        // nothing masks. The cost of a frame is ~0.3 ms in release
        // (`window_move_measurement`): throughput is not what is missing.
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default()
            .with_surface_config(eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY),
        ..Default::default()
    };
    // `run_native` blocks until the window is closed; `session` thus stays
    // alive for the whole duration of the application.
    let outcome = eframe::run_native(
        "stokesforge-app",
        options,
        Box::new(|cc| {
            // Task R4.1: explicitly set **the** project's font definitions on
            // the context, rather than counting on egui's defaults being the
            // same. This is what makes the screen/export agreement a property
            // and not a coincidence — if egui changed its default, the figure
            // would stay the same on both sides.
            cc.egui_ctx
                .set_fonts(stokesforge_render_egui::font_definitions());
            // House theme, light, independent of the system theme (see the top
            // doc of `stokesforge_app::theme`) — set here, once, next to the
            // fonts: both belong to the same gesture ("what the app imposes on
            // the `Context` before the first rendering").
            stokesforge_app::theme::install(&cc.egui_ctx);
            let mut app = StokesForgeApp::new(state);
            app.load_toolbar_layout(&stokesforge_app_state::preferences::Preferences::load());
            // The remembered layout of the panels, once, before the first frame (#148).
            app.apply_startup_panels(cc.winit_window().map(|w| w.available_monitors().count()));
            Ok(Box::new(app))
        }),
    );

    // The clean shutdown of the actor comes before reporting a window error:
    // without it, the thread would outlive the process exit.
    let shutdown = session.shutdown();
    outcome.map_err(|err| anyhow::anyhow!("window failure: {err}"))?;
    shutdown.map_err(|err| anyhow::anyhow!("session shutdown: {err}"))?;
    Ok(())
}

/// Native Wayland by default (#72). The winit/smithay-clipboard fixes share
/// the drop receiver with the clipboard, for each viewport. STOKESFORGE_X11=1
/// keeps an X11/XWayland escape hatch; without override, winit picks Wayland
/// then X11 according to the available connections. The former
/// STOKESFORGE_WAYLAND is no longer needed.
#[cfg(target_os = "linux")]
fn configure_window_backend(builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    use winit::platform::x11::EventLoopBuilderExtX11;
    if std::env::var_os("STOKESFORGE_X11").is_some_and(|value| value == "1") {
        builder.with_x11();
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_window_backend(_builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {}

/// Builds the session according to the startup plan ([`startup::plan`]).
///
/// Also returns the read report if the project was **loaded** from a file
/// (`None` otherwise): only this case sets the "saved" marker, and the report
/// is passed to `mark_loaded` so that what was discarded is told (#309).
/// Without argument, the in-memory demo has no path, so a `demo.stf` in the
/// current folder is neither read nor overwritten (#315).
fn session_for_startup(
    source: &StartupSource,
    max_limits: stokesforge_app_state::preferences::LoadLimits,
) -> anyhow::Result<(Session, PathBuf, Option<LoadReport>)> {
    match source {
        StartupSource::Load(path) => load_session(path, max_limits),
        StartupSource::DemoMemory(target) => Ok((demo_session()?, target.clone(), None)),
        StartupSource::Empty(target) => {
            let project = Project::new(stokesforge_session::NEW_PROJECT_NAME)?;
            let session = Session::spawn(project, None).context("starting the session thread")?;
            Ok((session, target.clone(), None))
        }
        StartupSource::DemoFile(file) => {
            let session = demo_session()?;
            // Write then read back: the opened demo is the one of the file,
            // which the user will find again. A failure (folder that cannot be
            // created, full disk) leaves the demo in memory, without a path:
            // never a missed startup because of that.
            match write_and_reload_demo(&session, file, max_limits) {
                Ok(report) => Ok((session, file.clone(), Some(report))),
                Err(err) => {
                    eprintln!(
                        "[stokesforge-app] demo not saved in {}: {err:#}",
                        file.display()
                    );
                    Ok((session, PathBuf::new(), None))
                }
            }
        }
    }
}

/// Loading goes through the actor so as to also restore the geometries, kept
/// next to the model. Loading only `Project` would lose them.
fn load_session(
    path: &std::path::Path,
    max_limits: stokesforge_app_state::preferences::LoadLimits,
) -> anyhow::Result<(Session, PathBuf, Option<LoadReport>)> {
    let session =
        Session::spawn(Project::new("Loading")?, None).context("starting the session thread")?;
    let (_, report) = session
        .handle()
        .load_with_limits(path, max_limits)
        .with_context(|| format!("loading {}", path.display()))?;
    Ok((session, path.to_path_buf(), Some(report)))
}

/// The in-memory demo, its formula columns computed.
fn demo_session() -> anyhow::Result<Session> {
    let session =
        Session::spawn(demo::demo_project()?, None).context("starting the session thread")?;
    // The demo's formula columns carry only their formula (invariant 11): a
    // full recalculation, right away, gives them their values before the first
    // snapshot, and before being saved.
    session
        .handle()
        .recalculate(&[])
        .context("recalculation of the demo")?;
    Ok(session)
}

fn write_and_reload_demo(
    session: &Session,
    file: &std::path::Path,
    max_limits: stokesforge_app_state::preferences::LoadLimits,
) -> anyhow::Result<LoadReport> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("creating {}", folder.display()))?;
    }
    session.handle().save(file).context("saving")?;
    let (_, report) = session
        .handle()
        .load_with_limits(file, max_limits)
        .context("reading back")?;
    Ok(report)
}

/// Locates the embedded interpreter and sets it on `state`, **without starting
/// it** — `EmbeddedPython::locate`/`with_home` do not touch CPython, only the
/// first script does (see `stokesforge_app_state::state`, section "The Python
/// script"). It is deliberately the binary, not `AppState`, that decides where
/// the interpreter comes from: `AppState` stays independent of the layout of a
/// packaged distribution.
///
/// A missing prefix (dev without `STOKESFORGE_PYTHON_HOME`, or unpackaged
/// binary) is **not** fatal: the window opens anyway, the script console stays
/// visible and [`AppState::script_available`] stays true (the bridge is
/// compiled) — the absence of an interpreter is reported at the first attempt
/// to run, like any other error of this bench.
#[cfg(feature = "python")]
fn configure_script_engine(state: &mut AppState) {
    match stokesforge_python::interpreter::EmbeddedPython::locate() {
        Ok(python) => state.set_python(python),
        Err(err) => {
            eprintln!("[stokesforge-app] Python bridge compiled, but interpreter not found: {err}");
        }
    }
}

/// Without the `python` feature, there is nothing to configure: `AppState` has
/// no field for an interpreter at all.
#[cfg(not(feature = "python"))]
fn configure_script_engine(_state: &mut AppState) {}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use stokesforge_core::Column;
    use stokesforge_session::WindowGeometry;

    #[test]
    fn startup_restores_the_geometries_of_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("geometries.stf");
        let mut project = Project::new("Saved layout").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let geometry = WindowGeometry {
            x: 42.0,
            y: 67.0,
            width: 520.0,
            height: 380.0,
            view: None,
        };
        let original = Session::spawn(project, None).unwrap();
        original
            .handle()
            .set_window_geometry(book, geometry)
            .unwrap();
        original.handle().save(&path).unwrap();
        original.shutdown().unwrap();

        let (loaded, actual_path, report) = session_for_startup(
            &StartupSource::Load(path.clone()),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        assert!(
            report.is_some_and(|r| r.is_clean()),
            "an existing file is loaded"
        );
        let snapshot = loaded.handle().snapshot().unwrap();
        assert_eq!(actual_path, path);
        assert_eq!(snapshot.window_geometries.get(&book), Some(&geometry));
        assert!(snapshot.tree.node(book).is_some());
        loaded.shutdown().unwrap();
    }

    /// #325: a column width saved in the `.stf` is the one of the snapshot at
    /// startup (same path as the geometries).
    #[test]
    fn startup_restores_the_column_widths_of_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("widths.stf");
        let mut project = Project::new("Widths").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let column = project
            .add_column(
                sheet,
                None,
                Some("A"),
                stokesforge_core::Column::from_f64(&[1.0]),
            )
            .unwrap();
        let original = Session::spawn(project, None).unwrap();
        original.handle().set_column_width(column, 91.5).unwrap();
        original.handle().save(&path).unwrap();
        original.shutdown().unwrap();

        let (loaded, _, _) = session_for_startup(
            &StartupSource::Load(path),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        let snapshot = loaded.handle().snapshot().unwrap();
        assert_eq!(snapshot.column_widths.get(&column), Some(&91.5));
        loaded.shutdown().unwrap();
    }

    fn limits() -> stokesforge_app_state::preferences::LoadLimits {
        stokesforge_app_state::preferences::default_load_limits()
    }

    fn demo_column_resolves(session: &Session) -> bool {
        let snapshot = session.handle().snapshot().unwrap();
        snapshot.tree.resolve("[Book1]Sheet1!A").is_ok()
    }

    /// #315: the forced demo (`STOKESFORGE_DEMO=1`) has no path and is not
    /// "loaded": a `demo.stf` in the current folder must be neither taken for
    /// the project nor overwritten without a check.
    #[test]
    fn the_in_memory_demo_has_neither_path_nor_loaded_marker() {
        let (session, path, report) =
            session_for_startup(&StartupSource::DemoMemory(PathBuf::new()), limits()).unwrap();
        assert!(path.as_os_str().is_empty());
        assert!(report.is_none());
        assert!(demo_column_resolves(&session));
        session.shutdown().unwrap();
    }

    /// #333: an empty project contains nothing, not even a workbook: it is the
    /// one of File > New.
    #[test]
    fn the_empty_project_has_no_workbook_and_targets_its_path() {
        let target = PathBuf::from("absent.stf");
        let (session, path, report) =
            session_for_startup(&StartupSource::Empty(target.clone()), limits()).unwrap();
        assert_eq!(
            path, target,
            "a missing path is the target of a future save"
        );
        assert!(report.is_none(), "this is not a load");
        let tree = session.handle().snapshot().unwrap().tree;
        assert!(tree.children(tree.root()).unwrap().is_empty());
        session.shutdown().unwrap();
    }

    /// #333: the first launch writes the demo into the configuration folder and
    /// opens it from that file, read back without discarding anything.
    #[test]
    fn the_first_launch_demo_is_written_then_opened_from_its_file() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("config").join("demo.stf");
        let (session, path, report) =
            session_for_startup(&StartupSource::DemoFile(file.clone()), limits()).unwrap();
        assert!(file.is_file(), "demo.stf is written");
        assert_eq!(path, file);
        assert!(report.is_some_and(|r| r.is_clean()));
        assert!(demo_column_resolves(&session));
        session.shutdown().unwrap();
    }

    /// #333, invariant 10: if `demo.stf` cannot be written, the demo stays open
    /// in memory, without a path, and the startup does not fail.
    #[test]
    fn a_demo_that_cannot_be_written_stays_in_memory_without_a_path() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("config");
        std::fs::write(&blocker, b"a file holds the place of the folder").unwrap();
        let file = blocker.join("demo.stf");
        let (session, path, report) =
            session_for_startup(&StartupSource::DemoFile(file), limits()).unwrap();
        assert!(path.as_os_str().is_empty());
        assert!(report.is_none());
        assert!(demo_column_resolves(&session));
        session.shutdown().unwrap();
    }

    /// #309: the report of the startup read is returned to the caller; a
    /// discarded calculation is not lost in it.
    #[test]
    fn startup_returns_the_report_of_discarded_calculations() {
        use stokesforge_analysis::correct::CorrectOp;
        use stokesforge_analysis::ops::Operation;
        use stokesforge_core::{
            CalculationDraft, ColumnMeta, OutputDraft, SeriesRef, SeriesResult,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("discarded.stf");
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let y = project
            .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        project
            .add_calculation(CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Subtract,
                },
                series: vec![SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }],
                aux: None,
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![0.0, 1.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: false,
            })
            .unwrap();
        let writer = Session::spawn(project, None).unwrap();
        writer.handle().save(&path).unwrap();
        writer.shutdown().unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes
            .windows(8)
            .position(|w| w == b"subtract")
            .expect("the operation is in the file");
        bytes[at..at + 8].copy_from_slice(b"subtraXX");
        std::fs::write(&path, bytes).unwrap();

        let (session, _, report) = session_for_startup(
            &StartupSource::Load(path),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        let report = report.expect("file loaded");
        assert!(!report.is_clean());
        assert_eq!(report.discarded_calculations.len(), 1);
        session.shutdown().unwrap();
    }
}
