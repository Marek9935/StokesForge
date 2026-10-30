//! `pyst.set_graph` (#331, batch 2, S4): settings of an existing graph through
//! the Graph Properties keys, in a single undoable write.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, NodeId, NodeKind, Project};
use stokesforge_plot::{AxisScale, LegendPosition, PlotSpec};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
}

fn bench() -> Bench {
    let mut project = Project::new("Graph").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
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
    }
}

fn run(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

/// Creates the graph `g` (a plot of B against A) in a separate script.
const GRAPH: &str = "a = pyst.resolve('[Book1]Sheet1!A')\nb = pyst.resolve('[Book1]Sheet1!B')\ng = pyst.plot(b, a, name='G')\n";

fn graph_spec(bench: &Bench) -> PlotSpec {
    let snapshot = bench.session.snapshot().unwrap();
    let id: NodeId = snapshot.tree.resolve("[G]").unwrap();
    match snapshot.tree.node(id).unwrap().kind() {
        NodeKind::Graph { spec, .. } => spec.clone(),
        other => panic!("{other:?}"),
    }
}

fn refused(bench: &Bench, body: &str) -> String {
    let source = format!(
        "g = pyst.resolve('[G]')\ntry:\n    {body}\nexcept pyst.InvalidValueError as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no InvalidValueError')\nprint(caught)"
    );
    run(bench, &source).stdout.trim().to_owned()
}

#[test]
fn set_graph_sets_the_graph_and_the_panel() {
    let b = bench();
    run(&b, GRAPH);
    run(
        &b,
        "g = pyst.resolve('[G]')\n\
         pyst.set_graph(g, title='T', title_visible=True, stale_warning=False, decimal_comma=True,\n\
             background='#102030', title_color='#FF0000', panel_title='P',\n\
             x_title='Time', y_title='Signal', x_unit='s', y_unit='V',\n\
             x_scale='log', y_scale='linear', x_from=1, x_to=100,\n\
             y_from=10, y_to=0, x_grid=False, y_grid=True, y_inverted=True,\n\
             x_visible=True, y_visible=True,\n\
             legend_visible=False, legend_position='bottom_left')",
    );
    let spec = graph_spec(&b);
    assert_eq!(spec.title, "T");
    assert!(spec.title_visible);
    assert!(!spec.stale_warning);
    assert!(spec.decimal_comma);
    assert_eq!(spec.background, "#102030");
    assert_eq!(spec.title_color, "#FF0000");
    let panel = &spec.panels[0];
    assert_eq!(panel.title, "P");
    assert_eq!(panel.bottom.label, "Time");
    assert_eq!(panel.left.label, "Signal");
    assert_eq!(panel.bottom.unit, "s");
    assert_eq!(panel.left.unit, "V");
    assert_eq!(panel.bottom.scale, AxisScale::Log);
    assert_eq!(panel.left.scale, AxisScale::Linear);
    assert!(!panel.bottom.auto_range);
    assert_eq!(
        (panel.bottom.range_min, panel.bottom.range_max),
        (Some(1.0), Some(100.0))
    );
    assert_eq!(
        (panel.left.range_min, panel.left.range_max),
        (Some(10.0), Some(0.0))
    );
    assert!(!panel.bottom.grid);
    assert!(panel.left.grid);
    assert!(panel.left.inverted);
    assert!(!panel.legend.visible);
    assert_eq!(panel.legend.position, LegendPosition::BottomLeft);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), legend_position='top_left')",
    );
    assert_eq!(
        graph_spec(&b).panels[0].legend.position,
        LegendPosition::TopLeft
    );
}

#[test]
fn set_graph_leaves_absent_keys_alone_and_targets_the_right_axis() {
    let b = bench();
    run(&b, GRAPH);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), title='T', x_title='X')",
    );
    let before = graph_spec(&b);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), axis='right', y_title='Right', y_scale='log')",
    );
    let after = graph_spec(&b);
    assert_eq!(after.title, "T");
    assert_eq!(after.panels[0].bottom.label, "X");
    assert_eq!(after.panels[0].right.label, "Right");
    assert_eq!(after.panels[0].right.scale, AxisScale::Log);
    assert_eq!(after.panels[0].left, before.panels[0].left);
}

