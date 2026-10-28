//! The tests of the demo. Each has its **witness**: a neighboring case where
//! the same check must fail, so that a test that no longer checks anything
//! shows (the test itself was tried by mutation: seed changed, graph with a
//! broken reference, Gaussian dataset replaced by a constant, recalculation
//! removed from `demo_session`).

use std::collections::BTreeSet;

use stokesforge_analysis::ops::{Operation, OperationKind};
use stokesforge_core::{
    compute, resolve_spec, Column, ColumnType, NodeId, NodeKind, Project, Tree,
};
use stokesforge_plot::{AxisScale, PlotSpec, PlotType, SeriesAxis};
use stokesforge_session::Session;

use super::analysis::{self, Dataset, Expected};
use super::demo_project;
use super::noise::Noise;
use crate::session_for_startup;
use stokesforge_app_state::startup::StartupSource;

/// The graphs of the project, `(name, spec)`, in the order of the names.
fn graphs(project: &Project) -> Vec<(String, PlotSpec)> {
    let mut graphs: Vec<(String, PlotSpec)> = project
        .tree()
        .iter()
        .filter_map(|(_, node)| match node.kind() {
            NodeKind::Graph { spec, .. } => Some((node.name().to_string(), spec.clone())),
            _ => None,
        })
        .collect();
    graphs.sort_by(|a, b| a.0.cmp(&b.0));
    graphs
}

fn column_by_path(project: &Project, path: &str) -> Column {
    let id = project.resolve_path(path).unwrap();
    stokesforge_core::resolve_column(project.tree(), id)
        .unwrap()
        .clone()
}

fn numbers(column: &Column) -> Vec<f64> {
    (0..column.len())
        .map(|row| column.values().number_at(row))
        .collect()
}

#[test]
fn demo_columns_and_script_follow_the_grid_letters() {
    let project = demo_project().unwrap();
    for (sheet, names) in [
        ("Sheet1", &["A", "B", "C"][..]),
        ("Sheet2", &["A"][..]),
        ("Spectrum", &["A", "B"][..]),
    ] {
        let id = project.resolve_path(&format!("[Book1]{sheet}")).unwrap();
        let actual: Vec<_> = project
            .tree()
            .children(id)
            .unwrap()
            .iter()
            .map(|id| project.tree().node(*id).unwrap().name())
            .collect();
        assert_eq!(actual, names);
    }
    let path = crate::EXAMPLE_SCRIPT.split('"').nth(1).unwrap();
    let id = project.resolve_path(path).unwrap();
    assert_eq!(id, project.resolve_path("[Book1]Sheet1!A").unwrap());
    let column = stokesforge_core::resolve_column(project.tree(), id).unwrap();
    assert_eq!(column.len(), 5);
}

/// Window names (workbook, graph, matrix, note) are unique across the whole
/// project: the `[Name]` path designates them without a folder.
fn duplicates(project: &Project) -> Vec<String> {
    let mut seen = BTreeSet::new();
    project
        .tree()
        .iter()
        .filter(|(_, node)| node.kind().is_window())
        .map(|(_, node)| node.name().to_string())
        .filter(|name| !seen.insert(name.clone()))
        .collect()
}

#[test]
fn window_names_are_unique_across_the_project() {
    let project = demo_project().unwrap();
    assert_eq!(duplicates(&project), Vec::<String>::new());

    // Witness: a second "Types", even filed in another folder, shows.
    let mut edited = demo_project().unwrap();
    let root = edited.root();
    edited
        .add_graph(root, Some("Types"), PlotSpec::empty(""))
        .unwrap();
    assert_eq!(duplicates(&edited), vec!["Types".to_string()]);
}

fn draw(project: &Project, spec: &PlotSpec) -> stokesforge_render_egui::DrawnPlot {
    let measurer = stokesforge_render_egui::TextMeasurer::default();
    let rect = eframe::egui::Rect::from_min_size(
        eframe::egui::Pos2::ZERO,
        eframe::egui::vec2(640.0, 480.0),
    );
    stokesforge_render_egui::collect_shapes(
        &mut Vec::new(),
        rect,
        &measurer,
        &resolve_spec(project.tree(), spec),
    )
    .unwrap()
}

