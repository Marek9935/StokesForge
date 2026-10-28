//! Acceptance tests of the project model, written from the brief of the
//! slice "nodes, persistent tree, Project". They go only through the public
//! API of the crate.

use std::sync::Arc;

use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CoreError, Node, NodeId,
    NodeKind, Project, Tree, ValueBlock, WindowMeta,
};

// -- helpers --

fn folder_kind() -> NodeKind {
    NodeKind::Folder {
        children: Vec::new(),
    }
}

fn workbook_kind() -> NodeKind {
    NodeKind::Workbook {
        sheets: Vec::new(),
        meta: WindowMeta::default(),
    }
}

fn column_kind() -> NodeKind {
    NodeKind::Column(Column::new(ColumnType::Numeric))
}

fn name_of(project: &Project, id: NodeId) -> String {
    project
        .tree()
        .get(id)
        .expect("node present")
        .name()
        .to_string()
}

fn child_names(project: &Project, parent: NodeId) -> Vec<String> {
    project
        .tree()
        .children(parent)
        .expect("parent present")
        .iter()
        .map(|id| name_of(project, *id))
        .collect()
}

fn floats(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .get(column)
        .expect("column present")
        .as_column()
        .expect("column node")
        .float_values()
        .expect("numeric column")
        .as_ref()
        .clone()
}

/// Brief fixture: `Demo` → `Experiments` → `[Book1]Sheet1` with `Time` and `Signal`.
struct Demo {
    project: Project,
    root: NodeId,
    folder: NodeId,
    book: NodeId,
    sheet: NodeId,
    time: NodeId,
    signal: NodeId,
}

impl Demo {
    fn new() -> Self {
        let mut project = Project::new("Demo").expect("valid name");
        let root = project.root();
        let folder = project.add_folder(root, "Experiments").expect("folder");
        let book = project.add_workbook(folder, None).expect("workbook");
        let sheet = project.tree().children(book).expect("workbook")[0];
        let time = project
            .add_column(
                sheet,
                None,
                Some("Time"),
                Column::from_f64(&[0.0, 1.0, 2.0]),
            )
            .expect("Time column");
        let signal = project
            .add_column(sheet, None, Some("Signal"), Column::from_f64(&[3.0, 4.0]))
            .expect("Signal column");
        Self {
            project,
            root,
            folder,
            book,
            sheet,
            time,
            signal,
        }
    }
}

/// Empty sheet in a new workbook, for the data tests.
fn sheet_with(columns: &[(&str, &[f64])]) -> (Project, NodeId, Vec<NodeId>) {
    let mut project = Project::new("Data").expect("valid name");
    let root = project.root();
    let book = project.add_workbook(root, None).expect("workbook");
    let sheet = project.tree().children(book).expect("workbook")[0];
    let ids = columns
        .iter()
        .map(|(name, values)| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(values))
                .expect("column")
        })
        .collect();
    (project, sheet, ids)
}

// -- 1. construction --

#[test]
fn criterion_1_new_project() {
    let project = Project::new("Empty").expect("valid name");
    assert_eq!(project.name(), "Empty");
    assert_eq!(project.tree().len(), 1);
    let root = project.root();
    let root_node = project.tree().get(root).expect("root");
    assert!(root_node.kind().is_folder());
    assert!(root_node.children().is_empty());
    assert_eq!(project.path_of(root).expect("path"), "/");
    assert_eq!(project.next_id_hint(), 2);
}

#[test]
fn criterion_2_default_window_names() {
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();

    let book1 = project.add_workbook(root, None).expect("workbook");
    assert_eq!(name_of(&project, book1), "Book1");
    assert_eq!(child_names(&project, book1), vec!["Sheet1".to_string()]);

    let book2 = project.add_workbook(root, None).expect("workbook");
    assert_eq!(name_of(&project, book2), "Book2");

    let graph = project
        .add_graph(root, None, Default::default())
        .expect("graph");
    assert_eq!(name_of(&project, graph), "Graph1");

    let matrix = project.add_matrix(root, None).expect("matrix");
    assert_eq!(name_of(&project, matrix), "MBook1");

    let note = project.add_note(root, None, "hello").expect("note");
    assert_eq!(name_of(&project, note), "Note1");
    match project.tree().get(note).expect("note").kind() {
        NodeKind::Note { text, .. } => assert_eq!(text, "hello"),
        other => panic!("expected a note, got {other:?}"),
    }
}

