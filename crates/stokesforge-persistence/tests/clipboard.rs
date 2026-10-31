//! The transport of objects preserves values and reuses the rules of the `.stf`.

use stokesforge_core::{Column, ColumnType, NodeCopy, NodeKind, Project, ValueBlock};
use stokesforge_persistence::clipboard::{decode_objects, encode_objects};

#[test]
fn missing_values_nan_and_metadata_survive_the_transport() {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let nan = f64::from_bits(0x7ff8_0000_0000_0042);
    let numeric = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[nan, -0.0, 1.5]))
        .unwrap();
    let text = project
        .add_column(sheet, None, Some("B"), Column::new(ColumnType::Mixed))
        .unwrap();
    project
        .set_values(
            text,
            0,
            ValueBlock::Mixed(vec![
                stokesforge_core::MixedCell::Empty,
                stokesforge_core::MixedCell::Text(String::new()),
                stokesforge_core::MixedCell::Text("Café".into()),
            ]),
        )
        .unwrap();
    let mut meta = project
        .tree()
        .node(numeric)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.unit = "nm".into();
    project.set_column_meta(numeric, meta).unwrap();
    let copy = NodeCopy::capture(project.tree(), book).unwrap();
    let encoded = encode_objects("source", &[copy]).unwrap();
    let (origin, copies) = decode_objects(&encoded).unwrap();
    assert_eq!(origin, "source");
    let mut destination = Project::new("Destination").unwrap();
    let root = destination.root();
    let book = destination.insert_copy(root, &copies[0], "Copy").unwrap();
    let sheet = destination.tree().children(book).unwrap()[0];
    let columns = destination.tree().children(sheet).unwrap();
    for (&source, &target) in [numeric, text].iter().zip(columns) {
        assert_eq!(
            project.tree().node(source).unwrap().as_column(),
            destination.tree().node(target).unwrap().as_column()
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copy.stf");
    stokesforge_persistence::save(&destination, &path).unwrap();
    assert_eq!(
        stokesforge_persistence::load(&path).unwrap().tree(),
        destination.tree()
    );
}

#[test]
fn a_note_and_a_matrix_are_copyable_without_commands_by_nature() {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let note = project.add_note(root, Some("Note1"), "λ = 500 nm").unwrap();
    let matrix = project.add_matrix(root, Some("Matrix1")).unwrap();
    let copies: Vec<_> = [note, matrix]
        .iter()
        .map(|id| NodeCopy::capture(project.tree(), *id).unwrap())
        .collect();
    let (_, copies) = decode_objects(&encode_objects("source", &copies).unwrap()).unwrap();
    let copy = project.insert_copy(root, &copies[0], "Note2").unwrap();
    assert!(
        matches!(project.tree().node(copy).unwrap().kind(), NodeKind::Note { text, .. } if text == "λ = 500 nm")
    );
    assert!(project.insert_copy(root, &copies[1], "Matrix2").is_ok());
    for text in [
        "ordinary text",
        "StokesForge Objects\n0",
        "StokesForge Objects\nzz",
        "StokesForge Objects\n00",
    ] {
        assert!(decode_objects(text).is_err());
    }
}

#[test]
fn external_graph_carries_x_y_errors_without_useless_columns_and_persists() {
    use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec};
    let mut source = Project::new("Source").unwrap();
    let root = source.root();
    let mut columns = Vec::new();
    // Two sheets with the same name, from different workbooks.
    for name in ["Book1", "Book2"] {
        let book = source.add_workbook(root, Some(name)).unwrap();
        let sheet = source.tree().children(book).unwrap()[0];
        for index in 0..3 {
            columns.push(
                source
                    .add_column(sheet, None, None, Column::from_f64(&[index as f64]))
                    .unwrap(),
            );
        }
    }
    let mut series = SeriesSpec::new_xy(columns[0], columns[1]);
    series.x_error = Some(columns[3]);
    series.y_error = Some(columns[4]);
    let mut panel = PanelSpec::default();
    panel.series.push(series.clone());
    // A second series shares X and keeps an already broken reference.
    let broken = stokesforge_core::NodeId::from_raw(999);
    panel.series.push(SeriesSpec::new_xy(columns[0], broken));
    let graph = source
        .add_graph(
            root,
            Some("Graph1"),
            PlotSpec::default().with_panels(Panels::one(panel)),
        )
        .unwrap();
    let copy = NodeCopy::capture(source.tree(), graph).unwrap();
    let (_, copies) = decode_objects(&encode_objects("source", &[copy]).unwrap()).unwrap();
    let mut target = Project::new("Target").unwrap();
    let root = target.root();
    // Force a collision with the source broken reference.
    for _ in 0..1000 {
        target.add_note(root, None, "").unwrap();
    }
    let copied = target
        .insert_copy(root, &copies[0].clone().into_external(), "Graph1")
        .unwrap();
    let NodeKind::Graph { spec, .. } = target.tree().node(copied).unwrap().kind() else {
        panic!("graph");
    };
    let s = &spec.panels[0].series;
    assert_eq!(s[0].x, s[1].x);
    assert!(!target.tree().contains(s[1].y));
    assert_ne!(s[1].y, broken);
    for (new, old) in s[0]
        .referenced_columns()
        .into_iter()
        .zip(series.referenced_columns())
    {
        assert_eq!(
            target
                .tree()
                .node(new)
                .unwrap()
                .as_column()
                .unwrap()
                .float_values(),
            source
                .tree()
                .node(old)
                .unwrap()
                .as_column()
                .unwrap()
                .float_values()
        );
    }
    let books: Vec<_> = target
        .tree()
        .walk_all()
        .into_iter()
        .filter(|id| {
            matches!(
                target.tree().node(*id).unwrap().kind(),
                NodeKind::Workbook { .. }
            )
        })
        .collect();
    assert_eq!(books.len(), 1);
    assert!(
        matches!(target.tree().node(books[0]).unwrap().kind(), NodeKind::Workbook { meta, .. } if meta.hidden)
    );
    assert_eq!(target.tree().walk(books[0]).unwrap().len(), 7); // workbook + two sheets + four columns
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.stf");
    stokesforge_persistence::save(&target, &path).unwrap();
    let mut loaded = stokesforge_persistence::load(&path).unwrap();
    assert_eq!(loaded.tree(), target.tree());
    let next = loaded.add_note(root, None, "").unwrap();
    assert_ne!(next, s[1].y);
    // An error after the insertion of the data restores the whole model.
    let before = target.tree().clone();
    assert!(target
        .insert_copy(root, &copies[0].clone().into_external(), "Graph1")
        .is_err());
    assert_eq!(target.tree(), &before);
}