/// #103: no demo graph reserves a title, like the graphs created by the user;
/// an explicit title stays possible and is drawn.
#[test]
fn no_demo_graph_reserves_a_title_but_an_explicit_title_stays_possible() {
    let project = demo_project().unwrap();
    let all = graphs(&project);
    assert_eq!(all.len(), 7, "Graph1 and the six demo graphs");
    for (name, spec) in &all {
        assert!(
            spec.title.is_empty(),
            "{name}: the demo gives no default title"
        );
        let drawing = draw(&project, spec);
        assert_eq!(drawing.title_height, 0, "{name}");
        assert!(drawing.title.is_none(), "{name}");
    }

    // Witness: removing the default does not disable the rendering of a chosen
    // title.
    let (_, graph1) = all.iter().find(|(name, _)| name == "Graph1").unwrap();
    let without_title = draw(&project, graph1);
    let mut title = graph1.clone();
    title.title = "Chosen spectrum".into();
    let with_title = draw(&project, &title);
    assert!(with_title.title_height > 0);
    assert!(with_title.title.is_some());
    assert!(with_title.panels[0].pixel_origin.1 > without_title.panels[0].pixel_origin.1);
}

#[test]
fn every_graph_resolves_without_broken_reference_and_draws() {
    let project = demo_project().unwrap();
    for (name, spec) in graphs(&project) {
        let resolved = resolve_spec(project.tree(), &spec);
        assert_eq!(resolved.broken_series().count(), 0, "{name}");
        assert!(resolved.point_count() > 0, "{name} plots no point");
        draw(&project, &spec);
    }

    // Witness: a deleted column breaks the series that cites it.
    let mut edited = demo_project().unwrap();
    let (_, spec) = graphs(&edited)
        .into_iter()
        .find(|(name, _)| name == "Panels")
        .unwrap();
    let victim: NodeId = spec.referenced_columns()[0];
    edited.remove(victim).unwrap();
    assert!(
        resolve_spec(edited.tree(), &spec).broken_series().count() > 0,
        "the detection of a broken reference no longer sees anything"
    );
}

/// The values of a series column, by identifier.
fn values(tree: &Tree, column: NodeId) -> Vec<f64> {
    numbers(stokesforge_core::resolve_column(tree, column).unwrap())
}

/// The series of a logarithmic-scale axis that carry a value ≤ 0 or non-finite
/// (invariant 10): `(graph, series name)`.
fn log_series_not_positive(tree: &Tree, name: &str, spec: &PlotSpec) -> Vec<(String, String)> {
    let mut faulty = Vec::new();
    for panel in spec.panels.as_slice() {
        for series in &panel.series {
            let (y_axis, x_axis) = (
                panel.axis(series.axis.side()).scale == AxisScale::Log,
                panel.bottom.scale == AxisScale::Log,
            );
            let mut read_values = Vec::new();
            if y_axis {
                read_values.extend(values(tree, series.y));
            }
            if let (true, Some(x)) = (x_axis, series.x) {
                read_values.extend(values(tree, x));
            }
            if read_values.iter().any(|v| v.is_nan() || *v <= 0.0) {
                faulty.push((name.to_string(), series.name.clone()));
            }
        }
    }
    faulty
}

#[test]
fn log_axes_carry_only_strictly_positive_data() {
    let project = demo_project().unwrap();
    let mut log_scales = 0;
    for (name, spec) in graphs(&project) {
        log_scales += spec
            .panels
            .as_slice()
            .iter()
            .filter(|p| p.left.scale == AxisScale::Log || p.bottom.scale == AxisScale::Log)
            .count();
        assert_eq!(
            log_series_not_positive(project.tree(), &name, &spec),
            Vec::new()
        );
    }
    assert!(
        log_scales > 0,
        "the demo shows at least one logarithmic axis"
    );

    // Witness: the first column of the kinetics (t = 0) contains zeros, and
    // the check finds them.
    let kinetics = project.resolve_path("[Book1]Kinetics").unwrap();
    let column = project.tree().children(kinetics).unwrap()[0];
    let mut spec = PlotSpec::empty("");
    let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
    panel.left.scale = AxisScale::Log;
    panel.series.push(stokesforge_plot::SeriesSpec::new(column));
    spec = spec.with_panels(panel);
    assert_eq!(
        log_series_not_positive(project.tree(), "Witness", &spec).len(),
        1
    );
}

