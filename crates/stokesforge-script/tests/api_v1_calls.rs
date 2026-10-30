//! Freezing the `pyst` API (#331, batch 1): English parameters, `redo`, `save` /
//! `save_as`, `export_graph` and `analysis.readiness`.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, NodeId, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
    a: NodeId,
}

/// `[Book1]Sheet1`: `A` = 1, 2, 3 and `B` = 4, 5, 6.
fn bench() -> Bench {
    let mut project = Project::new("Freeze").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[4.0, 5.0, 6.0]))
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
    Bench {
        engine,
        session,
        _actor: actor,
        a,
    }
}

fn run(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

fn floats(bench: &Bench, id: NodeId) -> Vec<f64> {
    bench
        .session
        .snapshot()
        .unwrap()
        .tree
        .node(id)
        .unwrap()
        .as_column()
        .unwrap()
        .float_values()
        .unwrap()
        .as_ref()
        .clone()
}

const HANDLES: &str = "a = pyst.resolve('[Book1]Sheet1!A')\nb = pyst.resolve('[Book1]Sheet1!B')\n";

/// The script must raise an exception of type `kind`; returns its message.
fn refused(bench: &Bench, kind: &str, body: &str) -> String {
    refused_after(bench, kind, "", body)
}

/// Like [`refused`], with a `setup` outside the `try`: an error in the setup
/// must not pass for the expected refusal.
fn refused_after(bench: &Bench, kind: &str, setup: &str, body: &str) -> String {
    let source = format!(
        "{HANDLES}{setup}try:\n    {body}\nexcept pyst.{kind} as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no {kind}')\nprint(caught)"
    );
    run(bench, &source).stdout.trim().to_owned()
}

#[test]
fn parameters_are_in_english() {
    let b = bench();
    run(
        &b,
        r#"
import numpy as np
h = pyst.resolve(path='[Book1]Sheet1!A')
assert list(pyst.column(handle=h)) == [1.0, 2.0, 3.0]
pyst.set_column(handle=h, values=np.array([7.0, 8.0, 9.0]))
assert list(pyst.column(h)) == [7.0, 8.0, 9.0]
for call in (
    lambda: pyst.resolve(chemin='[Book1]Sheet1!A'),
    lambda: pyst.column(poignee=h),
    lambda: pyst.set_column(h, valeurs=np.array([1.0, 2.0, 3.0])),
):
    try:
        call()
    except TypeError:
        pass
    else:
        raise AssertionError('French keyword must be refused')
"#,
    );
    assert_eq!(floats(&b, b.a), vec![7.0, 8.0, 9.0]);
}

#[test]
fn redo_restores_what_undo_undid() {
    let b = bench();
    run(
        &b,
        &format!(
            "{HANDLES}assert pyst.redo() is False\n\
             pyst.set_values(a, [9.0, 9.0, 9.0])\n\
             assert pyst.undo() is True\n\
             assert list(pyst.values(a)) == [1.0, 2.0, 3.0]\n\
             assert pyst.redo() is True\n\
             assert list(pyst.values(a)) == [9.0, 9.0, 9.0]\n\
             assert pyst.redo() is False\n"
        ),
    );
    assert_eq!(floats(&b, b.a), vec![9.0, 9.0, 9.0]);
}

#[test]
fn save_without_a_path_is_refused_then_save_as_sets_the_path() {
    let b = bench();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("project.stf");
    let message = refused(&b, "InvalidValueError", "pyst.save()");
    assert!(message.contains("save_as"), "{message}");
    assert!(!file.exists());

    run(
        &b,
        &format!(
            "{HANDLES}pyst.save_as({file:?})\n\
             assert pyst.info(a)['name'] == 'A'\n"
        ),
    );
    assert!(file.metadata().unwrap().len() > 0);
    assert_eq!(b.engine.project_path().as_deref(), Some(file.as_path()));

    // `save()` now writes to the retained path: deleting the file proves it,
    // since only a save recreates it.
    std::fs::remove_file(&file).unwrap();
    run(
        &b,
        &format!("{HANDLES}pyst.save()\nassert pyst.info(a)['name'] == 'A'"),
    );
    assert!(file.exists());
    // The file is a readable project, with the current data.
    b.session.load(&file).unwrap();
    let snapshot = b.session.snapshot().unwrap();
    let sheet = snapshot.tree.resolve("[Book1]Sheet1!A").unwrap();
    assert_eq!(
        snapshot
            .tree
            .node(sheet)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_ref(),
        &vec![1.0, 2.0, 3.0]
    );
}

#[test]
fn save_uses_the_path_set_by_the_host_and_commits_the_group() {
    let b = bench();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("host.stf");
    b.engine.set_project_path(Some(file.clone()));
    // The write precedes `save()` in the same group: without committing the
    // group, the session would refuse to write (open macro). A later error
    // no longer undoes what was committed.
    let outcome = b
        .engine
        .run(&format!(
            "{HANDLES}pyst.set_values(a, [5.0, 5.0, 5.0])\npyst.save()\nraise RuntimeError('after')"
        ))
        .unwrap();
    assert!(outcome.error.is_some());
    assert!(file.exists());
    assert_eq!(floats(&b, b.a), vec![5.0, 5.0, 5.0]);
    b.session.load(&file).unwrap();
    assert_eq!(floats(&b, b.a), vec![5.0, 5.0, 5.0]);
}

fn graph(b: &Bench) -> &'static str {
    let _ = b;
    "g = pyst.plot(b, a, name='Graph1', title='Title')\n"
}

