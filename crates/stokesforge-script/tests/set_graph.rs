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

// --- Free layer placement (#398, #411) -------------------------------------------------

/// The graph `[G]` on a 1 x 2 grid with a Left of 150 pt, put in free placement
/// the way the application does (nothing moves), so that the image does not
/// start at the stored origin.
fn free_1x2() -> Bench {
    let b = grid_1x2();
    let mut spec = graph_spec(&b);
    spec.frame_widths = vec![300.0, 200.0];
    spec.frame_heights = vec![250.0];
    spec.figure_left = Some(150.0);
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    assert!(stokesforge_render_svg::set_layer_placement(
        &mut spec, true, &measurer, &choice
    ));
    store(&b, spec);
    b
}

fn free_frame(spec: &PlotSpec, panel: usize) -> stokesforge_plot::FreeFrame {
    spec.panels[panel].free_frame.expect("a free frame")
}

fn frame_rects(spec: &PlotSpec) -> Vec<(u32, u32, u32, u32)> {
    let layout = layout_of(spec);
    let mut rects: Vec<_> = if layout.free_cells.is_empty() {
        let grid = &layout;
        spec.panels
            .iter()
            .map(|p| grid.frame_rect(p.row, p.column).expect("frame"))
            .collect()
    } else {
        layout.free_cells.iter().map(|c| c.frame).collect()
    };
    rects.sort();
    rects
}

/// Switching a grid graph to free moves nothing: every frame and the image
/// size are the ones of the grid.
#[test]
fn switching_to_free_keeps_every_frame_and_the_image_size() {
    let b = grid_1x2();
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), plot_width=7)");
    let before = graph_spec(&b);
    let (rects, size) = (frame_rects(&before), layout_of(&before).size);
    let history = history_len(&b);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), layer_placement='free')",
    );
    let after = graph_spec(&b);
    assert_eq!(
        after.layer_placement,
        stokesforge_plot::LayerPlacement::Free
    );
    assert_eq!(history_len(&b), history + 1);
    assert_eq!(frame_rects(&after), rects);
    assert_eq!(layout_of(&after).size, size);
}

/// The position and size of one layer; the other layer is untouched, and
/// `graph_size` reads the stored numbers back exactly, in the unit asked for.
#[test]
fn a_free_layer_is_positioned_and_sized_alone_and_read_back() {
    let b = free_1x2();
    let before = graph_spec(&b);
    assert_ne!(layout_of(&before).free_origin, (0.0, 0.0));
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), panel=(0, 1), layer_left=-2, layer_top=1, plot_width=6)",
    );
    let after = graph_spec(&b);
    let frame = free_frame(&after, 1);
    near(frame.left, -2.0 * CM);
    near(frame.top, CM);
    near(frame.width, 6.0 * CM);
    near(frame.height, free_frame(&before, 1).height);
    assert_eq!(free_frame(&after, 0), free_frame(&before, 0));
    let out = run(
        &b,
        "s = pyst.graph_size(pyst.resolve('[G]'))\n\
         assert s['layer_placement'] == 'free'\n\
         assert s['plot_widths'] == [] and s['plot_heights'] == []\n\
         l = s['layers'][1]\n\
         print(l['panel'], l['left'], l['top'], l['width'], len(s['layers']))",
    );
    let line = out.stdout.trim();
    assert!(line.starts_with("(0, 1) "), "{line}");
    let numbers: Vec<f64> = line[7..]
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    near(numbers[0], -2.0);
    near(numbers[1], 1.0);
    near(numbers[2], 6.0);
    assert_eq!(numbers[3], 2.0);
}

/// Refusals leave the graph and the history alone.
#[test]
fn free_placement_refusals_write_nothing() {
    let b = free_1x2();
    let before = graph_spec(&b);
    let history = history_len(&b);
    for (body, needle) in [
        ("pyst.set_graph(g, layer_left=354)", "between"),
        ("pyst.set_graph(g, layer_top=-354)", "between"),
        ("pyst.set_graph(g, plot_left=3)", "layer_left"),
        ("pyst.set_graph(g, plot_top=None)", "layer_top"),
        (
            "pyst.set_graph(g, layer_placement='diagonal')",
            "grid or free",
        ),
        ("pyst.set_graph(g, title='T', layer_left=999)", "between"),
    ] {
        let message = refused(&b, body);
        assert!(message.contains(needle), "{body}: {message}");
    }
    assert_eq!(graph_spec(&b), before);
    assert_eq!(history_len(&b), history);
    // The bound itself is accepted.
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), layer_left=-10000, layer_top=10000, unit='pt')",
    );
}