#[test]
fn set_graph_is_a_single_undoable_write() {
    let b = bench();
    run(&b, GRAPH);
    let before = graph_spec(&b);
    // `undo` in the same script: everything `set_graph` set comes back at once.
    run(
        &b,
        "g = pyst.resolve('[G]')\n\
         pyst.set_graph(g, title='T', x_title='X', y_scale='log', legend_visible=False)\n\
         assert pyst.undo() is True\n",
    );
    assert_eq!(graph_spec(&b), before);
    // And through a Ctrl+Z of the interface after the run: a single entry.
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), title='T', x_title='X', y_scale='log')",
    );
    assert_eq!(graph_spec(&b).title, "T");
    let snapshot = b.session.snapshot().unwrap();
    assert!(snapshot.history.can_undo);
    b.session.undo().unwrap();
    assert_eq!(graph_spec(&b), before);
}

#[test]
fn set_graph_with_no_effect_writes_nothing() {
    let b = bench();
    run(&b, GRAPH);
    let history = b.session.snapshot().unwrap().history;
    run(
        &b,
        "g = pyst.resolve('[G]')\npyst.set_graph(g)\npyst.set_graph(g, legend_visible=True)",
    );
    assert_eq!(b.session.snapshot().unwrap().history, history);
}

#[test]
fn set_graph_refuses_without_writing_anything() {
    let b = bench();
    run(&b, GRAPH);
    let before = graph_spec(&b);
    let history = b.session.snapshot().unwrap().history;
    let unknown = refused(&b, "pyst.set_graph(g, title='ok', titel='x')");
    assert!(unknown.contains("unknown graph option: titel"), "{unknown}");
    for body in [
        "pyst.set_graph(g, title=3)",
        "pyst.set_graph(g, x_scale='sqrt')",
        "pyst.set_graph(g, x_from=1)",
        "pyst.set_graph(g, x_from=2, x_to=2)",
        "pyst.set_graph(g, x_from=float('nan'), x_to=3)",
        "pyst.set_graph(g, x_scale='log', x_from=-1, x_to=5)",
        "pyst.set_graph(g, x_from=1, x_to=5, x_auto_range=True)",
        "pyst.set_graph(g, legend_visible='yes')",
        "pyst.set_graph(g, legend_position='middle')",
        "pyst.set_graph(g, background='red')",
        "pyst.set_graph(g, axis='top')",
        "pyst.set_graph(g, panel=(3, 3), title='x')",
        "pyst.set_graph(pyst.resolve('[Book1]Sheet1!A'), title='x')",
    ] {
        refused(&b, body);
    }
    assert_eq!(graph_spec(&b), before);
    assert_eq!(b.session.snapshot().unwrap().history, history);
}

#[test]
fn set_graph_restores_automatic_framing() {
    let b = bench();
    run(&b, GRAPH);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), x_from=1, x_to=2)");
    assert!(!graph_spec(&b).panels[0].bottom.auto_range);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), x_auto_range=True)");
    assert!(graph_spec(&b).panels[0].bottom.auto_range);
}

#[test]
fn each_key_announced_by_the_error_is_really_accepted() {
    let b = bench();
    run(&b, GRAPH);
    let message = refused(&b, "pyst.set_graph(g, nope=1)");
    let list = message
        .split("(accepted: ")
        .nth(1)
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{message}"));
    let keys: Vec<&str> = list.split(", ").collect();
    assert!(keys.len() >= 20, "{keys:?}");
    for key in keys {
        // An object of a type that no key accepts: the refusal concerns the
        // value, never "unknown graph option".
        let source = format!(
            "g = pyst.resolve('[G]')\ntry:\n    pyst.set_graph(g, {key}=object())\nexcept pyst.InvalidValueError as error:\n    print(str(error))"
        );
        let out = run(&b, &source).stdout;
        assert!(
            !out.is_empty() && !out.contains("unknown graph option"),
            "{key}: {out}"
        );
    }
}

