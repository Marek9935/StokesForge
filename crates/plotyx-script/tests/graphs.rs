//! Graphes natifs construits par Python : mêmes identités, spec et export.
#![cfg(feature = "python")]

use plotyx_core::{Column, NodeId, NodeKind, Project};
use plotyx_plot::{AxisScale, LineStyle, PlotSpec, PlotType, SeriesAxis, SeriesSpec, SymbolShape};
use plotyx_python::interpreter::{self, EmbeddedPython};
use plotyx_script::ScriptEngine;
use plotyx_session::{Session, SessionHandle};
use std::path::PathBuf;

struct Banc {
    engine: ScriptEngine,
    session: SessionHandle,
    _actor: Session,
    x: NodeId,
    y: NodeId,
}
fn banc() -> Banc {
    let mut project = Project::new("Graphes").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1., 2., 3.]))
        .unwrap();
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[2., 4., 8.]))
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let home = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home)).unwrap();
    Banc {
        engine,
        session,
        _actor: actor,
        x,
        y,
    }
}
fn run(b: &Banc, source: &str) {
    let out = b.engine.run(source).unwrap();
    assert!(out.error.is_none(), "{:?}", out.error);
}
fn graph_spec(b: &Banc) -> PlotSpec {
    let snap = b.session.snapshot().unwrap();
    let id = snap.tree.resolve("[Graph1]").unwrap();
    match snap.tree.node(id).unwrap().kind() {
        NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("graphe attendu"),
    }
}
const HANDLES: &str = "x = pyx.resolve('[Book1]Sheet1!X')\ny = pyx.resolve('[Book1]Sheet1!Y')\n";

#[test]
fn spec_exact_export_stable_et_temoin_de_style() {
    let b = banc();
    let before = b.session.snapshot().unwrap();
    run(&b, &format!("{HANDLES}g = pyx.plot(y, x, name='Graph1', plot_type='line_scatter', color='#12345680', line_style='dash', line_width=2.5, symbol='square', symbol_size=9., title='$alpha$', x_title='Temps (s)', y_title='$I$', x_scale='log', x_range=(1., 10.), label='Signal')"));
    let mut expected = PlotSpec {
        title: "$alpha$".into(),
        ..Default::default()
    };
    let panel = expected.panels.get_mut(0).unwrap();
    let mut series = SeriesSpec::new_xy(b.x, b.y);
    series.plot_type = PlotType::LineScatter;
    series.line.color = "#12345680".into();
    series.symbol.color = series.line.color.clone();
    series.line.style = LineStyle::Dash;
    series.line.width = 2.5;
    series.symbol.shape = SymbolShape::Square;
    series.symbol.size = 9.;
    series.name = "Signal".into();
    panel.series.push(series);
    panel.bottom.label = "Temps (s)".into();
    panel.bottom.scale = AxisScale::Log;
    panel.bottom.auto_range = false;
    panel.bottom.range_min = Some(1.);
    panel.bottom.range_max = Some(10.);
    panel.left.label = "$I$".into();
    assert_eq!(graph_spec(&b), expected);
    let snapshot = b.session.snapshot().unwrap();
    let measurer = plotyx_render::TextMeasurer::default();
    let render = |spec: &PlotSpec| {
        plotyx_render_svg::render_spec(&snapshot.tree, spec, (640, 480), &measurer).unwrap()
    };
    let svg = render(&graph_spec(&b));
    assert_eq!(svg, render(&expected));
    assert_eq!(svg, render(&expected));
    let mut witness = expected.clone();
    witness.panels.get_mut(0).unwrap().series[0].line.color = "#ff0000".into();
    assert_ne!(svg, render(&witness), "le témoin doit modifier la figure");
    let rich = plotyx_render_svg::render_to_string_with_typst(
        &plotyx_core::resolve_spec(&snapshot.tree, &expected),
        (640, 480),
        &measurer,
        &plotyx_render::FontChoice::default(),
    )
    .unwrap();
    assert_ne!(rich, svg, "les titres markup exercent bien Typst");
    b.session.undo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);
    b.session.redo().unwrap();
    assert_eq!(graph_spec(&b), expected);
}

