//! Tests d'intégration du recalcul des colonnes calculées (v1), via la seule
//! API publique de `stokesforge-session` — voir la doc de tête de
//! `stokesforge-session::recalc::recalc_formulas` pour la portée exacte du
//! déclenchement que ces tests vérifient depuis l'extérieur.

use stokesforge_commands::{ColumnRef, Command, HistoryLabel};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnType, MixedCell, NodeId, Project, ValueBlock,
};
use stokesforge_session::{Session, SessionHandle, Snapshot};

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

/// La commande qui pose `formula` (saisie en **lettres**) : lettres vers
/// identifiants par `Tree::formula_from_letters`, comme l'interface (#321).
fn set_formula(handle: &SessionHandle, column: NodeId, formula: &str) -> Command {
    let formula = handle
        .snapshot()
        .expect("snapshot")
        .tree
        .formula_from_letters(column, formula)
        .expect("formule saisie");
    Command::SetColumnMeta {
        column,
        meta: ColumnMeta {
            formula: Some(formula),
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
        .execute(set_formula(&handle, c, "A+B"))
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
        .execute(set_formula(&handle, c, "A+B"))
        .expect("poser la formule");
    let undo_label_after_formula = handle.snapshot().expect("snapshot").history.undo_label;

    handle.execute(set_one(a, 0, 100.0)).expect("écrire dans A");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(read_floats(&snapshot, a)[0], 100.0);
    assert_eq!(read_floats(&snapshot, c)[0], 110.0);
    assert_eq!(
        snapshot.history.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }),
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
/// bloc entièrement manquant (`NumericStatus::Missing` partout) plutôt
/// qu'une valeur qui n'aurait de toute façon aucun sens à choisir (invariant
/// 10). Chacune référence une colonne, donc se retype en `Mixed`
/// (`desired_formula_column_type`) et affiche `"---"` plutôt qu'une case
/// vide — un cycle est un résultat non calculé comme un autre.
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
        .execute(set_formula(&handle, c, "D+1"))
        .expect("poser C = D+1");
    handle
        .execute(set_formula(&handle, d, "C+1"))
        .expect("poser D = C+1, ce qui referme le cycle");

    let snapshot = handle.snapshot().expect("snapshot");
    for &column in &[c, d] {
        let node = snapshot.tree.node(column).expect("colonne");
        let col = node.as_column().expect("le nœud est une colonne");
        assert_eq!(
            col.column_type(),
            ColumnType::Mixed,
            "chacune référence l'autre, donc se retype"
        );
        for row in 0..col.len() {
            assert_eq!(col.cell(row), MixedCell::Text("---".to_string()));
        }
    }
}

/// Poser une formule sur une colonne qui portait déjà des valeurs (données
/// utilisateur, pas encore une colonne calculée) écrase ces valeurs par le
/// recalcul immédiat (`setting_a_formula_recalculates_immediately` ci-dessus
/// le vérifie déjà) — mais un seul `undo` doit alors restaurer **et** la
/// formule (retirée) **et** les valeurs d'origine, pas seulement la formule :
/// c'était le bug (`DataDelta::ColumnMeta::values_before`, `stokesforge-commands`).
#[test]
fn undoing_a_formula_set_on_a_column_with_prior_values_restores_them() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    // C porte déjà des données utilisateur avant la moindre formule.
    handle
        .execute(Command::SetCells {
            column: c,
            start: 0,
            values: ValueBlock::Float(vec![7.0, 8.0, 9.0]),
            expected_version: None,
        })
        .expect("valeurs initiales de C");
    let undo_label_before_formula = handle.snapshot().expect("snapshot").history.undo_label;

    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("poser C = A+B");
    let snapshot = handle.snapshot().expect("snapshot");
    assert_eq!(
        read_floats(&snapshot, c),
        vec![11.0, 22.0, 33.0],
        "recalculée immédiatement, écrasant les valeurs saisies"
    );

    handle.undo().expect("annuler la pose de la formule");
    let snapshot = handle.snapshot().expect("snapshot");
    let meta = snapshot
        .tree
        .node(c)
        .expect("colonne C")
        .as_column()
        .expect("le nœud est une colonne")
        .meta();
    assert_eq!(meta.formula, None, "la formule doit être retirée");
    assert_eq!(
        read_floats(&snapshot, c),
        vec![7.0, 8.0, 9.0],
        "les valeurs d'avant la formule doivent être restaurées, pas rester à 11/22/33"
    );
    assert_eq!(
        snapshot.history.undo_label, undo_label_before_formula,
        "un seul undo a suffi"
    );
}

