//! Native graphs built by Python: same identities, spec and export.
#![cfg(feature = "python")]

use std::path::PathBuf;
use stokesforge_core::{Column, NodeId, NodeKind, Project};
use stokesforge_plot::{
    AxisScale, LineStyle, PlotSpec, PlotType, SeriesAxis, SeriesSpec, SymbolShape,
};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: ScriptEngine,
    session: SessionHandle,
    _actor: Session,
    x: NodeId,
    y: NodeId,
    /// Third column "E" (no role), only for `bench_with_error`.
    e: Option<NodeId>,
}
fn bench() -> Bench {
    bench_columns(false)
}
fn bench_with_error() -> Bench {
    bench_columns(true)
}
fn bench_columns(with_error: bool) -> Bench {
    let mut project = Project::new("Graphs").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1., 2., 3.]))
        .unwrap();
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[2., 4., 8.]))
        .unwrap();
    let e = with_error.then(|| {
        project
            .add_column(sheet, None, Some("E"), Column::from_f64(&[0.1, 0.2, 0.3]))
            .unwrap()
    });
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let home = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home)).unwrap();
    Bench {
        engine,
        session,
        _actor: actor,
        x,
        y,
        e,
    }
}
fn run(b: &Bench, source: &str) {
    let out = b.engine.run(source).unwrap();
    assert!(out.error.is_none(), "{:?}", out.error);
}
fn graph_spec(b: &Bench) -> PlotSpec {
    let snap = b.session.snapshot().unwrap();
    let id = snap.tree.resolve("[Graph1]").unwrap();
    match snap.tree.node(id).unwrap().kind() {
        NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("graph expected"),
    }
}
const HANDLES: &str = "x = pyst.resolve('[Book1]Sheet1!X')\ny = pyst.resolve('[Book1]Sheet1!Y')\n";

/// #103: neither the graph name nor the column name becomes an implicit title.
#[test]
fn a_python_graph_without_a_title_option_stays_untitled() {
    for expression in [
        "pyst.plot(y, x, name='Graph1')",
        "pyst.plot(y, name='Graph1')",
    ] {
        let b = bench();
        run(&b, &format!("{HANDLES}g = {expression}"));
        let spec = graph_spec(&b);
        assert!(
            spec.title.is_empty(),
            "{expression} must leave the title empty"
        );
        assert_eq!(
            spec.panels[0].series.len(),
            1,
            "the graph was indeed plotted"
        );
    }
}