/// Replays the calculation of a dataset on fresh columns and returns what
/// deviates from what it must recover (empty: all is well).
fn deviations(dataset: &Dataset) -> Vec<String> {
    let mut project = Project::new("Trial").unwrap();
    let root = project.root();
    let columns = analysis::build(&mut project, root, dataset).unwrap();
    let computed = match compute(
        project.tree(),
        &dataset.operation,
        &dataset.series(&columns),
        columns.reference.as_ref(),
    ) {
        Ok(computed) => computed,
        Err(error) => return vec![format!("{}: {error}", dataset.name)],
    };
    let mut deviations = Vec::new();
    for expected in &dataset.expected {
        match *expected {
            Expected::Value(key, index, value, tolerance) => {
                let read = computed.results[0]
                    .values
                    .iter()
                    .find(|v| v.key == key && v.index == index)
                    .map(|v| v.value);
                if !read.is_some_and(|read| (read - value).abs() <= tolerance) {
                    deviations.push(format!(
                        "{}: {key} {index:?} = {read:?}, expected {value} ± {tolerance}",
                        dataset.name
                    ));
                }
            }
            Expected::Row(key, row, value, tolerance) => {
                let read = computed
                    .columns
                    .iter()
                    .find(|c| c.key == key)
                    .and_then(|c| c.data.get(row).copied());
                if !read.is_some_and(|read| (read - value).abs() <= tolerance) {
                    deviations.push(format!(
                        "{}: {key}[{row}] = {read:?}, expected {value} ± {tolerance}",
                        dataset.name
                    ));
                }
            }
        }
    }
    deviations
}

#[test]
fn every_analysis_dataset_recovers_its_parameters() {
    for dataset in analysis::datasets() {
        assert_eq!(deviations(&dataset), Vec::<String>::new());
    }

    // Witness: the Gaussian dataset whose data are replaced by a constant no
    // longer recovers its parameters (or its fit fails).
    let mut constant = analysis::datasets()
        .into_iter()
        .find(|j| j.name == "FitGaussian")
        .unwrap();
    constant.y = vec![1.0; constant.x.len()];
    assert!(!deviations(&constant).is_empty());
}

#[test]
fn datasets_cover_every_operation_family() {
    let families = |datasets: &[Dataset]| -> BTreeSet<&'static str> {
        datasets.iter().map(|j| j.operation.kind().key()).collect()
    };
    let expected_kinds: BTreeSet<&'static str> =
        OperationKind::ALL.iter().map(|kind| kind.key()).collect();
    assert_eq!(families(&analysis::datasets()), expected_kinds);

    // Witness: without the peaks dataset, the family is missing.
    let without_peaks: Vec<Dataset> = analysis::datasets()
        .into_iter()
        .filter(|j| !matches!(j.operation, Operation::Peaks { .. }))
        .collect();
    assert_ne!(families(&without_peaks), expected_kinds);
}

#[test]
fn datasets_are_placed_in_the_project_and_some_have_their_calculation() {
    let project = demo_project().unwrap();
    let tree = project.tree();
    for dataset in analysis::datasets() {
        let workbook = project
            .resolve_path(&format!("[{}]", dataset.name))
            .unwrap();
        assert!(tree.node(workbook).is_some(), "{}", dataset.name);
    }
    // The calculations already done: visible in the Results pane (results kept).
    let done = tree.calculations();
    let expected = analysis::datasets().iter().filter(|j| j.computed).count();
    assert_eq!(done.len(), expected);
    assert!(expected >= 3);
    for calculation in done.values() {
        assert!(calculation.keep_results);
        assert!(!calculation.results.is_empty());
        assert!(calculation.results.iter().all(|r| !r.values.is_empty()));
        assert!(calculation.live_outputs(tree).count() > 0);
        assert!(calculation.stale.is_none());
    }
}

#[test]
fn noise_and_demo_are_deterministic() {
    // Pinned values: changing the seed or the algorithm moves them.
    let mut noise = Noise::new(7);
    let pinned: Vec<f64> = (0..3).map(|_| noise.gauss(1.0)).collect();
    assert_eq!(pinned.len(), 3);
    for (got, expected) in pinned.iter().zip(PINNED_NOISE) {
        assert!((got - expected).abs() < 1e-9, "{got} ≠ {expected}");
    }
    let first = column_by_path(&demo_project().unwrap(), "[Measurements]Sheet1!B");
    let second = column_by_path(&demo_project().unwrap(), "[Measurements]Sheet1!B");
    assert_eq!(numbers(&first), numbers(&second));
    assert!((numbers(&first)[0] - PINNED_MEASURE).abs() < 1e-9);

    // Witness: another seed does not give the same values.
    let mut other = Noise::new(8);
    let different: Vec<f64> = (0..3).map(|_| other.gauss(1.0)).collect();
    assert!(different
        .iter()
        .zip(PINNED_NOISE)
        .all(|(got, expected)| (got - expected).abs() > 1e-9));
}

const PINNED_NOISE: [f64; 3] = [0.312_483_526_6, 0.594_715_800_3, -0.104_312_865_4];
const PINNED_MEASURE: f64 = 94.427_690_068_6;

