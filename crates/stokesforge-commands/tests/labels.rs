//! Libellés d'historique structurés (issue #46) : chaque variante de
//! `Command` produit son `HistoryLabel`, et le `Display` anglais dit la même
//! chose que le catalogue de l'application.

use std::collections::BTreeSet;

use stokesforge_commands::{ColumnRef, Command, HistoryLabel, MacroKind, PlotEditKind};
use stokesforge_core::{Column, ColumnMeta, ColumnType, NodeId, Project, ValueBlock, WindowMeta};
use stokesforge_plot::{PlotSpec, SavedColors};

struct World {
    project: Project,
    book: NodeId,
    sheet: NodeId,
    first: NodeId,
    third: NodeId,
}

/// `Book1` / `Sheet1` avec trois colonnes nommées `Temps`, `Signal`, `Bruit` :
/// des noms qui ne sont **pas** les lettres de la grille, pour que le test
/// distingue la position du nom de nœud.
fn world() -> World {
    let mut project = Project::new("Demo").expect("projet");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let mut columns = Vec::new();
    for name in ["Temps", "Signal", "Bruit"] {
        columns.push(
            project
                .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0]))
                .expect("colonne"),
        );
    }
    World {
        project,
        book,
        sheet,
        first: columns[0],
        third: columns[2],
    }
}

/// Le rang de chaque variante : le `match` est **exhaustif** (aucune branche
/// par défaut), donc une variante ajoutée à `Command` ne compile pas ici tant
/// qu'elle n'a pas son rang — et le test de couverture exige alors un exemple.
fn rank(command: &Command) -> usize {
    match command {
        Command::SetCells { .. } => 0,
        Command::SetCellsPromoting { .. } => 1,
        Command::AddColumn { .. } => 2,
        Command::DeleteColumn { .. } => 3,
        Command::Rename { .. } => 4,
        Command::Move { .. } => 5,
        Command::SetColumnType { .. } => 6,
        Command::InsertRows { .. } => 7,
        Command::DeleteRows { .. } => 8,
        Command::DeleteCells { .. } => 9,
        Command::AddFolder { .. } => 10,
        Command::AddWorkbook { .. } => 11,
        Command::AddEmptyWorkbook { .. } => 12,
        Command::AddWorksheet { .. } => 13,
        Command::AddGraph { .. } => 14,
        Command::AddMatrix { .. } => 15,
        Command::AddNote { .. } => 16,
        Command::RemoveNode { .. } => 17,
        Command::SetWindowMeta { .. } => 18,
        Command::SetNoteText { .. } => 19,
        Command::SetColumnMeta { .. } => 20,
        Command::SetPlotSpec { .. } => 21,
        Command::SetSavedColors { .. } => 22,
        Command::EditPlotSpec { .. } => 23,
    }
}

const VARIANTS: usize = 24;

fn named(name: &str) -> Option<String> {
    Some(name.to_string())
}

/// Une commande de chaque variante, avec le libellé attendu.
fn samples(w: &World) -> Vec<(Command, HistoryLabel)> {
    let root = w.project.root();
    vec![
        (
            Command::SetCells {
                column: w.third,
                start: 0,
                values: ValueBlock::Float(vec![1.0]),
                expected_version: None,
            },
            HistoryLabel::SetCells {
                column: Some(ColumnRef(2)),
            },
        ),
        (
            Command::SetCellsPromoting {
                column: w.first,
                start: 0,
                values: ValueBlock::Float(vec![1.0]),
                expected_version: None,
            },
            HistoryLabel::SetCells {
                column: Some(ColumnRef(0)),
            },
        ),
        (
            Command::AddColumn {
                sheet: w.sheet,
                index: None,
                name: None,
                column: Column::new(ColumnType::Numeric),
            },
            HistoryLabel::AddColumn,
        ),
        (
            Command::DeleteColumn { column: w.third },
            HistoryLabel::DeleteColumn {
                column: Some(ColumnRef(2)),
            },
        ),
        (
            Command::Rename {
                id: w.book,
                name: "Mesures".to_string(),
            },
            HistoryLabel::Rename {
                from: named("Book1"),
                to: "Mesures".to_string(),
            },
        ),
        (
            Command::Move {
                id: w.first,
                new_parent: w.sheet,
                index: 1,
            },
            HistoryLabel::Move {
                name: named("Temps"),
            },
        ),
        (
            Command::SetColumnType {
                column: w.third,
                column_type: ColumnType::Mixed,
            },
            HistoryLabel::SetColumnType {
                column: Some(ColumnRef(2)),
            },
        ),
        (
            Command::InsertRows {
                sheet: w.sheet,
                at: 0,
                count: 3,
            },
            HistoryLabel::InsertRows { count: 3 },
        ),
        (
            Command::DeleteRows {
                sheet: w.sheet,
                at: 0,
                count: 1,
            },
            HistoryLabel::DeleteRows { count: 1 },
        ),
        (
            Command::DeleteCells {
                columns: vec![w.first],
                at: 0,
                count: 2,
            },
            HistoryLabel::DeleteCells { count: 2 },
        ),
        (
            Command::AddFolder {
                parent: root,
                name: "Manips".to_string(),
            },
            HistoryLabel::AddFolder {
                name: "Manips".to_string(),
            },
        ),
        (
            Command::AddWorkbook {
                parent: root,
                name: None,
            },
            HistoryLabel::AddWorkbook,
        ),
        (
            Command::AddEmptyWorkbook {
                parent: root,
                name: None,
            },
            HistoryLabel::AddEmptyWorkbook,
        ),
        (
            Command::AddWorksheet {
                book: w.book,
                name: None,
            },
            HistoryLabel::AddWorksheet,
        ),
        (
            Command::AddGraph {
                parent: root,
                name: None,
                spec: PlotSpec::default(),
            },
            HistoryLabel::AddGraph,
        ),
        (
            Command::AddMatrix {
                parent: root,
                name: None,
            },
            HistoryLabel::AddMatrix,
        ),
        (
            Command::AddNote {
                parent: root,
                name: None,
                text: String::new(),
            },
            HistoryLabel::AddNote,
        ),
        (
            Command::RemoveNode { id: w.sheet },
            HistoryLabel::RemoveNode {
                name: named("Sheet1"),
            },
        ),
        (
            Command::SetWindowMeta {
                id: w.book,
                meta: WindowMeta::default(),
            },
            HistoryLabel::SetWindowMeta {
                name: named("Book1"),
            },
        ),
        (
            Command::SetNoteText {
                id: w.book,
                text: String::new(),
            },
            HistoryLabel::SetNoteText {
                name: named("Book1"),
            },
        ),
        (
            Command::SetColumnMeta {
                column: w.first,
                meta: ColumnMeta::default(),
            },
            HistoryLabel::SetColumnMeta {
                column: Some(ColumnRef(0)),
            },
        ),
        (
            Command::SetPlotSpec {
                graph: w.book,
                spec: PlotSpec::default(),
            },
            HistoryLabel::SetPlotSpec {
                name: named("Book1"),
            },
        ),
        (
            Command::SetSavedColors {
                colors: SavedColors::default(),
            },
            HistoryLabel::SetSavedColors,
        ),
    ]
}