/// #344: the direction is read from the order of the bounds. `x_inverted=True`
/// on increasing bounds swaps them; `x_from > x_to` gives an inverted axis;
/// the `pyst` keys themselves do not change.
#[test]
fn set_graph_orients_the_axes_from_the_order_of_the_bounds() {
    let b = bench();
    run(&b, GRAPH);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), x_from=1, x_to=5, y_from=7, y_to=2)",
    );
    let panel = &graph_spec(&b).panels[0];
    assert!(!panel.bottom.inverted);
    assert!(panel.left.inverted, "y_from=7 > y_to=2: inverted axis");
    assert_eq!(
        (panel.left.range_min, panel.left.range_max),
        (Some(7.0), Some(2.0))
    );
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), x_inverted=True)");
    let panel = &graph_spec(&b).panels[0];
    assert!(panel.bottom.inverted);
    assert_eq!(
        (panel.bottom.range_min, panel.bottom.range_max),
        (Some(5.0), Some(1.0)),
        "(1, 5) becomes (5, 1)"
    );
}

// --- Sizes (#399) ----------------------------------------------------------------------

const CM: f64 = 72.0 / 2.54;

fn layout_of(spec: &PlotSpec) -> stokesforge_render::pipeline::FigureLayout {
    stokesforge_render_svg::figure_layout(
        spec,
        &stokesforge_render_svg::SvgTextMeasurer::default(),
        &stokesforge_render_svg::FontChoice::default(),
    )
    .expect("layout")
}

/// Stores `spec` as the graph `[G]` without going through a script.
fn store(bench: &Bench, spec: PlotSpec) {
    let id = bench
        .session
        .snapshot()
        .unwrap()
        .tree
        .resolve("[G]")
        .unwrap();
    bench
        .session
        .execute(stokesforge_commands::Command::SetPlotSpec { graph: id, spec })
        .unwrap();
}

/// The graph `[G]` laid out on a grid of one row and two columns.
fn grid_1x2() -> Bench {
    let b = bench();
    run(&b, GRAPH);
    let mut spec = graph_spec(&b);
    stokesforge_core::compose::insert_cell(
        &mut spec,
        0,
        stokesforge_core::compose::CellSide::Right,
    )
    .expect("a second column");
    store(&b, spec);
    b
}

/// The graph `[G]` as an old file stores it: no plot area, a title.
fn legacy_graph() -> Bench {
    let b = bench();
    run(&b, GRAPH);
    let mut spec = graph_spec(&b);
    spec.title = "Old".to_owned();
    spec.frame_widths.clear();
    spec.frame_heights.clear();
    store(&b, spec);
    b
}

fn history_len(bench: &Bench) -> usize {
    // The history keeps no counter: undo to the bottom, then redo it all.
    let mut depth = 0;
    while bench.session.snapshot().unwrap().history.can_undo {
        bench.session.undo().unwrap();
        depth += 1;
    }
    for _ in 0..depth {
        bench.session.redo().unwrap();
    }
    depth
}

fn near(value: f64, expected: f64) {
    assert!((value - expected).abs() < 1e-9, "{value} != {expected}");
}

#[test]
fn the_size_keys_set_the_plot_area_of_the_column_and_the_row_of_the_panel() {
    let b = grid_1x2();
    let before = graph_spec(&b);
    assert_eq!(before.frame_widths.len(), 2);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), panel=(0, 1), plot_width=6, plot_height=4)",
    );
    let after = graph_spec(&b);
    near(after.frame_widths[1], 6.0 * CM);
    near(after.frame_widths[0], before.frame_widths[0]);
    near(after.frame_heights[0], 4.0 * CM);
}

