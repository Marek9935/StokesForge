//! Tests of the `.stf` round trip of column formulas (#412): every formula
//! feature through save / load, crafted formula records (bad remote parts,
//! cycles, over-deep text, dangling identifiers), and what a load must never
//! do (panic, or read a different column than the one written).

use std::collections::BTreeMap;

use stokesforge_core::formula::{
    evaluate_with_x, ColumnFormula, FormulaError, FormulaPart, RemoteRef, MAX_DEPTH,
};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, ColumnValues, NodeId, Project};
use stokesforge_persistence::{load, save};

struct Fixture {
    project: Project,
    sheet1: NodeId,
    sheet3: NodeId,
    book1: NodeId,
    book2: NodeId,
    sheet5: NodeId,
    a: NodeId,
    b: NodeId,
}

fn column(project: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> NodeId {
    // Names that collide in a sheet (`F`, `R`) get the automatic name.
    let name = (name.len() > 1 || ["A", "B"].contains(&name)).then_some(name);
    project
        .add_column(sheet, None, name, Column::from_f64(values))
        .expect("column added")
}

fn fixture() -> Fixture {
    let mut project = Project::new("Demo").expect("project");
    let root = project.root();
    let book1 = project.add_empty_workbook(root, Some("Book1")).unwrap();
    let sheet1 = project.add_worksheet(book1, Some("Sheet1")).unwrap();
    let sheet3 = project.add_worksheet(book1, Some("Sheet3")).unwrap();
    let blank = project.add_worksheet(book1, Some("Blanc 2")).unwrap();
    let book2 = project.add_empty_workbook(root, Some("Book2")).unwrap();
    let sheet5 = project.add_worksheet(book2, Some("Sheet5")).unwrap();
    let a = column(&mut project, sheet1, "A", &[1.5, 2.5, 3.5, 4.5]);
    let b = column(&mut project, sheet1, "B", &[0.5, 1.0, 2.0, 4.0]);
    let a3 = column(&mut project, sheet3, "A", &[10.0, 20.0, 30.0, 40.0]);
    // A is the X of B (the nearest X to its left), in both sheets.
    for x in [a, a3] {
        project
            .set_column_meta(
                x,
                ColumnMeta {
                    role: ColumnRole::X,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    column(&mut project, sheet3, "B", &[1.0, 2.0, 3.0, 4.0]);
    column(&mut project, blank, "A", &[7.0, 8.0, 9.0, 10.0]);
    column(&mut project, blank, "B", &[0.25, 0.5, 0.75, 1.0]);
    column(&mut project, sheet5, "A", &[100.0, 200.0, 300.0, 400.0]);
    column(&mut project, sheet5, "B", &[5.0, 6.0, 7.0, 8.0]);
    Fixture {
        project,
        sheet1,
        sheet3,
        book1,
        book2,
        sheet5,
        a,
        b,
    }
}

fn add_formula(project: &mut Project, sheet: NodeId, text: &str) -> NodeId {
    let id = column(project, sheet, "F", &[0.0]);
    let formula = project
        .tree()
        .formula_for_entry(id, text)
        .unwrap_or_else(|error| panic!("`{text}` refused: {error}"));
    project
        .set_column_meta(
            id,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .expect("meta");
    id
}

fn stored(project: &Project, id: NodeId) -> ColumnFormula {
    project
        .tree()
        .node(id)
        .and_then(|node| node.as_column())
        .and_then(|column| column.meta().formula.clone())
        .expect("formula stored")
}

/// Values the formula of `id` gives now, evaluated over every column of the
/// project (the sheets of the real recalculation pass are looked up by
/// identifier, so the result is the same). `Err` carries the error.
fn evaluate_column(project: &Project, id: NodeId) -> Result<Vec<u64>, FormulaError> {
    let formula = stored(project, id);
    let compiled = formula.compile()?;
    let inputs = project.tree().formula_inputs(id, &formula, &compiled);
    if !inputs.all_present() {
        return Err(FormulaError::DeletedColumn);
    }
    let columns: Vec<(NodeId, ColumnValues)> = project
        .tree()
        .iter()
        .filter_map(|(node_id, node)| Some((node_id, node.as_column()?.values().clone())))
        .collect();
    let result = evaluate_with_x(&compiled, &columns, &inputs.paired_x, 4);
    match result.error {
        Some(error) => Err(error),
        None => Ok(result.values.iter().map(|v| v.to_bits()).collect()),
    }
}

fn round_trip(project: &Project) -> Project {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("p.stf");
    save(project, &path).expect("save");
    load(&path).expect("load")
}

#[test]
fn every_formula_feature_survives_save_and_load() {
    let mut f = fixture();
    let sheet1 = f.sheet1;
    let texts = [
        // Table functions.
        "exp(A)",
        "ln(A)",
        "log(A)",
        "log2(A)",
        "logx(A, 2)",
        "sqrt(A)",
        "root(A, 3)",
        "pow(A, 2)",
        "abs(A)",
        "sign(A)",
        "sin(A)",
        "cos(A)",
        "tan(A)",
        "asin(B / 5)",
        "acos(B / 5)",
        "atan(A)",
        "atan2(A, B)",
        "sinh(A)",
        "cosh(A)",
        "tanh(A)",
        "erf(A)",
        "erfc(A)",
        "floor(A)",
        "ceil(A)",
        "round(A, 1)",
        "mod(A, 3)",
        "min(A, B)",
        "max(A, B)",
        "pi() * A",
        // Operators.
        "A^2",
        "A^B",
        "-A^2",
        "A < B",
        "A <= B",
        "A > B",
        "A >= B",
        "A == B",
        "A != B",
        "A * 2 + B / 3 - 1",
        "if(A > 2, A, B)",
        // Column functions, plain, with a range, with a third argument.
        "max(A)",
        "min(A)",
        "max(B, 1, 3)",
        "min(B, 1, 3)",
        "mean(A)",
        "mean(B, 1, 3)",
        "sum(A)",
        "sum(B, 1, 3)",
        "std(A)",
        "std(B, 1, 3)",
        "integral(B)",
        "integral(B, 1, 3)",
        "at(B, 2)",
        "at(B, 2, A)",
        "A - mean(A)",
        // Other sheets.
        "max(Sheet3!A)",
        "max(Sheet3!B, 1, 3)",
        "mean([Book2]Sheet5!B)",
        "sum('Blanc 2'!A)",
        "at(Sheet3!B, 2, A)",
        "A + max([Book2]Sheet5!A)",
    ];
    let mut ids = Vec::new();
    for text in texts {
        ids.push((text, add_formula(&mut f.project, sheet1, text)));
    }
    let loaded = round_trip(&f.project);
    let mut errors = Vec::new();
    for (text, id) in ids {
        assert_eq!(
            f.project.tree().formula_letters(id),
            loaded.tree().formula_letters(id),
            "displayed letters of `{text}`"
        );
        assert_eq!(stored(&f.project, id), stored(&loaded, id), "`{text}`");
        let before = evaluate_column(&f.project, id);
        let after = evaluate_column(&loaded, id);
        assert_eq!(before, after, "values of `{text}`");
        if let Err(error) = before {
            errors.push(format!("{text}: {error}"));
        }
    }
    // Most formulas must really compute: a test that compares two errors
    // proves nothing. Print what does not.
    assert!(
        errors.is_empty(),
        "too many formulas that do not compute: {errors:#?}"
    );
}

#[test]
fn a_deleted_reference_stays_ref_through_save_and_load() {
    let mut f = fixture();
    let own = add_formula(&mut f.project, f.sheet1, "max(Sheet3!B) + A");
    let local = add_formula(&mut f.project, f.sheet1, "B * 2");
    f.project.remove(f.b).unwrap();
    let remote_b = f.project.tree().children(f.sheet3).unwrap()[1];
    f.project.remove(remote_b).unwrap();
    assert!(f.project.tree().formula_has_deleted_reference(local));
    assert!(f.project.tree().formula_has_deleted_reference(own));
    let loaded = round_trip(&f.project);
    for id in [own, local] {
        assert!(loaded.tree().formula_has_deleted_reference(id));
        assert_eq!(
            f.project.tree().formula_letters(id),
            loaded.tree().formula_letters(id)
        );
        assert!(
            loaded
                .tree()
                .formula_letters(id)
                .is_some_and(|t| t.contains("#REF")),
            "{:?}",
            loaded.tree().formula_letters(id)
        );
        assert_eq!(
            evaluate_column(&loaded, id),
            Err(FormulaError::DeletedColumn)
        );
    }
}

#[test]
fn a_deleted_id_is_never_reassigned_after_reload() {
    // The formula keeps the id of a deleted column; a column created after the
    // reload must not take that number and be read silently.
    let mut f = fixture();
    let victim = column(&mut f.project, f.sheet1, "Victim", &[1.0, 2.0, 3.0, 4.0]);
    let reader = column(&mut f.project, f.sheet1, "Reader", &[0.0]);
    let formula = ColumnFormula::from_parts(vec![
        FormulaPart::Column(victim),
        FormulaPart::Text("*2".into()),
    ]);
    f.project
        .set_column_meta(
            reader,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .unwrap();
    f.project.remove(victim).unwrap();
    let mut loaded = round_trip(&f.project);
    let fresh = column(&mut loaded, f.sheet1, "Fresh", &[9.0, 9.0, 9.0, 9.0]);
    assert_ne!(fresh, victim, "an identifier was reused");
    assert!(loaded.tree().formula_has_deleted_reference(reader));
}

#[test]
fn a_dangling_id_beyond_the_counter_is_not_reassigned() {
    // Crafted: a formula reads an id that never existed and is above every id
    // of the file (and above `next_id_hint`).
    let mut f = fixture();
    let reader = column(&mut f.project, f.sheet1, "Reader", &[0.0]);
    let far = NodeId::from_raw(f.project.next_id_hint() + 5);
    f.project
        .set_column_meta(
            reader,
            ColumnMeta {
                formula: Some(ColumnFormula::from_parts(vec![FormulaPart::Column(far)])),
                ..Default::default()
            },
        )
        .unwrap();
    let mut loaded = round_trip(&f.project);
    let mut seen = Vec::new();
    for i in 0..12 {
        seen.push(column(&mut loaded, f.sheet1, &format!("N{i}"), &[1.0]));
    }
    assert!(!seen.contains(&far), "dangling id {far:?} got reassigned");
    assert!(loaded.tree().formula_has_deleted_reference(reader));
}

fn set_formula(project: &mut Project, id: NodeId, formula: ColumnFormula) {
    project
        .set_column_meta(
            id,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .expect("meta");
}

fn remote(index: usize, sheet: NodeId) -> BTreeMap<usize, RemoteRef> {
    BTreeMap::from([(
        index,
        RemoteRef {
            sheet,
            prefixed: true,
        },
    )])
}

fn remote_formula(column: NodeId, sheet: NodeId, function: &str) -> ColumnFormula {
    ColumnFormula::from_parts_with_remote(
        vec![
            FormulaPart::Text(format!("{function}(")),
            FormulaPart::Column(column),
            FormulaPart::Text(")".into()),
        ],
        remote(1, sheet),
    )
}

#[test]
fn a_remote_record_on_a_non_sheet_never_reads_a_column() {
    let mut f = fixture();
    let remote_b = f.project.tree().children(f.sheet3).unwrap()[1];
    for (label, target) in [
        ("workbook", f.book1),
        ("other workbook", f.book2),
        ("missing id", NodeId::from_raw(4242)),
        ("column", f.a),
        ("root", f.project.root()),
    ] {
        let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
        set_formula(
            &mut f.project,
            reader,
            remote_formula(remote_b, target, "max"),
        );
        let loaded = round_trip(&f.project);
        assert_eq!(
            evaluate_column(&loaded, reader),
            Err(FormulaError::DeletedColumn),
            "remote sheet = {label}"
        );
        assert!(
            loaded
                .tree()
                .formula_letters(reader)
                .is_some_and(|t| t.contains("#REF")),
            "{label}: {:?}",
            loaded.tree().formula_letters(reader)
        );
    }
}

#[test]
fn a_remote_record_naming_the_wrong_sheet_is_a_ref_not_a_wrong_column() {
    let mut f = fixture();
    let remote_b = f.project.tree().children(f.sheet3).unwrap()[1];
    let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
    // The column lives in Sheet3, the record says Sheet5.
    set_formula(
        &mut f.project,
        reader,
        remote_formula(remote_b, f.sheet5, "max"),
    );
    let loaded = round_trip(&f.project);
    assert_eq!(
        evaluate_column(&loaded, reader),
        Err(FormulaError::DeletedColumn)
    );
}

#[test]
fn remote_records_with_bad_part_indices_are_dropped_but_never_read_wrong() {
    let mut f = fixture();
    let remote_b = f.project.tree().children(f.sheet3).unwrap()[1];
    for (label, index) in [("text part", 0usize), ("past the end", 50usize)] {
        let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
        let formula = ColumnFormula::from_parts_with_remote(
            vec![
                FormulaPart::Text("max(".into()),
                FormulaPart::Column(remote_b),
                FormulaPart::Text(")".into()),
            ],
            remote(index, f.sheet3),
        );
        set_formula(&mut f.project, reader, formula);
        let loaded = round_trip(&f.project);
        // The column is in Sheet3, the formula in Sheet1: without its remote
        // record the part reads Sheet1, where the column is not: `#REF`.
        assert!(
            loaded.tree().formula_has_deleted_reference(reader),
            "{label}"
        );
        assert_eq!(
            evaluate_column(&loaded, reader),
            Err(FormulaError::DeletedColumn),
            "{label}"
        );
    }
}

#[test]
fn a_remote_record_on_the_wrong_column_part_silently_changes_the_read() {
    // Design question: two column parts, the record is on the wrong one. Nothing
    // can detect it (shape is valid); only the sheet check protects. Here the
    // wrong part is in the formula's own sheet, so it becomes a `#REF`.
    let mut f = fixture();
    let remote_b = f.project.tree().children(f.sheet3).unwrap()[1];
    let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
    let formula = ColumnFormula::from_parts_with_remote(
        vec![
            FormulaPart::Text("max(".into()),
            FormulaPart::Column(remote_b),
            FormulaPart::Text(")+max(".into()),
            FormulaPart::Column(f.a),
            FormulaPart::Text(")".into()),
        ],
        remote(3, f.sheet3),
    );
    set_formula(&mut f.project, reader, formula);
    let loaded = round_trip(&f.project);
    assert!(loaded.tree().formula_has_deleted_reference(reader));
}

#[test]
fn unknown_function_text_loads_and_fails_softly() {
    let mut f = fixture();
    let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
    let formula = ColumnFormula::from_parts(vec![
        FormulaPart::Text("frobnicate(".into()),
        FormulaPart::Column(f.a),
        FormulaPart::Text(")".into()),
    ]);
    set_formula(&mut f.project, reader, formula);
    let loaded = round_trip(&f.project);
    // Letters in a stored text part are not a call: the lexer refuses them
    // (`Parse`), it does not read them as column letters.
    let compiled = stored(&loaded, reader).compile();
    assert!(compiled.is_err(), "{compiled:?}");
    let letters = loaded.tree().formula_letters(reader).expect("letters");
    assert_eq!(letters, "frobnicate(A)");
}

#[test]
fn garbage_and_empty_text_parts_load_and_fail_softly() {
    let mut f = fixture();
    let cases: Vec<Vec<FormulaPart>> = vec![
        vec![],
        vec![FormulaPart::Text(String::new())],
        vec![FormulaPart::Text("))((".into())],
        vec![FormulaPart::Text("1e999999 + \u{0}\u{202e}".into())],
        vec![FormulaPart::Text("A".into())],
        vec![FormulaPart::Text("Sheet3!B".into())],
        vec![FormulaPart::Text("#REF".into())],
    ];
    for parts in cases {
        let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
        set_formula(&mut f.project, reader, ColumnFormula::from_parts(parts));
        let loaded = round_trip(&f.project);
        let _ = loaded.tree().formula_letters(reader);
        let _ = evaluate_column(&loaded, reader);
        let _ = loaded.tree().formula_in_loop(reader);
    }
}

#[test]
fn a_cycle_stored_in_the_file_loads_and_is_detected() {
    let mut f = fixture();
    let p = column(&mut f.project, f.sheet1, "P", &[0.0]);
    let q = column(&mut f.project, f.sheet3, "Q", &[0.0]);
    set_formula(
        &mut f.project,
        p,
        ColumnFormula::from_parts_with_remote(
            vec![
                FormulaPart::Text("max(".into()),
                FormulaPart::Column(q),
                FormulaPart::Text(")".into()),
            ],
            remote(1, f.sheet3),
        ),
    );
    set_formula(
        &mut f.project,
        q,
        ColumnFormula::from_parts_with_remote(
            vec![
                FormulaPart::Text("max(".into()),
                FormulaPart::Column(p),
                FormulaPart::Text(")".into()),
            ],
            remote(1, f.sheet1),
        ),
    );
    let loaded = round_trip(&f.project);
    assert!(loaded.tree().formula_in_loop(p));
    assert!(loaded.tree().formula_in_loop(q));
    // Self reference.
    let own = column(&mut f.project, f.sheet1, "S", &[0.0]);
    set_formula(
        &mut f.project,
        own,
        ColumnFormula::from_parts(vec![
            FormulaPart::Column(own),
            FormulaPart::Text("+1".into()),
        ]),
    );
    let loaded = round_trip(&f.project);
    assert!(loaded.tree().formula_in_loop(own));
}

#[test]
fn a_formula_deeper_than_the_cap_stored_in_the_file_fails_softly() {
    let mut f = fixture();
    let reader = column(&mut f.project, f.sheet1, "R", &[0.0]);
    for depth in [MAX_DEPTH + 1, 100_000] {
        let formula = ColumnFormula::from_parts(vec![
            FormulaPart::Text("(".repeat(depth)),
            FormulaPart::Column(f.a),
            FormulaPart::Text(")".repeat(depth)),
        ]);
        set_formula(&mut f.project, reader, formula);
        let loaded = round_trip(&f.project);
        assert!(matches!(
            stored(&loaded, reader).compile(),
            Err(FormulaError::TooDeep)
        ));
        let _ = loaded.tree().formula_letters(reader);
    }
    // Signs and unary minus nest as well.
    let formula = ColumnFormula::from_parts(vec![
        FormulaPart::Text("-".repeat(200_000)),
        FormulaPart::Column(f.a),
    ]);
    set_formula(&mut f.project, reader, formula);
    let loaded = round_trip(&f.project);
    assert!(stored(&loaded, reader).compile().is_err());
}

#[test]
fn a_formula_on_a_column_of_the_other_workbook_reloads_in_the_same_sheet() {
    let mut f = fixture();
    let own = add_formula(&mut f.project, f.sheet5, "max(A)");
    let remote = add_formula(&mut f.project, f.sheet5, "max([Book1]Sheet1!A)");
    let loaded = round_trip(&f.project);
    assert_eq!(
        loaded.tree().formula_letters(own).as_deref(),
        Some("max(A)")
    );
    assert_eq!(
        loaded.tree().formula_letters(remote).as_deref(),
        Some("max([Book1]Sheet1!A)")
    );
}