/// Chaque variante de `Command` produit son libellé — et les exemples couvrent
/// **toutes** les variantes (le témoin : sans lui, un exemple oublié passerait
/// pour un test exhaustif).
#[test]
fn every_command_variant_produces_its_label() {
    let w = world();
    let mut samples = samples(&w);
    samples.extend(PlotEditKind::ALL.map(|kind| {
        (
            Command::EditPlotSpec {
                graph: w.book,
                spec: PlotSpec::default(),
                kind,
            },
            HistoryLabel::EditPlotSpec(kind),
        )
    }));
    let covered: BTreeSet<usize> = samples.iter().map(|(command, _)| rank(command)).collect();
    assert_eq!(
        covered,
        (0..VARIANTS).collect::<BTreeSet<_>>(),
        "une variante de Command n'a pas d'exemple"
    );
    for (command, expected) in samples {
        assert_eq!(
            command.label(w.project.tree()),
            expected,
            "libellé de {command:?}"
        );
    }
}

/// Une colonne se désigne par sa position, jamais par son nom de nœud : le
/// libellé de `Temps` (première colonne) porte `ColumnRef(0)`, dont la lettre
/// est celle de l'en-tête de la grille.
#[test]
fn a_column_is_designated_by_its_position_not_its_node_name() {
    let w = world();
    let label = Command::DeleteColumn { column: w.third }.label(w.project.tree());
    assert_eq!(label.to_string(), "Delete Column C");
    assert!(!label.to_string().contains("Bruit"));
}

/// Un nœud absent de l'arbre donne la forme générique, sans échec : c'est
/// l'application de la commande qui échouera, avec la bonne erreur.
#[test]
fn a_missing_node_gives_the_generic_label() {
    let mut w = world();
    let ghost = w.first;
    w.project.remove(ghost).expect("suppression");
    let tree = w.project.tree();
    let cases = [
        (
            Command::SetCells {
                column: ghost,
                start: 0,
                values: ValueBlock::Float(vec![1.0]),
                expected_version: None,
            },
            "Edit Cells",
        ),
        (Command::DeleteColumn { column: ghost }, "Delete Column"),
        (
            Command::Rename {
                id: ghost,
                name: "Z".to_string(),
            },
            "Rename to “Z”",
        ),
        (Command::RemoveNode { id: ghost }, "Delete"),
        (
            Command::SetColumnMeta {
                column: ghost,
                meta: ColumnMeta::default(),
            },
            "Edit Column Properties",
        ),
    ];
    for (command, expected) in cases {
        assert_eq!(command.label(tree).to_string(), expected, "{command:?}");
    }
}

/// Le pluriel anglais du `Display` : seul 1 est au singulier. Le même cas est
/// tenu côté interface par le catalogue (`stokesforge-app/tests/shell.rs`).
#[test]
fn the_english_display_pluralizes_rows() {
    let text = |count| HistoryLabel::InsertRows { count }.to_string();
    assert_eq!(text(1), "Insert 1 Row");
    assert_eq!(text(3), "Insert 3 Rows");
    assert_eq!(text(0), "Insert 0 Rows");
    assert_eq!(
        HistoryLabel::DeleteCells { count: 1 }.to_string(),
        "Delete Cells and Shift Up (1 Row)"
    );
}

/// Un libellé se compare : l'application n'a plus à lire un texte pour savoir
/// si l'entrée du dessus est un import.
#[test]
fn macro_labels_compare_by_kind() {
    assert_eq!(
        HistoryLabel::Macro(MacroKind::Import),
        HistoryLabel::Macro(MacroKind::Import)
    );
    assert_ne!(
        HistoryLabel::Macro(MacroKind::Import),
        HistoryLabel::Macro(MacroKind::PasteCells)
    );
    for kind in MacroKind::ALL {
        assert!(!HistoryLabel::Macro(kind).to_string().is_empty());
    }
}
