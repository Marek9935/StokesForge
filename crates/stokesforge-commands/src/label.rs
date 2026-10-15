//! Libellés d'historique **structurés** (issue #46, chantier #39).
//!
//! Un libellé d'historique n'est pas une phrase : c'est une action et ses
//! paramètres (« insérer *3* lignes », « renommer *A* en *Z* »). Ce crate rend
//! la donnée ; la phrase, dans la langue de l'utilisateur, est celle de
//! `stokesforge-app` (`tr!`, pluriels Fluent). Aucune dépendance à Fluent ici :
//! `stokesforge-commands` reste sous la frontière de traduction (invariant 2).
//!
//! # Pourquoi pas une `String`
//!
//! Une phrase toute faite fige une langue au moment de l'application de la
//! commande, et oblige chaque crate à refaire sa grammaire (le pluriel de
//! « ligne » écrit à la main dans `Command::label`). Un libellé structuré se
//! met en phrase **à l'affichage**, donc suit la langue courante ; il se
//! compare (`==`) sans parser un texte (l'application demandait
//! `label == "Importer"`), et il se transporte tel quel si l'historique est un
//! jour écrit dans un fichier — l'historique n'est **pas** dans le `.stf`
//! aujourd'hui (`docs/persistance.md`).
//!
//! # Le `Display` anglais
//!
//! [`HistoryLabel`] s'affiche en **anglais** : c'est le texte des journaux et
//! du CLI, qui ne traduisent pas (chantier #39). Il double le catalogue `en`
//! de l'application (clés `history-*`) ; `stokesforge-app/tests/shell.rs` vérifie
//! que les deux disent la même chose, variante par variante.

use std::fmt;

use stokesforge_analysis::ops::OperationKind;
use stokesforge_core::family_english;

/// Désignation d'une **colonne** dans un libellé : sa position dans sa
/// feuille, jamais son nom de nœud.
///
/// Une colonne porte deux identités et une seule est visible : son nom de
/// nœud est une clé du modèle, la grille n'affiche que la lettre de position
/// (A, B, C…). Un libellé qui nommerait le nœud annoncerait « Temps » pour une
/// colonne que l'utilisateur ne connaît que sous le nom « A ». La lettre se
/// calcule par [`stokesforge_core::spreadsheet_name`], la fonction de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnRef(pub usize);

impl ColumnRef {
    /// La lettre d'en-tête de la grille : `A`, `B`, … `Z`, `AA`…
    pub fn letter(self) -> String {
        stokesforge_core::spreadsheet_name(self.0)
    }
}

/// Les groupes d'actions que l'application ouvre par `begin_macro` : une
/// énumération **fermée**, pour qu'un groupe soit une donnée et non un texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroKind {
    /// Coller des cellules.
    PasteCells,
    /// Coller des objets du projet.
    PasteObjects,
    /// Dupliquer les objets sélectionnés.
    DuplicateObjects,
    /// Supprimer les objets sélectionnés.
    DeleteObjects,
    /// Déplacer les objets sélectionnés.
    MoveObjects,
    /// Renommer la sélection du Project Manager.
    RenameObjects,
    /// Effacer la sélection de la grille.
    ClearSelection,
    /// Appliquer la fenêtre des propriétés d'un graphe.
    GraphProperties,
    /// Actualiser des colonnes depuis leur fichier source.
    RefreshFromFile,
    /// Importer des fichiers d'appareil.
    Import,
    /// Créer un classeur et le remplir (tracé, import dans un nouveau classeur).
    AddWorkbook,
    /// Créer une feuille et la remplir.
    AddWorksheet,
    /// Exécuter les écritures d’un script Python.
    Script,
    /// Changer le type de plusieurs colonnes d'un coup.
    SetColumnsType,
    /// Changer le rôle de plusieurs colonnes d'un coup.
    SetColumnsRole,
    /// Un calcul d'analyse et le tracé de ses colonnes sur un graphe (#200).
    AnalysisToGraph,
    /// Retirer la formule d'une colonne, avec ou sans ses valeurs (#236).
    RemoveFormula,
    /// Supprimer plusieurs colonnes entières d'une feuille.
    DeleteColumns,
}