#[test]
fn the_unit_keyword_converts_and_defaults_to_centimetres() {
    let b = bench();
    run(&b, GRAPH);
    for call in [
        "plot_width=72, unit='pt'",
        "plot_width=1, unit='in'",
        "plot_width=25.4, unit='mm'",
        "plot_width=2.54",
    ] {
        // Move away from 72 pt first so that each call really writes.
        run(
            &b,
            "pyst.set_graph(pyst.resolve('[G]'), plot_width=10, unit='cm')",
        );
        run(&b, &format!("pyst.set_graph(pyst.resolve('[G]'), {call})"));
        near(graph_spec(&b).frame_widths[0], 72.0);
    }
}

#[test]
fn graph_size_reads_in_the_unit_asked_for() {
    let b = bench();
    run(&b, GRAPH);
    let history = history_len(&b);
    let out = run(
        &b,
        "g = pyst.resolve('[G]')\n\
         pyst.set_graph(g, plot_width=6, plot_height=4)\n\
         s = pyst.graph_size(g, unit='mm')\n\
         print(repr(s['plot_widths'][0]), repr(s['plot_heights'][0]), repr(s['width']), repr(s['height']))\n",
    );
    let numbers: Vec<f64> = out
        .stdout
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    near(numbers[0], 60.0);
    near(numbers[1], 40.0);
    let size = layout_of(&graph_spec(&b)).size;
    near(numbers[2], f64::from(size.0) * 25.4 / 72.0);
    near(numbers[3], f64::from(size.1) * 25.4 / 72.0);
    assert_eq!(history_len(&b), history + 1);
}

#[test]
fn a_size_key_on_a_legacy_graph_fixes_the_drawn_plot_areas_in_one_entry() {
    let b = legacy_graph();
    let drawn = layout_of(&graph_spec(&b));
    assert!(!drawn.is_explicit());
    let history = history_len(&b);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), plot_height=5)");
    let spec = graph_spec(&b);
    assert_eq!(spec.frame_widths, drawn.frame_widths);
    assert_eq!(spec.frame_heights.len(), 1);
    near(spec.frame_heights[0], 5.0 * CM);
    assert_eq!(history_len(&b), history + 1);
    b.session.undo().unwrap();
    let undone = graph_spec(&b);
    assert!(undone.frame_widths.is_empty() && undone.frame_heights.is_empty());
}

#[test]
fn a_size_call_that_changes_nothing_leaves_a_legacy_graph_legacy() {
    let b = legacy_graph();
    let history = b.session.snapshot().unwrap().history;
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), fixed_size=False, plot_left=None)",
    );
    assert_eq!(b.session.snapshot().unwrap().history, history);
    assert!(graph_spec(&b).frame_widths.is_empty());
}

#[test]
fn size_keys_and_other_keys_share_a_single_entry() {
    let b = bench();
    run(&b, GRAPH);
    let before = graph_spec(&b);
    let history = history_len(&b);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), plot_width=8, plot_left=3, fixed_size=True, title='T')",
    );
    let after = graph_spec(&b);
    assert_eq!(after.title, "T");
    assert!(after.fixed_figure);
    assert_eq!(history_len(&b), history + 1);
    b.session.undo().unwrap();
    assert_eq!(graph_spec(&b), before);
}

#[test]
fn a_refused_size_leaves_the_graph_and_the_history_alone() {
    let b = bench();
    run(&b, GRAPH);
    let before = graph_spec(&b);
    let history = b.session.snapshot().unwrap().history;
    let message = refused(&b, "pyst.set_graph(g, title='T', plot_width=0.1)");
    assert!(
        message.contains("plot_width must be between 1.27 and 70.56 cm"),
        "{message}"
    );
    for body in [
        "pyst.set_graph(g, plot_width=1000)",
        "pyst.set_graph(g, plot_width=float('nan'))",
        "pyst.set_graph(g, plot_left=-1)",
        "pyst.set_graph(g, plot_left=100)",
        "pyst.set_graph(g, fixed_size=1)",
        "pyst.set_graph(g, plot_width=6, unit='furlong')",
        "pyst.set_graph(g, panel=(0, 5), plot_width=6)",
        "pyst.graph_size(g, unit='furlong')",
    ] {
        refused(&b, body);
    }
    let in_points = refused(&b, "pyst.set_graph(g, plot_width=20, unit='pt')");
    assert!(in_points.contains("between 36 and 2000 pt"), "{in_points}");
    assert_eq!(graph_spec(&b), before);
    assert_eq!(b.session.snapshot().unwrap().history, history);
}

