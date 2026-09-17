//! L'état du banc d'essai, **sans aucune dépendance à egui**.
//!
//! # Pourquoi ce découpage
//!
//! Une fenêtre ne se teste pas en intégration continue. Toute la logique du
//! banc vit donc ici, pilotée par un enum d'[`Action`] ; la couche `eframe`
//! (module [`crate::app`]) ne fait que traduire clavier et souris en actions
//! et peindre l'état. C'est ce qui rend la boucle complète — modèle →
//! commande → annulation → sauvegarde → rechargement — vérifiable
//! automatiquement (`tests/shell.rs`).
//!
//! # Deux règles de conduite
//!
//! 1. [`AppState::dispatch`] **ne rend jamais d'erreur** : toute erreur
//!    devient un [`StatusMessage`] affiché à l'utilisateur. Un banc d'essai
//!    qui meurt sur une saisie invalide ne prouve rien.
//! 2. L'instantané détenu est **figé** (l'arbre du modèle est persistant) : il
//!    faut le redemander à l'acteur après chaque mutation, ce que fait
//!    [`AppState::refresh`], appelé par `dispatch` après toute action qui a pu
//!    changer le modèle.
//!
//! # Le script Python (feature `python`)
//!
//! Trois décisions, prises ici et nulle part ailleurs :
//!
//! 1. **L'interpréteur ne démarre jamais à l'ouverture de la fenêtre.**
//!    [`AppState::new`] ne construit ni ne touche à `plotyx_script::
//!    ScriptEngine` ; seul le premier [`Action::RunScript`] le fait, via
//!    `AppState::run_script`. Le temps de démarrage de l'interpréteur est
//!    l'un des motifs du portage — le perdre à l'ouverture annulerait le
//!    bénéfice.
//! 2. **`RunScript` ne bloque jamais l'appelant.** `ScriptEngine::run`, lui,
//!    bloque jusqu'au terme du script — l'appeler depuis la boucle de rendu
//!    figerait la fenêtre, y compris le bouton « Arrêter ». `run_script` lance
//!    donc (la création paresseuse du moteur comprise) sur un **fil détaché**
//!    et revient aussitôt ; [`AppState::poll_script`], à appeler à **chaque
//!    image** par [`crate::app`] (et par un test qui attend une exécution),
//!    sonde le résultat par `try_recv` et l'applique dès qu'il est prêt —
//!    rafraîchissement de l'instantané compris. C'est ce qui rend `dispatch`
//!    sûr à appeler depuis la boucle de rendu pour **toutes** les actions,
//!    script inclus, sans cas particulier côté `crate::app`.
//! 3. **Une exception de script n'est pas une erreur d'application.** Elle
//!    atterrit dans la sortie ([`AppState::script_output`]) avec sa trace
//!    d'appels ; le message de statut posé par `poll_script` reste neutre
//!    (`is_error: false`), et l'état reste utilisable.

use std::path::PathBuf;
#[cfg(feature = "python")]
use std::sync::mpsc;
use std::sync::Arc;
#[cfg(feature = "python")]
use std::thread;

use plotyx_commands::Command;
use plotyx_core::node::NodeKind;
use plotyx_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, Node, NodeId, Tree, ValueBlock,
};
#[cfg(feature = "python")]
use plotyx_python::interpreter::EmbeddedPython;
#[cfg(feature = "python")]
use plotyx_script::{ScriptEngine, ScriptOutcome};
use plotyx_session::{SessionHandle, SessionResult, Snapshot};

use crate::selection::Selection;

/// Ce que l'utilisateur peut demander. La couche fenêtre ne fait que produire
/// ces valeurs ; elle ne touche jamais l'acteur directement.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Annule la dernière entrée d'historique.
    Undo,
    /// Rétablit la dernière entrée annulée.
    Redo,
    /// Enregistre le projet dans le fichier courant ([`AppState::path`]).
    Save,
    /// Recharge le projet depuis le fichier courant.
    Load,
    /// Sélectionne la feuille affichée dans la grille du classeur qui la
    /// contient (le classeur est déduit du parent de `id` dans l'arbre) — un
    /// classeur affiché dans son propre onglet [`crate::app`], la sélection
    /// d'une feuille dans l'un n'affecte jamais celle d'un autre classeur
    /// ouvert simultanément.
    SelectSheet(NodeId),
    /// Valide la saisie d'une cellule : le texte tel que tapé, à convertir
    /// selon le type de la colonne.
    CommitCell {
        /// Colonne visée.
        column: NodeId,
        /// Ligne visée.
        row: usize,
        /// Texte saisi, non converti.
        text: String,
    },
    /// Ajoute une colonne en fin de `sheet`.
    AddColumn {
        /// Feuille visée — explicite, puisque plusieurs classeurs peuvent
        /// être affichés simultanément, chacun dans son propre onglet.
        sheet: NodeId,
    },
    /// Change le type d'une colonne — choix explicite de l'utilisateur
    /// depuis l'en-tête, conversion destructrice assumée (`Column::
    /// set_column_type`, `plotyx-core`) : un texte non numérique dans une
    /// colonne qui devient `Numeric` disparaît (`NaN`), comme le reste déjà
    /// pour une saisie directe.
    SetColumnType {
        /// Colonne visée.
        column: NodeId,
        /// Type demandé.
        column_type: ColumnType,
    },
    /// Change le rôle de tracé d'une colonne (`ColumnMeta::role`) — choix
    /// explicite depuis le sous-menu « Rôle » du menu contextuel de
    /// l'en-tête, à côté du type. N'affecte, pour l'instant, que
    /// l'affichage (`A(x)`…) et la persistance : rien côté résolution de
    /// série ne le lit encore.
    SetColumnRole {
        /// Colonne visée.
        column: NodeId,
        /// Rôle demandé.
        role: ColumnRole,
    },
    /// Insère `count` lignes à `at` dans `sheet`.
    InsertRows {
        /// Feuille visée.
        sheet: NodeId,
        /// Ligne d'insertion.
        at: usize,
        /// Nombre de lignes insérées.
        count: usize,
    },
    /// Supprime `count` lignes à `at` dans `sheet`.
    DeleteRows {
        /// Feuille visée.
        sheet: NodeId,
        /// Première ligne supprimée.
        at: usize,
        /// Nombre de lignes supprimées.
        count: usize,
    },
    /// Remplace le texte d'une note en bloc (perte de focus de l'éditeur,
    /// comme [`Action::CommitCell`] pour une cellule de grille).
    SetNoteText {
        /// Note visée.
        note: NodeId,
        /// Nouveau texte.
        text: String,
    },
    /// Lance le script actuellement dans la zone de saisie
    /// ([`AppState::script_source`]).
    ///
    /// **Ne bloque pas** : voir la section « Le script Python » en tête de ce
    /// module. Sans la feature `python`, produit un [`StatusMessage`]
    /// explicite et n'envoie aucune commande.
    RunScript,
    /// Demande l'arrêt du script en cours. Sans effet s'il n'y en a pas.
    /// Ne bloque pas.
    InterruptScript,
    /// Valide la saisie d'une cellule (comme [`Action::CommitCell`]) puis,
    /// si `row` était la dernière ligne de `column` et que la validation a
    /// réussi, ajoute une ligne à `sheet` — Entrée sur la dernière cellule
    /// d'une colonne.
    CommitCellAndAdvance {
        /// Colonne visée.
        column: NodeId,
        /// Ligne visée.
        row: usize,
        /// Texte saisi, non converti.
        text: String,
        /// Feuille à laquelle ajouter une ligne si besoin.
        sheet: NodeId,
    },
    /// Vide le contenu des cellules actuellement sélectionnées dans `sheet`,
    /// sans décaler ce qui suit — Retour arrière ou Ctrl+X.
    ClearSelectedCells {
        /// Feuille visée.
        sheet: NodeId,
    },
    /// Vide le contenu des cellules actuellement sélectionnées dans `sheet`
    /// **et** fait remonter les cellules suivantes de chaque colonne
    /// touchée, indépendamment — touche Suppr.
    DeleteSelectedCells {
        /// Feuille visée.
        sheet: NodeId,
    },
    /// Colle `text` (TSV) à partir de l'ancre de la sélection courante de
    /// `sheet`.
    PasteTsv {
        /// Feuille visée.
        sheet: NodeId,
        /// Texte collé, au format TSV (`\t` entre colonnes, `\n` entre
        /// lignes).
        text: String,
    },
    /// Remplace le nom long affiché d'une colonne
    /// ([`plotyx_core::ColumnMeta::long_name`]).
    SetColumnLongName {
        /// Colonne visée.
        column: NodeId,
        /// Nouveau nom long.
        text: String,
    },
    /// Remplace l'unité affichée d'une colonne
    /// ([`plotyx_core::ColumnMeta::unit`]).
    SetColumnUnit {
        /// Colonne visée.
        column: NodeId,
        /// Nouvelle unité.
        text: String,
    },
    /// Remplace le commentaire affiché d'une colonne
    /// ([`plotyx_core::ColumnMeta::comment`]).
    SetColumnComment {
        /// Colonne visée.
        column: NodeId,
        /// Nouveau commentaire.
        text: String,
    },
    /// Remplace la formule affichée d'une colonne
    /// ([`plotyx_core::ColumnMeta::formula`]) — stockage/affichage
    /// seulement, l'évaluation reste celle de `plotyx_core::formula` /
    /// `plotyx-session::actor`, hors du périmètre de cette action.
    SetColumnFormula {
        /// Colonne visée.
        column: NodeId,
        /// Nouvelle formule, ou `None` pour une colonne non calculée.
        formula: Option<String>,
    },
    /// Importe chaque fichier de `paths` dans sa propre nouvelle feuille de
    /// `book`, avec la recette `recipe` — voir [`AppState::import_files`].
    /// Tout le lot (toutes les feuilles, toutes les colonnes) tient dans une
    /// seule entrée d'historique.
    ImportFiles {
        /// Classeur d'accueil.
        book: NodeId,
        /// Réglages d'analyse à appliquer à chaque fichier.
        recipe: plotyx_import::ImportRecipe,
        /// Fichiers à importer, chacun dans sa propre feuille.
        paths: Vec<std::path::PathBuf>,
    },
}

/// Un message à afficher à l'utilisateur, avec sa gravité.
///
/// `is_error` sépare deux choses que Qt distinguait par le grisage d'une
/// action : « impossible pour l'instant » (annuler à vide) n'est **pas** une
/// erreur, alors qu'un chemin illisible en est une.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusMessage {
    /// Texte affiché, en français.
    pub text: String,
    /// Vrai si le message signale un échec.
    pub is_error: bool,
}

/// Ce qu'une exécution de script a produit, tel que l'état applicatif le
/// présente.
///
/// Copie **volontairement indépendante** de `plotyx_script::ScriptOutcome` :
/// `state.rs` doit rester lisible, et sa surface publique stable, que la
/// feature `python` soit active ou non — [`AppState::script_output`] rend ce
/// type dans les deux configurations plutôt que d'exposer un type qui
/// n'existerait que sous la feature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptOutput {
    /// Ce que le script a écrit sur la sortie standard (`print`).
    pub stdout: String,
    /// Ce que le script a écrit sur la sortie d'erreur.
    pub stderr: String,
    /// Message d'erreur **avec sa trace d'appels Python complète**, si le
    /// script a levé une exception. Une exception de script n'est pas une
    /// erreur d'application : ce champ n'influence jamais [`StatusMessage::
    /// is_error`].
    pub error: Option<String>,
    /// Vrai si l'exécution s'est arrêtée sur une interruption demandée
    /// ([`Action::InterruptScript`]).
    pub interrupted: bool,
}

#[cfg(feature = "python")]
impl From<ScriptOutcome> for ScriptOutput {
    fn from(outcome: ScriptOutcome) -> Self {
        Self {
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: outcome.error,
            interrupted: outcome.interrupted,
        }
    }
}

/// Ce que [`AppState::cell_text`] affiche pour une valeur **absente**
/// (`Cell::Text(None)`) — voir sa documentation (A9, option 2). Un espace
/// insécable plutôt qu'une chaîne vide : il occupe la cellule comme un blanc
/// ordinaire (rien à l'écran ne « saute aux yeux »), mais reste, en mémoire,
/// une chaîne différente de `""` — donc distincte d'une chaîne vide
/// **présente**. Choisi plutôt qu'un caractère visible (`—`, `∅`, `?`) parce
/// qu'aucun de ceux-là n'est une réponse illégitime à « qu'est-ce qu'un
/// utilisateur pourrait vouloir taper dans une cellule de texte ? » — un
/// espace insécable, lui, n'est accessible à la frappe sur aucun clavier
/// usuel.
pub const MISSING_TEXT_DISPLAY: &str = "\u{00A0}";