#[test]
fn criterion_3_spreadsheet_style_column_names() {
    let (mut project, sheet, _) = sheet_with(&[]);
    for expected in ["A", "B", "C"] {
        let id = project
            .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
            .expect("column");
        assert_eq!(name_of(&project, id), expected);
    }

    // 26 columns present → the next one is named AA.
    let (mut project, sheet, _) = sheet_with(&[]);
    for _ in 0..26 {
        project
            .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
            .expect("column");
    }
    assert_eq!(
        child_names(&project, sheet).last().map(String::as_str),
        Some("Z")
    );
    let id = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("column");
    assert_eq!(name_of(&project, id), "AA");

    // A name already taken is skipped — never reassigned, even if it occupies a
    // rank earlier than the number of columns already present. D1
    // (`guide-de-correction-robustesse.md` §F7) starts the search from the
    // NUMBER of columns already present rather than from "A": finding the
    // smallest free gap would mean rebuilding the list of taken names
    // for every column added, precisely the O(n³) cost that D1
    // removes. A single column exists ("B") when the first
    // automatic one is requested, so the first candidate tried is "B"
    // (already taken), then "C" (free) — not "A", which stays a valid name
    // but is no longer searched.
    let (mut project, sheet, _) = sheet_with(&[]);
    project
        .add_column(sheet, None, Some("B"), Column::new(ColumnType::Numeric))
        .expect("column B");
    let first = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("column");
    assert_eq!(name_of(&project, first), "C");
    let second = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("column");
    assert_eq!(name_of(&project, second), "D");
}

#[test]
fn criterion_4_default_sheet_name() {
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    let book = project.add_workbook(root, None).expect("workbook");
    let sheet2 = project.add_worksheet(book, None).expect("sheet");
    assert_eq!(name_of(&project, sheet2), "Sheet2");
    assert_eq!(
        child_names(&project, book),
        vec!["Sheet1".to_string(), "Sheet2".to_string()]
    );
}

// -- 5-10. uniqueness, structure --

#[test]
fn criterion_5_name_uniqueness_rules() {
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    let a = project.add_folder(root, "A").expect("folder A");
    let b = project.add_folder(root, "B").expect("folder B");

    // Two windows with the same name in distinct folders: accepted (#105).
    project
        .add_empty_workbook(a, Some("Book1"))
        .expect("workbook");
    project
        .add_empty_workbook(b, Some("Book1"))
        .expect("same name in another folder");
    assert!(project.resolve_path("[Book1]").is_err());
    assert_ne!(
        project.resolve_path("A/[Book1]").unwrap(),
        project.resolve_path("B/[Book1]").unwrap()
    );
    assert!(matches!(
        project.add_empty_workbook(a, Some("Book1")),
        Err(CoreError::DuplicateName(_))
    ));

    // Two sibling folders with the same name: refused.
    project.add_folder(a, "Day1").expect("folder");
    assert!(matches!(
        project.add_folder(a, "Day1"),
        Err(CoreError::DuplicateName(_))
    ));

    // Two folders with the same name in different parents: accepted.
    project
        .add_folder(b, "Day1")
        .expect("same-named folder elsewhere");

    // A folder and a sibling workbook with the same name: refused (A8,
    // `guide-de-correction-robustesse.md` §F9 — see
    // `criterion_5_bis_sibling_uniqueness_does_not_depend_on_order`).
    project
        .add_folder(root, "Measurements")
        .expect("Measurements folder");
    assert!(
        matches!(
            project.add_empty_workbook(root, Some("Measurements")),
            Err(CoreError::DuplicateName(_))
        ),
        "a workbook must no longer be able to take the name of a sibling folder"
    );
}

#[test]
fn criterion_5_bis_sibling_uniqueness_does_not_depend_on_order() {
    // A8 (`guide-de-correction-robustesse.md` §F9) — fixed: the Python
    // version (and the Rust core until this fix) let a folder be
    // unique among its siblings of all kinds, but a window
    // only among windows — an asymmetry that made a folder
    // with the same name as a window unfindable by its canonical path depending on
    // the creation order of the two
    // (`b11_path_of_se_re_resout_toujours_sur_le_meme_noeud`). This
    // asymmetry is no longer "accepted": refuse more, never less (§6 of the
    // guide) — a folder and a sibling window never share a name,
    // whichever order each one took it in.
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    project
        .add_empty_workbook(root, Some("X"))
        .expect("workbook X");
    assert!(matches!(
        project.add_folder(root, "X"),
        Err(CoreError::DuplicateName(_))
    ));

    // And in the other direction now too.
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    project.add_folder(root, "Y").expect("folder Y");
    assert!(matches!(
        project.add_empty_workbook(root, Some("Y")),
        Err(CoreError::DuplicateName(_))
    ));
}

