//! Tests d'intégration du recalcul des colonnes calculées (v1), via la seule
//! API publique de `plotyx-session` — voir la doc de tête de
//! `plotyx-session::actor::recalc_formulas` pour la portée exacte du
//! déclenchement que ces tests vérifient depuis l'extérieur.

use plotyx_commands::Command;
use plotyx_core::{Cell, Column, ColumnMeta, ColumnType, NodeId, Project, ValueBlock};
use plotyx_session::{Session, Snapshot};

/// `Book1 → Sheet1` avec `A [1,2,3]`, `B [10,20,30]` et une colonne `C`
/// numérique vide — de quoi lui poser une formule dans chaque test. Rend
/// `(project, a, b, c)`.
fn fixture() -> (Project, NodeId, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille résolue");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("colonne B");
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("colonne C");
    (project, a, b, c)
}

fn read_floats(snapshot: &Snapshot, column: NodeId) -> Vec<f64> {
    let node = snapshot.tree.node(column).expect("colonne présente");
    let column = node.as_column().expect("le nœud est une colonne");
    (0..column.len())
        .map(|row| match column.get(row) {
            Cell::Float(value) => value,
            Cell::Text(_) => unreachable!("colonnes de fixture toujours numériques"),
        })
        .collect()
}

fn set_formula(column: NodeId, formula: &str) -> Command {
    Command::SetColumnMeta {
        column,
        meta: ColumnMeta {
            formula: Some(formula.to_string()),
            ..Default::default()
        },
    }
}

fn set_one(column: NodeId, start: usize, value: f64) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    }
}

/// Poser une formule (`SetColumnMeta`) recalcule immédiatement la colonne —
/// C = A+B (lettres de position : A est la première colonne de la feuille,
/// B la deuxième), sans qu'aucune commande n'écrive les valeurs elle-même.
#[test]
fn setting_a_formula_recalculates_immediately() {
    let (project, a, b, c) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_formula(c, "A+B"))
        .expect("poser la formule");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, c), vec![11.0, 22.0, 33.0]);
    // Les colonnes sources n'ont pas bougé.
    assert_eq!(read_floats(&snapshot, a), vec![1.0, 2.0, 3.0]);
    assert_eq!(read_floats(&snapshot, b), vec![10.0, 20.0, 30.0]);
}

/// Éditer une colonne source (`SetCells`, une commande de données ordinaire,
/// annulable normalement) déclenche un recalcul silencieux de la
/// colonne-formule qui en dépend — sans créer sa propre entrée d'historique :
/// un seul `undo` suffit à revenir en arrière sur *et* la cellule éditée
/// *et* la valeur recalculée.
#[test]
fn editing_a_source_column_recalculates_the_formula_column() {
    let (project, a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_formula(c, "A+B"))
        .expect("poser la formule");
    let undo_label_after_formula = handle.snapshot().expect("snapshot").history.undo_label;

    handle.execute(set_one(a, 0, 100.0)).expect("écrire dans A");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a)[0], 100.0);
    assert_eq!(read_floats(&snapshot, c)[0], 110.0);
    assert_eq!(
        snapshot.history.undo_label.as_deref(),
        Some("Saisie dans A"),
        "le recalcul ne pousse pas sa propre entrée d'historique"
    );

    handle.undo().expect("annuler la saisie dans A");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a)[0], 1.0);
    assert_eq!(
        read_floats(&snapshot, c)[0],
        11.0,
        "la colonne-formule doit refléter la valeur restaurée de A, pas rester sur 110.0"
    );
    assert_eq!(
        snapshot.history.undo_label, undo_label_after_formula,
        "un seul undo a suffi : la formule elle-même est toujours la prochaine entrée"
    );
}

/// Un cycle de dépendance entre deux colonnes-formule (`C = D+1`,
/// `D = C+1`) ne boucle jamais indéfiniment : les deux colonnes reçoivent un
/// bloc entièrement `NaN` plutôt qu'une valeur qui n'aurait de toute façon
/// aucun sens à choisir (invariant 10).
#[test]
fn a_dependency_cycle_degrades_to_nan_without_hanging() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let sheet = handle
        .snapshot()
        .expect("snapshot")
        .tree
        .node(c)
        .expect("colonne C")
        .parent()
        .expect("C a une feuille parente");
    let d = handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("D".to_string()),
            column: Column::new(ColumnType::Numeric),
        })
        .map(|_| {
            handle
                .snapshot()
                .expect("snapshot")
                .tree
                .node(sheet)
                .expect("feuille")
                .children()
                .last()
                .copied()
                .expect("D vient d'être ajoutée")
        })
        .expect("colonne D ajoutée");

    handle
        .execute(set_formula(c, "D+1"))
        .expect("poser C = D+1");
    handle
        .execute(set_formula(d, "C+1"))
        .expect("poser D = C+1, ce qui referme le cycle");

    let snapshot = handle.snapshot().expect("snapshot");
    assert!(read_floats(&snapshot, c).iter().all(|v| v.is_nan()));
    assert!(read_floats(&snapshot, d).iter().all(|v| v.is_nan()));
}

/// Un `load` recalcule systématiquement les colonnes-formule plutôt que de
/// faire confiance aux valeurs lues du disque (voir la doc de
/// `schema::KindRecord::Column::formula`) : une valeur volontairement fausse
/// dans le fichier ne survit pas au chargement.
#[test]
fn load_recalculates_rather_than_trusting_the_saved_values() {
    let (mut project, a, b, c) = fixture();
    project
        .set_column_meta(
            c,
            ColumnMeta {
                formula: Some("A+B".to_string()),
                ..Default::default()
            },
        )
        .expect("formule posée hors session");
    // Valeurs volontairement fausses, jamais recalculées par ce chemin
    // direct (on ne passe pas par l'acteur ici).
    project
        .set_values(c, 0, ValueBlock::Float(vec![-1.0, -1.0, -1.0]))
        .expect("valeurs de départ, fausses à dessein");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("formule.plx");
    plotyx_persistence::save(&project, &path).expect("sauvegarde");

    let empty = Project::new("Vide").expect("projet vide");
    let session = Session::spawn(empty, None).expect("thread de l'acteur démarré");
    let handle = session.handle();
    handle.load(&path).expect("chargement réussi");

    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a), vec![1.0, 2.0, 3.0]);
    assert_eq!(read_floats(&snapshot, b), vec![10.0, 20.0, 30.0]);
    assert_eq!(
        read_floats(&snapshot, c),
        vec![11.0, 22.0, 33.0],
        "recalculé au chargement, pas les -1.0 lus du fichier"
    );
}