#[test]
fn ajoute_sans_ecraser_les_series_panneaux_et_reglages() {
    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyx.plot(y, x, name='Graph1', title='Premier', x_title='Temps')"),
    );
    let mut first = graph_spec(&b);
    first.panels.push(plotyx_plot::PanelSpec::at(0, 1));
    let graph = b
        .session
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Graph1]")
        .unwrap();
    b.session
        .execute(plotyx_commands::Command::SetPlotSpec {
            graph,
            spec: first.clone(),
        })
        .unwrap();
    run(&b, "pyx.plot(y, graph=g, plot_type='scatter', axis='right', y_title='Second', y_scale='log', y_range=(1., 20.))");
    let second = graph_spec(&b);
    assert_eq!(second.panels.get(1), first.panels.get(1));
    let panel = second.panels.first().unwrap();
    assert_eq!(panel.series[0], first.panels.first().unwrap().series[0]);
    assert_eq!(second.title, first.title);
    assert_eq!(panel.bottom, first.panels.first().unwrap().bottom);
    assert_eq!(panel.series[1].axis, SeriesAxis::Right);
    assert_eq!(panel.series[1].x, None);
    assert_eq!(panel.series[1].line.color, plotyx_plot::palette_color(1));
    assert_eq!(panel.right.label, "Second");
    assert_eq!(panel.right.scale, AxisScale::Log);
    b.session.undo().unwrap();
    assert_eq!(graph_spec(&b), first);
    b.session.redo().unwrap();
    assert_eq!(graph_spec(&b), second);
}