#[test]
fn criterion_6_rename() {
    let mut demo = Demo::new();
    let other = demo
        .project
        .add_empty_workbook(demo.folder, Some("Book2"))
        .expect("workbook");

    let before = demo.project.tree().clone();
    assert!(matches!(
        demo.project.rename(other, "Book1"),
        Err(CoreError::DuplicateName(_))
    ));
    assert_eq!(**demo.project.tree(), *before);
    assert!(Arc::ptr_eq(demo.project.tree(), &before));

    assert_eq!(demo.project.rename(other, "Book2").expect("no-op"), "Book2");

    assert_eq!(
        demo.project.rename(other, "Trials").expect("rename"),
        "Book2"
    );
    assert_eq!(name_of(&demo.project, other), "Trials");

    assert_eq!(demo.project.rename(demo.root, "X").expect("root"), "Demo");
    assert_eq!(demo.project.name(), "X");
}

#[test]
fn criterion_7_refused_kinds_and_indexes() {
    let mut demo = Demo::new();

    assert!(matches!(
        demo.project.insert_node(demo.sheet, 0, "X", folder_kind()),
        Err(CoreError::Structure(_))
    ));
    assert!(matches!(
        demo.project
            .add_column(demo.book, None, None, Column::new(ColumnType::Numeric)),
        Err(CoreError::Structure(_))
    ));
    assert!(matches!(
        demo.project
            .add_column(demo.sheet, Some(5), None, Column::new(ColumnType::Numeric)),
        Err(CoreError::IndexOutOfBounds(_))
    ));
    let last = demo
        .project
        .add_column(demo.sheet, Some(2), None, Column::new(ColumnType::Numeric))
        .expect("insertion at the end");
    assert_eq!(demo.project.tree().index_in_parent(last).expect("index"), 2);
}

#[test]
fn criterion_8_removal() {
    let mut demo = Demo::new();
    assert_eq!(demo.project.remove(demo.time).expect("removal"), 0);
    assert!(!demo.project.tree().contains(demo.time));
    assert_eq!(
        child_names(&demo.project, demo.sheet),
        vec!["Signal".to_string()]
    );

    let before = demo.project.tree().len();
    // Book1 + Sheet1 + Signal = 3 nodes.
    assert_eq!(demo.project.remove(demo.book).expect("removal"), 0);
    assert_eq!(demo.project.tree().len(), before - 3);
    assert!(!demo.project.tree().contains(demo.sheet));
    assert!(!demo.project.tree().contains(demo.signal));

    assert!(matches!(
        demo.project.remove(demo.root),
        Err(CoreError::Structure(_))
    ));
}

#[test]
fn criterion_9_accepted_moves() {
    let mut demo = Demo::new();
    let c = demo
        .project
        .add_column(
            demo.sheet,
            None,
            Some("Other"),
            Column::new(ColumnType::Numeric),
        )
        .expect("column");

    // Reorder: index 0 → 2 over three columns (Time, Signal, Other).
    assert_eq!(
        demo.project
            .move_node(demo.time, demo.sheet, 2)
            .expect("reordering"),
        (demo.sheet, 0)
    );
    assert_eq!(
        child_names(&demo.project, demo.sheet),
        vec![
            "Signal".to_string(),
            "Other".to_string(),
            "Time".to_string()
        ]
    );
    assert!(demo.project.tree().contains(c));

    // Move a sheet to another workbook.
    let book2 = demo
        .project
        .add_empty_workbook(demo.root, None)
        .expect("empty workbook");
    assert_eq!(
        demo.project
            .move_node(demo.sheet, book2, 0)
            .expect("sheet move"),
        (demo.book, 0)
    );
    assert_eq!(
        demo.project.path_of(demo.time).expect("path"),
        format!("/[{}]Sheet1!Time", name_of(&demo.project, book2))
    );

    // Move a workbook to another folder.
    let elsewhere = demo
        .project
        .add_folder(demo.root, "Elsewhere")
        .expect("folder");
    let (old_parent, old_index) = demo
        .project
        .move_node(book2, elsewhere, 0)
        .expect("workbook move");
    assert_eq!(old_parent, demo.root);
    assert_eq!(old_index, 1);
    assert_eq!(
        demo.project.path_of(demo.time).expect("path"),
        format!("/Elsewhere/[{}]Sheet1!Time", name_of(&demo.project, book2))
    );
    // The identifiers did not move.
    assert!(demo.project.tree().contains(demo.time));
    assert!(demo.project.tree().contains(demo.sheet));
}