/// Seuils de bascule en notation scientifique pour [`format_f64_for_display`].
///
/// Choisis pour que `1.602176634e-19` (charge de l'électron) et
/// `6.022140076e23` (nombre d'Avogadro) — les deux mesures de physicien
/// citées par la reproduction de C4 — tombent chacune du bon côté, avec une
/// marge confortable de part et d'autre plutôt qu'un seuil collé aux deux
/// exemples : `SCI_LOWER` largement en dessous de la plus petite magnitude
/// « ordinaire » qu'une mesure de laboratoire prend en unités SI de base, et
/// `SCI_UPPER` largement au-dessus.
const SCI_LOWER: f64 = 1e-4;
const SCI_UPPER: f64 = 1e16;

/// Formate un `f64` **fini et non nul** pour l'affichage en cellule de
/// grille — bascule en notation scientifique aux magnitudes extrêmes plutôt
/// que de s'en remettre au seul `Display` de `f64`, qui n'émet jamais
/// d'exposant.
///
/// # C4 (moitié `plotyx-app`, `guide-de-correction-robustesse.md` §F8)
///
/// Une mesure de physicien comme la charge de l'électron
/// (`1.602176634e-19`) s'écrivait, avant ce correctif, `0.0000000000000000001602176634`
/// — trente caractères dans une colonne de grille large d'une centaine de
/// pixels (une quinzaine de caractères ; vingt-quatre est déjà large). En
/// dessous de [`SCI_LOWER`] ou à partir de [`SCI_UPPER`] (exclu en dessous,
/// inclus au-dessus — une magnitude *nulle* ne bascule jamais : `0.0` reste
/// `"0"`, pas `"0e0"`), la valeur s'écrit en notation scientifique
/// (`{:e}` — Rust y choisit, comme pour `Display`, la représentation la plus
/// courte qui redonne le même `f64` en le reparsant : l'aller-retour
/// `f64 → texte → f64` reste exact bit à bit). En dehors de cette plage, le
/// `Display` décimal habituel suffit et reste le plus lisible.
///
/// # §F8 — « une seule table, deux directions »
///
/// Le guide nomme deux appelants qui formatent aujourd'hui un `f64` sans se
/// concerter : `AppState::cell_text` (ici, l'affichage) et
/// `Column::set_column_type` (`Float → Text`, dans `plotyx-core`, la
/// conversion) — « sans quoi l'affichage et la conversion diront deux choses
/// différentes de la même valeur ». Le correctif de `plotyx-core` est mené en
/// parallèle, dans un autre arbre de travail, par un autre agent ; au moment
/// d'écrire cette fonction, `plotyx_core::column` n'expose encore aucune
/// fonction de formatage publique à réutiliser ici (vérifié par
/// `grep -n "pub fn format" crates/plotyx-core/src/column.rs`). Cette
/// fonction reproduit donc, côté `plotyx-app` seulement, la même idée
/// (seuils, bascule en notation scientifique) sans prétendre être
/// bit-à-bit identique à ce que produira l'autre moitié du correctif : les
/// deux devront être unifiées — dans `plotyx-core`, pour que `cell_text`
/// délègue à la fonction publique du cœur plutôt que de la dupliquer — une
/// fois les deux travaux fusionnés.
fn format_f64_for_display(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude != 0.0 && (magnitude < SCI_LOWER || magnitude >= SCI_UPPER) {
        format!("{value:e}")
    } else {
        format!("{value}")
    }
}

/// Construit le bloc de valeurs d'une colonne collée depuis un TSV : la
/// colonne d'indice `col_index` de chaque ligne de `rows`, convertie selon
/// `kind` avec la même tolérance que [`AppState::commit_cell`] — mais sans
/// jamais refuser (invariant 10, « un défaut de donnée dégrade, il ne
/// plante pas ») : un texte numérique inanalysable devient `NaN`, jamais une
/// erreur qui bloquerait tout le collage. Une ligne plus courte que
/// `col_index` fournit une valeur manquante pour cette colonne.
fn value_block_from_tsv_column(
    kind: ColumnType,
    rows: &[Vec<&str>],
    col_index: usize,
) -> ValueBlock {
    match kind {
        ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(
            rows.iter()
                .map(|row| {
                    let text = row.get(col_index).copied().unwrap_or("").trim();
                    if text.is_empty() {
                        f64::NAN
                    } else {
                        text.parse::<f64>().unwrap_or(f64::NAN)
                    }
                })
                .collect(),
        ),
        ColumnType::Text => ValueBlock::Text(
            rows.iter()
                .map(|row| {
                    let text = row.get(col_index).copied().unwrap_or("");
                    (!text.is_empty()).then(|| text.to_string())
                })
                .collect(),
        ),
        // Colonne mixte : chaque cellule décide pour elle-même, par la même
        // règle que le cœur (`MixedCell::from_text`) — un jeton qui s'analyse
        // devient un nombre, les autres restent du texte, et un jeton vide
        // une cellule vide. C'est le collage qui se rapproche le plus de ce
        // qu'un tableur fait : rien n'est perdu, et ce qui est un nombre est
        // traçable.
        ColumnType::Mixed => ValueBlock::Mixed(
            rows.iter()
                .map(|row| {
                    let text = row.get(col_index).copied().unwrap_or("").trim();
                    MixedCell::from_text((!text.is_empty()).then(|| text.to_string()))
                })
                .collect(),
        ),
    }
}

/// Corps de [`AppState::cell_text`], factorisé sur un `&Tree` plutôt que sur
/// `&self` : c'est ce qui permet à une fenêtre détachée
/// ([`crate::app::PlotyxApp::show_detached_tabs`], qui ne porte qu'un clone
/// de [`Snapshot`], jamais `AppState` elle-même — voir sa doc de tête) de
/// peindre une grille identique à celle du dock, sans dupliquer la règle
/// d'affichage (`MISSING_TEXT_DISPLAY`, bascule scientifique A9/C4 — voir
/// [`AppState::cell_text`] pour le détail de ces choix).
pub(crate) fn cell_text_in(tree: &Tree, column: NodeId, row: usize) -> String {
    let Some(column) = tree.node(column).and_then(Node::as_column) else {
        return String::new();
    };
    // Une colonne **mixte** se lit par `Column::cell`, pas par `Column::get` :
    // seul ce chemin distingue une cellule vide (`Empty`, qui s'affiche comme
    // une cellule numérique vide, `""`) d'une cellule texte sans valeur — que
    // `get` confond toutes deux en `Cell::Text(None)`. Le choix A9 de
    // `MISSING_TEXT_DISPLAY` vaut pour une colonne *texte*, où une cellule
    // vide **présente** existe et doit rester indiscernable à l'œil ; une
    // colonne mixte, elle, porte l'information dans son type de cellule.
    if column.column_type() == ColumnType::Mixed {
        return match column.cell(row) {
            MixedCell::Empty => String::new(),
            MixedCell::Number(value) if value.is_nan() => String::new(),
            MixedCell::Number(value) => format_f64_for_display(value),
            MixedCell::Text(text) => text,
        };
    }
    match column.get(row) {
        Cell::Float(value) if value.is_nan() => String::new(),
        Cell::Float(value) => format_f64_for_display(value),
        Cell::Text(None) => MISSING_TEXT_DISPLAY.to_string(),
        Cell::Text(Some(text)) => text,
    }
}

/// Corps de [`AppState::visible_columns`], factorisé sur un `&Tree` — voir
/// [`cell_text_in`].
pub(crate) fn visible_columns_in(tree: &Tree, sheet: NodeId) -> Vec<(NodeId, String, ColumnType)> {
    let Some(node) = tree.node(sheet) else {
        return Vec::new();
    };
    node.children()
        .iter()
        .filter_map(|id| {
            let child = tree.node(*id)?;
            let column = child.as_column()?;
            Some((*id, child.name().to_string(), column.column_type()))
        })
        .collect()
}

/// Corps de [`AppState::row_count`], factorisé sur un `&Tree` — voir
/// [`cell_text_in`].
pub(crate) fn row_count_in(tree: &Tree, sheet: NodeId) -> usize {
    tree.row_count(sheet).unwrap_or(0)
}

/// Corps de [`AppState::worksheets_of`], factorisé sur un `&Tree` — voir
/// [`cell_text_in`].
pub(crate) fn worksheets_of_in(tree: &Tree, book: NodeId) -> Vec<(NodeId, String)> {
    let Some(node) = tree.node(book) else {
        return Vec::new();
    };
    node.children()
        .iter()
        .filter_map(|id| tree.node(*id).map(|sheet| (*id, sheet.name().to_string())))
        .collect()
}

/// Corps de [`AppState::note_text`], factorisé sur un `&Tree` — voir
/// [`cell_text_in`].
pub(crate) fn note_text_in(tree: &Tree, note: NodeId) -> Option<String> {
    match tree.node(note)?.kind() {
        NodeKind::Note { text, .. } => Some(text.clone()),
        _ => None,
    }
}

/// Nom d'un nœud, ou `"{missing} {id} (supprimé)"` s'il n'existe plus dans
/// `tree` — pendant de `crate::app::node_title`, factorisé sur un `&Tree`
/// pour la même raison que [`cell_text_in`].
pub(crate) fn node_title_in(tree: &Tree, id: NodeId, missing: &str) -> String {
    tree.node(id)
        .map(|node| node.name().to_string())
        .unwrap_or_else(|| format!("{missing} {id} (supprimé)"))
}

/// Saisie en cours dans une cellule.
///
/// Une seule cellule est éditable à la fois : c'est ce qui permet à la grille
/// virtualisée de ne garder qu'un tampon, et non un par cellule visible. Le
/// tampon n'est envoyé au modèle qu'à la validation (perte de focus ou
/// Entrée), jamais à chaque frappe — sinon l'historique compterait une entrée
/// par caractère.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    /// Colonne en cours de saisie.
    pub column: NodeId,
    /// Ligne en cours de saisie.
    pub row: usize,
    /// Texte tapé jusqu'ici.
    pub buffer: String,
}

/// Champ de [`plotyx_core::ColumnMeta`] éditable dans l'en-tête empilé d'une
/// colonne — voir [`HeaderEdit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    /// Nom long ([`plotyx_core::ColumnMeta::long_name`]).
    LongName,
    /// Unité ([`plotyx_core::ColumnMeta::unit`]).
    Unit,
    /// Commentaire ([`plotyx_core::ColumnMeta::comment`]).
    Comment,
    /// Formule ([`plotyx_core::ColumnMeta::formula`]).
    Formula,
}

/// Direction d'un déplacement clavier d'une cellule dans la grille (flèches
/// directionnelles) — voir [`AppState::move_selection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Saisie en cours dans un champ d'en-tête (nom long, unité, commentaire,
/// formule). Le pendant exact de [`CellEdit`] pour l'en-tête empilé d'une
/// colonne — un seul champ éditable à la fois, y compris entre colonnes
/// différentes, envoyé au modèle seulement à la validation.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderEdit {
    /// Colonne en cours de saisie.
    pub column: NodeId,
    /// Champ en cours de saisie.
    pub field: HeaderField,
    /// Texte tapé jusqu'ici.
    pub buffer: String,
}

/// Saisie en cours dans une note. Le pendant exact de [`CellEdit`] pour
/// [`Tab::Note`](crate::app) : un seul tampon à la fois (voir la doc de
/// [`AppState::note_editing`]), envoyé au modèle seulement à la validation.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteEdit {
    /// Note en cours de saisie.
    pub note: NodeId,
    /// Texte tapé jusqu'ici.
    pub buffer: String,
}

/// État transitoire de la fenêtre d'import (voir [`crate::app`] pour son
/// rendu) : jamais dans l'historique, jamais envoyé à l'acteur — même
/// famille que [`HeaderEdit`]/[`CellEdit`]/[`NoteEdit`]. Porte tout ce que la
/// fenêtre a besoin de retenir entre deux images, y compris le
/// [`plotyx_import::store::RecipeStore`] chargé une seule fois à l'ouverture
/// (pas rechargé à chaque image — `RecipeStore::load` touche le disque).
pub struct ImportState {
    /// Classeur cible, fixé à l'ouverture ([`AppState::open_import`]).
    pub book: NodeId,
    /// Recette en cours d'édition dans la fenêtre.
    pub recipe: plotyx_import::ImportRecipe,
    /// Fichiers choisis (« Choisir des fichiers… »), dans l'ordre du
    /// sélecteur.
    pub paths: Vec<std::path::PathBuf>,
    /// Modèles enregistrés, chargés une fois à l'ouverture de la fenêtre.
    pub store: plotyx_import::RecipeStore,
    /// Nom sous lequel enregistrer la recette courante comme modèle (champ
    /// texte de la fenêtre, distinct de `recipe.name` : on ne veut pas
    /// écraser le nom du modèle chargé tant que « Enregistrer le modèle »
    /// n'a pas été cliqué).
    pub save_as_name: String,
}

/// Nature d'une fenêtre du [`ProjectTreeNode`] montré par le panneau Project
/// Manager — le sous-ensemble de `NodeKind::is_window()` qui a une fenêtre à
/// ouvrir côté UI (`Unknown` n'en a pas : rien à afficher pour une nature de
/// nœud que ce lecteur ne comprend pas, invariant 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// Classeur — [`crate::app`] l'ouvre en `Tab::Book`.
    Book,
    /// Graphe — `Tab::Graph`.
    Graph,
    /// Matrice — `Tab::Matrix`.
    Matrix,
    /// Note — `Tab::Note`.
    Note,
}