#[test]
fn exact_spec_stable_export_and_style_witness() {
    let b = bench();
    let before = b.session.snapshot().unwrap();
    run(&b, &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1', plot_type='line_scatter', color='#12345680', line_style='dash', line_width=2.5, symbol='square', symbol_size=9., title='$alpha$', x_title='Temps (s)', y_title='$I$', x_scale='log', x_from=1., x_to=10., label='Signal')"));
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
    // A graph created by a script is explicit (#394): the frames of the
    // 480 x 360 pt figure, the title band inside.
    let legacy = layout_of(&expected);
    expected.frame_widths = legacy.frame_widths;
    expected.frame_heights = legacy.frame_heights;
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
    assert_ne!(svg, render(&witness), "the witness must change the figure");
    let rich = stokesforge_render_svg::render_to_string_with_typst(
        &stokesforge_core::resolve_spec(&snapshot.tree, &expected),
        (640, 480),
        &measurer,
        &stokesforge_render::FontChoice::default(),
        stokesforge_render_svg::SvgTextMode::default(),
    )
    .unwrap();
    assert_ne!(rich, svg, "markup titles do exercise Typst");
    b.session.undo().unwrap();
    assert_eq!(*b.session.snapshot().unwrap().tree, *before.tree);
    b.session.redo().unwrap();
    assert_eq!(graph_spec(&b), expected);
}

#[test]
fn adds_without_overwriting_series_panels_and_settings() {
    let b = bench();
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
    run(&b, "pyst.plot(y, graph=g, plot_type='scatter', axis='right', y_title='Second', y_scale='log', y_from=1., y_to=20.)");
    let second = graph_spec(&b);
    // `y_from=1., y_to=20.`: bounds in the given order, axis not flipped.
    assert_eq!(
        second
            .panels
            .first()
            .unwrap()
            .right
            .range_min
            .zip(second.panels.first().unwrap().right.range_max),
        Some((1.0, 20.0))
    );
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
    assert!(panel.right.visible, "axis='right' shows the right axis");
    assert_eq!(panel.right.scale, AxisScale::Log);
    b.session.undo().unwrap();
    assert_eq!(graph_spec(&b), first);
    b.session.redo().unwrap();
    assert_eq!(graph_spec(&b), second);
}

#[test]
fn types_errors_histograms_and_mixed_cells_use_the_model() {
    let b = bench();
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
        "the automatic name stays literal and follows the renaming"
    );
    let snap = b.session.snapshot().unwrap();
    let resolved = stokesforge_core::resolve_spec(&snap.tree, &spec);
    assert_eq!(
        resolved.panels[0].series[6].label,
        stokesforge_core::SeriesLabel::Text("#fonction_inconnue[x] $alpha$".to_string())
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
    // Typst also composes literal texts. Their compilation succeeds, whereas
    // the same text entered as markup triggers the raw SVG fallback.
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
        "the witness must interpret the explicit markup"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graphe.stf");
    b.session.save(&path).unwrap();
    b.session.load(&path).unwrap();
    assert_eq!(graph_spec(&b), spec);
}

#[test]
fn refusals_change_neither_graph_nor_history() {
    let b = bench();
    run(&b, &format!("{HANDLES}g = pyst.plot(y, name='Graph1')"));
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
for opts in ({'plot_type':'inconnu'}, {'color':'red'}, {'color':['#ff0000','red']}, {'symbol':'o'}, {'line_style':'--'}, {'line_width':float('nan')}, {'symbol_size':0}, {'histogram_bins':0}, {'histogram_bins':65537}, {'x_scale':'symlog'}, {'x_from':1.}, {'x_from':2.,'x_to':2.}, {'y_scale':'log','y_from':2.,'y_to':0.}, {'y_error':pyst.root()}, {'unknown':1}, {'parent':pyst.root()}, {'name':'Autre'}):
    try: pyst.plot(y, graph=g, **opts)
    except pyst.InvalidValueError: pass
    else: raise AssertionError('invalid option accepted')
for cible in (pyst.root(), pyst.info(y)['parent']):
    try: pyst.plot(cible)
    except pyst.InvalidValueError: pass
    else: raise AssertionError('wrong target accepted')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn exception_undoes_creation_and_additions_and_load_invalidates_the_handles() {
    let b = bench();
    let before = b.session.snapshot().unwrap();
    let out = b.engine.run(&format!("{HANDLES}g = pyst.plot(y, x, name='Graph1')\npyst.plot(y, graph=g)\nraise RuntimeError('failure')")).unwrap();
    assert!(out.error.unwrap().contains("failure"));
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
    else: raise AssertionError('stale handle accepted')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn graphic_objects_alone_or_with_data_join_the_script_group() {
    let b = bench();
    run(&b, &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1')"));
    run(&b, "pyst.add_shape(g, 'rectangle', 0, 0, 1, 1)");
    let before = b.session.snapshot().unwrap();
    for mutation in [
        "pyst.add_shape(g, 'arrow', 0, 0, 2, 2)",
        "pyst.remove_shape(g, 0)",
    ] {
        let out = b
            .engine
            .run(&format!(
                "{mutation}\nraise RuntimeError('object rollback')"
            ))
            .unwrap();
        assert!(out.error.unwrap().contains("object rollback"));
        let after = b.session.snapshot().unwrap();
        assert_eq!(
            *after.tree, *before.tree,
            "the object written alone must be undone on error"
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
    run(&b, "for action in (lambda: pyst.add_shape(g, 'line', 0, 0, 1, 1), lambda: pyst.remove_shape(g, 0)):\n    try: action()\n    except pyst.BrokenReferenceError: pass\n    else: raise AssertionError('stale handle accepted')");
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

/// #196: several columns in one call form a group, like the series added
/// together from the interface; a single column does not form one.
#[test]
fn several_columns_form_a_group_and_a_single_one_does_not() {
    let b = bench();
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

    // Adding to an existing graph: new group, the old one intact.
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

    let b = bench();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y], x, name='Graph1')"),
    );
    assert_eq!(graph_spec(&b).panels[0].series[0].group, None);
}

#[test]
fn read_the_series_and_the_groups() {
    let b = bench();
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
    // The reads wrote nothing, apart from the renaming.
    assert_eq!(graph_spec(&b).panels[0].group_names.len(), 0);
}

/// All the changes of a script (gather, move, rename, dissolve, delete,
/// style, color gradient) are undone by a single `undo`.
#[test]
fn changing_the_groups_joins_the_undoable_script_group() {
    let b = bench();
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
        "the names of vanished groups are erased"
    );
    assert_eq!(panel.series[0].line.color, "#000000");
}

#[test]
fn a_script_in_error_leaves_no_group() {
    let b = bench();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y], x, name='Graph1')"),
    );
    let before = b.session.snapshot().unwrap();
    let out = b
        .engine
        .run("gid = pyst.series_groups(g)[0]['id']\npyst.remove_group(g, gid)\nraise RuntimeError('group rollback')")
        .unwrap();
    assert!(out.error.unwrap().contains("group rollback"));
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

#[test]
fn incorrect_group_gestures_are_refused_without_changing_anything() {
    let b = bench();
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
    else: raise AssertionError('invalid gesture accepted')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn a_stale_graph_handle_is_refused_by_group_gestures() {
    let b = bench();
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
    else: raise AssertionError('stale handle accepted')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

/// A group gesture with no effect opens no macro and writes no history.
#[test]
fn a_group_gesture_with_no_effect_writes_nothing() {
    let b = bench();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y], x, name='Graph1', group_name='Lots')\npyst.apply_gradient(g, 1, 'blue-red')"),
    );
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        "gid = pyst.series_groups(g)[0]['id']
pyst.rename_group(g, gid, 'Lots')
assert pyst.set_series_group(g, 1, gid) == 1
pyst.set_group_style(g, gid)
pyst.apply_gradient(g, gid, 'blue-red')
",
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.history, before.history);
}