#[test]
fn criterion_10_refused_moves_leave_the_tree_intact() {
    let mut demo = Demo::new();
    let f1 = demo.project.add_folder(demo.root, "F1").expect("folder");
    let f2 = demo.project.add_folder(demo.root, "F2").expect("folder");
    let x1 = demo.project.add_folder(f1, "X").expect("folder X in F1");
    demo.project.add_folder(f2, "X").expect("folder X in F2");
    let inner = demo.project.add_folder(f1, "Inner").expect("folder");

    let before = demo.project.tree().clone();

    // A column does not go into a workbook.
    assert!(matches!(
        demo.project.move_node(demo.time, demo.book, 0),
        Err(CoreError::Structure(_))
    ));
    // A folder does not go into its own descendant.
    match demo.project.move_node(f1, inner, 0) {
        Err(CoreError::Structure(message)) => {
            assert!(message.contains("circular"), "{message}")
        }
        other => panic!("expected circular Structure, got {other:?}"),
    }
    // Name already taken in the target.
    assert!(matches!(
        demo.project.move_node(x1, f2, 0),
        Err(CoreError::DuplicateName(_))
    ));
    // Index too large (name free in the target: it really is the index that blocks).
    assert!(matches!(
        demo.project.move_node(inner, f2, 9),
        Err(CoreError::IndexOutOfBounds(_))
    ));
    // The root does not move.
    assert!(matches!(
        demo.project.move_node(demo.root, f1, 0),
        Err(CoreError::Structure(_))
    ));

    assert!(Arc::ptr_eq(demo.project.tree(), &before));
    assert_eq!(**demo.project.tree(), *before);
}

// -- 11-13. paths --

#[test]
fn criterion_11_path_resolution() {
    let demo = Demo::new();
    let p = &demo.project;
    assert_eq!(p.resolve_path("/").expect("root"), demo.root);
    assert_eq!(p.resolve_path("/Experiments").expect("folder"), demo.folder);
    assert_eq!(p.resolve_path("[Book1]").expect("workbook"), demo.book);
    assert_eq!(p.resolve_path("[Book1]Sheet1").expect("sheet"), demo.sheet);
    assert_eq!(
        p.resolve_path("[Book1]Sheet1!Time").expect("column"),
        demo.time
    );
    assert_eq!(
        p.resolve_path("/Experiments/[Book1]Sheet1!Time")
            .expect("prefixed column"),
        demo.time
    );
    match p.resolve_path("/Elsewhere/[Book1]Sheet1!Time") {
        Err(CoreError::Path(message)) => {
            assert!(message.contains("folder not found"), "{message}")
        }
        other => panic!("expected Path, got {other:?}"),
    }
}

#[test]
fn criterion_12_unresolvable_paths() {
    let mut demo = Demo::new();
    demo.project
        .add_graph(demo.root, Some("Fig1"), Default::default())
        .expect("graph");
    let p = &demo.project;
    for text in [
        "[Missing]",
        "[Book1]SheetX",
        "[Book1]Sheet1!Missing",
        "/Unknown",
        "/Experiments/Unknown",
        "/Experiments/Book1/Unknown",
    ] {
        assert!(
            matches!(p.resolve_path(text), Err(CoreError::Path(_))),
            "expected Path for {text:?}, got {:?}",
            p.resolve_path(text)
        );
    }
    match p.resolve_path("[Fig1]Sheet1") {
        Err(CoreError::Path(message)) => {
            assert!(message.contains("is not a workbook"), "{message}")
        }
        other => panic!("expected Path, got {other:?}"),
    }
}

#[test]
fn criterion_13_path_of_and_resolve_are_inverse() {
    let mut demo = Demo::new();
    // An empty sheet and an empty column, plus a window outside any folder.
    let empty = demo
        .project
        .add_empty_workbook(demo.root, Some("Empty"))
        .expect("empty workbook");
    let empty_sheet = demo
        .project
        .add_worksheet(empty, None)
        .expect("empty sheet");
    demo.project
        .add_column(empty_sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("empty column");
    demo.project
        .add_graph(demo.root, None, Default::default())
        .expect("graph");
    demo.project.add_note(demo.folder, None, "").expect("note");

    for id in demo.project.tree().walk_all() {
        let path = demo.project.path_of(id).expect("path");
        assert_eq!(
            demo.project.resolve_path(&path).expect(&path),
            id,
            "round trip for {path}"
        );
    }

    assert!(matches!(
        demo.project.path_of(NodeId::from_raw(9999)),
        Err(CoreError::UnknownNode(_))
    ));
}

// -- 14-17. data and rows --

#[test]
fn criterion_14_writing_values() {
    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0, 3.0])]);
    let column = ids[0];
    let delta = project
        .set_values(column, 1, ValueBlock::Float(vec![20.0, 30.0]))
        .expect("write");
    assert_eq!(floats(&project, column), vec![1.0, 20.0, 30.0]);
    assert_eq!(delta.start, 1);
    assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
    assert_eq!(delta.previous_length, 3);

    assert!(matches!(
        project.set_values(sheet, 0, ValueBlock::Float(vec![1.0])),
        Err(CoreError::Structure(_))
    ));
}

