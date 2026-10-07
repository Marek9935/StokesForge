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

#[test]
fn graphe_externe_emporte_x_y_erreurs_sans_colonnes_inutiles_et_persiste() {
    use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec};
    let mut source = Project::new("Source").unwrap();
    let root = source.root();
    let mut columns = Vec::new();
    // Deux feuilles homonymes, issues de classeurs différents.
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
    // Une seconde série partage X et garde une référence déjà rompue.
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
    // Forcer une collision avec la référence rompue source.
    for _ in 0..1000 {
        target.add_note(root, None, "").unwrap();
    }
    let copied = target
        .insert_copy(root, &copies[0].clone().into_external(), "Graph1")
        .unwrap();
    let NodeKind::Graph { spec, .. } = target.tree().node(copied).unwrap().kind() else {
        panic!("graphe");
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
    assert_eq!(target.tree().walk(books[0]).unwrap().len(), 7); // classeur + deux feuilles + quatre colonnes
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.stf");
    stokesforge_persistence::save(&target, &path).unwrap();
    let mut loaded = stokesforge_persistence::load(&path).unwrap();
    assert_eq!(loaded.tree(), target.tree());
    let next = loaded.add_note(root, None, "").unwrap();
    assert_ne!(next, s[1].y);
    // Une erreur après l'insertion des données restaure tout le modèle.
    let before = target.tree().clone();
    assert!(target
        .insert_copy(root, &copies[0].clone().into_external(), "Graph1")
        .is_err());
    assert_eq!(target.tree(), &before);
}

/// #166 : une série « ligne » lit toutes les colonnes x' de sa feuille, pas
/// seulement son ancre — la copie du graphe vers un autre projet les emporte
/// toutes (et seulement elles), et la série se résout à l'identique.
#[test]
fn graphe_externe_a_serie_ligne_emporte_les_colonnes_x_prime_de_la_feuille() {
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
        panic!("graphe");
    };
    let resolved = stokesforge_core::resolve_spec(target.tree(), spec);
    let series = &resolved.panels[0].series[0];
    assert_eq!(series.x.as_ref().unwrap().as_slice(), [0.0, 10.0]);
    assert_eq!(series.y.as_ref().unwrap().as_slice(), [1.0, 2.0]);
}