/// Un nœud tel que le panneau Project Manager le montre : un dossier
/// dépliable, ou une fenêtre feuille cliquable. Construit par
/// [`AppState::project_tree`], qui **s'arrête** à `Workbook`/`Graph`/
/// `Matrix`/`Note` — jamais de descente dans les feuilles d'un classeur ni
/// dans le contenu d'un graphe, ces natures sont des feuilles de cet arbre-là
/// même si elles ne le sont pas de `Tree` (`Workbook` a des `Worksheet`
/// enfants dans le modèle, mais aucun dans cette vue).
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectTreeNode {
    /// Dossier de rangement.
    Folder {
        /// Identifiant du dossier.
        id: NodeId,
        /// Nom affiché.
        name: String,
        /// Enfants ordonnés, dossiers et fenêtres mêlés — l'ordre de l'arbre.
        children: Vec<ProjectTreeNode>,
    },
    /// Fenêtre cliquable, jamais développée davantage.
    Window {
        /// Identifiant de la fenêtre.
        id: NodeId,
        /// Nom affiché.
        name: String,
        /// Nature de la fenêtre.
        kind: WindowKind,
    },
}

/// L'état du banc d'essai : une poignée vers l'acteur, le dernier instantané
/// reçu, ce qui est sélectionné, le chemin de fichier courant, le dernier
/// message et la saisie en cours.
pub struct AppState {
    /// Poignée cliente vers l'acteur qui possède le `Project`.
    handle: SessionHandle,
    /// Dernier instantané reçu : figé, à rafraîchir après toute mutation.
    snapshot: Snapshot,
    /// Feuille affichée dans la grille de chaque classeur **ouvert** —
    /// `Workbook -> Worksheet` sélectionnée dans son propre onglet
    /// [`crate::app::Tab::Book`]. Un classeur qui n'a pas d'entrée ici (ou
    /// dont l'entrée est devenue invalide) retombe sur sa première feuille,
    /// voir [`AppState::selected_sheet_in`] — la carte n'a donc jamais besoin
    /// d'être pré-remplie.
    selected_sheets: std::collections::HashMap<NodeId, NodeId>,
    /// Chemin du fichier de travail (saisi à la main, cf. `crate::app`).
    path: String,
    /// Dernier message destiné à l'utilisateur.
    status: Option<StatusMessage>,
    /// Saisie de cellule en cours, s'il y en a une.
    editing: Option<CellEdit>,
    /// Saisie de note en cours, s'il y en a une (voir [`NoteEdit`]).
    note_editing: Option<NoteEdit>,
    /// Sélection courante de la grille de chaque feuille affichée — état UI
    /// pur, jamais dans l'historique (voir la doc de tête de
    /// [`crate::selection`]). Clé absente = pas de sélection dans cette
    /// feuille.
    selection: std::collections::HashMap<NodeId, Selection>,
    /// Saisie en cours dans un champ d'en-tête de colonne, s'il y en a une.
    header_editing: Option<HeaderEdit>,
    /// État de la fenêtre d'import, tant qu'elle est ouverte — voir
    /// [`ImportState`].
    import_state: Option<ImportState>,
    /// Texte de la zone de script.
    script_source: String,
    /// Sortie de la dernière exécution de script, s'il y en a eu une.
    script_output: Option<ScriptOutput>,
    /// Descripteur de l'interpréteur embarqué, posé par [`AppState::
    /// set_python`]. `None` tant qu'il n'a pas été posé — l'interpréteur
    /// lui-même n'est démarré ni ici ni là, seulement au premier script.
    #[cfg(feature = "python")]
    python: Option<EmbeddedPython>,
    /// Le moteur de script, une fois démarré (au premier [`Action::
    /// RunScript`] réussi). Partagé par `Arc` avec le fil détaché qui
    /// exécute le script en cours, s'il y en a un.
    #[cfg(feature = "python")]
    engine: Option<Arc<ScriptEngine>>,
    /// Canal de réponse d'une exécution en cours sur un fil détaché.
    /// `Some` tant que [`AppState::poll_script`] n'a pas reçu de réponse.
    #[cfg(feature = "python")]
    pending_script: Option<mpsc::Receiver<PendingScriptResult>>,
}

/// Ce que le fil détaché d'exécution rend à [`AppState::poll_script`] : le
/// moteur (nouvellement créé ou réutilisé, pour que l'appelant le mette en
/// cache) et le résultat du script, déjà réduit à un message si l'hôte a
/// refusé ou échoué — une [`plotyx_script::ScriptError`] n'a pas à traverser
/// ce canal, `state.rs` n'en a pas besoin au-delà de son message.
#[cfg(feature = "python")]
type PendingScriptResult = (Option<Arc<ScriptEngine>>, Result<ScriptOutcome, String>);

impl AppState {
    /// Construit l'état depuis une poignée d'acteur. `Err` seulement si
    /// l'acteur ne répond pas — il n'y a alors rien à afficher du tout.
    ///
    /// Ne présélectionne aucune feuille : avec un classeur par onglet
    /// (`crate::app::Tab::Book`), il n'y a plus de « la » grille à préparer
    /// à l'ouverture — chaque onglet de classeur retombe sur sa première
    /// feuille de lui-même ([`AppState::selected_sheet_in`]) dès qu'il
    /// s'ouvre.
    ///
    /// Ne touche **jamais** à l'interpréteur Python : voir la section « Le
    /// script Python » en tête de ce module.
    pub fn new(handle: SessionHandle, path: String) -> SessionResult<Self> {
        let snapshot = handle.snapshot()?;
        Ok(Self {
            handle,
            snapshot,
            selected_sheets: std::collections::HashMap::new(),
            path,
            status: None,
            editing: None,
            note_editing: None,
            selection: std::collections::HashMap::new(),
            header_editing: None,
            import_state: None,
            script_source: String::new(),
            script_output: None,
            #[cfg(feature = "python")]
            python: None,
            #[cfg(feature = "python")]
            engine: None,
            #[cfg(feature = "python")]
            pending_script: None,
        })
    }

    /// Dernier instantané reçu.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Arbre du dernier instantané.
    pub fn tree(&self) -> &Arc<Tree> {
        &self.snapshot.tree
    }