#[test]
fn criterion_15_rows_only_the_long_enough_columns() {
    let (mut project, sheet, ids) =
        sheet_with(&[("long", &[1.0, 2.0, 3.0, 4.0, 5.0]), ("short", &[1.0, 2.0])]);
    let (long, short) = (ids[0], ids[1]);

    let lengthened = project.insert_rows(sheet, 3, 2).expect("insertion");
    assert_eq!(lengthened, vec![long]);
    assert_eq!(floats(&project, long).len(), 7);
    assert_eq!(floats(&project, short), vec![1.0, 2.0]);

    let removed = project.delete_rows(sheet, 3, 2).expect("removal");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, long);
    match &removed[0].1 {
        ValueBlock::Float(values) => {
            assert_eq!(values.len(), 2);
            assert!(values.iter().all(|v| v.is_nan()), "{values:?}");
        }
        other => panic!("expected a Float block, got {other:?}"),
    }
    assert_eq!(floats(&project, long), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(floats(&project, short), vec![1.0, 2.0]);

    // Beyond all the columns: no-op.
    let before = project.tree().clone();
    let lengthened = project.insert_rows(sheet, 10, 3).expect("no-op");
    assert_eq!(lengthened, Vec::<NodeId>::new());
    assert_eq!(**project.tree(), *before);

    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0, 3.0]), ("B", &[1.0])]);
    let removed = project.delete_rows(sheet, 2, 1).expect("removal");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, ids[0]);
    assert_eq!(floats(&project, ids[0]), vec![1.0, 2.0]);
    assert_eq!(floats(&project, ids[1]), vec![1.0]);
}

proptest::proptest! {
    /// Criterion 16: on a sheet of columns of arbitrary lengths,
    /// `insert_rows` then `delete_rows` at the same coordinates give back
    /// exactly the initial tree.
    #[test]
    fn criterion_16_row_insertion_then_removal_is_the_identity(
        lengths in proptest::collection::vec(0usize..20, 1..5),
        at in 0usize..25,
        count in 0usize..10,
    ) {
        let specs: Vec<(String, Vec<f64>)> = lengths
            .iter()
            .enumerate()
            .map(|(i, len)| {
                (format!("C{i}"), (0..*len).map(|row| row as f64).collect())
            })
            .collect();
        let borrowed: Vec<(&str, &[f64])> = specs
            .iter()
            .map(|(name, values)| (name.as_str(), values.as_slice()))
            .collect();
        let (mut project, sheet, _) = sheet_with(&borrowed);

        let before = project.tree().clone();
        project.insert_rows(sheet, at, count).expect("insertion");
        project.delete_rows(sheet, at, count).expect("removal");
        proptest::prop_assert_eq!(&**project.tree(), &*before);
    }
}

#[test]
fn criterion_17_row_count() {
    let (project, sheet, _) = sheet_with(&[]);
    assert_eq!(project.tree().row_count(sheet).expect("empty sheet"), 0);

    let (project, sheet, _) = sheet_with(&[("A", &[1.0, 2.0]), ("B", &[1.0, 2.0, 3.0, 4.0])]);
    assert_eq!(project.tree().row_count(sheet).expect("sheet"), 4);
}

#[test]
fn column_and_window_metadata_return_the_previous_ones() {
    let mut demo = Demo::new();
    let meta = ColumnMeta {
        role: ColumnRole::X,
        unit: "s".to_string(),
        ..Default::default()
    };
    let previous = demo
        .project
        .set_column_meta(demo.time, meta.clone())
        .expect("metadata");
    assert_eq!(previous, ColumnMeta::default());
    assert_eq!(
        demo.project
            .tree()
            .get(demo.time)
            .expect("column")
            .as_column()
            .expect("column")
            .meta(),
        &meta
    );

    let window_meta = WindowMeta {
        label: "Experiment 3".to_string(),
        comment: "trial".to_string(),
        ..Default::default()
    };
    assert_eq!(
        demo.project
            .set_window_meta(demo.book, window_meta)
            .expect("window metadata"),
        WindowMeta::default()
    );
    assert!(matches!(
        demo.project
            .set_window_meta(demo.sheet, WindowMeta::default()),
        Err(CoreError::Structure(_))
    ));

    let note = demo.project.add_note(demo.root, None, "one").expect("note");
    assert_eq!(
        demo.project.set_note_text(note, "two").expect("text"),
        "one"
    );
    assert!(matches!(
        demo.project.set_note_text(demo.book, "x"),
        Err(CoreError::Structure(_))
    ));

    let graph = demo
        .project
        .add_graph(demo.root, None, stokesforge_plot::PlotSpec::default())
        .expect("graph");
    let spec = stokesforge_plot::PlotSpec {
        title: "Figure".to_string(),
        ..Default::default()
    };
    assert_eq!(
        demo.project
            .set_plot_spec(graph, spec.clone())
            .expect("spec"),
        stokesforge_plot::PlotSpec::default()
    );
    assert_eq!(
        demo.project
            .tree()
            .get(graph)
            .expect("graph")
            .kind()
            .clone(),
        stokesforge_core::NodeKind::Graph {
            meta: WindowMeta::default(),
            spec: spec.clone(),
        }
    );
    assert!(matches!(
        demo.project.set_plot_spec(demo.book, spec),
        Err(CoreError::Structure(_))
    ));
}