impl MacroKind {
    /// Toutes les natures de groupe, pour les tests exhaustifs.
    pub const ALL: [MacroKind; 18] = [
        MacroKind::PasteCells,
        MacroKind::PasteObjects,
        MacroKind::DuplicateObjects,
        MacroKind::DeleteObjects,
        MacroKind::MoveObjects,
        MacroKind::RenameObjects,
        MacroKind::ClearSelection,
        MacroKind::GraphProperties,
        MacroKind::RefreshFromFile,
        MacroKind::Import,
        MacroKind::AddWorkbook,
        MacroKind::AddWorksheet,
        MacroKind::Script,
        MacroKind::SetColumnsType,
        MacroKind::SetColumnsRole,
        MacroKind::AnalysisToGraph,
        MacroKind::RemoveFormula,
        MacroKind::DeleteColumns,
    ];
}

/// Le libellé d'une entrée d'historique : l'action et ses paramètres.
///
/// Un champ `Option` vaut `None` quand le nœud visé n'est plus dans l'arbre au
/// moment de calculer le libellé (une commande qui échouera de toute façon) :
/// la phrase retombe alors sur une forme générique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryLabel {
    /// Écriture de cellules (`SetCells`, `SetCellsPromoting`).
    SetCells {
        /// Colonne écrite.
        column: Option<ColumnRef>,
    },
    /// Ajout d'une colonne.
    AddColumn,
    /// Suppression d'une colonne.
    DeleteColumn {
        /// Colonne supprimée.
        column: Option<ColumnRef>,
    },
    /// Renommage d'un nœud.
    Rename {
        /// Nom d'avant.
        from: Option<String>,
        /// Nom d'après.
        to: String,
    },
    /// Déplacement d'un nœud.
    Move {
        /// Nom du nœud déplacé.
        name: Option<String>,
    },
    /// Changement du type d'une colonne.
    SetColumnType {
        /// Colonne convertie.
        column: Option<ColumnRef>,
    },
    /// Insertion de lignes.
    InsertRows {
        /// Nombre de lignes.
        count: usize,
    },
    /// Suppression de lignes.
    DeleteRows {
        /// Nombre de lignes.
        count: usize,
    },
    /// Suppression de cellules avec remontée de celles du dessous.
    DeleteCells {
        /// Nombre de lignes retirées dans chaque colonne.
        count: usize,
    },
    /// Ajout d'un dossier.
    AddFolder {
        /// Nom du dossier.
        name: String,
    },
    /// Ajout d'un classeur.
    AddWorkbook,
    /// Ajout d'un classeur sans feuille.
    AddEmptyWorkbook,
    /// Ajout d'une feuille.
    AddWorksheet,
    /// Duplication d'une feuille.
    DuplicateWorksheet {
        /// Nom de la feuille dupliquée.
        name: Option<String>,
    },
    /// Ajout d'un graphique.
    AddGraph,
    /// Ajout d'une matrice.
    AddMatrix,
    /// Ajout d'une note.
    AddNote,
    /// Suppression d'un nœud et de sa descendance.
    RemoveNode {
        /// Nom du nœud supprimé.
        name: Option<String>,
    },
    /// Modification des propriétés d'une fenêtre.
    SetWindowMeta {
        /// Nom de la fenêtre.
        name: Option<String>,
    },
    /// Modification du texte d'une note.
    SetNoteText {
        /// Nom de la note.
        name: Option<String>,
    },
    /// Modification des propriétés d'une colonne.
    SetColumnMeta {
        /// Colonne modifiée.
        column: Option<ColumnRef>,
    },
    /// Modification du tracé d'un graphique.
    SetPlotSpec {
        /// Nom du graphique.
        name: Option<String>,
    },
    /// Modification des couleurs enregistrées du projet.
    SetSavedColors,
    /// Enregistrement d'un calcul d'analyse (#181).
    AddCalculation(OperationKind),
    /// Correction définitive des valeurs originales.
    ReplaceAnalysisData(OperationKind),
    /// Restauration des valeurs originales qu'une correction avait remplacées
    /// (#248), avec la famille de la correction si on la connaît encore.
    RestoreOriginalData(Option<OperationKind>),
    /// Modification des paramètres d'un calcul d'analyse.
    UpdateCalculation(OperationKind),
    /// Retrait d'un calcul d'analyse.
    RemoveCalculation {
        /// Famille du calcul retiré, si on la connaît encore.
        kind: Option<OperationKind>,
        /// Ses colonnes de sortie sont supprimées avec lui.
        with_columns: bool,
    },
    /// Édition ciblée du contenu du graphe, toujours par remplacement du spec.
    EditPlotSpec(PlotEditKind),
    /// Un groupe d'actions annulé d'un bloc.
    Macro(MacroKind),
}