#[test]
fn the_bounds_are_inclusive_to_the_rounding_of_the_conversion() {
    let b = bench();
    run(&b, GRAPH);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), plot_width=1.27)");
    near(graph_spec(&b).frame_widths[0], 36.0);
    // 2000 pt written in millimetres lands 2e-13 above the bound.
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), plot_width=705.5555555555555, unit='mm')",
    );
    near(graph_spec(&b).frame_widths[0], 2000.0);
}

#[test]
fn graph_size_of_a_legacy_graph_reads_the_drawn_areas_and_writes_nothing() {
    let b = legacy_graph();
    let drawn = layout_of(&graph_spec(&b));
    let history = b.session.snapshot().unwrap().history;
    let out = run(
        &b,
        &format!(
            "s = pyst.graph_size(pyst.resolve('[G]'), unit='pt')\n\
             print(s['plot_widths'] == {:?}, s['plot_heights'] == {:?}, s['plot_left'], s['plot_top'], s['fixed_size'])\n",
            drawn.frame_widths, drawn.frame_heights
        ),
    );
    assert_eq!(out.stdout.trim(), "True True None None False");
    assert_eq!(b.session.snapshot().unwrap().history, history);
    assert!(graph_spec(&b).frame_widths.is_empty());
}

#[test]
fn left_below_the_minimum_is_stored_and_read_raised_and_auto_is_none() {
    let b = bench();
    run(&b, GRAPH);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), plot_left=0, unit='pt')",
    );
    assert_eq!(graph_spec(&b).figure_left, Some(0.0));
    let min_left = f64::from(layout_of(&graph_spec(&b)).min_left);
    assert!(min_left > 0.0);
    let out = run(
        &b,
        "print(pyst.graph_size(pyst.resolve('[G]'), unit='pt')['plot_left'])",
    );
    near(out.stdout.trim().parse().unwrap(), min_left);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), plot_left=None)");
    assert_eq!(graph_spec(&b).figure_left, None);
    let out = run(
        &b,
        "print(pyst.graph_size(pyst.resolve('[G]'))['plot_left'])",
    );
    assert_eq!(out.stdout.trim(), "None");
}

#[test]
fn fixed_size_is_stored_and_read() {
    let b = bench();
    run(&b, GRAPH);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), fixed_size=True)");
    assert!(graph_spec(&b).fixed_figure);
    let out = run(
        &b,
        "print(pyst.graph_size(pyst.resolve('[G]'))['fixed_size'])",
    );
    assert_eq!(out.stdout.trim(), "True");
}

#[test]
fn a_legacy_graph_too_large_to_store_its_areas_refuses_a_size_key() {
    let b = bench();
    run(&b, GRAPH);
    let mut spec = graph_spec(&b);
    spec.frame_widths.clear();
    spec.frame_heights.clear();
    // 20 x 20 cells of one layer each: beyond the 256 cells the layout stores.
    let template = spec.panels[0].clone();
    let mut panels = Vec::new();
    for row in 0..20 {
        for column in 0..20 {
            let mut panel = template.clone();
            panel.row = row;
            panel.column = column;
            panels.push(panel);
        }
    }
    spec.panels = stokesforge_plot::Panels::new(panels);
    store(&b, spec);
    let history = b.session.snapshot().unwrap().history;
    refused(&b, "pyst.set_graph(g, plot_width=6)");
    assert_eq!(b.session.snapshot().unwrap().history, history);
}