#[test]
fn column_type_change_and_restoration() {
    let (mut project, _, ids) = sheet_with(&[("A", &[1.0, 2.0])]);
    let column = ids[0];
    let (old_type, old_values) = project
        .set_column_type(column, ColumnType::Mixed)
        .expect("type change");
    assert_eq!(old_type, ColumnType::Numeric);
    assert_eq!(
        project
            .tree()
            .get(column)
            .expect("column")
            .as_column()
            .expect("column")
            .get(0),
        // `Numeric → Mixed` is lossless (`Column::set_column_type`):
        // `1.0` becomes `MixedCell::Number(1.0)`, read through `Cell::Float`
        // — not reformatted as text, unlike the old conversion
        // to `Text`.
        Cell::Float(1.0)
    );
    project
        .restore_column(column, old_type, old_values)
        .expect("restoration");
    assert_eq!(floats(&project, column), vec![1.0, 2.0]);

    let delta = project.set_length(column, 4).expect("length");
    assert_eq!(delta.previous_length, 2);
    assert_eq!(floats(&project, column).len(), 4);
}

// -- 18-21. structural sharing, identity --

#[test]
fn criterion_18_structural_sharing_after_rename() {
    let demo = Demo::new();
    let mut project = demo.project;
    let before = project.tree().clone();

    project.rename(demo.time, "Z").expect("rename");

    assert_eq!(before.get(demo.time).expect("column").name(), "Time");
    assert_eq!(name_of(&project, demo.time), "Z");

    for id in before.walk_all() {
        let old = before.node_arc(id).expect("node");
        let new = project.tree().node_arc(id).expect("node");
        if id == demo.time {
            assert!(!Arc::ptr_eq(old, new), "the renamed column must be cloned");
        } else {
            assert!(Arc::ptr_eq(old, new), "{id} must stay shared");
        }
    }
}

#[test]
fn criterion_18_bis_structural_sharing_after_write() {
    let demo = Demo::new();
    let mut project = demo.project;
    let before = project.tree().clone();

    project
        .set_values(demo.time, 0, ValueBlock::Float(vec![9.0]))
        .expect("write");

    for id in before.walk_all() {
        let old = before.node_arc(id).expect("node");
        let new = project.tree().node_arc(id).expect("node");
        if id == demo.time {
            assert!(!Arc::ptr_eq(old, new));
        } else {
            assert!(Arc::ptr_eq(old, new), "{id} must stay shared");
        }
    }

    // The buffers of the other columns are physically shared.
    let old_signal = before
        .get(demo.signal)
        .expect("column")
        .as_column()
        .expect("column")
        .float_values()
        .expect("numeric");
    let new_signal = project
        .tree()
        .get(demo.signal)
        .expect("column")
        .as_column()
        .expect("column")
        .float_values()
        .expect("numeric");
    assert!(Arc::ptr_eq(old_signal, new_signal));
}

#[test]
fn criterion_19_copy_on_write_under_snapshot() {
    let demo = Demo::new();
    let mut project = demo.project;

    let before = project.tree().clone();
    project
        .set_values(demo.time, 0, ValueBlock::Float(vec![9.0]))
        .expect("write");
    let old = before
        .get(demo.time)
        .expect("column")
        .as_column()
        .expect("column");
    assert_eq!(old.get(0), Cell::Float(0.0));
    assert_eq!(floats(&project, demo.time), vec![9.0, 1.0, 2.0]);
    drop(before);

    // Without a live snapshot, the write happens in place: same buffer.
    let ptr_before = buffer_ptr(&project, demo.time);
    project
        .set_values(demo.time, 0, ValueBlock::Float(vec![7.0]))
        .expect("write");
    assert_eq!(ptr_before, buffer_ptr(&project, demo.time));
}

fn buffer_ptr(project: &Project, column: NodeId) -> *const Vec<f64> {
    Arc::as_ptr(
        project
            .tree()
            .get(column)
            .expect("column")
            .as_column()
            .expect("column")
            .float_values()
            .expect("numeric"),
    )
}