#[test]
fn export_graph_writes_svg_pdf_and_png_like_the_application() {
    let b = bench();
    let dir = tempfile::tempdir().unwrap();
    let (svg, pdf, png, png1) = (
        dir.path().join("f.svg"),
        dir.path().join("f.PDF"),
        dir.path().join("f.png"),
        dir.path().join("f1.png"),
    );
    run(
        &b,
        &format!(
            "{HANDLES}{}pyst.export_graph(g, {svg:?})\n\
             pyst.export_graph(g, {pdf:?})\n\
             pyst.export_graph(g, {png:?})\n\
             pyst.export_graph(g, {png1:?}, png_scale=1.0)\n",
            graph(&b)
        ),
    );
    // Same rendering as the application: same bytes as `export_bytes` on the
    // graph spec, default size and text mode.
    let snapshot = b.session.snapshot().unwrap();
    let node = snapshot.tree.resolve("[Graph1]").unwrap();
    let spec = match snapshot.tree.node(node).unwrap().kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        other => panic!("{other:?}"),
    };
    let plot = stokesforge_core::resolve::resolve_spec(&snapshot.tree, &spec);
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    // The page is the size of the figure (#394): the cells of the new graph
    // (480 x 360 pt) plus the band of its title.
    let size = stokesforge_render_svg::figure_size(&spec, &measurer, &choice);
    assert_eq!(size.0, 480);
    assert!(size.1 > 360, "the title adds its band");
    let expected = stokesforge_render_svg::export_bytes(
        &plot,
        size,
        &measurer,
        &choice,
        stokesforge_render_svg::ExportFormat::Svg {
            text: stokesforge_render_svg::SvgTextMode::Text,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&svg).unwrap(), expected);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let big = std::fs::read(&png).unwrap();
    let small = std::fs::read(&png1).unwrap();
    assert!(big.starts_with(b"\x89PNG"));
    let width = |bytes: &[u8]| u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(width(&big), 4 * width(&small), "png_scale enlarges the PNG");
}

#[test]
fn export_graph_refuses_an_unknown_extension_a_zero_scale_and_a_non_graph() {
    let b = bench();
    let dir = tempfile::tempdir().unwrap();
    let jpg = dir.path().join("f.jpg");
    let (png, svg) = (dir.path().join("f.png"), dir.path().join("f.svg"));
    let setup = graph(&b);
    let message = refused_after(
        &b,
        "InvalidValueError",
        setup,
        &format!("pyst.export_graph(g, {jpg:?})"),
    );
    assert!(message.contains(".svg"), "{message}");
    assert!(!jpg.exists());
    // Also refused for a format that ignores the scale (SVG): a wrong
    // parameter is not silently tolerated.
    for path in [&png, &svg] {
        let message = refused_after(
            &b,
            "InvalidValueError",
            "",
            &format!("pyst.export_graph(g, {path:?}, png_scale=0.0)"),
        );
        assert!(message.contains("png_scale"), "{message}");
        assert!(!path.exists());
    }
    let message = refused(
        &b,
        "InvalidValueError",
        &format!("pyst.export_graph(a, {svg:?})"),
    );
    assert!(message.contains("not a graph"), "{message}");
    assert!(!svg.exists());
}

#[test]
fn readiness_says_what_is_missing_without_writing_anything() {
    let b = bench();
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        "nan = float('nan')\n\
         r = pyst.analysis.readiness('baseline', method='line', points=[(0., 0.), (nan, 0.)])\n\
         assert r['ready'] is False and r['reason'] == 'needs_points', r\n\
         assert (r['needed'], r['have']) == (2, 1), r\n\
         assert pyst.analysis.readiness('baseline', method='line', points=[(0., 0.), (1., 1.)])['ready'] is True\n\
         r = pyst.analysis.readiness('baseline', method='zones', zones=[(nan, 1.)])\n\
         assert r['reason'] == 'needs_zone', r\n\
         r = pyst.analysis.readiness('integrate', ranges=[(nan, 1.)])\n\
         assert r['reason'] == 'needs_zone', r\n\
         assert pyst.analysis.readiness('integrate')['ready'] is True\n\
         r = pyst.analysis.readiness('fit', model='a*')\n\
         assert r['reason'] == 'bad_expression' and r['message'], r\n\
         assert pyst.analysis.readiness('fit')['ready'] is True\n\
         r = pyst.analysis.readiness('fit', model='1+2')\n\
         assert r['reason'] == 'no_parameter', r\n\
         assert pyst.analysis.readiness('peaks')['ready'] is True\n",
    );
    for body in [
        "pyst.analysis.readiness('fit', modle='linear')",
        "pyst.analysis.readiness('peaks', prominence=0.1)",
        "pyst.analysis.readiness('nothing')",
        "pyst.analysis.readiness('integrate', ranges='x')",
    ] {
        refused(&b, "InvalidValueError", body);
    }
    // Read-only: neither project nor history touched.
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}