#[test]
fn layer_position_keys_on_a_grid_graph_are_refused() {
    let b = grid_1x2();
    let before = graph_spec(&b);
    let history = history_len(&b);
    for body in [
        "pyst.set_graph(g, layer_left=1)",
        "pyst.set_graph(g, layer_top=1, title='T')",
    ] {
        let message = refused(&b, body);
        assert!(message.contains("regular grid"), "{body}: {message}");
    }
    assert_eq!(graph_spec(&b), before);
    assert_eq!(history_len(&b), history);
}

/// Free to grid is `compose::set_grid_placement`, one entry, and undo gives
/// the free frames back.
#[test]
fn switching_back_to_the_grid_is_the_core_conversion_in_one_entry() {
    let b = free_1x2();
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), panel=(0, 1), layer_top=50, plot_height=3)",
    );
    let free = graph_spec(&b);
    let mut expected = free.clone();
    assert!(stokesforge_core::compose::set_grid_placement(&mut expected));
    let history = history_len(&b);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), layer_placement='grid')",
    );
    assert_eq!(graph_spec(&b), expected);
    assert_eq!(history_len(&b), history + 1);
    run(&b, "pyst.undo()");
    assert_eq!(graph_spec(&b), free);
}

/// A switch, size keys and a title in one call are one history entry; the
/// switch goes first so the size keys act on its result.
#[test]
fn a_switch_with_size_keys_and_a_title_is_a_single_entry() {
    let b = grid_1x2();
    let history = history_len(&b);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), layer_placement='free', panel=(0, 1), layer_left=12, plot_width=5, title='T')",
    );
    assert_eq!(history_len(&b), history + 1);
    let spec = graph_spec(&b);
    assert_eq!(spec.title, "T");
    near(free_frame(&spec, 1).left, 12.0 * CM);
    near(free_frame(&spec, 1).width, 5.0 * CM);
    // Back to the grid with a size in the same call: the new grid cell is used.
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), layer_placement='grid', panel=(0, 1), plot_width=4)",
    );
    assert_eq!(history_len(&b), history + 2);
}

/// Free placement keeps `fixed_size` and a call that changes nothing writes
/// nothing.
#[test]
fn fixed_size_works_in_free_and_a_repeat_writes_nothing() {
    let b = free_1x2();
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), fixed_size=True)");
    assert!(graph_spec(&b).fixed_figure);
    let history = history_len(&b);
    run(
        &b,
        "g = pyst.resolve('[G]')\npyst.set_graph(g, fixed_size=True, layer_placement='free')",
    );
    assert_eq!(history_len(&b), history);
}

/// `graph_size` lists the layers of a free graph in stacking order, with
/// their `(row, column)`, and an empty list on the grid.
#[test]
fn graph_size_lists_the_free_layers_in_stacking_order() {
    let b = free_1x2();
    let mut spec = graph_spec(&b);
    let mut panels: Vec<_> = spec.panels.iter().cloned().collect();
    panels.swap(0, 1);
    spec.panels = stokesforge_plot::Panels::new(panels);
    store(&b, spec.clone());
    let out = run(
        &b,
        "s = pyst.graph_size(pyst.resolve('[G]'), unit='pt')\n\
         print([l['panel'] for l in s['layers']])\n\
         print([round(l['left'], 3) for l in s['layers']])",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[0], "[(0, 1), (0, 0)]");
    let expected: Vec<String> = spec
        .panels
        .iter()
        .map(|p| {
            format!(
                "{:?}",
                (p.free_frame.unwrap().left * 1000.0).round() / 1000.0
            )
        })
        .collect();
    assert_eq!(lines[1], format!("[{}]", expected.join(", ")));
    let grid = grid_1x2();
    let out = run(
        &grid,
        "s = pyst.graph_size(pyst.resolve('[G]'))\nprint(s['layer_placement'], s['layers'])",
    );
    assert_eq!(out.stdout.trim(), "grid []");
}
