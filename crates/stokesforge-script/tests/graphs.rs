//! Graphes natifs construits par Python : mêmes identités, spec et export.
#![cfg(feature = "python")]

use std::path::PathBuf;
use stokesforge_core::{Column, NodeId, NodeKind, Project};
use stokesforge_plot::{
    AxisScale, LineStyle, PlotSpec, PlotType, SeriesAxis, SeriesSpec, SymbolShape,
};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::{Session, SessionHandle};

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
const HANDLES: &str = "x = pyst.resolve('[Book1]Sheet1!X')\ny = pyst.resolve('[Book1]Sheet1!Y')\n";

/// #103 : ni le nom du graphe ni celui de la colonne ne devient un titre implicite.
#[test]
fn un_graphe_python_sans_option_title_reste_sans_titre() {
    for expression in [
        "pyst.plot(y, x, name='Graph1')",
        "pyst.plot(y, name='Graph1')",
    ] {
        let b = banc();
        run(&b, &format!("{HANDLES}g = {expression}"));
        let spec = graph_spec(&b);
        assert!(
            spec.title.is_empty(),
            "{expression} doit laisser le titre vide"
        );
        assert_eq!(spec.panels[0].series.len(), 1, "le graphe a bien été tracé");
    }
}

#[test]
fn spec_exact_export_stable_et_temoin_de_style() {
    let b = banc();
    let before = b.session.snapshot().unwrap();
    run(&b, &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1', plot_type='line_scatter', color='#12345680', line_style='dash', line_width=2.5, symbol='square', symbol_size=9., title='$alpha$', x_title='Temps (s)', y_title='$I$', x_scale='log', x_range=(1., 10.), label='Signal')"));
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
    let measurer = stokesforge_render::TextMeasurer::default();
    let render = |spec: &PlotSpec| {
        stokesforge_render_svg::render_spec(&snapshot.tree, spec, (640, 480), &measurer).unwrap()
    };
    let svg = render(&graph_spec(&b));
    assert_eq!(svg, render(&expected));
    assert_eq!(svg, render(&expected));
    let mut witness = expected.clone();
    witness.panels.get_mut(0).unwrap().series[0].line.color = "#ff0000".into();
    assert_ne!(svg, render(&witness), "le témoin doit modifier la figure");
    let rich = stokesforge_render_svg::render_to_string_with_typst(
        &stokesforge_core::resolve_spec(&snapshot.tree, &expected),
        (640, 480),
        &measurer,
        &stokesforge_render::FontChoice::default(),
        stokesforge_render_svg::SvgTextMode::default(),
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
        &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1', title='Premier', x_title='Temps')"),
    );
    let mut first = graph_spec(&b);
    first.panels.push(stokesforge_plot::PanelSpec::at(0, 1));
    let graph = b
        .session
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Graph1]")
        .unwrap();
    b.session
        .execute(stokesforge_commands::Command::SetPlotSpec {
            graph,
            spec: first.clone(),
        })
        .unwrap();
    run(&b, "pyst.plot(y, graph=g, plot_type='scatter', axis='right', y_title='Second', y_scale='log', y_range=(1., 20.))");
    let second = graph_spec(&b);
    assert_eq!(second.panels.get(1), first.panels.get(1));
    let panel = second.panels.first().unwrap();
    assert_eq!(panel.series[0], first.panels.first().unwrap().series[0]);
    assert_eq!(second.title, first.title);
    assert_eq!(panel.bottom, first.panels.first().unwrap().bottom);
    assert_eq!(panel.series[1].axis, SeriesAxis::Right);
    assert_eq!(panel.series[1].x, None);
    assert_eq!(
        panel.series[1].line.color,
        stokesforge_plot::palette_color(1)
    );
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
g = pyst.plot(y, name='Graph1')
for kind in ('scatter', 'line_scatter', 'bar', 'histogram', 'error_bar'):
    pyst.plot(y, x, graph=g, plot_type=kind, x_error=x, y_error=y, histogram_bins=12)
t = pyst.create_column(pyst.info(y)['parent'], column_type='mixed')
pyst.set_values(t, [1., 'texte', None, 2.])
pyst.set_column_meta(t, long_name='#fonction_inconnue[x] $alpha$')
pyst.plot(t, graph=g)
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
    let resolved = stokesforge_core::resolve_spec(&snap.tree, &spec);
    assert_eq!(
        resolved.panels[0].series[6].label,
        "#fonction_inconnue[x] $alpha$"
    );
    let svg = stokesforge_render_svg::render_spec(
        &snap.tree,
        &spec,
        (640, 480),
        &stokesforge_render::TextMeasurer::default(),
    )
    .unwrap();
    assert!(svg.contains("#fonction_inconnue[x] $alpha$"));
    let with_typst = stokesforge_render_svg::render_to_string_with_typst(
        &resolved,
        (640, 480),
        &stokesforge_render::TextMeasurer::default(),
        &stokesforge_render::FontChoice::default(),
        stokesforge_render_svg::SvgTextMode::default(),
    )
    .unwrap();
    // Typst compose aussi les textes littéraux. Leur compilation réussit,
    // alors que le même texte saisi comme markup déclenche le repli SVG brut.
    assert!(!with_typst.contains("#fonction_inconnue[x] $alpha$"));
    let mut typed = spec.clone();
    typed.panels.get_mut(0).unwrap().series[6].name = "#fonction_inconnue[x] $alpha$".into();
    let typed = stokesforge_render_svg::render_to_string_with_typst(
        &stokesforge_core::resolve_spec(&snap.tree, &typed),
        (640, 480),
        &stokesforge_render::TextMeasurer::default(),
        &stokesforge_render::FontChoice::default(),
        stokesforge_render_svg::SvgTextMode::default(),
    )
    .unwrap();
    assert!(typed.contains("#fonction_inconnue[x] $alpha$"));
    assert!(
        typed != with_typst,
        "le témoin doit interpréter le markup explicite"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graphe.stf");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    assert_eq!(graph_spec(&b), spec);
}

#[test]
fn refus_ne_modifient_ni_graphe_ni_historique() {
    let b = banc();
    run(&b, &format!("{HANDLES}g = pyst.plot(y, name='Graph1')"));
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
for opts in ({'plot_type':'inconnu'}, {'color':'red'}, {'symbol':'o'}, {'line_style':'--'}, {'line_width':float('nan')}, {'symbol_size':0}, {'histogram_bins':0}, {'histogram_bins':65537}, {'x_scale':'symlog'}, {'x_range':(2.,1.)}, {'y_scale':'log','y_range':(0.,2.)}, {'y_error':pyst.root()}, {'unknown':1}, {'parent':pyst.root()}, {'name':'Autre'}):
    try: pyst.plot(y, graph=g, **opts)
    except pyst.InvalidValueError: pass
    else: raise AssertionError('option invalide acceptée')
for cible in (pyst.root(), pyst.info(y)['parent']):
    try: pyst.plot(cible)
    except pyst.InvalidValueError: pass
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
    let out = b.engine.run(&format!("{HANDLES}g = pyst.plot(y, x, name='Graph1')\npyst.plot(y, graph=g)\nraise RuntimeError('échec')")).unwrap();
    assert!(out.error.unwrap().contains("échec"));
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);
    run(&b, "g = pyst.plot(y, x, name='Graph1')");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("epoch.stf");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
fresh_y = pyst.resolve('[Book1]Sheet1!Y')
for action in (lambda: pyst.plot(y), lambda: pyst.plot(fresh_y, x), lambda: pyst.plot(fresh_y, graph=g), lambda: pyst.plot(fresh_y, y_error=y)):
    try: action()
    except pyst.BrokenReferenceError: pass
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
    run(&b, &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1')"));
    run(&b, "pyst.add_shape(g, 'rectangle', 0, 0, 1, 1)");
    let before = b.session.snapshot().unwrap();
    for mutation in [
        "pyst.add_shape(g, 'arrow', 0, 0, 2, 2)",
        "pyst.remove_shape(g, 0)",
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
        "pyst.add_shape(g, 'arrow', 0, 0, 2, 2)\npyst.set_values(y, [42.])",
    );
    assert_eq!(graph_spec(&b).panels[0].shapes.len(), 2);
    let after = b.session.snapshot().unwrap();
    b.session.undo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);
    b.session.redo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *after.tree);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shapes.stf");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    let before = b.session.snapshot().unwrap();
    run(&b, "for action in (lambda: pyst.add_shape(g, 'line', 0, 0, 1, 1), lambda: pyst.remove_shape(g, 0)):\n    try: action()\n    except pyst.BrokenReferenceError: pass\n    else: raise AssertionError('ancienne poignée acceptée')");
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