/// Motif d'une édition de contenu du graphe (#84–85), sans texte traduit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotEditKind {
    Visibility,
    RenameSeries,
    ReorderSeries,
    RemoveSeries,
    RemoveText,
    RemoveObject,
    ReorderObject,
    /// Nom d'un groupe de séries (#197).
    RenameGroup,
    /// Un groupe de séries dissous : ses séries restent (#197).
    DissolveGroup,
    /// Un groupe de séries supprimé avec ses séries (#197).
    RemoveGroup,
    /// Un groupe de séries déplacé en bloc (#197).
    ReorderGroup,
}
impl PlotEditKind {
    pub const ALL: [Self; 11] = [
        Self::Visibility,
        Self::RenameSeries,
        Self::ReorderSeries,
        Self::RemoveSeries,
        Self::RemoveText,
        Self::RemoveObject,
        Self::ReorderObject,
        Self::RenameGroup,
        Self::DissolveGroup,
        Self::RemoveGroup,
        Self::ReorderGroup,
    ];
}
impl std::fmt::Display for PlotEditKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Visibility => "Change Visibility",
            Self::RenameSeries => "Rename Series",
            Self::ReorderSeries => "Reorder Series",
            Self::RemoveSeries => "Delete Series",
            Self::RemoveText => "Delete Text",
            Self::RemoveObject => "Delete Object",
            Self::ReorderObject => "Reorder Object",
            Self::RenameGroup => "Rename Group",
            Self::DissolveGroup => "Ungroup Series",
            Self::RemoveGroup => "Delete Group",
            Self::ReorderGroup => "Reorder Group",
        })
    }
}

/// « Row » ou « Rows » : en anglais seul 1 est au singulier.
fn rows(count: usize) -> &'static str {
    if count == 1 {
        "Row"
    } else {
        "Rows"
    }
}

impl fmt::Display for MacroKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MacroKind::PasteCells => "Paste",
            MacroKind::PasteObjects => "Paste Objects",
            MacroKind::DuplicateObjects => "Duplicate Objects",
            MacroKind::DeleteObjects => "Delete Objects",
            MacroKind::MoveObjects => "Move Objects",
            MacroKind::RenameObjects => "Rename Objects",
            MacroKind::ClearSelection => "Clear Selection",
            MacroKind::GraphProperties => "Edit Graph Properties",
            MacroKind::RefreshFromFile => "Refresh from File",
            MacroKind::Import => "Import",
            MacroKind::AddWorkbook => "Add Workbook",
            MacroKind::AddWorksheet => "Add Sheet",
            MacroKind::Script => "Run Script",
            MacroKind::SetColumnsType => "Change Column Types",
            MacroKind::SetColumnsRole => "Change Column Roles",
            MacroKind::AnalysisToGraph => "Analyze and Plot",
            MacroKind::RemoveFormula => "Remove Formula",
            MacroKind::DeleteColumns => "Delete Columns",
        })
    }
}