#[test]
fn criterion_20_identifiers_are_never_reused() {
    let (mut project, sheet, _) = sheet_with(&[]);
    let x = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("column A");

    let snapshot = project.tree().clone();
    project.remove(x).expect("removal");
    assert!(!project.tree().contains(x));

    project.replace_tree(snapshot);
    assert!(project.tree().contains(x));
    assert_eq!(name_of(&project, x), "A");

    let next = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("column");
    assert!(next.raw() > x.raw(), "{next} must exceed {x}");
    for (id, _) in project.tree().iter() {
        assert!(id == next || id.raw() < next.raw());
    }

    // Without restoration, the freed identifier is not recycled either.
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    let y = project.add_folder(root, "Y").expect("folder");
    project.remove(y).expect("removal");
    let z = project.add_folder(root, "Z").expect("folder");
    assert_ne!(z, y);
    assert!(z.raw() > y.raw());
}

#[test]
fn criterion_21_from_tree_resumes_the_counter() {
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    // Identifiers 1..=7: root + 6 folders.
    for n in 0..6 {
        project.add_folder(root, &format!("D{n}")).expect("folder");
    }
    assert_eq!(project.next_id_hint(), 8);

    let reloaded = Project::from_tree(project.tree().clone(), 3);
    assert_eq!(reloaded.next_id_hint(), 8);

    // `next_id_hint` IS already the next identifier to assign, not the
    // largest identifier already assigned: with a hint of 20, nothing in
    // the tree (largest id: 7) contradicts it, so the counter restarts
    // directly at 20. The old value expected here (21) came from
    // treating `hint` as "the largest identifier already assigned" and
    // adding 1 to it after the `max` — a confusion fixed in
    // `IdAllocator::resume`.
    let reloaded = Project::from_tree(project.tree().clone(), 20);
    assert_eq!(reloaded.next_id_hint(), 20);
}

