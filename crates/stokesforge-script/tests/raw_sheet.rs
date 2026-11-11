//! A script that writes the value of an imported column renames the "Raw Data"
//! sheet like the interface does, inside the script group (#488).
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_commands::{HistoryLabel, MacroKind};
use stokesforge_core::{Column, ImportSource, Project, RawSheetMark};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::Session;

#[test]
fn a_pyst_write_renames_the_sheet_and_the_old_path_stops_resolving() {
    let mut project = Project::new("Raw").unwrap();
    let book = project.add_workbook(project.root(), Some("x")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    project.rename(sheet, "Raw Data").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let mut meta = project
        .tree()
        .node(a)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.import_source = Some(ImportSource {
        path: "/tmp/x.csv".into(),
        ..Default::default()
    });
    project.set_column_meta(a, meta).unwrap();
    project
        .set_raw_mark(
            sheet,
            Some(RawSheetMark {
                modified_base: "Sheet".into(),
            }),
        )
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(prefix)).unwrap());

    let outcome = engine
        .run("h = pyst.resolve('[x]Raw Data!A')\nassert len(pyst.values(h)) == 2\n")
        .unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(
        !session.snapshot().unwrap().history.can_undo,
        "a read renames nothing"
    );

    let outcome = engine
        .run("pyst.set_values(pyst.resolve('[x]Raw Data!A'), [7.])\n")
        .unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.tree.node(sheet).unwrap().name(), "Sheet1");
    assert_eq!(
        snapshot.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    let outcome = engine
        .run("pyst.resolve('[x]Sheet1!A')\ntry:\n    pyst.resolve('[x]Raw Data!A')\n    raise SystemExit('still resolves')\nexcept SystemExit:\n    raise\nexcept Exception:\n    pass\n")
        .unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    session.undo().unwrap();
    assert_eq!(
        session.snapshot().unwrap().tree.node(sheet).unwrap().name(),
        "Raw Data"
    );
}