/// #257: `plot(y)` without `x=` looks for the X column by role (the nearest
/// one to the left of y, like the window); without an X column to the left,
/// the abscissa stays the row number; an explicit `x=` takes precedence.
#[test]
fn plot_without_x_takes_the_x_column_on_the_left_by_role() {
    // No column with the X role: row number, as before.
    let b = bench();
    run(&b, &format!("{HANDLES}g = pyst.plot(y, name='Graph1')"));
    assert_eq!(graph_spec(&b).panels[0].series[0].x, None);

    // X has the X role and is left of Y: Y pairs with it without the script saying so.
    let b = bench();
    run(
        &b,
        &format!("{HANDLES}pyst.set_column_meta(x, role='x')\ng = pyst.plot(y, name='Graph1')"),
    );
    let series = &graph_spec(&b).panels[0].series;
    assert_eq!((series[0].x, series[0].y), (Some(b.x), b.y));

    // An X column located to the right of y does not count: row number.
    let b = bench();
    run(
        &b,
        &format!("{HANDLES}pyst.set_column_meta(y, role='x')\ng = pyst.plot(x, name='Graph1')"),
    );
    assert_eq!(graph_spec(&b).panels[0].series[0].x, None);

    // An explicit `x=` wins over the role, and the palette follows the rank.
    let b = bench();
    run(
        &b,
        &format!(
            "{HANDLES}pyst.set_column_meta(x, role='x')\n\
             g = pyst.plot(y, y, name='Graph1')\n\
             pyst.plot(y, graph=g)"
        ),
    );
    let panel = &graph_spec(&b).panels[0];
    assert_eq!(panel.series[0].x, Some(b.y), "x= explicite");
    assert_eq!(panel.series[1].x, Some(b.x), "X role, y added to a graph");
    assert_eq!(
        panel.series[1].line.color,
        stokesforge_plot::palette_color(1)
    );
    assert_eq!(
        panel.series[1].symbol.color,
        stokesforge_plot::palette_color(1)
    );
}