/// #196 : plusieurs colonnes en un appel forment un groupe, comme les séries
/// ajoutées ensemble depuis l'interface ; une colonne seule n'en forme pas.
#[test]
fn plusieurs_colonnes_forment_un_groupe_et_une_seule_non() {
    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y, y], x, name='Graph1', label=['A', 'B', 'C'], group_name='Lots')"),
    );
    let panel = &graph_spec(&b).panels[0];
    assert_eq!(panel.series.len(), 3);
    let group = panel.series[0].group.expect("groupe");
    assert!(panel.series.iter().all(|s| s.group == Some(group)));
    assert_eq!(panel.group_label(group), Some("Lots"));
    let names: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C"]);
    assert!(panel.series.iter().all(|s| s.x == Some(b.x)));
    let colors: Vec<&str> = panel.series.iter().map(|s| s.line.color.as_str()).collect();
    assert_eq!(colors[0], stokesforge_plot::palette_color(0));
    assert_eq!(colors[2], stokesforge_plot::palette_color(2));

    // Ajouter à un graphe existant : nouveau groupe, l'ancien intact.
    run(&b, "pyst.plot([y, y], graph=g)\npyst.plot(y, graph=g)");
    let panel = &graph_spec(&b).panels[0];
    assert_eq!(panel.series.len(), 6);
    assert_eq!(panel.group_ids().len(), 2);
    assert_eq!(panel.series[5].group, None);
    assert_eq!(panel.series[3].group, panel.series[4].group);
    assert_eq!(
        panel.series[4].line.color,
        stokesforge_plot::palette_color(4)
    );

    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y], x, name='Graph1')"),
    );
    assert_eq!(graph_spec(&b).panels[0].series[0].group, None);
}

