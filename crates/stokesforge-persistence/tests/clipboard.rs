//! Le transport d'objets préserve les valeurs et réutilise les règles du `.stf`.

use stokesforge_core::{Column, ColumnType, NodeCopy, NodeKind, Project, ValueBlock};
use stokesforge_persistence::clipboard::{decode_objects, encode_objects};

#[test]
fn valeurs_manquantes_nan_et_metadonnees_survivent_au_transport() {
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
                stokesforge_core::MixedCell::Text("échantillon".into()),
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
fn une_note_et_une_matrice_sont_copiables_sans_commandes_par_nature() {
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