/// #207: `color` accepts a list (one color per series); a string applies to
/// all the series; a list of the wrong length is refused.
#[test]
fn color_as_a_list_gives_one_color_per_series() {
    let b = bench();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot([y, y, y], x, name='Graph1', color=['#ff0000', '#00ff00', '#0000ff80'])"),
    );
    let panel = &graph_spec(&b).panels[0];
    let colors: Vec<&str> = panel.series.iter().map(|s| s.line.color.as_str()).collect();
    assert_eq!(colors, ["#ff0000", "#00ff00", "#0000ff80"]);
    assert_eq!(panel.series[1].symbol.color, "#00ff00");

    // A single color applies to all the series of the batch.
    run(&b, "pyst.plot([y, y], graph=g, color='#123456')");
    let panel = &graph_spec(&b).panels[0];
    assert_eq!(panel.series[3].line.color, "#123456");
    assert_eq!(panel.series[4].line.color, "#123456");

    // Wrong length: refusal, nothing is written.
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
for colors in (['#ff0000'], ['#ff0000', '#00ff00', '#0000ff'], []):
    try: pyst.plot([y, y], graph=g, color=colors)
    except pyst.InvalidValueError: pass
    else: raise AssertionError('list of the wrong length accepted')
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
}

/// #360: `pyst.plot(y)` pairs the first Y Error column to the right of the Y
/// (the rule of the window); `y_error=None` goes without; an explicit handle
/// wins over the pairing.
#[test]
fn plot_pairs_the_error_column_like_the_window_unless_told_otherwise() {
    let b = bench_with_error();
    let e = b.e.unwrap();
    let setup = format!(
        "{HANDLES}e = pyst.resolve('[Book1]Sheet1!E')\n\
         pyst.set_column_meta(x, role='x')\n\
         pyst.set_column_meta(y, role='y')\n\
         pyst.set_column_meta(e, role='y_error')\n"
    );
    run(
        &b,
        &format!(
            "{setup}pyst.plot(y, name='Graph1')\n\
             pyst.plot(y, graph=pyst.resolve('[Graph1]'), y_error=None)\n\
             pyst.plot(y, graph=pyst.resolve('[Graph1]'), y_error=x)\n"
        ),
    );
    let spec = graph_spec(&b);
    let series = &spec.panels[0].series;
    assert_eq!(series[0].y_error, Some(e), "paired by role");
    assert_eq!(series[0].x, Some(b.x));
    assert_eq!(
        series[1].y_error, None,
        "explicit None disables the pairing"
    );
    assert_eq!(series[2].y_error, Some(b.x), "an explicit handle wins");
    assert_eq!(series[0].x_error, None);
}

fn layout_of(spec: &PlotSpec) -> stokesforge_render::pipeline::FigureLayout {
    stokesforge_render_svg::figure_layout(
        spec,
        &stokesforge_render_svg::SvgTextMeasurer::default(),
        &stokesforge_render_svg::FontChoice::default(),
    )
    .expect("layout")
}

/// #394: a graph created by a script is explicit from the start, and its
/// figure is exactly 480 x 360 pt, like one created by the application.
#[test]
fn a_python_graph_is_created_explicit_and_480_by_360() {
    let b = bench();
    run(&b, &format!("{HANDLES}pyst.plot(y, x, name='Graph1')"));
    let spec = graph_spec(&b);
    assert_eq!(spec.frame_widths.len(), 1);
    assert_eq!(spec.frame_heights.len(), 1);
    assert_eq!(layout_of(&spec).size, (480, 360));
}

/// #394 (G7): a script that edits a graph from an old file (nothing stored)
/// fixes the frames it is drawn with, in the very command of the edit: removing
/// the title leaves the plot area as it was and shrinks the figure.
#[test]
fn a_script_edit_of_a_legacy_graph_fixes_its_frames() {
    let b = bench();
    run(&b, &format!("{HANDLES}pyst.plot(y, x, name='Graph1')"));
    let snap = b.session.snapshot().unwrap();
    let graph = snap.tree.resolve("[Graph1]").unwrap();
    let mut legacy = graph_spec(&b);
    legacy.title = "Old".to_owned();
    legacy.frame_widths.clear();
    legacy.frame_heights.clear();
    b.session
        .execute(stokesforge_commands::Command::SetPlotSpec {
            graph,
            spec: legacy.clone(),
        })
        .unwrap();
    let drawn = layout_of(&legacy);
    assert!(!drawn.is_explicit() && drawn.title_band > 0);

    run(&b, "pyst.set_graph(pyst.resolve('[Graph1]'), title='')");
    let spec = graph_spec(&b);
    assert_eq!(spec.frame_widths, drawn.frame_widths);
    assert_eq!(spec.frame_heights, drawn.frame_heights);
    assert_eq!(layout_of(&spec).size, (480, 360 - drawn.title_band));
}