#[test]
fn lire_les_series_et_les_groupes() {
    let b = banc();
    run(
        &b,
        &format!(
            "{HANDLES}g = pyst.plot([y, y, y], x, name='Graph1', label=['A', 'B', 'C'], group_name='Lots')
pyst.plot(y, graph=g, label='Seule')
rows = pyst.series(g)
assert [r['name'] for r in rows] == ['A', 'B', 'C', 'Seule'], rows
assert [r['index'] for r in rows] == [0, 1, 2, 3]
assert rows[0]['group'] is not None and rows[0]['group'] == rows[2]['group']
assert rows[3]['group'] is None
assert rows[0]['visible'] is True
groups = pyst.series_groups(g)
assert len(groups) == 1
assert groups[0]['id'] == rows[0]['group']
assert groups[0]['name'] == 'Lots'
assert groups[0]['members'] == [0, 1, 2]
pyst.rename_group(g, groups[0]['id'], '')
assert pyst.series_groups(g)[0]['name'] is None
"
        ),
    );
    // Rien n'a été écrit par les lectures, hors le renommage.
    assert_eq!(graph_spec(&b).panels[0].group_names.len(), 0);
}

/// Toutes les modifications d'un script — rassembler, déplacer, renommer,
/// dissoudre, supprimer, style, nuancier — se défont d'un seul `undo`.
#[test]
fn modifier_les_groupes_rejoint_le_groupe_annulable_du_script() {
    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1', label='A')\nfor n in 'BCD':\n    pyst.plot(y, graph=g, label=n)"),
    );
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        "gid = pyst.group_series(g, [3, 1], name='Duo')
assert [r['name'] for r in pyst.series(g)] == ['A', 'D', 'B', 'C'], pyst.series(g)
assert pyst.series_groups(g) == [{'id': gid, 'name': 'Duo', 'members': [1, 2]}]
rank = pyst.set_series_group(g, 3, gid)
assert rank == 3
assert pyst.series_groups(g)[0]['members'] == [1, 2, 3]
assert pyst.set_series_group(g, 0, gid) == 3
assert [r['name'] for r in pyst.series(g)] == ['D', 'B', 'C', 'A']
pyst.set_series_group(g, 3, None)
assert pyst.series_groups(g)[0]['members'] == [0, 1, 2]
pyst.set_group_style(g, gid, line_style='dash', line_width=3., symbol='square', symbol_size=8.)
pyst.apply_gradient(g, gid, 'blue-red')
",
    );
    let panel = &graph_spec(&b).panels[0];
    let gid = panel.series[0].group.unwrap();
    for member in panel.group_members(gid) {
        let s = &panel.series[member];
        assert_eq!(s.line.style, LineStyle::Dash);
        assert_eq!(s.line.width, 3.);
        assert_eq!(s.symbol.shape, SymbolShape::Square);
        assert_eq!(s.symbol.size, 8.);
    }
    assert_eq!(panel.series[0].line.color, "#0000ff");
    assert_eq!(panel.series[2].line.color, "#ff0000");
    assert_eq!(
        panel.series[3].line.color,
        stokesforge_plot::palette_color(0)
    );
    b.session.undo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);

    run(
        &b,
        "gid = pyst.group_series(g, [0, 1, 2], name='Trio')
pyst.apply_gradient(g, gid, ['#000000', '#00ff00'])
pyst.ungroup(g, gid)
assert pyst.series_groups(g) == []
assert [r['name'] for r in pyst.series(g)] == ['A', 'B', 'C', 'D']
gid = pyst.group_series(g, [1, 2])
pyst.remove_group(g, gid)
assert [r['name'] for r in pyst.series(g)] == ['A', 'D']
",
    );
    let panel = &graph_spec(&b).panels[0];
    assert_eq!(panel.series.len(), 2);
    assert!(
        panel.group_names.is_empty(),
        "les noms des groupes disparus s'effacent"
    );
    assert_eq!(panel.series[0].line.color, "#000000");
}