/// Un `load` recalcule systématiquement les colonnes-formule plutôt que de
/// faire confiance aux valeurs lues du disque (voir la doc de
/// `schema::KindRecord::Column::formula`) : une valeur volontairement fausse
/// dans le fichier ne survit pas au chargement.
#[test]
fn load_recalculates_rather_than_trusting_the_saved_values() {
    let (mut project, a, b, c) = fixture();
    let formula = project
        .tree()
        .formula_from_letters(c, "A+B")
        .expect("formule saisie");
    project
        .set_column_meta(
            c,
            ColumnMeta {
                formula: Some(formula),
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
    let path = dir.path().join("formule.stf");
    stokesforge_persistence::save(&project, &path).expect("sauvegarde");

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

/// Une colonne-formule qui référence une colonne mixte se retype
/// automatiquement en `Mixed` (l'échec de calcul devient possible), et une
/// ligne qui n'a pas pu être calculée s'affiche `"---"` (`MixedCell::Text`)
/// plutôt qu'une case vide — que ce soit un vrai échec (texte non numérique
/// dans une colonne référencée) ou une opérande simplement manquante : les
/// deux sont un résultat non calculé, pas une case qu'on n'a pas encore
/// remplie. Reprend l'exemple de la tâche : A = [1,2,3,4,""] (numérique, la
/// 5ᵉ case est hors longueur → vide), B = [1,1,"zut",1,1] (mixte), C = A+B.
#[test]
fn mixed_reference_retypes_the_formula_column_and_shows_dashes_on_failure() {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille résolue");
    let _a = project
        .add_column(
            sheet,
            None,
            Some("A"),
            Column::from_f64(&[1.0, 2.0, 3.0, 4.0]),
        )
        .expect("colonne A");
    let _b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_mixed(&[
                MixedCell::Number(1.0),
                MixedCell::Number(1.0),
                MixedCell::Text("zut".to_string()),
                MixedCell::Number(1.0),
                MixedCell::Number(1.0),
            ]),
        )
        .expect("colonne B");
    // C créée numérique au départ : la pose de la formule doit la retyper.
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("colonne C");

    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();
    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("poser C = A+B");

    let snapshot = handle.snapshot().expect("snapshot");
    let node = snapshot.tree.node(c).expect("colonne C");
    let column = node.as_column().expect("le nœud est une colonne");
    assert_eq!(
        column.column_type(),
        ColumnType::Mixed,
        "B est mixte : C doit se retyper automatiquement en Mixed"
    );

    // Ligne 0 : 1+1 = 2 (Ok).
    assert_eq!(column.cell(0), MixedCell::Number(2.0));
    // Ligne 1 : 2+1 = 3 (Ok).
    assert_eq!(column.cell(1), MixedCell::Number(3.0));
    // Ligne 2 : 3+"zut" échoue (texte non numérique) : "---", pas une case
    // vide.
    assert_eq!(column.cell(2), MixedCell::Text("---".to_string()));
    // Ligne 3 : 4+1 = 5 (Ok).
    assert_eq!(column.cell(3), MixedCell::Number(5.0));
    // Ligne 4 : A hors bornes (longueur 4) donc manquante, B = 1 : "---"
    // aussi — Missing n'a plus droit à une case muette, exactement comme
    // Failed (voir la doc de tête du test).
    assert_eq!(column.cell(4), MixedCell::Text("---".to_string()));
}

/// Une colonne-formule référençant des colonnes `Numeric`/`DateTime` — donc
/// où l'échec (texte non numérique) est impossible — se retype quand même en
/// `Mixed` dès qu'elle référence au moins une colonne : n'importe quelle
/// ligne de n'importe quelle colonne référencée peut être vide, et seule une
/// colonne `Mixed` peut porter le `"---"` que ça affiche
/// (`write_formula_column`). Les valeurs présentes restent des
/// `MixedCell::Number` ordinaires, lisibles comme des `Cell::Float` par
/// n'importe quel appelant — le retype ne change rien pour un résultat
/// entièrement calculé.
#[test]
fn formula_referencing_only_numeric_columns_still_becomes_mixed() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, c, "A+B"))
        .expect("poser C = A+B");

    let snapshot = handle.snapshot().expect("snapshot");
    let node = snapshot.tree.node(c).expect("colonne C");
    let column = node.as_column().expect("le nœud est une colonne");
    assert_eq!(
        column.column_type(),
        ColumnType::Mixed,
        "une référence à A/B suffit, même toutes deux Numeric"
    );
    assert_eq!(column.cell(0), MixedCell::Number(11.0));
    assert_eq!(column.cell(1), MixedCell::Number(22.0));
    assert_eq!(column.cell(2), MixedCell::Number(33.0));
}

/// Une formule sans la moindre référence de colonne (une constante pure) ne
/// peut jamais ni manquer ni échouer : elle reste `Numeric`, pas de retype
/// superflu vers `Mixed`.
#[test]
fn a_formula_with_no_column_reference_stays_numeric() {
    let (project, _a, _b, c) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_formula(&handle, c, "42"))
        .expect("poser C = 42, une constante pure");

    let snapshot = handle.snapshot().expect("snapshot");
    let node = snapshot.tree.node(c).expect("colonne C");
    let column = node.as_column().expect("le nœud est une colonne");
    assert_eq!(column.column_type(), ColumnType::Numeric);
    assert_eq!(read_floats(&snapshot, c), vec![42.0, 42.0, 42.0]);
}