/// #166: a "line" series reads all the x' columns of its sheet, not only
/// its anchor — copying the graph to another project carries all of them
/// (and only them), and the series resolves identically.
#[test]
fn external_graph_with_line_series_carries_the_x_prime_columns_of_the_sheet() {
    use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec};
    let mut source = Project::new("Source").unwrap();
    let root = source.root();
    let book = source.add_workbook(root, Some("Book1")).unwrap();
    let sheet = source.tree().children(book).unwrap()[0];
    let mut ids = Vec::new();
    for (name, value, x_prime) in [
        ("A", 1.0, Some(0.0)),
        ("B", 2.0, Some(10.0)),
        ("C", 3.0, None),
    ] {
        let id = source
            .add_column(sheet, None, Some(name), Column::from_f64(&[value]))
            .unwrap();
        let mut meta = source
            .tree()
            .node(id)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .clone();
        meta.x_prime = x_prime;
        source.set_column_meta(id, meta).unwrap();
        ids.push(id);
    }
    let mut series = SeriesSpec::new(ids[0]);
    series.row = Some(0);
    let mut panel = PanelSpec::default();
    panel.series.push(series);
    let graph = source
        .add_graph(
            root,
            Some("Graph1"),
            PlotSpec::default().with_panels(Panels::one(panel)),
        )
        .unwrap();
    let copy = NodeCopy::capture(source.tree(), graph).unwrap();
    let (_, copies) = decode_objects(&encode_objects("source", &[copy]).unwrap()).unwrap();
    let mut target = Project::new("Target").unwrap();
    let root = target.root();
    let copied = target
        .insert_copy(root, &copies[0].clone().into_external(), "Graph1")
        .unwrap();
    let NodeKind::Graph { spec, .. } = target.tree().node(copied).unwrap().kind() else {
        panic!("graph");
    };
    let resolved = stokesforge_core::resolve_spec(target.tree(), spec);
    let series = &resolved.panels[0].series[0];
    assert_eq!(series.x.as_ref().unwrap().as_slice(), [0.0, 10.0]);
    assert_eq!(series.y.as_ref().unwrap().as_slice(), [1.0, 2.0]);
}