#[test]
fn types_erreurs_histogrammes_et_cellules_mixtes_utilisent_le_modele() {
    let b = banc();
    run(
        &b,
        &format!(
            r#"{HANDLES}
g = pyx.plot(y, name='Graph1')
for kind in ('scatter', 'line_scatter', 'bar', 'histogram', 'error_bar'):
    pyx.plot(y, x, graph=g, plot_type=kind, x_error=x, y_error=y, histogram_bins=12)
t = pyx.create_column(pyx.info(y)['parent'], column_type='mixed')
pyx.set_values(t, [1., 'texte', None, 2.])
pyx.set_column_meta(t, long_name='#fonction_inconnue[x] $alpha$')
pyx.plot(t, graph=g)
"#
        ),
    );
    let spec = graph_spec(&b);
    let panel = spec.panels.first().unwrap();
    assert_eq!(panel.series.len(), 7);
    for (s, kind) in panel.series.iter().zip([
        PlotType::Line,
        PlotType::Scatter,
        PlotType::LineScatter,
        PlotType::Bar,
        PlotType::Histogram,
        PlotType::ErrorBar,
    ]) {
        assert_eq!(s.plot_type, kind);
    }
    assert_eq!(panel.series[5].y_error, Some(b.y));
    assert_eq!(panel.series[4].histogram_bins.get(), 12);
    assert!(
        panel.series[6].name.is_empty(),
        "le nom automatique reste littéral et suit le renommage"
    );
    let snap = b.session.snapshot().unwrap();
    let resolved = plotyx_core::resolve_spec(&snap.tree, &spec);
    assert_eq!(
        resolved.panels[0].series[6].label,
        "#fonction_inconnue[x] $alpha$"
    );
    let svg = plotyx_render_svg::render_spec(
        &snap.tree,
        &spec,
        (640, 480),
        &plotyx_render::TextMeasurer::default(),
    )
    .unwrap();
    assert!(svg.contains("#fonction_inconnue[x] $alpha$"));
    let with_typst = plotyx_render_svg::render_to_string_with_typst(
        &resolved,
        (640, 480),
        &plotyx_render::TextMeasurer::default(),
        &plotyx_render::FontChoice::default(),
    )
    .unwrap();
    // Typst compose aussi les textes littéraux. Leur compilation réussit,
    // alors que le même texte saisi comme markup déclenche le repli SVG brut.
    assert!(!with_typst.contains("#fonction_inconnue[x] $alpha$"));
    let mut typed = spec.clone();
    typed.panels.get_mut(0).unwrap().series[6].name = "#fonction_inconnue[x] $alpha$".into();
    let typed = plotyx_render_svg::render_to_string_with_typst(
        &plotyx_core::resolve_spec(&snap.tree, &typed),
        (640, 480),
        &plotyx_render::TextMeasurer::default(),
        &plotyx_render::FontChoice::default(),
    )
    .unwrap();
    assert!(typed.contains("#fonction_inconnue[x] $alpha$"));
    assert!(
        typed != with_typst,
        "le témoin doit interpréter le markup explicite"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graphe.plx");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    assert_eq!(graph_spec(&b), spec);
}

#[test]
fn refus_ne_modifient_ni_graphe_ni_historique() {
    let b = banc();
    run(&b, &format!("{HANDLES}g = pyx.plot(y, name='Graph1')"));
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
for opts in ({'plot_type':'inconnu'}, {'color':'red'}, {'symbol':'o'}, {'line_style':'--'}, {'line_width':float('nan')}, {'symbol_size':0}, {'histogram_bins':0}, {'histogram_bins':65537}, {'x_scale':'symlog'}, {'x_range':(2.,1.)}, {'y_scale':'log','y_range':(0.,2.)}, {'y_error':pyx.root()}, {'unknown':1}, {'parent':pyx.root()}, {'name':'Autre'}):
    try: pyx.plot(y, graph=g, **opts)
    except pyx.InvalidValueError: pass
    else: raise AssertionError('option invalide acceptée')
for cible in (pyx.root(), pyx.info(y)['parent']):
    try: pyx.plot(cible)
    except pyx.InvalidValueError: pass
    else: raise AssertionError('mauvaise cible acceptée')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn exception_annule_creation_et_ajouts_et_load_invalide_les_poignees() {
    let b = banc();
    let before = b.session.snapshot().unwrap();
    let out = b.engine.run(&format!("{HANDLES}g = pyx.plot(y, x, name='Graph1')\npyx.plot(y, graph=g)\nraise RuntimeError('échec')")).unwrap();
    assert!(out.error.unwrap().contains("échec"));
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);
    run(&b, "g = pyx.plot(y, x, name='Graph1')");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("epoch.plx");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
fresh_y = pyx.resolve('[Book1]Sheet1!Y')
for action in (lambda: pyx.plot(y), lambda: pyx.plot(fresh_y, x), lambda: pyx.plot(fresh_y, graph=g), lambda: pyx.plot(fresh_y, y_error=y)):
    try: action()
    except pyx.BrokenReferenceError: pass
    else: raise AssertionError('ancienne poignée acceptée')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn objets_graphiques_seuls_ou_avec_donnees_rejoignent_le_groupe_du_script() {
    let b = banc();
    run(&b, &format!("{HANDLES}g = pyx.plot(y, x, name='Graph1')"));
    run(&b, "pyx.add_shape(g, 'rectangle', 0, 0, 1, 1)");
    let before = b.session.snapshot().unwrap();
    for mutation in [
        "pyx.add_shape(g, 'arrow', 0, 0, 2, 2)",
        "pyx.remove_shape(g, 0)",
    ] {
        let out = b
            .engine
            .run(&format!("{mutation}\nraise RuntimeError('rollback objet')"))
            .unwrap();
        assert!(out.error.unwrap().contains("rollback objet"));
        let after = b.session.snapshot().unwrap();
        assert_eq!(
            *after.tree, *before.tree,
            "l'objet écrit seul doit être annulé sur erreur"
        );
        assert_eq!(after.history, before.history);
    }
    run(
        &b,
        "pyx.add_shape(g, 'arrow', 0, 0, 2, 2)\npyx.set_values(y, [42.])",
    );
    assert_eq!(graph_spec(&b).panels[0].shapes.len(), 2);
    let after = b.session.snapshot().unwrap();
    b.session.undo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);
    b.session.redo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *after.tree);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shapes.plx");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    let before = b.session.snapshot().unwrap();
    run(&b, "for action in (lambda: pyx.add_shape(g, 'line', 0, 0, 1, 1), lambda: pyx.remove_shape(g, 0)):\n    try: action()\n    except pyx.BrokenReferenceError: pass\n    else: raise AssertionError('ancienne poignée acceptée')");
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}