    /// Feuille affichée dans la grille du classeur `book` : la dernière
    /// sélectionnée par [`Action::SelectSheet`] pour ce classeur si elle
    /// existe encore et lui appartient toujours, sinon sa première feuille.
    /// `None` seulement si `book` n'a plus aucune feuille (ou n'existe plus).
    ///
    /// Toujours recalculé, jamais mis en cache : c'est ce qui rend la carte
    /// [`AppState::selected_sheets`](Self) auto-cicatrisante — un classeur
    /// jamais explicitement sélectionné, ou dont la sélection a été rendue
    /// invalide par une suppression, retombe de lui-même sur sa première
    /// feuille au prochain appel, sans passage par [`AppState::refresh`].
    pub fn selected_sheet_in(&self, book: NodeId) -> Option<NodeId> {
        let tree = &self.snapshot.tree;
        if let Some(&sheet) = self.selected_sheets.get(&book) {
            if tree
                .node(sheet)
                .is_some_and(|node| node.kind().is_worksheet() && node.parent() == Some(book))
            {
                return Some(sheet);
            }
        }
        tree.node(book)?.children().iter().copied().find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.kind().is_worksheet())
        })
    }

    /// Chemin du fichier de travail.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Remplace le chemin du fichier de travail.
    pub fn set_path(&mut self, path: String) {
        self.path = path;
    }

    /// Dernier message destiné à l'utilisateur.
    pub fn status(&self) -> Option<&StatusMessage> {
        self.status.as_ref()
    }

    /// Texte actuel de la zone de script.
    pub fn script_source(&self) -> &str {
        &self.script_source
    }

    /// Remplace le texte de la zone de script.
    pub fn set_script_source(&mut self, source: String) {
        self.script_source = source;
    }

    /// Sortie de la dernière exécution de script, s'il y en a eu une.
    pub fn script_output(&self) -> Option<&ScriptOutput> {
        self.script_output.as_ref()
    }

    /// Vrai si ce binaire embarque le pont Python (feature `python`).
    ///
    /// Reflète uniquement la **compilation**, pas la localisation d'un
    /// interpréteur : un binaire compilé avec la feature mais qui n'a pas
    /// trouvé son préfixe embarqué ([`AppState::set_python`] jamais appelé)
    /// reste `true` ici, et [`Action::RunScript`] l'explique à l'exécution —
    /// le même choix que pour toute autre action de ce banc (échouer avec un
    /// message plutôt que griser par anticipation).
    #[must_use]
    pub fn script_available(&self) -> bool {
        cfg!(feature = "python")
    }

    /// Vrai pendant qu'un script s'exécute (entre [`Action::RunScript`] et le
    /// [`AppState::poll_script`] qui en récupère le résultat).
    #[must_use]
    pub fn script_running(&self) -> bool {
        #[cfg(feature = "python")]
        {
            self.pending_script.is_some()
        }
        #[cfg(not(feature = "python"))]
        {
            false
        }
    }

    /// Pose le descripteur de l'interpréteur embarqué à utiliser au premier
    /// script. **Ne démarre rien** : `EmbeddedPython::locate`/`with_home` ne
    /// touchent pas à CPython, seul `ScriptEngine::spawn` le fait — et lui
    /// n'est appelé que depuis [`AppState::run_script`].
    #[cfg(feature = "python")]
    pub fn set_python(&mut self, python: EmbeddedPython) {
        self.python = Some(python);
    }

    /// Libellé de l'action « Annuler » : `Some("Annuler Saisie dans A")`, ou
    /// `None` si la pile est vide (l'entrée de menu est alors grisée).
    pub fn undo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .undo_label
            .as_ref()
            .map(|label| format!("Annuler {label}"))
    }

    /// Libellé de l'action « Rétablir », `None` si rien à rétablir.
    pub fn redo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .redo_label
            .as_ref()
            .map(|label| format!("Rétablir {label}"))
    }

    /// Vrai s'il y a quelque chose à annuler.
    pub fn can_undo(&self) -> bool {
        self.snapshot.history.can_undo
    }

    /// Vrai s'il y a quelque chose à rétablir.
    pub fn can_redo(&self) -> bool {
        self.snapshot.history.can_redo
    }

    /// Colonnes de `sheet`, dans l'ordre du modèle : `(NodeId, nom, type)`.
    /// Vide si `sheet` n'est pas (ou plus) une feuille de ce projet.
    pub fn visible_columns(&self, sheet: NodeId) -> Vec<(NodeId, String, ColumnType)> {
        visible_columns_in(&self.snapshot.tree, sheet)
    }

    /// Nombre de lignes de `sheet` — le **maximum** des longueurs de ses
    /// colonnes, pas la longueur de la première : une feuille n'est pas un
    /// tableau rectangulaire. 0 si `sheet` n'est pas (ou plus) une feuille.
    pub fn row_count(&self, sheet: NodeId) -> usize {
        row_count_in(&self.snapshot.tree, sheet)
    }

    /// Texte à afficher dans une cellule : [`MISSING_TEXT_DISPLAY`] pour une
    /// valeur absente (`Cell::Text(None)`, y compris hors bornes — `get`
    /// rend `None` au-delà de la fin d'une colonne texte), `""` pour une
    /// valeur manquante numérique (`NaN`) ou pour une chaîne vide
    /// **présente** (`Cell::Text(Some(String::new()))`), sinon la valeur
    /// formatée.
    ///
    /// # A9 (option 2, `guide-de-correction-robustesse.md` §F1/§F8) — deux
    /// choses différentes ne doivent pas avoir la même apparence
    ///
    /// Le modèle distingue `Some("")` (une chaîne vide qu'on a écrite) de
    /// `None` (rien n'a jamais été écrit) — c'est la seule valeur manquante
    /// du texte (voir la doc de tête de `column.rs`, invariant F1). Avant ce
    /// correctif, `cell_text` rendait `""` dans les deux cas : rien, à
    /// l'écran, ne permettait de savoir laquelle des deux était sous les
    /// yeux — l'anomalie A9 (moitié affichage ; l'autre moitié, la survie de
    /// `Some("")` à la traversée d'une cellule, est déjà corrigée dans
    /// `commit_cell`, option 1).
    ///
    /// La convention retenue ici : une valeur **absente** s'affiche comme un
    /// unique espace insécable U+00A0 ([`MISSING_TEXT_DISPLAY`]) — visible
    /// nulle part (il occupe la cellule comme un blanc), mais distinct de la
    /// chaîne vide en mémoire, et d'un caractère qu'un utilisateur ne tape
    /// jamais en cherchant à écrire du texte ordinaire (contrairement à un
    /// tiret ou un point d'interrogation, que « rien » comme « — » sont
    /// d'authentiques réponses possibles à une question). Une chaîne vide
    /// **présente** garde `""` : c'est littéralement ce qu'elle est.
    ///
    /// # C4 (moitié `plotyx-app`, §F8) — le formatage numérique
    ///
    /// Le formatage numérique n'est plus le seul `Display` de `f64` (qui
    /// n'émet jamais d'exposant : une mesure comme la charge de l'électron,
    /// 1,602176634·10⁻¹⁹, s'écrivait sur 30 caractères dans une cellule de
    /// grille large d'une quinzaine). [`format_f64_for_display`] bascule en
    /// notation scientifique pour les magnitudes extrêmes ; voir sa
    /// documentation pour les seuils exacts et pour ce que cette moitié doit
    /// à l'autre — la conversion `Column::set_column_type` (`Float → Text`)
    /// de `plotyx-core`, corrigée séparément (§F8 : « une seule table, deux
    /// directions »). Le vrai formatage restera un jour celui de
    /// `ColumnMeta::display_format`, que cette phase ne lit pas.
    pub fn cell_text(&self, column: NodeId, row: usize) -> String {
        cell_text_in(&self.snapshot.tree, column, row)
    }

    /// Métadonnées d'affichage d'une colonne (rôle, unité, nom long,
    /// commentaire, formule) — [`plotyx_core::ColumnMeta::default`] si
    /// `column` n'est pas (ou plus) une colonne de ce projet.
    pub fn column_meta(&self, column: NodeId) -> ColumnMeta {
        self.snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(|c| c.meta().clone())
            .unwrap_or_default()
    }

    /// Sélection courante de la grille de `sheet`, s'il y en a une.
    pub fn selection(&self, sheet: NodeId) -> Option<&Selection> {
        self.selection.get(&sheet)
    }

    /// Pose une sélection d'une seule cellule (l'ancre d'un futur
    /// rectangle) — simple clic. Mutation transitoire (comme
    /// [`AppState::begin_edit`]) : sans effet sur le modèle, donc sûre à
    /// appeler directement pendant le rendu plutôt que via [`Action`].
    pub fn select_cell(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        self.selection.insert(sheet, Selection::cell(column, row));
    }

    /// Étend la sélection courante de `sheet` jusqu'à `(column, row)`, en
    /// gardant l'ancre déjà posée — glisser ou Shift-clic. Pose une
    /// sélection d'une seule cellule si `sheet` n'en avait pas encore.
    pub fn extend_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) => selection.extend_to(column, row),
            None => self.select_cell(sheet, column, row),
        }
    }

    /// Sélectionne `column` dans son entier — clic sur sa lettre d'en-tête.
    pub fn select_column(&mut self, sheet: NodeId, column: NodeId) {
        self.selection.insert(sheet, Selection::column(column));
    }

    /// Sélectionne un champ d'en-tête précis (nom long/unité/commentaire/
    /// formule) — simple clic, même comportement que [`AppState::
    /// select_cell`] pour une cellule de données.
    pub fn select_header_field(&mut self, sheet: NodeId, column: NodeId, field: HeaderField) {
        self.selection
            .insert(sheet, Selection::header_field(column, field));
    }

    /// Efface la sélection de `sheet` (sans effet sur le modèle).
    pub fn deselect_all(&mut self, sheet: NodeId) {
        self.selection.remove(&sheet);
    }

    /// Colonnes visibles de `sheet`, dans l'ordre — juste les identifiants,
    /// pour la navigation clavier ([`AppState::move_selection`]/
    /// [`AppState::move_selection_tab`]) comme pour [`AppState::paste_tsv`].
    fn column_order(&self, sheet: NodeId) -> Vec<NodeId> {
        self.visible_columns(sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect()
    }

    /// Déplace la sélection de `sheet` d'une cellule dans `direction`
    /// (flèches directionnelles), en gardant l'ancre déjà posée si
    /// `extend` (Maj+flèche étend le rectangle plutôt que déplacer la
    /// cellule sélectionnée). Clampé aux bords de la grille — jamais de
    /// retour à la ligne (voir [`AppState::move_selection_tab`] pour ça) ni
    /// d'ajout de ligne (réservé à [`AppState::commit_cell_and_advance`],
    /// déclenché par la validation d'une saisie, pas par la navigation
    /// seule). Sans effet si `sheet` n'a pas de sélection de cellules
    /// (colonne entière ou champ d'en-tête sélectionné, ou aucune sélection)
    /// — mutation transitoire, comme [`AppState::select_cell`].
    pub fn move_selection(&mut self, sheet: NodeId, direction: MoveDirection, extend: bool) {
        let Some((from_column, from_row)) = self.selection.get(&sheet).and_then(Selection::cursor)
        else {
            return;
        };
        let order = self.column_order(sheet);
        let Some(index) = order.iter().position(|&id| id == from_column) else {
            return;
        };
        let last_row = self.row_count(sheet).saturating_sub(1);
        let (to_index, to_row) = match direction {
            MoveDirection::Up => (index, from_row.saturating_sub(1)),
            MoveDirection::Down => (index, (from_row + 1).min(last_row)),
            MoveDirection::Left => (index.saturating_sub(1), from_row),
            MoveDirection::Right => ((index + 1).min(order.len().saturating_sub(1)), from_row),
        };
        let Some(&to_column) = order.get(to_index) else {
            return;
        };
        if extend {
            self.extend_selection(sheet, to_column, to_row);
        } else {
            self.select_cell(sheet, to_column, to_row);
        }
    }

    /// Déplace la sélection de `sheet` d'une cellule vers la colonne
    /// suivante (`backward: false`) ou précédente (`true`), avec retour à
    /// la ligne aux bords — Tab/Maj+Tab, comme dans tout tableur : Tab en
    /// fin de ligne repart au début de la ligne suivante, Maj+Tab en début
    /// de ligne revient à la fin de la précédente. Jamais d'extension de
    /// sélection (Tab n'étend pas, contrairement aux flèches sous Maj) ; les
    /// mêmes gardes que [`AppState::move_selection`] sinon.
    pub fn move_selection_tab(&mut self, sheet: NodeId, backward: bool) {
        let Some((from_column, from_row)) = self.selection.get(&sheet).and_then(Selection::cursor)
        else {
            return;
        };
        let order = self.column_order(sheet);
        let Some(index) = order.iter().position(|&id| id == from_column) else {
            return;
        };
        let row_count = self.row_count(sheet);
        if order.is_empty() || row_count == 0 {
            return;
        }
        let mut column_index = index as isize;
        let mut row = from_row as isize;
        if backward {
            column_index -= 1;
            if column_index < 0 {
                column_index = order.len() as isize - 1;
                row -= 1;
            }
        } else {
            column_index += 1;
            if column_index >= order.len() as isize {
                column_index = 0;
                row += 1;
            }
        }
        let row = row.clamp(0, row_count as isize - 1) as usize;
        let column_index = column_index.clamp(0, order.len() as isize - 1) as usize;
        self.select_cell(sheet, order[column_index], row);
    }

    /// Vrai si `(column, row)` fait partie de la sélection de `sheet`.
    pub fn is_selected(&self, sheet: NodeId, column: NodeId, row: usize) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.contains(&self.snapshot.tree, sheet, column, row))
    }

    /// Vrai si `column` est sélectionnée dans son entier (clic sur sa lettre
    /// d'en-tête).
    pub fn is_column_selected(&self, sheet: NodeId, column: NodeId) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_column_selected(column))
    }

    /// Vrai si `(column, field)` est le champ d'en-tête actuellement
    /// sélectionné dans `sheet`.
    pub fn is_header_field_selected(
        &self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_header_field_selected(column, field))
    }

    /// Champ d'en-tête sélectionné dans `sheet`, s'il y en a un — pour les
    /// raccourcis clavier (Ctrl+C/Suppr) scopés à la grille.
    pub fn selected_header_field(&self, sheet: NodeId) -> Option<(NodeId, HeaderField)> {
        self.selection.get(&sheet)?.selected_header_field()
    }

    /// Sérialise la sélection courante de `sheet` en TSV (`\t` entre
    /// colonnes, `\n` entre lignes) — pour Ctrl+C/Ctrl+X. `None` si `sheet`
    /// n'a pas de sélection ou si elle ne désigne plus de colonne valide.
    /// Une valeur manquante s'y écrit comme une chaîne vide (jamais
    /// [`MISSING_TEXT_DISPLAY`], qui n'a de sens que pour l'écran, pas pour
    /// un échange avec un tableur externe).
    pub fn selected_cells_tsv(&self, sheet: NodeId) -> Option<String> {
        let selection = self.selection.get(&sheet)?;
        let tree = &self.snapshot.tree;
        let columns = selection.columns(tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let (row_start, row_end) = match selection.row_range() {
            Some(range) => range,
            None => {
                let len = columns
                    .iter()
                    .filter_map(|&column| tree.node(column).and_then(Node::as_column))
                    .map(Column::len)
                    .max()
                    .unwrap_or(0);
                (0, len.saturating_sub(1))
            }
        };
        let mut out = String::new();
        for row in row_start..=row_end {
            let line: Vec<String> = columns
                .iter()
                .map(|&column| cell_text_in(tree, column, row).replace(MISSING_TEXT_DISPLAY, ""))
                .collect();
            out.push_str(&line.join("\t"));
            out.push('\n');
        }
        Some(out)
    }

    /// Saisie en cours dans un champ d'en-tête de colonne, s'il y en a une.
    pub fn header_editing(&self) -> Option<&HeaderEdit> {
        self.header_editing.as_ref()
    }

    /// Saisie en cours dans un champ d'en-tête, modifiable.
    pub fn header_editing_mut(&mut self) -> Option<&mut HeaderEdit> {
        self.header_editing.as_mut()
    }

    /// Ouvre la saisie d'un champ d'en-tête, tampon initialisé à sa valeur
    /// actuelle. Remplace sans valider une éventuelle saisie précédente, et
    /// annule au passage une saisie de cellule en cours — une seule chose
    /// éditable à la fois dans la fenêtre (sans quoi une cellule et un champ
    /// d'en-tête peuvent se retrouver simultanément en édition, chacun
    /// réclamant le focus clavier pour lui-même).
    pub fn begin_header_edit(&mut self, column: NodeId, field: HeaderField) {
        self.editing = None;
        let meta = self.column_meta(column);
        let buffer = match field {
            HeaderField::LongName => meta.long_name,
            HeaderField::Unit => meta.unit,
            HeaderField::Comment => meta.comment,
            HeaderField::Formula => meta.formula.unwrap_or_default(),
        };
        self.header_editing = Some(HeaderEdit {
            column,
            field,
            buffer,
        });
    }

    /// Abandonne la saisie d'en-tête en cours sans rien écrire.
    pub fn cancel_header_edit(&mut self) {
        self.header_editing = None;
    }

    /// Valide la saisie d'en-tête en cours. Sans effet s'il n'y a pas de
    /// saisie ouverte.
    pub fn commit_header_edit(&mut self) {
        let Some(edit) = self.header_editing.take() else {
            return;
        };
        let action = match edit.field {
            HeaderField::LongName => Action::SetColumnLongName {
                column: edit.column,
                text: edit.buffer,
            },
            HeaderField::Unit => Action::SetColumnUnit {
                column: edit.column,
                text: edit.buffer,
            },
            HeaderField::Comment => Action::SetColumnComment {
                column: edit.column,
                text: edit.buffer,
            },
            HeaderField::Formula => Action::SetColumnFormula {
                column: edit.column,
                formula: (!edit.buffer.trim().is_empty()).then_some(edit.buffer),
            },
        };
        self.dispatch(action);
    }

    /// Saisie de cellule en cours, s'il y en a une.
    pub fn editing(&self) -> Option<&CellEdit> {
        self.editing.as_ref()
    }

    /// Saisie en cours, modifiable : la couche fenêtre y branche son
    /// `TextEdit` pour que les frappes atterrissent dans le tampon sans
    /// toucher au modèle.
    pub fn editing_mut(&mut self) -> Option<&mut CellEdit> {
        self.editing.as_mut()
    }

    /// Ouvre la saisie d'une cellule, tampon initialisé au texte affiché.
    /// Remplace sans valider une éventuelle saisie précédente : c'est à
    /// l'appelant de valider avant, s'il le souhaite. Annule au passage une
    /// saisie d'en-tête en cours — voir la même remarque sur
    /// [`AppState::begin_header_edit`].
    pub fn begin_edit(&mut self, column: NodeId, row: usize) {
        self.header_editing = None;
        let buffer = self.cell_text(column, row);
        self.editing = Some(CellEdit {
            column,
            row,
            buffer,
        });
    }

    /// Abandonne la saisie en cours sans rien écrire.
    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    /// Valide la saisie en cours (équivaut à [`Action::CommitCell`] sur le
    /// tampon). Sans effet s'il n'y a pas de saisie ouverte.
    pub fn commit_edit(&mut self) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        self.dispatch(Action::CommitCell {
            column: edit.column,
            row: edit.row,
            text: edit.buffer,
        });
    }

    /// Saisie de note en cours, s'il y en a une. Le pendant exact de
    /// [`AppState::editing`] pour une note — un seul tampon à la fois, pour
    /// la même raison de principe (même si rien ici ne force la limite à un
    /// seul tampon : une note n'est pas virtualisée comme une grille d'un
    /// million de lignes, mais suivre la même convention garde `crate::app`
    /// simple, un seul mécanisme d'édition différée à comprendre).
    pub fn note_editing(&self) -> Option<&NoteEdit> {
        self.note_editing.as_ref()
    }

    /// Saisie de note en cours, modifiable.
    pub fn note_editing_mut(&mut self) -> Option<&mut NoteEdit> {
        self.note_editing.as_mut()
    }

    /// Ouvre la saisie d'une note, tampon initialisé à son texte actuel (vide
    /// si `note` n'est pas — ou plus — une note de ce projet).
    pub fn begin_note_edit(&mut self, note: NodeId) {
        let buffer = self.note_text(note).unwrap_or_default();
        self.note_editing = Some(NoteEdit { note, buffer });
    }

    /// Abandonne la saisie de note en cours sans rien écrire.
    pub fn cancel_note_edit(&mut self) {
        self.note_editing = None;
    }

    /// Valide la saisie de note en cours (équivaut à [`Action::SetNoteText`]
    /// sur le tampon). Sans effet s'il n'y a pas de saisie ouverte.
    pub fn commit_note_edit(&mut self) {
        let Some(edit) = self.note_editing.take() else {
            return;
        };
        self.dispatch(Action::SetNoteText {
            note: edit.note,
            text: edit.buffer,
        });
    }

    /// Les feuilles de `book`, dans l'ordre des onglets internes
    /// [`crate::app::Tab::Book`] : `(NodeId, nom)`. Vide si `book` n'existe
    /// plus ou n'est pas un classeur.
    pub fn worksheets_of(&self, book: NodeId) -> Vec<(NodeId, String)> {
        worksheets_of_in(&self.snapshot.tree, book)
    }

    /// Les classeurs du projet, sous la forme `(identifiant, chemin
    /// lisible)`. Le pendant exact de [`AppState::graphs`], pour la
    /// disposition initiale de [`crate::app::PlotyxApp::new`] : ouvrir le
    /// premier classeur du projet, comme le premier graphe l'était déjà.
    pub fn books(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| matches!(node.kind(), NodeKind::Workbook { .. }))
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// Texte d'une note, ou `None` si `note` n'est pas (ou plus) une note de
    /// ce projet.
    pub fn note_text(&self, note: NodeId) -> Option<String> {
        note_text_in(&self.snapshot.tree, note)
    }

    /// L'arbre du projet tel que le panneau Project Manager le montre :
    /// dossiers dépliables, feuilles cliquables (`Workbook`, `Graph`,
    /// `Matrix`, `Note`) jamais développées davantage — voir la doc de tête
    /// de [`ProjectTreeNode`]. Racine exclue (elle n'a pas de nom à afficher
    /// comme nœud du panneau) : ses enfants directs sont la racine de
    /// l'arbre rendu.
    pub fn project_tree(&self) -> Vec<ProjectTreeNode> {
        self.project_tree_children(self.snapshot.tree.root())
    }

    fn project_tree_children(&self, parent: NodeId) -> Vec<ProjectTreeNode> {
        let tree = &self.snapshot.tree;
        let Some(node) = tree.node(parent) else {
            return Vec::new();
        };
        node.children()
            .iter()
            .filter_map(|id| {
                let child = tree.node(*id)?;
                let name = child.name().to_string();
                match child.kind() {
                    NodeKind::Folder { .. } => Some(ProjectTreeNode::Folder {
                        id: *id,
                        name,
                        children: self.project_tree_children(*id),
                    }),
                    NodeKind::Workbook { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Book,
                    }),
                    NodeKind::Graph { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Graph,
                    }),
                    NodeKind::Matrix { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Matrix,
                    }),
                    NodeKind::Note { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Note,
                    }),
                    // `Worksheet`/`Column` : jamais rencontrés ici, enfants
                    // d'un `Workbook`/`Worksheet` que ce parcours n'atteint
                    // jamais (on s'arrête à `Workbook`). `Unknown` : pas de
                    // fenêtre à ouvrir côté UI pour une nature non reconnue
                    // (invariant 10) — le nœud existe toujours et sera
                    // réécrit tel quel à la sauvegarde, mais le Project
                    // Manager n'a rien à en montrer.
                    NodeKind::Worksheet { .. } | NodeKind::Column(_) | NodeKind::Unknown(_) => None,
                }
            })
            .collect()
    }

    /// Les feuilles du projet, pour les tests et les usages génériques :
    /// `(NodeId, chemin lisible)`, en ordre de parcours préfixe. Le panneau
    /// Project Manager ne s'en sert pas (il s'arrête à `Workbook`, voir
    /// [`AppState::project_tree`]) ; cette méthode reste le pendant exact de
    /// [`AppState::graphs`] pour qui a besoin de la liste à plat.
    ///
    /// Le chemin est reconstruit ici (« Book1 / Sheet1 ») plutôt qu'obtenu de
    /// `Project::path_of` : cette méthode vit sur `Project`, que l'UI ne
    /// détient pas — elle n'a qu'un `Arc<Tree>`. C'est un manque relevé par
    /// cette phase, pas un choix esthétique.
    pub fn sheets(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| node.kind().is_worksheet())
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// Les graphes du projet, sous la forme `(identifiant, chemin lisible)`.
    ///
    /// Le pendant exact de [`AppState::sheets`], pour le panneau de rendu de
    /// la phase R4. Comme lui, il reconstruit le chemin à la main faute de
    /// `Project::path_of` côté `Arc<Tree>`.
    pub fn graphs(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| matches!(node.kind(), NodeKind::Graph { .. }))
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// Chemin lisible d'un nœud : les noms de ses ancêtres (racine exclue)
    /// puis le sien, séparés par « / ».
    fn readable_path(&self, id: NodeId) -> String {
        let tree = &self.snapshot.tree;
        let root = tree.root();
        let mut parts: Vec<String> = tree
            .ancestors(id)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .filter(|ancestor| *ancestor != root)
            .filter_map(|ancestor| tree.node(ancestor).map(|node| node.name().to_string()))
            .collect();
        if let Some(node) = tree.node(id) {
            parts.push(node.name().to_string());
        }
        parts.join(" / ")
    }

    /// Applique une action. Ne rend jamais d'erreur : toute erreur devient un
    /// [`StatusMessage`]. Rafraîchit l'instantané après toute action qui a pu
    /// changer le modèle.
    ///
    /// **Ne bloque jamais**, `RunScript` compris : voir la section « Le
    /// script Python » en tête de ce module. C'est ce qui rend cette méthode
    /// sûre à appeler depuis la boucle de rendu pour toute action.
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Save => self.save(),
            Action::Load => self.load(),
            Action::SelectSheet(id) => self.select_sheet(id),
            Action::CommitCell { column, row, text } => {
                self.commit_cell(column, row, text);
            }
            Action::AddColumn { sheet } => self.add_column(sheet),
            Action::SetColumnType {
                column,
                column_type,
            } => self.execute(Command::SetColumnType {
                column,
                column_type,
            }),
            Action::SetColumnRole { column, role } => {
                self.set_column_meta_field(column, |meta| meta.role = role);
            }
            Action::InsertRows { sheet, at, count } => self.rows(sheet, at, count, true),
            Action::DeleteRows { sheet, at, count } => self.rows(sheet, at, count, false),
            Action::SetNoteText { note, text } => self.set_note_text(note, text),
            Action::RunScript => self.run_script(),
            Action::InterruptScript => self.interrupt_script(),
            Action::CommitCellAndAdvance {
                column,
                row,
                text,
                sheet,
            } => self.commit_cell_and_advance(column, row, text, sheet),
            Action::ClearSelectedCells { sheet } => self.clear_selected_cells(sheet),
            Action::DeleteSelectedCells { sheet } => self.delete_selected_cells(sheet),
            Action::PasteTsv { sheet, text } => self.paste_tsv(sheet, text),
            Action::SetColumnLongName { column, text } => {
                self.set_column_meta_field(column, |meta| meta.long_name = text);
            }
            Action::SetColumnUnit { column, text } => {
                self.set_column_meta_field(column, |meta| meta.unit = text);
            }
            Action::SetColumnComment { column, text } => {
                self.set_column_meta_field(column, |meta| meta.comment = text);
            }
            Action::SetColumnFormula { column, formula } => {
                self.set_column_meta_field(column, |meta| meta.formula = formula);
            }
            Action::ImportFiles {
                book,
                recipe,
                paths,
            } => self.import_files(book, recipe, paths),
        }
    }

    /// Rafraîchit l'instantané depuis l'acteur et revalide la sélection.
    ///
    /// La revalidation n'est pas réservée au rechargement : c'est le seul
    /// endroit qui garantit que la feuille affichée existe encore, quelle que
    /// soit la façon dont l'arbre a changé (y compris par un autre client de
    /// l'acteur). Un échec de rafraîchissement écrase le message courant —
    /// l'acteur injoignable est plus grave que ce qu'on allait dire.
    pub fn refresh(&mut self) {
        match self.handle.snapshot() {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.revalidate_selection();
            }
            Err(err) => self.fail(format!("Session injoignable : {err}")),
        }
    }

    /// Sonde le fil détaché d'une exécution de script en cours, sans jamais
    /// bloquer. À appeler à **chaque image** (voir [`crate::app`]) : sans
    /// notification poussée de l'acteur, c'est le seul endroit qui rafraîchit
    /// l'instantané après un script — l'oublier laisserait la grille figée
    /// alors que le modèle a changé.
    ///
    /// Sans effet si aucun script n'est en cours (y compris sans la feature
    /// `python`, où cette méthode ne fait jamais rien) : sûr à appeler à
    /// chaque image sans test préalable.
    pub fn poll_script(&mut self) {
        #[cfg(feature = "python")]
        {
            let received = match &self.pending_script {
                Some(receiver) => match receiver.try_recv() {
                    Ok(reply) => Some(reply),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.pending_script = None;
                        self.fail(
                            "Le fil de script a disparu sans répondre (a-t-il paniqué ?)."
                                .to_string(),
                        );
                        return;
                    }
                },
                None => None,
            };
            let Some((engine, result)) = received else {
                return;
            };
            self.pending_script = None;
            if let Some(engine) = engine {
                self.engine = Some(engine);
            }
            match result {
                Ok(outcome) => self.finish_script_run(outcome),
                Err(message) => self.fail(message),
            }
        }
    }

    /// État courant de la fenêtre d'import, si elle est ouverte.
    pub fn import_state(&self) -> Option<&ImportState> {
        self.import_state.as_ref()
    }

    /// État courant de la fenêtre d'import, modifiable — la couche fenêtre y
    /// branche ses widgets (ComboBox, boutons radio, champs texte) pour muter
    /// `recipe` directement, sans passer par [`Action`] : mutation
    /// transitoire, comme [`AppState::header_editing_mut`].
    pub fn import_state_mut(&mut self) -> Option<&mut ImportState> {
        self.import_state.as_mut()
    }

    /// Ouvre la fenêtre d'import pour `book`. Charge le [`plotyx_import::
    /// RecipeStore`] depuis le disque une fois pour toute la durée
    /// d'ouverture de la fenêtre (jamais rechargé à chaque image) et
    /// démarre sur son premier modèle enregistré s'il y en a un, sinon sur
    /// une recette permissive par défaut ([`plotyx_import::ImportRecipe::
    /// new`]). Remplace sans confirmation une fenêtre d'import déjà ouverte
    /// (même patron que [`AppState::begin_edit`] pour une saisie de
    /// cellule).
    pub fn open_import(&mut self, book: NodeId) {
        let store = plotyx_import::RecipeStore::load();
        let recipe = store
            .recipes()
            .first()
            .cloned()
            .unwrap_or_else(|| plotyx_import::ImportRecipe::new("Sans titre"));
        let save_as_name = recipe.name.clone();
        self.import_state = Some(ImportState {
            book,
            recipe,
            paths: Vec::new(),
            store,
            save_as_name,
        });
    }

    /// Ferme la fenêtre d'import sans rien importer.
    pub fn cancel_import(&mut self) {
        self.import_state = None;
    }

    /// Repart d'une recette neuve et permissive (bouton « Nouveau modèle »).
    /// Sans effet si la fenêtre d'import n'est pas ouverte.
    pub fn new_import_recipe(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.recipe = plotyx_import::ImportRecipe::new("Sans titre");
            state.save_as_name = state.recipe.name.clone();
        }
    }

    /// Charge le modèle `name` du `RecipeStore` dans la recette en cours
    /// d'édition (sélection dans la liste déroulante). Sans effet si `name`
    /// n'est pas un modèle enregistré, ou si la fenêtre n'est pas ouverte.
    pub fn select_import_recipe(&mut self, name: &str) {
        if let Some(state) = &mut self.import_state {
            if let Some(recipe) = state.store.recipes().iter().find(|r| r.name == name) {
                state.recipe = recipe.clone();
                state.save_as_name = recipe.name.clone();
            }
        }
    }

    /// Remplace les fichiers choisis (« Choisir des fichiers… », sélecteur
    /// natif ouvert par la couche fenêtre — `rfd` est une dépendance UI,
    /// elle n'a pas sa place ici). Sans effet si la fenêtre n'est pas
    /// ouverte.
    pub fn set_import_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        if let Some(state) = &mut self.import_state {
            state.paths = paths;
        }
    }

    /// Enregistre la recette en cours d'édition comme modèle nommé
    /// `save_as_name` (champ texte de la fenêtre) dans le `RecipeStore`, et
    /// persiste le store sur disque. Un échec d'écriture (dossier de config
    /// inaccessible) devient un [`StatusMessage`], jamais une panique — sans
    /// effet si la fenêtre n'est pas ouverte ou si le nom est vide.
    pub fn save_import_recipe(&mut self) {
        let Some(state) = &mut self.import_state else {
            return;
        };
        let name = state.save_as_name.trim().to_string();
        if name.is_empty() {
            self.fail("Le modèle doit avoir un nom pour être enregistré".to_string());
            return;
        }
        state.recipe.name = name;
        state.store.upsert(state.recipe.clone());
        let save_result = state.store.save();
        let saved_name = state.recipe.name.clone();
        if let Err(err) = save_result {
            self.fail(format!("Échec de l'enregistrement du modèle : {err}"));
            return;
        }
        self.notice(format!("Modèle « {saved_name} » enregistré"));
    }

    /// Supprime le modèle `name` du `RecipeStore` et persiste le store sur
    /// disque. Même politique d'échec que [`AppState::save_import_recipe`].
    pub fn delete_import_recipe(&mut self, name: &str) {
        let Some(state) = &mut self.import_state else {
            return;
        };
        state.store.remove(name);
        if let Err(err) = state.store.save() {
            self.fail(format!("Échec de la suppression du modèle : {err}"));
            return;
        }
        self.notice(format!("Modèle « {name} » supprimé"));
    }

    /// Aperçu du **premier** fichier choisi, reparsé avec les réglages
    /// courants — recalculé à chaque appel plutôt que mis en cache (les
    /// fichiers d'aperçu sont petits, voir la doc de tête de [`ImportState`]).
    /// `None` si la fenêtre n'est pas ouverte ou si aucun fichier n'a encore
    /// été choisi ; `Some(Err(_))` si la lecture ou l'analyse du fichier a
    /// échoué (fichier illisible traité comme une erreur d'analyse pour ne
    /// pas multiplier les types d'échec exposés à la fenêtre).
    pub fn import_preview(&self) -> Option<Result<plotyx_import::ParsedTable, String>> {
        let state = self.import_state.as_ref()?;
        let path = state.paths.first()?;
        let result = std::fs::read(path)
            .map_err(|err| format!("{}: {err}", path.display()))
            .and_then(|bytes| {
                plotyx_import::parse(&bytes, &state.recipe).map_err(|err| err.to_string())
            });
        Some(result)
    }

    /// Aperçu du découpage en lignes (en-tête/sous-en-tête/données/pied de
    /// page) du **premier** fichier choisi, avec les réglages courants —
    /// voir [`plotyx_import::preview::preview`] pour le budget de coût.
    /// Recalculé à chaque appel, comme [`AppState::import_preview`] : ce
    /// budget est précisément ce qui rend ce recalcul par image acceptable,
    /// y compris sur un gros fichier (le travail cellule à cellule reste
    /// borné à `recipe.preview_lines`, jamais à la taille du fichier). `None`
    /// dans les mêmes conditions qu'`import_preview`.
    pub fn import_line_preview(&self) -> Option<Result<plotyx_import::FilePreview, String>> {
        let state = self.import_state.as_ref()?;
        let path = state.paths.first()?;
        let result = std::fs::read(path)
            .map_err(|err| format!("{}: {err}", path.display()))
            .and_then(|bytes| {
                plotyx_import::preview(&bytes, &state.recipe).map_err(|err| err.to_string())
            });
        Some(result)
    }

    /// Réévalue `header_layout` (nombre de lignes d'en-tête principal et de
    /// sous-en-tête) depuis le contenu du **premier** fichier choisi —
    /// équivalent du bouton de réévaluation automatique d'OriginLab
    /// ([`plotyx_import::preview::detect_header_layout`]) : une action
    /// explicite, pas un recalcul à chaque image, pour ne jamais écraser une
    /// valeur que l'utilisateur vient de saisir à la main. Sans effet si la
    /// fenêtre n'est pas ouverte ou si aucun fichier n'a encore été choisi.
    pub fn auto_detect_import_header(&mut self) {
        let Some(import) = &self.import_state else {
            return;
        };
        let Some(path) = import.paths.first().cloned() else {
            self.fail("Choisis un fichier avant de détecter l'en-tête".to_string());
            return;
        };
        let (separator, decimal) = (import.recipe.separator, import.recipe.decimal);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let layout = plotyx_import::detect_header_layout(&bytes, separator, decimal);
                if let Some(import) = &mut self.import_state {
                    import.recipe.header_layout = layout;
                }
            }
            Err(err) => self.fail(format!("{}: {err}", path.display())),
        }
    }

    // -- détail interne ----------------------------------------------------

    /// Purge les sélections de feuille et les tampons de saisie devenus
    /// périmés — un classeur, une feuille ou une colonne qui n'existe plus
    /// dans le nouvel arbre (rechargement, suppression, y compris par un
    /// autre client de l'acteur).
    ///
    /// Contrairement à l'ancienne sélection globale unique, `selected_sheets`
    /// n'a pas besoin d'être **réaffectée** ici : [`AppState::
    /// selected_sheet_in`] retombe de lui-même sur la première feuille d'un
    /// classeur dont l'entrée a disparu (ou n'a jamais existé). Cette méthode
    /// ne fait donc qu'écarter ce qui ne pointe plus rien de valide, pour ne
    /// pas laisser grossir la carte avec des entrées mortes au fil des
    /// rechargements.
    fn revalidate_selection(&mut self) {
        let tree = &self.snapshot.tree;
        self.selected_sheets.retain(|&book, &mut sheet| {
            tree.node(book)
                .is_some_and(|node| matches!(node.kind(), NodeKind::Workbook { .. }))
                && tree
                    .node(sheet)
                    .is_some_and(|node| node.kind().is_worksheet() && node.parent() == Some(book))
        });
        // Le tampon de saisie pointe une colonne de l'arbre précédent : s'il
        // n'a plus de nœud colonne, il n'a plus de sens ici (typiquement, un
        // rechargement : les identifiants n'ont aucun rapport avec ceux
        // d'avant). S'il pointe toujours une colonne valide, on le laisse
        // survivre — c'est le cas d'un `refresh` déclenché par une action qui
        // ne touche pas la colonne en cours de saisie.
        if let Some(edit) = &self.editing {
            if tree.node(edit.column).and_then(Node::as_column).is_none() {
                self.editing = None;
            }
        }
        if let Some(edit) = &self.note_editing {
            if !matches!(
                tree.node(edit.note).map(Node::kind),
                Some(NodeKind::Note { .. })
            ) {
                self.note_editing = None;
            }
        }
        // Une sélection dont plus aucune colonne n'existe (feuille ou
        // colonnes supprimées) n'a plus de sens — voir `Selection::columns`,
        // qui rend justement une liste vide dans ce cas. Un champ d'en-tête
        // sélectionné n'a en revanche *jamais* de colonne via `columns()`
        // (voir sa doc) : sa colonne se vérifie directement.
        self.selection.retain(
            |&sheet, selection| match selection.selected_header_field() {
                Some((column, _)) => tree.node(column).and_then(Node::as_column).is_some(),
                None => !selection.columns(tree, sheet).is_empty(),
            },
        );
        if let Some(edit) = &self.header_editing {
            if tree.node(edit.column).and_then(Node::as_column).is_none() {
                self.header_editing = None;
            }
        }
    }

    /// Enregistre un message informatif.
    fn notice(&mut self, text: String) {
        self.status = Some(StatusMessage {
            text,
            is_error: false,
        });
    }

    /// Enregistre un message d'échec.
    fn fail(&mut self, text: String) {
        self.status = Some(StatusMessage {
            text,
            is_error: true,
        });
    }

    /// Envoie une commande à l'acteur et rend compte du résultat.
    fn execute(&mut self, command: Command) {
        // Le libellé se calcule sur l'arbre d'**avant** : après coup, une
        // colonne supprimée n'a plus de nom.
        let label = command.label(&self.snapshot.tree);
        match self.handle.execute(command) {
            Ok(_) => {
                self.notice(label);
                self.refresh();
            }
            Err(err) => self.fail(format!("{err}")),
        }
    }

    fn undo(&mut self) {
        // Le libellé disparaît de la pile dès que l'annulation est faite : il
        // faut le lire avant.
        let label = self.snapshot.history.undo_label.clone();
        match self.handle.undo() {
            Ok(Some(_)) => {
                let label = label.unwrap_or_else(|| "la dernière action".to_string());
                self.notice(format!("Annulé : {label}"));
                self.refresh();
            }
            Ok(None) => self.notice("Rien à annuler".to_string()),
            Err(err) => self.fail(format!("{err}")),
        }
    }

    fn redo(&mut self) {
        let label = self.snapshot.history.redo_label.clone();
        match self.handle.redo() {
            Ok(Some(_)) => {
                let label = label.unwrap_or_else(|| "la dernière action".to_string());
                self.notice(format!("Rétabli : {label}"));
                self.refresh();
            }
            Ok(None) => self.notice("Rien à rétablir".to_string()),
            Err(err) => self.fail(format!("{err}")),
        }
    }

    fn save(&mut self) {
        let path = PathBuf::from(&self.path);
        match self.handle.save(&path) {
            Ok(()) => {
                let text = format!("Enregistré dans {}", self.path);
                self.notice(text);
            }
            Err(err) => self.fail(format!("Échec de l'enregistrement : {err}")),
        }
    }

    fn load(&mut self) {
        let path = PathBuf::from(&self.path);
        match self.handle.load(&path) {
            Ok(_) => {
                // Les identifiants du fichier n'ont aucun rapport avec ceux de
                // l'arbre précédent : la sélection doit être revalidée, ce que
                // fait `refresh`.
                self.editing = None;
                let text = format!("Chargé depuis {}", self.path);
                self.notice(text);
                self.refresh();
            }
            Err(err) => self.fail(format!("Échec du chargement : {err}")),
        }
    }

    fn select_sheet(&mut self, id: NodeId) {
        let node = self.snapshot.tree.node(id);
        if !node.is_some_and(|node| node.kind().is_worksheet()) {
            self.fail(format!("{id} n'est pas une feuille de ce projet"));
            return;
        }
        // Une feuille est toujours l'enfant directe d'un classeur (le seul
        // parent qu'`accepts_child` autorise) : c'est ce classeur, pas la
        // feuille elle-même, qui est la clé de `selected_sheets`.
        let Some(book) = node.and_then(Node::parent) else {
            self.fail(format!("{id} n'a pas de classeur parent"));
            return;
        };
        self.selected_sheets.insert(book, id);
        self.editing = None;
        self.status = None;
    }

    /// Rend `true` si la saisie a été acceptée (y compris un no-op parce que
    /// le texte n'a pas changé), `false` seulement sur un échec de
    /// validation (colonne inconnue, texte numérique inanalysable) — c'est
    /// ce que [`AppState::commit_cell_and_advance`] utilise pour décider si
    /// Entrée doit ajouter une ligne.
    fn commit_cell(&mut self, column: NodeId, row: usize, text: String) -> bool {
        // B6 + A9 (option 1, `guide-de-correction-robustesse.md` §F5) — un
        // seul correctif pour deux anomalies : si le texte validé est
        // exactement celui déjà affiché, rien n'a été saisi, et on ne
        // dispatche pas de commande. Sans cette sortie anticipée, traverser
        // une cellule sans y taper une frappe écrivait quand même (B6 :
        // révision, version de colonne et entrée d'historique pour rien), et
        // une chaîne vide présente (`Text(Some(""))`, affichée « » comme
        // `Text(None)`) était réécrite en valeur absente à la moindre
        // traversée (A9) — la branche `ColumnType::Text` ci-dessous ne
        // distingue jamais les deux en entrée. Comparer au texte affiché
        // évite les deux sans introduire de convention nouvelle : le cas où
        // l'utilisateur retape à l'identique n'a jamais besoin d'écrire.
        if text == self.cell_text(column, row) {
            return true;
        }
        let Some(kind) = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(Column::column_type)
        else {
            self.fail(format!("{column} n'est pas une colonne de ce projet"));
            return false;
        };
        // Une saisie qui ne rentre pas dans le type de la colonne ne la fait
        // plus échouer : elle **élargit** la colonne en `ColumnType::Mixed`,
        // par `Command::SetCellsPromoting`, qui groupe la conversion et
        // l'écriture dans une seule entrée d'historique. Taper « n/a » sous
        // une colonne de mesures est le geste courant d'un cahier de manip ;
        // le refuser obligeait à retaper la colonne entière en texte, donc à
        // rendre tout le reste intraçable.
        let (values, promoting) = match kind {
            ColumnType::Numeric | ColumnType::DateTime => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    // Blanc = valeur manquante, pas zéro : c'est la façon
                    // d'effacer une cellule numérique.
                    (ValueBlock::Float(vec![f64::NAN]), false)
                } else {
                    match trimmed.parse::<f64>() {
                        Ok(value) => (ValueBlock::Float(vec![value]), false),
                        // Le bloc est texte : `SetCellsPromoting` en fera
                        // une cellule texte dans une colonne devenue mixte.
                        Err(_) => (ValueBlock::Text(vec![Some(trimmed.to_string())]), true),
                    }
                }
            }
            // En texte, `""` est la valeur manquante ; le modèle distingue
            // `None` de `Some("")`, mais rien dans une cellule de saisie ne
            // permet de taper l'un plutôt que l'autre. Le banc choisit `None`.
            ColumnType::Text => {
                if text.is_empty() {
                    (ValueBlock::Text(vec![None]), false)
                } else {
                    (ValueBlock::Text(vec![Some(text)]), false)
                }
            }
            // La colonne est déjà mixte : chaque cellule décide pour
            // elle-même (`MixedCell::from_text`), il n'y a plus rien à
            // promouvoir ni rien à refuser.
            ColumnType::Mixed => {
                let trimmed = text.trim();
                let cell = MixedCell::from_text((!trimmed.is_empty()).then(|| trimmed.to_string()));
                (ValueBlock::Mixed(vec![cell]), false)
            }
        };
        self.execute(if promoting {
            Command::SetCellsPromoting {
                column,
                start: row,
                values,
                expected_version: None,
            }
        } else {
            Command::SetCells {
                column,
                start: row,
                values,
                expected_version: None,
            }
        });
        // Une colonne texte dont toutes les valeurs présentes s'analysent
        // comme des nombres se convertit seule en numérique — pendant du
        // sens inverse déjà géré ci-dessus (`SetCellsPromoting` : une
        // colonne numérique qui reçoit du texte s'élargit en `Mixed`).
        // `MixedCell::from_text` porte la même analyse que `Column::
        // set_column_type` (`Text → Numeric`, `parse_f64_text`) : ne pas la
        // dupliquer ici évite qu'elle ne diverge de ce que la conversion
        // manuelle ferait. Aucun effet si la colonne n'a aucune valeur
        // présente (colonne encore vide) ou si `kind` n'était pas `Text`.
        if kind == ColumnType::Text {
            self.auto_promote_text_column_if_all_numeric(column);
        }
        true
    }

    /// Convertit `column` (supposée `Text`) en `Numeric` si toutes ses
    /// valeurs présentes s'analysent comme des nombres finis — voir l'appel
    /// dans [`AppState::commit_cell`].
    fn auto_promote_text_column_if_all_numeric(&mut self, column: NodeId) {
        let Some(col) = self.snapshot.tree.node(column).and_then(Node::as_column) else {
            return;
        };
        let mut any_present = false;
        for row in 0..col.len() {
            let Cell::Text(text) = col.get(row) else {
                return;
            };
            match text {
                None => {}
                Some(text) => {
                    any_present = true;
                    if !matches!(MixedCell::from_text(Some(text)), MixedCell::Number(_)) {
                        return;
                    }
                }
            }
        }
        if any_present {
            self.execute(Command::SetColumnType {
                column,
                column_type: ColumnType::Numeric,
            });
        }
    }

    /// Valide la saisie d'une cellule puis, si `row` était la dernière ligne
    /// *affichée* de `sheet` et que la validation a réussi, ajoute une ligne
    /// à `sheet` (`at == len` est une cible valide pour `InsertRows`) —
    /// Entrée sur la dernière cellule d'une colonne.
    ///
    /// Comparer à `row_count(sheet)` (le max des longueurs de **toutes**
    /// les colonnes de la feuille, `Tree::row_count_in`) plutôt qu'à la
    /// longueur du buffer interne de `column` seule est essentiel : une
    /// colonne texte jamais remplie jusqu'au bas de la feuille a un buffer
    /// plus court que les autres colonnes (buffers sparses, voir
    /// `Column::len`/`ColumnValues::get`), donc `row + 1 >= column.len()`
    /// pouvait devenir vrai au *milieu* du tableau — une ligne fantôme
    /// était alors insérée en bas de toute la feuille dès qu'on validait
    /// une cellule d'une ligne jamais écrite plus bas dans cette colonne.
    fn commit_cell_and_advance(&mut self, column: NodeId, row: usize, text: String, sheet: NodeId) {
        let was_last_row = row + 1 >= self.row_count(sheet);
        let accepted = self.commit_cell(column, row, text);
        if accepted && was_last_row {
            self.rows(sheet, row + 1, 1, true);
        }
    }

    /// Vide le contenu des cellules sélectionnées de `sheet`, sans décaler
    /// ce qui suit (Retour arrière/Ctrl+X) — une seule `Command::SetCells`
    /// par colonne touchée, groupées en une seule entrée d'historique dès
    /// que plus d'une colonne est concernée (`History::begin_macro`/
    /// `end_macro`).
    fn clear_selected_cells(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            return;
        };
        let ranges = selection.clear_ranges(&self.snapshot.tree, sheet);
        if ranges.is_empty() {
            return;
        }
        let grouped = ranges.len() > 1;
        if grouped && self.handle.begin_macro("Effacer la sélection").is_err() {
            self.fail("Impossible de grouper l'effacement de la sélection".to_string());
            return;
        }
        for (column, start, count) in ranges {
            let Some(kind) = self
                .snapshot
                .tree
                .node(column)
                .and_then(Node::as_column)
                .map(Column::column_type)
            else {
                continue;
            };
            self.execute(Command::SetCells {
                column,
                start,
                values: ValueBlock::missing_for(kind, count),
                expected_version: None,
            });
        }
        if grouped {
            let _ = self.handle.end_macro();
        }
    }

    /// Vide le contenu des cellules sélectionnées de `sheet` **et** fait
    /// remonter les cellules suivantes de chaque colonne touchée,
    /// indépendamment (touche Suppr) — une seule `Command::DeleteCells`,
    /// déjà multi-colonnes.
    fn delete_selected_cells(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            return;
        };
        let (columns, at, count) = selection.delete_at_count(&self.snapshot.tree, sheet);
        if columns.is_empty() || count == 0 {
            return;
        }
        self.execute(Command::DeleteCells { columns, at, count });
    }

    /// Colle `text` (TSV) à partir de l'ancre de la sélection courante de
    /// `sheet` : les colonnes cibles sont les `n` colonnes suivant l'ancre
    /// dans l'ordre de la feuille (`n` = nombre de colonnes du TSV collé),
    /// **pas** la sélection active — comme tout tableur de référence, coller
    /// étire depuis l'ancre indépendamment de la taille de la sélection.
    /// Une colonne formule refuse naturellement l'écriture
    /// (`CoreError::FormulaColumnReadOnly`) : signalée par le statut, sans
    /// bloquer les autres colonnes déjà collées.
    fn paste_tsv(&mut self, sheet: NodeId, text: String) {
        let Some(selection) = self.selection.get(&sheet) else {
            self.fail("Aucune cellule sélectionnée pour coller".to_string());
            return;
        };
        let (anchor_column, anchor_row) = selection.anchor();
        let mut rows: Vec<Vec<&str>> = text
            .split('\n')
            .map(|line| line.trim_end_matches('\r').split('\t').collect())
            .collect();
        // La plupart des presse-papiers terminent par un `\n` : la ligne
        // vide qui en résulte ne doit pas devenir une ligne collée de plus.
        if matches!(rows.last(), Some(row) if row.len() == 1 && row[0].is_empty()) {
            rows.pop();
        }
        if rows.is_empty() {
            return;
        }
        let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
        let order: Vec<NodeId> = self
            .visible_columns(sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let Some(anchor_index) = order.iter().position(|&id| id == anchor_column) else {
            self.fail(format!(
                "{anchor_column} n'est pas une colonne de cette feuille"
            ));
            return;
        };
        let targets: Vec<NodeId> = order
            .iter()
            .copied()
            .skip(anchor_index)
            .take(column_count)
            .collect();
        if targets.is_empty() {
            return;
        }
        let grouped = targets.len() > 1;
        if grouped && self.handle.begin_macro("Coller").is_err() {
            self.fail("Impossible de grouper le collage".to_string());
            return;
        }
        for (index, &column) in targets.iter().enumerate() {
            let Some(kind) = self
                .snapshot
                .tree
                .node(column)
                .and_then(Node::as_column)
                .map(Column::column_type)
            else {
                continue;
            };
            let values = value_block_from_tsv_column(kind, &rows, index);
            self.execute(Command::SetCells {
                column,
                start: anchor_row,
                values,
                expected_version: None,
            });
        }
        if grouped {
            let _ = self.handle.end_macro();
        }
    }

    /// Relit les métadonnées actuelles de `column`, laisse `update` en
    /// modifier un seul champ, puis les remplace en bloc
    /// (`Command::SetColumnMeta` — doctrine « remplacement en bloc, jamais
    /// par sous-champ » du crate `plotyx-commands`, voir sa doc de tête).
    fn set_column_meta_field(&mut self, column: NodeId, update: impl FnOnce(&mut ColumnMeta)) {
        let Some(mut meta) = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(|c| c.meta().clone())
        else {
            self.fail(format!("{column} n'est pas une colonne de ce projet"));
            return;
        };
        update(&mut meta);
        self.execute(Command::SetColumnMeta { column, meta });
    }

    fn add_column(&mut self, sheet: NodeId) {
        self.execute(Command::AddColumn {
            sheet,
            index: None,
            name: None,
            column: Column::new(ColumnType::Numeric),
        });
    }

    /// Insertion (`insert`) ou suppression de lignes sur `sheet`.
    fn rows(&mut self, sheet: NodeId, at: usize, count: usize, insert: bool) {
        let command = if insert {
            Command::InsertRows { sheet, at, count }
        } else {
            Command::DeleteRows { sheet, at, count }
        };
        self.execute(command);
    }

    /// Importe chaque fichier de `paths` dans sa propre nouvelle feuille de
    /// `book`, avec `recipe` — [`Action::ImportFiles`]. Un fichier illisible
    /// ou inanalysable devient un [`StatusMessage`] et fait passer au
    /// suivant, jamais un abandon complet du lot. Tout le lot tient dans une
    /// seule entrée d'historique : `self.handle.begin_macro`/`end_macro`
    /// encadrent toute la boucle, même si un seul fichier réussit — voir
    /// `History::begin_macro`/`end_macro` (`plotyx-commands/src/
    /// history.rs`) pour l'absorption d'une macro qui finirait vide.
    fn import_files(
        &mut self,
        book: NodeId,
        recipe: plotyx_import::ImportRecipe,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }
        if self.handle.begin_macro("Importer").is_err() {
            self.fail(
                "Impossible de grouper l'import dans une seule entrée d'historique".to_string(),
            );
            return;
        }
        let mut imported = 0usize;
        // Accumulées plutôt que passées à `self.fail` au fil de la boucle :
        // `self.fail`/`self.notice` remplacent `self.status` sans le
        // combiner à ce qu'il portait déjà (voir leur doc), donc un
        // `self.fail` par fichier en échec se serait fait écraser par le
        // suivant, puis par le message récapitulatif final — l'utilisateur
        // n'aurait jamais vu **pourquoi** un fichier a échoué, seulement un
        // décompte muet. Ici, tout finit dans un seul message, en échec
        // (`self.fail`) dès qu'au moins un fichier a échoué, pour que ça se
        // voie.
        let mut failures: Vec<String> = Vec::new();
        for path in &paths {
            let file_stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("fichier");
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    failures.push(format!("{}: {err}", path.display()));
                    continue;
                }
            };
            let table = match plotyx_import::parse(&bytes, &recipe) {
                Ok(table) => table,
                Err(err) => {
                    failures.push(format!("{}: {err}", path.display()));
                    continue;
                }
            };
            // `Command::AddWorksheet` ne rend que la `Revision`, jamais
            // l'identifiant de la feuille créée. `Project::add_worksheet`
            // (côté `plotyx-core`) insère toujours la nouvelle feuille en
            // dernière position des enfants de `book` ; `self.execute`
            // rafraîchit déjà `self.snapshot.tree` en cas de succès (voir sa
            // doc), donc le dernier enfant de `book` juste après est bien la
            // feuille qu'on vient de créer — c'est le seul moyen de la
            // retrouver depuis ce banc, qui n'a qu'un `Arc<Tree>` figé, pas
            // le `Project` qui l'a construite. Piège à ne pas redécouvrir à
            // la dure : voir la tâche « feat/import-ui » §2.
            self.execute(Command::AddWorksheet { book, name: None });
            let Some(sheet) = self
                .snapshot
                .tree
                .node(book)
                .and_then(|node| node.children().last().copied())
            else {
                failures.push(format!(
                    "{}: échec de la création de la feuille",
                    path.display()
                ));
                continue;
            };
            // Nomme la feuille d'après le fichier plutôt que de garder le nom
            // automatique « SheetN » — best-effort : si le nom est déjà pris,
            // `Rename` échoue (`self.execute` l'écrit dans `self.status`,
            // écrasé par le message final ci-dessous de toute façon), la
            // feuille garde son nom automatique. Ce n'est pas un échec
            // d'import — les données sont bien là — donc ça ne compte pas
            // dans `failures`.
            self.execute(Command::Rename {
                id: sheet,
                name: file_stem.to_string(),
            });
            for (index, parsed_column) in table.columns.iter().enumerate() {
                // Une colonne dont au moins une cellule porte un texte
                // conservé (`n/a`, un code d'instrument…) devient une
                // colonne à types mixtes plutôt qu'une colonne numérique qui
                // aurait tout aplati en `NaN` — c'est le cas d'usage
                // motivant du chantier « colonnes à types mixtes »,
                // qu'`import` ne servait pas encore avant ce chantier-ci
                // (voir docs/chantier-colonnes-mixtes.md, section « ce qui
                // reste »). Une colonne entièrement numérique garde le
                // chemin `Numeric` d'avant, à l'identique : minimiser la
                // casse, même politique que le chantier précédent.
                let mut column = if parsed_column.texts.iter().any(Option::is_some) {
                    let cells: Vec<MixedCell> = parsed_column
                        .values
                        .iter()
                        .zip(&parsed_column.texts)
                        .map(|(value, text)| match text {
                            Some(text) => MixedCell::Text(text.clone()),
                            None => MixedCell::from_f64(*value),
                        })
                        .collect();
                    Column::from_mixed(&cells)
                } else {
                    Column::from_f64(&parsed_column.values)
                };
                // Le nom long privilégie le rôle de sous-en-tête
                // `long_name`, quand la recette en assigne un — sinon la
                // même chaîne de repli qu'avant ce chantier (ligne d'en-tête
                // de colonnes, puis nom de fichier/clé d'en-tête pour la
                // première colonne).
                let long_name = parsed_column.long_name.clone().unwrap_or_else(|| {
                    match (index, &parsed_column.name) {
                        (_, Some(name)) => name.clone(),
                        (0, None) => "X".to_string(),
                        (_, None) => plotyx_import::resolve_series_name(&recipe, &table, file_stem),
                    }
                });
                column.set_meta(ColumnMeta {
                    long_name,
                    unit: parsed_column.unit.clone().unwrap_or_default(),
                    comment: parsed_column.comment.clone().unwrap_or_default(),
                    ..Default::default()
                });
                self.execute(Command::AddColumn {
                    sheet,
                    index: None,
                    name: None,
                    column,
                });
            }
            imported += 1;
        }
        if self.handle.end_macro().is_err() {
            failures.push("échec de la clôture de l'entrée d'historique d'import".to_string());
        }
        // `self.execute` (appelé dans la boucle ci-dessus) rafraîchit déjà
        // l'instantané à chaque commande, mais toujours **pendant** que la
        // macro est ouverte : `self.snapshot.history` n'y reflète donc
        // jamais l'annulabilité de la macro une fois close. `end_macro` lui-
        // même ne rafraîchit rien (il ne passe pas par `self.execute`) — sans
        // cet appel, `AppState::can_undo` resterait sur l'état d'avant la
        // clôture jusqu'au prochain rafraîchissement fortuit.
        self.refresh();
        let summary = format!("{imported} fichier(s) importé(s) sur {}", paths.len());
        if failures.is_empty() {
            self.notice(summary);
        } else {
            self.fail(format!("{summary} — {}", failures.join(" ; ")));
        }
    }

    /// Remplace le texte d'une note, sauf s'il est déjà exactement celui-là —
    /// le même garde-fou que [`AppState::commit_cell`] (B6/A9 option 1) :
    /// traverser l'éditeur d'une note sans y taper une frappe ne doit pas
    /// écrire une entrée d'historique pour rien.
    fn set_note_text(&mut self, note: NodeId, text: String) {
        if self.note_text(note).as_ref() == Some(&text) {
            return;
        }
        self.execute(Command::SetNoteText { id: note, text });
    }

    /// Refuse d'exécuter un script : ce binaire n'embarque pas le pont Python
    /// (critère 1 de `tests/shell.rs`). N'envoie aucune commande, la
    /// révision ne bouge pas.
    #[cfg(not(feature = "python"))]
    fn run_script(&mut self) {
        self.fail(
            "Impossible d'exécuter un script : ce binaire est compilé sans le pont Python \
             (feature `python` absente)."
                .to_string(),
        );
    }

    /// Démarre une exécution de script sur un fil détaché, en réutilisant le
    /// moteur s'il existe déjà ou en le créant paresseusement sinon — la
    /// création comme l'exécution se font **hors du fil appelant**, pour que
    /// même le tout premier script (coût de démarrage de l'interpréteur
    /// compris) ne fige jamais la fenêtre. Le résultat n'est récupéré que par
    /// [`AppState::poll_script`].
    #[cfg(feature = "python")]
    fn run_script(&mut self) {
        if self.pending_script.is_some() {
            self.fail("Un script est déjà en cours d'exécution.".to_string());
            return;
        }
        let Some(python) = self.python.clone() else {
            self.fail(
                "Aucun interpréteur Python n'a été localisé pour ce binaire \
                 (PLOTYX_PYTHON_HOME, ou dist/.../lib/python)."
                    .to_string(),
            );
            return;
        };
        let existing_engine = self.engine.clone();
        let handle = self.handle.clone();
        let source = self.script_source.clone();
        let (reply_tx, reply_rx) = mpsc::channel();

        let spawned = thread::Builder::new()
            .name("plotyx-app-script".to_owned())
            .spawn(move || {
                let engine = match existing_engine {
                    Some(engine) => Ok(engine),
                    None => ScriptEngine::spawn(handle, python)
                        .map(Arc::new)
                        .map_err(|err| err.to_string()),
                };
                let reply = match engine {
                    Ok(engine) => {
                        let outcome = engine.run(&source).map_err(|err| err.to_string());
                        (Some(engine), outcome)
                    }
                    Err(message) => (None, Err(message)),
                };
                // Personne à prévenir si `AppState` a disparu entre-temps
                // (fenêtre fermée pendant l'exécution) : le canal est alors
                // simplement lâché.
                let _ = reply_tx.send(reply);
            });

        match spawned {
            Ok(_detached) => {
                self.pending_script = Some(reply_rx);
                self.notice("Script en cours d'exécution…".to_string());
            }
            Err(err) => self.fail(format!("Impossible de démarrer le fil de script : {err}")),
        }
    }

    /// Applique le résultat d'un script terminé : mémorise sa sortie, pose un
    /// message de statut **neutre** — une exception de script n'est pas une
    /// erreur d'application, sa trace se lit dans la sortie, pas en rouge
    /// dans la barre d'état — puis rafraîchit l'instantané (c'est le piège de
    /// cette tâche : sans ce geste, la grille resterait figée alors que le
    /// modèle a changé).
    ///
    /// Le message est posé **avant** `refresh`, comme dans [`AppState::
    /// execute`] : si le rafraîchissement échoue (acteur injoignable), il
    /// écrase ce message neutre par l'erreur — plus grave, elle doit
    /// l'emporter, pas l'inverse.
    #[cfg(feature = "python")]
    fn finish_script_run(&mut self, outcome: ScriptOutcome) {
        let interrupted = outcome.interrupted;
        let failed = outcome.error.is_some();
        self.script_output = Some(ScriptOutput::from(outcome));
        self.notice(
            if interrupted {
                "Script interrompu."
            } else if failed {
                "Script terminé avec une exception : voir la sortie."
            } else {
                "Script exécuté."
            }
            .to_string(),
        );
        self.refresh();
    }

    /// Refuse d'interrompre un script : ce binaire n'embarque pas le pont
    /// Python.
    #[cfg(not(feature = "python"))]
    fn interrupt_script(&mut self) {
        self.fail(
            "Impossible d'interrompre un script : ce binaire est compilé sans le pont Python \
             (feature `python` absente)."
                .to_string(),
        );
    }

    /// Demande l'arrêt du script en cours. Ne bloque pas : pose un drapeau
    /// que le fil détaché consulte à son prochain point de contrôle (voir
    /// `plotyx_script::ScriptEngine::interrupt`). Sans effet s'il n'y a pas
    /// de script en cours.
    #[cfg(feature = "python")]
    fn interrupt_script(&mut self) {
        // L'`Arc` est cloné puis le prêt relâché avant `notice` : les deux
        // champs de `self` ne peuvent pas être empruntés en même temps que
        // `self.notice` a besoin d'un emprunt exclusif.
        let engine = self.engine.clone();
        match engine {
            Some(engine) if self.pending_script.is_some() => {
                engine.interrupt();
                self.notice("Interruption demandée.".to_string());
            }
            _ => self.notice("Aucun script en cours à interrompre.".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests unitaires ciblés sur ce qui ne dépend d'aucune session ni
    //! d'aucun projet : la frontière valeur ↔ texte elle-même (A9 option 2 et
    //! C4, `guide-de-correction-robustesse.md` §F1/§F8). Le reste du fichier
    //! (dispatch, historique, sauvegarde…) est couvert en intégration par
    //! `tests/shell.rs` et par le banc `plotyx-robustesse` (batterie 15), qui
    //! ont besoin d'une session réelle.

    use super::{format_f64_for_display, MISSING_TEXT_DISPLAY};

    /// A9 (option 2) — la convention d'affichage à elle seule : une valeur
    /// absente ne doit jamais s'afficher comme la chaîne vide qui représente
    /// une chaîne vide **présente**, quel que soit l'appelant.
    #[test]
    fn a9_option2_le_marqueur_dabsence_nest_pas_la_chaine_vide() {
        assert_ne!(
            MISSING_TEXT_DISPLAY, "",
            "sans quoi une valeur absente et une chaîne vide présente \
             s'affichent identiquement (l'anomalie A9)"
        );
        assert_eq!(
            MISSING_TEXT_DISPLAY, "\u{00A0}",
            "convention retenue : un unique espace insécable"
        );
    }

    /// C4 — la reproduction du banc (`b15_une_mesure_scientifique_tient_dans_une_cellule`)
    /// fixe la spécification ; ce test fige les mêmes trois valeurs au niveau
    /// de la fonction de formatage elle-même, sans passer par une session.
    #[test]
    fn c4_les_trois_mesures_de_la_reproduction_tiennent_en_24_caracteres() {
        const LISIBLE: usize = 24;
        for valeur in [1.602_176_634e-19_f64, 6.022_140_76e23, 0.1 + 0.2] {
            let affiche = format_f64_for_display(valeur);
            assert!(
                affiche.len() <= LISIBLE,
                "{valeur} → « {affiche} » ({} caractères)",
                affiche.len()
            );
        }
    }

    /// C4 — l'aller-retour reste exact bit à bit : c'est ce que §F8 du guide
    /// vérifie déjà pour le `Display` par défaut, et qui doit rester vrai
    /// pour la notation scientifique.
    #[test]
    fn c4_laller_retour_texte_est_exact_bit_a_bit() {
        for valeur in [
            1.602_176_634e-19_f64,
            6.022_140_76e23,
            0.1 + 0.2,
            0.0,
            -0.0,
            f64::MIN_POSITIVE,
            f64::MAX,
        ] {
            let affiche = format_f64_for_display(valeur);
            let relu: f64 = affiche.parse().unwrap_or_else(|e| {
                panic!("« {affiche} » (depuis {valeur:e}) ne se reparse pas : {e}")
            });
            assert_eq!(
                relu.to_bits(),
                valeur.to_bits(),
                "{valeur:e} → « {affiche} » → {relu:e} : bits différents"
            );
        }
    }

    /// C4 — zéro ne bascule jamais en notation scientifique (`"0e0"` serait
    /// correct mais illisible pour un cas aussi courant).
    #[test]
    fn c4_zero_reste_decimal() {
        assert_eq!(format_f64_for_display(0.0), "0");
        assert_eq!(format_f64_for_display(-0.0), "-0");
    }

    /// C4 — une valeur ordinaire (celle que la grille affiche le plus
    /// souvent) n'est pas touchée par le seuil.
    #[test]
    fn c4_une_valeur_ordinaire_reste_decimale() {
        assert_eq!(format_f64_for_display(42.5), "42.5");
        assert_eq!(format_f64_for_display(0.0), "0");
        assert_eq!(format_f64_for_display(1.0), "1");
    }
}