#[test]
fn criterion_21_bis_round_trip_without_disk_preserves_next_id_hint() {
    // A natural criterion that a disk round trip must respect, checked here
    // without even going through stokesforge-persistence: rebuilding a `Project`
    // from its own tree and its own `next_id_hint` must neither
    // make it drift, nor waste an identifier.
    let mut project = Project::new("X").expect("valid name");
    let root = project.root();

    let book1 = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet1 = project.add_worksheet(book1, Some("Sheet1")).expect("sheet");
    project
        .add_column(sheet1, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column");

    let book2 = project
        .add_empty_workbook(root, Some("Book2"))
        .expect("workbook");
    let sheet2 = project.add_worksheet(book2, Some("Sheet2")).expect("sheet");
    project
        .add_column(sheet2, None, Some("B"), Column::from_f64(&[3.0]))
        .expect("column");

    let expected = project.next_id_hint();
    let mut reloaded = Project::from_tree(project.tree().clone(), expected);
    assert_eq!(reloaded.next_id_hint(), expected);

    // A node added after resumption receives exactly the identifier announced
    // by `next_id_hint()`, not one more.
    let new_id = reloaded
        .add_column(sheet2, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("column added after resumption");
    assert_eq!(new_id.raw(), expected);
}

// -- 22-23. from_nodes --

#[test]
fn criterion_22_from_nodes_rebuilds_the_fixture() {
    let demo = Demo::new();
    let nodes: Vec<Node> = demo
        .project
        .tree()
        .iter()
        .map(|(_, node)| node.clone())
        .collect();
    let rebuilt = Tree::from_nodes(demo.root, nodes).expect("valid tree");
    assert_eq!(rebuilt, **demo.project.tree());
}

#[test]
fn criterion_23_from_nodes_refuses_inconsistent_trees() {
    let root_id = NodeId::from_raw(1);
    let a = NodeId::from_raw(2);
    let b = NodeId::from_raw(3);

    let root_with = |children: Vec<NodeId>| {
        Node::new(root_id, "Project", NodeKind::Folder { children }).expect("root")
    };
    let child = |id: NodeId, name: &str, parent: NodeId, kind: NodeKind| {
        Node::with_parent(id, name, Some(parent), kind).expect("node")
    };

    let cases: Vec<(&str, NodeId, Vec<Node>)> = vec![
        (
            "duplicate identifier",
            root_id,
            vec![root_with(vec![]), root_with(vec![])],
        ),
        ("missing root", root_id, vec![]),
        (
            "non-folder root",
            root_id,
            vec![Node::new(root_id, "Book1", workbook_kind()).expect("node")],
        ),
        (
            "root with a parent",
            root_id,
            vec![Node::with_parent(root_id, "Project", Some(a), folder_kind()).expect("node")],
        ),
        (
            "missing parent",
            root_id,
            vec![
                root_with(vec![]),
                child(a, "A", NodeId::from_raw(42), folder_kind()),
            ],
        ),
        (
            "missing from the child list of its parent",
            root_id,
            vec![root_with(vec![]), child(a, "A", root_id, folder_kind())],
        ),
        (
            "child whose parent is another node",
            root_id,
            vec![
                root_with(vec![a, b]),
                Node::with_parent(
                    a,
                    "A",
                    Some(root_id),
                    NodeKind::Folder { children: vec![b] },
                )
                .expect("node"),
                child(b, "B", root_id, folder_kind()),
            ],
        ),
        (
            "unreachable node (two roots)",
            root_id,
            vec![
                root_with(vec![]),
                Node::new(a, "Other", folder_kind()).expect("node"),
            ],
        ),
        (
            "column under a folder",
            root_id,
            vec![root_with(vec![a]), child(a, "A", root_id, column_kind())],
        ),
        (
            "two windows with the same name",
            root_id,
            vec![
                root_with(vec![a, b]),
                child(a, "Book1", root_id, workbook_kind()),
                child(b, "Book1", root_id, workbook_kind()),
            ],
        ),
    ];

    for (label, root, nodes) in cases {
        let outcome = Tree::from_nodes(root, nodes);
        assert!(
            matches!(outcome, Err(CoreError::Structure(_))),
            "expected Structure for \"{label}\", got {outcome:?}"
        );
    }
}

#[test]
fn from_nodes_refuses_a_folder_and_a_window_with_the_same_name() {
    // A8 (`guide-de-correction-robustesse.md` §F9) — this test used to claim
    // that "what `insert` can produce, `from_nodes` must be able
    // to read back", by building precisely the case that A8 fixes:
    // a folder and a workbook that are siblings with the same name. `insert` (via
    // `check_name_available`) now refuses this pair itself — the
    // premise of the test no longer holds, but the spirit remains: what
    // the mutation API refuses to produce, `from_nodes` must also
    // refuse when reading it back (that direction is covered by `criterion_23_from_nodes_refuses_inconsistent_trees`); this test checks the
    // first link, that the pair can no longer be produced at all.
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    project.add_folder(root, "X").expect("folder");
    assert!(matches!(
        project.add_empty_workbook(root, Some("X")),
        Err(CoreError::DuplicateName(_))
    ));
}

#[test]
fn typed_values_survive_a_from_nodes_round_trip() {
    let demo = Demo::new();
    let nodes: Vec<Node> = demo.project.tree().iter().map(|(_, n)| n.clone()).collect();
    let rebuilt = Tree::from_nodes(demo.root, nodes).expect("valid tree");
    let column = rebuilt
        .get(demo.signal)
        .expect("column")
        .as_column()
        .expect("column");
    assert!(matches!(column.values(), ColumnValues::Float(_)));
    assert_eq!(column.len(), 2);
}

// -- 24-26. per-column primitives, atomicity --

#[test]
fn criterion_24_per_column_primitives_undo_delete_rows() {
    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0])]);
    let column = ids[0];

    let removed = project.delete_rows(sheet, 1, 5).expect("removal");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, column);
    assert_eq!(removed[0].1, ValueBlock::Float(vec![2.0]));
    assert_eq!(floats(&project, column), vec![1.0]);

    // Undo recipe: reinsert then rewrite, column by column.
    for (id, block) in removed {
        let len = block.len();
        project
            .insert_column_rows(id, 1, len)
            .expect("per-column insertion");
        project.set_values(id, 1, block).expect("rewrite");
    }
    assert_eq!(floats(&project, column), vec![1.0, 2.0]);
}

#[test]
fn criterion_25_per_column_primitives_validate_their_bounds() {
    let (mut project, sheet, ids) = sheet_with(&[("A", &[1.0, 2.0])]);
    let column = ids[0];

    // The target must be a column, not the sheet.
    assert!(matches!(
        project.insert_column_rows(sheet, 0, 1),
        Err(CoreError::Structure(_))
    ));

    // Position beyond `len + 1` refused.
    assert!(matches!(
        project.insert_column_rows(column, 3, 1),
        Err(CoreError::IndexOutOfBounds(_))
    ));

    // Removal of zero rows: no-op, empty block returned.
    let block = project
        .delete_column_rows(column, 0, 0)
        .expect("removal of zero rows");
    assert!(block.is_empty());
    assert_eq!(floats(&project, column), vec![1.0, 2.0]);
}

#[test]
fn criterion_26_add_workbook_adds_exactly_two_nodes() {
    let mut project = Project::new("Demo").expect("valid name");
    let root = project.root();
    let before = project.tree().len();

    project.add_workbook(root, None).expect("workbook");

    assert_eq!(project.tree().len(), before + 2);
}

#[test]
fn worksheet_duplication_is_atomic_even_if_identifiers_are_exhausted() {
    let demo = Demo::new();
    let mut project = Project::from_tree(demo.project.tree().clone(), u64::MAX - 1);
    let before = project.tree().clone();
    assert!(project.duplicate_worksheet(demo.sheet, "Copy").is_err());
    assert_eq!(project.tree(), &before);
}