impl fmt::Display for HistoryLabel {
    /// Le libellé **en anglais**, pour les journaux et le CLI. L'interface ne
    /// l'emploie pas : elle met la variante en phrase par son catalogue.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryLabel::SetCells { column: Some(c) } => {
                write!(f, "Edit Cells in Column {}", c.letter())
            }
            HistoryLabel::SetCells { column: None } => f.write_str("Edit Cells"),
            HistoryLabel::AddColumn => f.write_str("Add Column"),
            HistoryLabel::DeleteColumn { column: Some(c) } => {
                write!(f, "Delete Column {}", c.letter())
            }
            HistoryLabel::DeleteColumn { column: None } => f.write_str("Delete Column"),
            HistoryLabel::Rename {
                from: Some(from),
                to,
            } => write!(f, "Rename “{from}” to “{to}”"),
            HistoryLabel::Rename { from: None, to } => write!(f, "Rename to “{to}”"),
            HistoryLabel::Move { name: Some(name) } => write!(f, "Move “{name}”"),
            HistoryLabel::Move { name: None } => f.write_str("Move"),
            HistoryLabel::SetColumnType { column: Some(c) } => {
                write!(f, "Change Type of Column {}", c.letter())
            }
            HistoryLabel::SetColumnType { column: None } => f.write_str("Change Column Type"),
            HistoryLabel::InsertRows { count } => write!(f, "Insert {count} {}", rows(*count)),
            HistoryLabel::DeleteRows { count } => write!(f, "Delete {count} {}", rows(*count)),
            HistoryLabel::DeleteCells { count } => {
                write!(f, "Delete Cells and Shift Up ({count} {})", rows(*count))
            }
            HistoryLabel::AddFolder { name } => write!(f, "Add Folder “{name}”"),
            HistoryLabel::AddWorkbook => f.write_str("Add Workbook"),
            HistoryLabel::AddEmptyWorkbook => f.write_str("Add Empty Workbook"),
            HistoryLabel::AddWorksheet => f.write_str("Add Sheet"),
            HistoryLabel::DuplicateWorksheet { name } => match name {
                Some(name) => write!(f, "Duplicate Sheet “{name}”"),
                None => f.write_str("Duplicate Sheet"),
            },
            HistoryLabel::AddGraph => f.write_str("Add Graph"),
            HistoryLabel::AddMatrix => f.write_str("Add Matrix"),
            HistoryLabel::AddNote => f.write_str("Add Note"),
            HistoryLabel::RemoveNode { name: Some(name) } => write!(f, "Delete “{name}”"),
            HistoryLabel::RemoveNode { name: None } => f.write_str("Delete"),
            HistoryLabel::SetWindowMeta { name: Some(name) } => {
                write!(f, "Edit Properties of “{name}”")
            }
            HistoryLabel::SetWindowMeta { name: None } => f.write_str("Edit Window Properties"),
            HistoryLabel::SetNoteText { name: Some(name) } => write!(f, "Edit Note “{name}”"),
            HistoryLabel::SetNoteText { name: None } => f.write_str("Edit Note"),
            HistoryLabel::SetColumnMeta { column: Some(c) } => {
                write!(f, "Edit Properties of Column {}", c.letter())
            }
            HistoryLabel::SetColumnMeta { column: None } => f.write_str("Edit Column Properties"),
            HistoryLabel::SetPlotSpec { name: Some(name) } => write!(f, "Edit Graph “{name}”"),
            HistoryLabel::SetPlotSpec { name: None } => f.write_str("Edit Graph"),
            HistoryLabel::EditPlotSpec(kind) => kind.fmt(f),
            HistoryLabel::SetSavedColors => f.write_str("Edit Saved Colors"),
            HistoryLabel::ReplaceAnalysisData(kind) => {
                write!(f, "Replace Original Data ({})", family_english(*kind))
            }
            HistoryLabel::RestoreOriginalData(Some(kind)) => {
                write!(f, "Restore Original Data ({})", family_english(*kind))
            }
            HistoryLabel::RestoreOriginalData(None) => f.write_str("Restore Original Data"),
            HistoryLabel::AddCalculation(kind) => {
                write!(f, "Add {} Calculation", family_english(*kind))
            }
            HistoryLabel::UpdateCalculation(kind) => {
                write!(f, "Edit {} Calculation", family_english(*kind))
            }
            HistoryLabel::RemoveCalculation { kind, with_columns } => {
                let verb = if *with_columns { "Delete" } else { "Detach" };
                match kind {
                    Some(kind) => write!(f, "{verb} {} Calculation", family_english(*kind)),
                    None => write!(f, "{verb} Calculation"),
                }
            }
            HistoryLabel::Macro(kind) => kind.fmt(f),
        }
    }
}