#[test]
fn demo_startup_recalculates_formula_columns() {
    let (session, _, _) = session_for_startup(
        &StartupSource::DemoMemory(std::path::PathBuf::new()),
        stokesforge_app_state::preferences::default_load_limits(),
    )
    .unwrap();
    let snapshot = session.handle().snapshot().unwrap();
    let tree = snapshot.tree;
    let column = |name: &str| -> Vec<f64> {
        let types = sheet_of_workbook(&tree, "Types");
        let id = tree.child_by_name(types, name).unwrap().unwrap();
        values(&tree, id)
    };
    // F = A*2+1 on A = 0, 1, 2 …
    let f = column("F");
    assert_eq!(f.len(), 12);
    assert_eq!(&f[..3], &[1.0, 3.0, 5.0]);
    // G = (B-A)/2: NaN where B is missing.
    let g = column("G");
    assert_eq!(g[0], 10.25);
    assert!(g[2].is_nan());
    // H = D+1 reads the mixed column: a number gives a number, a text NaN.
    let h = column("H");
    assert_eq!(h[0], 2.5);
    assert!(h[1].is_nan());
    session.shutdown().unwrap();

    // Witness: without a session or recalculation, the formula column is empty
    // — this is what the user would see if `recalculate` left
    // `demo_session`.
    let raw = demo_project().unwrap();
    assert_eq!(column_by_path(&raw, "[Types]Sheet1!F").len(), 0);
}

/// The first sheet of the workbook `name`.
fn sheet_of_workbook(tree: &Tree, name: &str) -> NodeId {
    let workbook = tree
        .iter()
        .find(|(_, node)| node.name() == name && node.kind().is_window())
        .map(|(id, _)| id)
        .unwrap();
    tree.children(workbook).unwrap()[0]
}

#[test]
fn demo_round_trips_through_stf_via_the_session() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("demo.stf");
    let (session, _, _) = session_for_startup(
        &StartupSource::DemoMemory(std::path::PathBuf::new()),
        stokesforge_app_state::preferences::default_load_limits(),
    )
    .unwrap();
    let before = session.handle().snapshot().unwrap();
    session.handle().save(&path).unwrap();
    session.shutdown().unwrap();

    let reloaded = Session::spawn(Project::new("Empty").unwrap(), None).unwrap();
    let (_, report) = reloaded
        .handle()
        .load_with_limit(
            &path,
            stokesforge_app_state::preferences::DEFAULT_MAX_FILE_BYTES,
        )
        .unwrap();
    assert!(report.is_clean(), "{report:?}");
    let after = reloaded.handle().snapshot().unwrap();
    assert_eq!(after.tree.iter().count(), before.tree.iter().count());

    let compare = |tree: &Tree| -> Vec<(String, PlotSpec)> {
        let mut graphs: Vec<(String, PlotSpec)> = tree
            .iter()
            .filter_map(|(_, node)| match node.kind() {
                NodeKind::Graph { spec, .. } => Some((node.name().to_string(), spec.clone())),
                _ => None,
            })
            .collect();
        graphs.sort_by(|a, b| a.0.cmp(&b.0));
        graphs
    };
    let (left, right) = (compare(&before.tree), compare(&after.tree));
    assert_eq!(left.len(), 7);
    assert_eq!(left.len(), right.len());
    for ((name, a), (other, b)) in left.iter().zip(&right) {
        assert_eq!(name, other);
        assert_eq!(a, b, "graph {name} changed on the round trip");
    }
    reloaded.shutdown().unwrap();

    // Witness: the comparison sees a difference of spec.
    let mut modified = left[0].1.clone();
    modified.background = "#123456".into();
    assert_ne!(modified, right[0].1);
}