#[test]
fn un_script_en_erreur_ne_laisse_aucun_groupe() {
    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y], x, name='Graph1')"),
    );
    let before = b.session.snapshot().unwrap();
    let out = b
        .engine
        .run("gid = pyst.series_groups(g)[0]['id']\npyst.remove_group(g, gid)\nraise RuntimeError('rollback groupe')")
        .unwrap();
    assert!(out.error.unwrap().contains("rollback groupe"));
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

#[test]
fn les_gestes_de_groupe_incorrects_sont_refuses_sans_rien_changer() {
    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y], x, name='Graph1')\npyst.plot(y, graph=g)"),
    );
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
gid = pyst.series_groups(g)[0]['id']
refus = [
    lambda: pyst.plot([y, y], x, label='seul'),
    lambda: pyst.plot([y, y], x, label=['un']),
    lambda: pyst.plot([y, y], [x, x, x]),
    lambda: pyst.plot([y, y], x, y_error=y),
    lambda: pyst.plot([], x),
    lambda: pyst.plot(y, x, group_name='sans groupe'),
    lambda: pyst.plot([y, 3], x),
    lambda: pyst.group_series(g, []),
    lambda: pyst.group_series(g, [0, 0]),
    lambda: pyst.group_series(g, [0, 9]),
    lambda: pyst.set_series_group(g, 9, gid),
    lambda: pyst.set_series_group(g, 2, 99),
    lambda: pyst.rename_group(g, 99, 'x'),
    lambda: pyst.ungroup(g, 99),
    lambda: pyst.remove_group(g, 99),
    lambda: pyst.set_group_style(g, 99, line_width=2.),
    lambda: pyst.set_group_style(g, gid, line_style='--'),
    lambda: pyst.set_group_style(g, gid, symbol='o'),
    lambda: pyst.set_group_style(g, gid, line_width=0.),
    lambda: pyst.apply_gradient(g, gid, 'inconnu'),
    lambda: pyst.apply_gradient(g, gid, []),
    lambda: pyst.apply_gradient(g, gid, ['rouge']),
    lambda: pyst.apply_gradient(g, gid, 3),
    lambda: pyst.apply_gradient(g, 99, 'blue-red'),
    lambda: pyst.series(g, panel=(5, 5)),
    lambda: pyst.group_series(g, [0, 1], panel=(5, 5)),
    lambda: pyst.series_groups(y),
]
for action in refus:
    try: action()
    except pyst.InvalidValueError: pass
    else: raise AssertionError('geste invalide accepté')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn une_ancienne_poignee_de_graphe_est_refusee_par_les_gestes_de_groupe() {
    let b = banc();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y], x, name='Graph1')"),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("groups.stf");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
for action in (lambda: pyst.series(g), lambda: pyst.series_groups(g), lambda: pyst.group_series(g, [0, 1]), lambda: pyst.ungroup(g, 1), lambda: pyst.remove_group(g, 1)):
    try: action()
    except pyst.BrokenReferenceError: pass
    else: raise AssertionError('ancienne poignée acceptée')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}