/// #391: a folder travels whole (subfolder, workbook, graph, note, calculation)
/// and comes back with the same tree and the calculation kept.
#[test]
fn a_folder_with_a_calculation_survives_the_transport() {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{CalculationDraft, ColumnMeta, OutputDraft, SeriesRef, SeriesResult};
    use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec};
    let mut source = Project::new("Source").unwrap();
    let root = source.root();
    let folder = source.add_folder(root, "Folder").unwrap();
    source.add_folder(folder, "Inner").unwrap();
    let book = source.add_workbook(folder, Some("Book1")).unwrap();
    let sheet = source.tree().children(book).unwrap()[0];
    let x = source
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let y = source
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .unwrap();
    source.add_note(folder, Some("Note1"), "hello").unwrap();
    let mut panel = PanelSpec::default();
    panel.series.push(SeriesSpec::new_xy(x, y));
    source
        .add_graph(
            folder,
            Some("Graph1"),
            PlotSpec::default().with_panels(Panels::one(panel)),
        )
        .unwrap();
    source
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Divide,
            },
            series: vec![SeriesRef {
                x: Some(x),
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![1.0, 2.0, 3.0],
            }],
            results: vec![SeriesResult::default()],
            keep_results: true,
            auto_recalc: true,
            low_r2_dialog: true,
        })
        .unwrap();
    let copy = NodeCopy::capture(source.tree(), folder).unwrap();
    assert_eq!(copy.calculations().len(), 1);
    let (_, copies) =
        decode_objects(&encode_objects("source", std::slice::from_ref(&copy)).unwrap()).unwrap();
    assert_eq!(copies[0].nodes().len(), copy.nodes().len());
    assert_eq!(copies[0].calculations().len(), 1);
    assert!(copies[0].graph_data().is_empty());
    // Same origin: pasted as a local copy, the calculation reads the copy.
    let pasted = source
        .insert_copy(root, &copies[0], "Folder copy 1")
        .unwrap();
    assert_eq!(
        source.tree().walk(pasted).unwrap().len(),
        copy.nodes().len()
    );
    assert_eq!(source.tree().calculations().len(), 2);
}

/// #391, external paste: a graph reads the pasted workbook with no hidden
/// workbook; a formula reading outside the folder is `#REF`; a calculation
/// that reads outside the folder is dropped (plain columns).
#[test]
fn an_external_folder_paste_reads_its_own_workbook_and_reserves_what_is_outside() {
    use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec};
    let mut source = Project::new("Source").unwrap();
    let root = source.root();
    let outside = source.add_workbook(root, Some("Outside")).unwrap();
    let outside_sheet = source.tree().children(outside).unwrap()[0];
    source
        .add_column(outside_sheet, None, Some("A"), Column::from_f64(&[9.0]))
        .unwrap();
    let folder = source.add_folder(root, "Folder").unwrap();
    let book = source.add_workbook(folder, Some("Book1")).unwrap();
    let sheet = source.tree().children(book).unwrap()[0];
    let a = source
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let f = source
        .add_column(sheet, None, Some("F"), Column::from_f64(&[0.0]))
        .unwrap();
    let formula = source
        .tree()
        .formula_from_letters(f, "max([Outside]Sheet1!A)")
        .unwrap();
    let mut meta = source
        .tree()
        .node(f)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.formula = Some(formula);
    source.set_column_meta(f, meta).unwrap();
    let mut panel = PanelSpec::default();
    panel.series.push(SeriesSpec::new(a));
    source
        .add_graph(
            folder,
            Some("Graph1"),
            PlotSpec::default().with_panels(Panels::one(panel)),
        )
        .unwrap();
    let copy = NodeCopy::capture(source.tree(), folder).unwrap();
    // The column read from inside the folder is not embedded.
    assert!(copy.graph_data().is_empty());
    let (_, copies) = decode_objects(&encode_objects("source", &[copy]).unwrap()).unwrap();
    let mut target = Project::new("Target").unwrap();
    let root = target.root();
    let pasted = target
        .insert_copy(root, &copies[0].clone().into_external(), "Folder copy 1")
        .unwrap();
    let books: Vec<_> = target
        .tree()
        .walk_all()
        .into_iter()
        .filter(|id| {
            matches!(
                target.tree().node(*id).unwrap().kind(),
                NodeKind::Workbook { .. }
            )
        })
        .collect();
    assert_eq!(books.len(), 1, "no hidden workbook for what is inside");
    let find = |parent, name: &str| {
        *target
            .tree()
            .children(parent)
            .unwrap()
            .iter()
            .find(|id| target.tree().node(**id).unwrap().name() == name)
            .unwrap()
    };
    let pasted_sheet = target.tree().children(find(pasted, "Book1")).unwrap()[0];
    let pasted_a = find(pasted_sheet, "A");
    let NodeKind::Graph { spec, .. } = target.tree().node(find(pasted, "Graph1")).unwrap().kind()
    else {
        panic!("graph");
    };
    assert_eq!(spec.panels[0].series[0].y, pasted_a);
    let pasted_f = find(pasted_sheet, "F");
    assert!(
        target.tree().formula_has_deleted_reference(pasted_f),
        "a formula reading outside the folder is #REF"
    );
}