#[test]
fn demo_covers_every_plot_type_and_plot_setting() {
    let covered_types = |project: &Project| -> Vec<PlotType> {
        let mut types: Vec<PlotType> = Vec::new();
        for (_, spec) in graphs(project) {
            for (_, series) in spec.iter_series() {
                if !types.contains(&series.plot_type) {
                    types.push(series.plot_type.clone());
                }
            }
        }
        types
    };
    let expected = [
        PlotType::Scatter,
        PlotType::Line,
        PlotType::LineScatter,
        PlotType::Bar,
        PlotType::Histogram,
        PlotType::ErrorBar,
    ];
    let covered = covered_types(&demo_project().unwrap());
    for expected in &expected {
        assert!(
            covered.contains(expected),
            "{expected:?} absent from the demo"
        );
    }
    assert!(!covered.iter().any(|t| matches!(t, PlotType::Unknown(_))));

    // Witness: `Book1` and `Graph1` alone do not cover everything.
    let mut thin = Project::new("Thin").unwrap();
    let book1 = super::workbooks::book1(&mut thin).unwrap();
    super::graphs::graph1(&mut thin, &book1).unwrap();
    assert!(!expected.iter().all(|t| covered_types(&thin).contains(t)));

    // The promised settings: error bars, right axis, layer, group, "row"
    // series, annotations, four shapes.
    let specs: Vec<PlotSpec> = graphs(&demo_project().unwrap())
        .into_iter()
        .map(|(_, spec)| spec)
        .collect();
    let panels = || specs.iter().flat_map(|s| s.panels.as_slice().iter());
    assert!(panels()
        .flat_map(|p| &p.series)
        .any(|s| s.y_error.is_some()));
    assert!(panels()
        .flat_map(|p| &p.series)
        .any(|s| s.axis == SeriesAxis::Right));
    assert!(panels().any(|p| p.overlay == Some(stokesforge_plot::OverlaySpec::SECOND_AXES)));
    assert!(panels().any(|p| !p.group_ids().is_empty() && !p.group_gradients.is_empty()));
    assert!(panels().flat_map(|p| &p.series).any(|s| s.row.is_some()));
    assert!(panels().any(|p| p.annotations.iter().any(|a| a.text.contains('$'))));
    let shapes: BTreeSet<String> = panels()
        .flat_map(|p| &p.shapes)
        .map(|f| format!("{:?}", f.kind))
        .collect();
    assert_eq!(shapes.len(), 4);
}

#[test]
fn demo_has_every_column_type() {
    let project = demo_project().unwrap();
    let types = |path: &str| column_by_path(&project, path).column_type();
    assert_eq!(types("[Types]Sheet1!A"), ColumnType::Numeric);
    assert_eq!(types("[Types]Sheet1!C"), ColumnType::Mixed);
    assert_eq!(types("[Types]Sheet1!D"), ColumnType::Mixed);
    assert_eq!(types("[Types]Sheet1!E"), ColumnType::DateTime);
    let temperature = column_by_path(&project, "[Types]Sheet1!B");
    assert!(numbers(&temperature).iter().any(|v| v.is_nan()));
    let meta = temperature.meta();
    assert!(
        !meta.unit.is_empty()
            && !meta.long_name.is_empty()
            && !meta.comment.is_empty()
            && !meta.display_format.is_empty()
    );
    for letter in ["F", "G", "H"] {
        assert!(column_by_path(&project, &format!("[Types]Sheet1!{letter}"))
            .meta()
            .formula
            .is_some());
    }
}

/// Every visible text of the demo that is not a French letter or a French
/// abbreviation: the demo is hard-coded English (#379).
///
/// The limit: the check looks for accented letters and `u.a.`. An unaccented
/// French word ("Temps", "Mesures") is **not** caught; the names are reviewed
/// by hand. Legitimate non-ASCII (λ µ ° · − σ ± ⁻¹) passes.
#[test]
fn demo_visible_texts_are_english() {
    const FRENCH_LETTERS: &str = "àâäçéèêëîïôöùûüÿœæÀÂÄÇÉÈÊËÎÏÔÖÙÛÜŸŒÆ";
    let project = demo_project().unwrap();
    let mut texts: Vec<String> = Vec::new();
    for (_, node) in project.tree().iter() {
        texts.push(node.name().to_string());
        match node.kind() {
            NodeKind::Column(column) => {
                let meta = column.meta();
                texts.extend([
                    meta.long_name.clone(),
                    meta.unit.clone(),
                    meta.comment.clone(),
                ]);
                for row in 0..column.len() {
                    if let stokesforge_core::MixedCell::Text(text) = column.values().cell(row) {
                        texts.push(text);
                    }
                }
            }
            NodeKind::Note { text, .. } => texts.push(text.clone()),
            _ => {}
        }
    }
    for (_, spec) in graphs(&project) {
        for panel in spec.panels.iter() {
            texts.push(panel.title.clone());
            for axis in [&panel.bottom, &panel.left, &panel.right, &panel.top] {
                texts.extend([axis.label.clone(), axis.unit.clone()]);
            }
            texts.extend(panel.series.iter().map(|series| series.name.clone()));
            texts.extend(panel.annotations.iter().map(|a| a.text.clone()));
        }
    }
    assert!(texts.len() > 100, "the collection no longer sees the demo");
    let offenders: Vec<&String> = texts
        .iter()
        .filter(|text| text.contains("u.a.") || text.chars().any(|c| FRENCH_LETTERS.contains(c)))
        .collect();
    assert!(
        offenders.is_empty(),
        "French texts in the demo: {offenders:?}"
    );
}
