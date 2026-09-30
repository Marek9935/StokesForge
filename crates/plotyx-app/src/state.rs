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
use plotyx_plot::{ColorUsage, PanelSpec, Panels, PlotSpec, PlotType, SavedColors};
#[cfg(feature = "python")]
use plotyx_python::interpreter::EmbeddedPython;
#[cfg(feature = "python")]
use plotyx_script::{ScriptEngine, ScriptOutcome};
use plotyx_session::{SessionHandle, SessionResult, Snapshot};

use crate::selection::{GridCursor, Selection};
use crate::user_colors::UserColors;

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
    /// Ajoute un classeur et sa feuille initiale à la racine du projet.
    ///
    /// Le menu Fichier ne porte pas de destination ambiguë : un nouveau
    /// classeur appartient toujours au dossier racine du projet ouvert.
    AddWorkbook,
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
    /// Ajoute une feuille en fin de `book`, avec un nom généré par
    /// `plotyx-core` (`Command::AddWorksheet { name: None, .. }`, même
    /// convention que [`Action::AddWorkbook`] pour le nom du classeur).
    AddWorksheet {
        /// Classeur visé.
        book: NodeId,
    },
    /// Ajoute une colonne en fin de `sheet` puis y sélectionne `row` — Tab
    /// sur la dernière colonne d'une ligne de données, symétrique de
    /// [`Action::CommitCellAndAdvance`] (Entrée sur la dernière ligne d'une
    /// colonne) mais pour l'axe des colonnes : la colonne visée n'existe pas
    /// encore au moment où l'action est poussée (contrairement à une ligne,
    /// qui n'est qu'un indice), la sélection ne peut donc se poser qu'une
    /// fois la colonne réellement créée, dans le traitement de cette action
    /// — jamais avant, comme `AppState::select_cell` le fait pour
    /// `CommitCellAndAdvance`.
    AddColumnAndSelect {
        /// Feuille visée.
        sheet: NodeId,
        /// Ligne à sélectionner dans la nouvelle colonne.
        row: usize,
    },
    /// Ajoute une colonne en fin de `sheet` puis y sélectionne `field` —
    /// pendant de [`Action::AddColumnAndSelect`] côté en-tête : Tab valide
    /// sur le champ Formule (le dernier des quatre) de la dernière colonne,
    /// même raison pour laquelle la sélection ne peut se poser qu'une fois
    /// la colonne réellement créée.
    AddColumnAndSelectHeaderField {
        /// Feuille visée.
        sheet: NodeId,
        /// Champ d'en-tête à sélectionner dans la nouvelle colonne.
        field: HeaderField,
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
    /// Colle `text` à partir de l'ancre de la sélection courante de `sheet`
    /// — Ctrl+V. `text` est le texte reçu d'egui (`egui::Event::Paste`),
    /// c'est-à-dire le contenu du presse-papier **système** : [`AppState::
    /// paste`] compare `text` à ce qu'il a lui-même poussé au presse-papier
    /// ([`AppState::copy_selection`]/[`AppState::copy_full_columns`]) pour
    /// décider de rejouer sa structure interne ou de traiter `text` comme un
    /// texte externe — voir la matrice de comportement en tête de ce
    /// module.
    Paste {
        /// Feuille visée.
        sheet: NodeId,
        /// Texte reçu du presse-papier système.
        text: String,
    },
    /// Colle en position absolue depuis le presse-papier interne — Ctrl+Maj+
    /// V. `egui-winit` le traduit en le **même** `egui::Event::Paste` que
    /// Ctrl+V (`is_paste_command` ne teste jamais `shift`) : `crate::app`
    /// les distingue en lisant `modifiers.shift` au moment de l'événement,
    /// et jette le texte porté par l'événement — tout vient ici du dernier
    /// [`AppState::copy_selection`]/
    /// [`AppState::copy_full_columns`] de cette session. Sans effet si rien
    /// n'a encore été copié.
    PasteAbsolute {
        /// Feuille visée.
        sheet: NodeId,
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
    /// Importe les fichiers vers la destination choisie dans le dialogue.
    /// Tout le lot (toutes les feuilles, toutes les colonnes) tient dans une
    /// seule entrée d'historique.
    ImportFiles {
        /// Destination des feuilles produites.
        destination: ImportDestination,
        /// Réglages d'analyse à appliquer à chaque fichier.
        recipe: plotyx_import::ImportRecipe,
        /// Fichiers à importer, chacun dans sa propre feuille.
        paths: Vec<std::path::PathBuf>,
    },
    /// Fichiers déposés sur l'application (glisser-déposer) : importés sans
    /// passer par la fenêtre d'import quand ils se lisent correctement, voir
    /// [`file_drop_plan`]. Même macro d'historique « Importer » que
    /// [`Action::ImportFiles`].
    DropFiles {
        /// Fichiers déposés, dans l'ordre fourni par le système.
        paths: Vec<std::path::PathBuf>,
        /// Classeur sous le curseur au moment du dépôt, s'il y en avait un.
        target_book: Option<NodeId>,
    },
    /// Crée un graphe à partir des colonnes sélectionnées d'une feuille de
    /// classeur — menu contextuel de l'en-tête de colonne. Le nouveau graphe
    /// est ajouté dans le même dossier parent que le classeur ([`Command::
    /// AddGraph`]), avec une série par colonne de rôle
    /// [`plotyx_core::ColumnRole::Y`] parmi `columns`, appariée à la colonne
    /// X la plus proche à sa gauche dans la feuille (voir [`AppState::
    /// plot_columns`] pour la règle complète). N'envoie aucune commande si
    /// `columns` ne contient aucun Y : pas de graphe vide.
    PlotColumns {
        /// Feuille contenant les colonnes.
        sheet: NodeId,
        /// Colonnes sélectionnées au moment du geste.
        columns: Vec<NodeId>,
        /// Type de tracé demandé pour chaque série créée.
        plot_type: PlotType,
    },
    /// Ajoute des séries à un graphe existant, construites depuis des
    /// colonnes de classeur — même règle d'appariement X/Y que
    /// [`Action::PlotColumns`], mais en relisant le [`plotyx_plot::PlotSpec`]
    /// courant du graphe plutôt qu'en en créant un nouveau
    /// ([`Command::SetPlotSpec`]). Sert le glisser-déposer du chantier 2 :
    /// aucun appelant ne la pousse encore depuis l'UI, mais elle existe et
    /// est testée dès ce chantier-ci.
    AddSeriesToGraph {
        /// Graphe visé.
        graph: NodeId,
        /// Colonnes à ajouter en séries.
        columns: Vec<NodeId>,
        /// Type de tracé demandé pour chaque nouvelle série.
        plot_type: PlotType,
    },
    /// Remplace en bloc le [`PlotSpec`] d'un graphe — fenêtre des
    /// propriétés d'un graphe ([`Command::SetPlotSpec`]). Poussée à la
    /// validation d'un brouillon ([`AppState::commit_plot_edit`]), jamais à
    /// chaque frappe ou chaque pixel de glisser : voir la doc de tête de
    /// [`PlotEdit`].
    SetPlotSpec {
        /// Graphe visé.
        graph: NodeId,
        /// Nouvelle description du graphe.
        spec: PlotSpec,
    },
    /// Ouvre (ou ramène sur l'onglet voulu) la fenêtre des propriétés du
    /// graphe `graph` — clic droit sur un composant du graphe, ou menu
    /// « Graphique ». `axis` choisit l'axe affiché par l'onglet « Axes »
    /// (`None` : garder celui déjà choisi).
    OpenPlotSettings {
        /// Graphe visé.
        graph: NodeId,
        /// Onglet à afficher.
        tab: PlotSettingsTab,
        /// Axe à afficher dans l'onglet « Axes ».
        axis: Option<PlotAxis>,
    },
    /// Enregistre une couleur pour `usage`, dans le projet (commande
    /// annulable [`Command::SetSavedColors`]) ou pour l'utilisateur (fichier
    /// de configuration, [`crate::user_colors`], hors historique).
    SaveColor {
        /// Usage : courbes, texte ou fonds.
        usage: ColorUsage,
        /// Couleur `#RRGGBB` ou `#RRGGBBAA`.
        color: String,
        /// Où l'enregistrer.
        target: SaveTarget,
    },
    /// Retire une couleur enregistrée — pendant de [`Action::SaveColor`].
    RemoveSavedColor {
        /// Usage : courbes, texte ou fonds.
        usage: ColorUsage,
        /// Couleur à retirer.
        color: String,
        /// D'où la retirer.
        target: SaveTarget,
    },
}

/// Où enregistrer une couleur du sélecteur de couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SaveTarget {
    /// Dans le projet : la couleur voyage avec le fichier `.plx`, et
    /// l'enregistrer s'annule par Ctrl+Z. Le choix par défaut.
    #[default]
    Project,
    /// Pour l'utilisateur : la couleur reste sur cette machine, proposée
    /// dans tous les projets.
    User,
}

impl SaveTarget {
    /// Libellé du choix.
    pub fn label(self) -> &'static str {
        match self {
            SaveTarget::Project => "dans le projet",
            SaveTarget::User => "utilisateur",
        }
    }
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
/// `kind` avec la même tolérance que [`AppState::commit_cell`] — un texte qui
/// ne rentre pas dans une colonne `Numeric`/`DateTime` ne devient plus un
/// `NaN` silencieux (qui affichait la case comme vide au lieu du texte
/// collé, sans que la colonne change de type) : le bloc rendu est alors
/// `Mixed`, avec `true` pour demander à l'appelant d'écrire via
/// `Command::SetCellsPromoting` plutôt que `Command::SetCells`, exactement
/// comme une saisie tolérante au clavier. Une ligne plus courte que
/// `col_index` fournit une valeur manquante pour cette colonne.
fn value_block_from_tsv_column(
    kind: ColumnType,
    rows: &[Vec<String>],
    col_index: usize,
) -> (ValueBlock, bool) {
    match kind {
        ColumnType::Numeric | ColumnType::DateTime => {
            let texts: Vec<&str> = rows
                .iter()
                .map(|row| row.get(col_index).map(String::as_str).unwrap_or("").trim())
                .collect();
            let promoting = texts
                .iter()
                .any(|text| !text.is_empty() && text.parse::<f64>().is_err());
            if promoting {
                let cells = texts
                    .iter()
                    .map(|text| MixedCell::from_text((!text.is_empty()).then(|| text.to_string())))
                    .collect();
                (ValueBlock::Mixed(cells), true)
            } else {
                let values = texts
                    .iter()
                    .map(|text| {
                        if text.is_empty() {
                            f64::NAN
                        } else {
                            text.parse::<f64>().unwrap_or(f64::NAN)
                        }
                    })
                    .collect();
                (ValueBlock::Float(values), false)
            }
        }
        // Colonne mixte : chaque cellule décide pour elle-même, par la même
        // règle que le cœur (`MixedCell::from_text`) — un jeton qui s'analyse
        // devient un nombre, les autres restent du texte, et un jeton vide
        // une cellule vide. C'est le collage qui se rapproche le plus de ce
        // qu'un tableur fait : rien n'est perdu, et ce qui est un nombre est
        // traçable. Déjà `Mixed`, jamais de promotion à demander.
        ColumnType::Mixed => {
            let cells = rows
                .iter()
                .map(|row| {
                    let text = row.get(col_index).map(String::as_str).unwrap_or("").trim();
                    MixedCell::from_text((!text.is_empty()).then(|| text.to_string()))
                })
                .collect();
            (ValueBlock::Mixed(cells), false)
        }
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
    // seul ce chemin distingue une cellule vide (`Empty`) d'une cellule texte
    // sans valeur — que `get` confond toutes deux en `Cell::Text(None)`.
    // `Empty` est la valeur manquante de `Mixed`, qu'elle prenne la place
    // d'un `NaN` ou d'un `None` : il n'y a plus de type `Text` homogène pour
    // qui trancherait entre « manquant numérique » (`""`) et « manquant
    // texte » (`MISSING_TEXT_DISPLAY`). A9 tranche pour l'espace insécable —
    // le même traitement que `Cell::Text(None)` ci-dessous — parce que la
    // distinction qui compte à l'écran est entre « valeur manquante » et
    // « chaîne vide présente » ([`MixedCell::Text(String::new())`]), pas
    // entre les origines possibles d'une cellule manquante.
    if column.column_type() == ColumnType::Mixed {
        return match column.cell(row) {
            MixedCell::Empty => MISSING_TEXT_DISPLAY.to_string(),
            MixedCell::Number(value) if value.is_nan() => MISSING_TEXT_DISPLAY.to_string(),
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

/// Vrai si la cellule `(column, row)` porte une valeur **numérique** — sert
/// à aligner le corps de la grille comme un tableur (nombres à droite, texte
/// à gauche, voir [`crate::app::cell_widget`]). Pour une colonne `Mixed`, le
/// type se lit par cellule (`Column::cell`), pas par colonne : deux lignes
/// voisines peuvent s'aligner différemment. `false` pour une cellule vide
/// (numérique manquante ou texte manquant) : un vide n'a pas de sens à
/// pousser à droite plutôt qu'à gauche, et le laisser à gauche évite qu'une
/// colonne entièrement vide ait l'air random d'une ligne à l'autre.
pub(crate) fn cell_is_numeric_in(tree: &Tree, column: NodeId, row: usize) -> bool {
    let Some(column) = tree.node(column).and_then(Node::as_column) else {
        return false;
    };
    if column.column_type() == ColumnType::Mixed {
        return matches!(column.cell(row), MixedCell::Number(value) if !value.is_nan());
    }
    matches!(column.get(row), Cell::Float(value) if !value.is_nan())
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

/// Construit les séries de [`Action::PlotColumns`]/[`Action::
/// AddSeriesToGraph`] à partir des colonnes de rôle Y présentes dans
/// `columns`, dans l'ordre où elles apparaissent dans `sheet`.
///
/// Règle d'appariement, façon OriginLab : chaque Y est apparié à la colonne
/// X la plus proche à sa **gauche** dans la feuille (rôle
/// [`ColumnRole::X`]), en incluant les colonnes X non sélectionnées — sans X
/// à gauche, l'abscisse reste implicite ([`SeriesSpec::new`]). Chaque série
/// prend la couleur de palette de son rang, `start_index` inclus : décalage
/// utile à [`AppState::add_series_to_graph`], qui prolonge la palette après
/// les séries déjà présentes dans le panneau visé plutôt que de la reprendre
/// à zéro. Rend une liste vide si `columns` ne contient aucun Y — c'est ce
/// que les deux appelants lisent pour refuser un graphe ou un ajout vide.
pub(crate) fn series_from_columns(
    tree: &Tree,
    sheet: NodeId,
    columns: &[NodeId],
    plot_type: &PlotType,
    start_index: usize,
) -> Vec<plotyx_plot::SeriesSpec> {
    let ordered = visible_columns_in(tree, sheet);
    let role_of = |id: NodeId| {
        tree.node(id)
            .and_then(Node::as_column)
            .map(|column| column.meta().role)
    };
    let mut series = Vec::new();
    for (position, (column, _, _)) in ordered.iter().enumerate() {
        if !columns.contains(column) || role_of(*column) != Some(ColumnRole::Y) {
            continue;
        }
        let x = ordered[..position]
            .iter()
            .rev()
            .find_map(|(id, _, _)| (role_of(*id) == Some(ColumnRole::X)).then_some(*id));
        let mut spec = match x {
            Some(x) => plotyx_plot::SeriesSpec::new_xy(x, *column),
            None => plotyx_plot::SeriesSpec::new(*column),
        };
        let index = start_index + series.len();
        spec.line = plotyx_plot::LineSpec::from_palette(index);
        spec.symbol = plotyx_plot::SymbolSpec::from_palette(index);
        spec.plot_type = plot_type.clone();
        series.push(spec);
    }
    series
}

/// Le [`PlotSpec`] courant de `graph`, ou `None` si ce n'est pas un graphe de
/// ce projet.
pub(crate) fn current_plot_spec(tree: &Tree, graph: NodeId) -> Option<PlotSpec> {
    match tree.node(graph)?.kind() {
        NodeKind::Graph { spec, .. } => Some(spec.clone()),
        _ => None,
    }
}

/// Nom affiché d'une série dans la liste de la fenêtre des propriétés : le
/// même calcul que `label_for` (`plotyx-core/src/resolve.rs`, privée à ce
/// crate) — `SeriesSpec::name` s'il est renseigné, sinon le nom long de la
/// colonne Y, sinon son nom de nœud, sinon « (référence rompue) » si la
/// colonne n'existe plus. Dupliqué plutôt que rendu public côté
/// `plotyx-core` : c'est une règle d'affichage, pas une règle de résolution,
/// et la fonction est petite.
pub(crate) fn series_list_label(tree: &Tree, series: &plotyx_plot::SeriesSpec) -> String {
    if !series.name.is_empty() {
        return series.name.clone();
    }
    let Some(y_node) = tree.node(series.y).filter(|node| node.kind().is_column()) else {
        return "(référence rompue)".to_string();
    };
    let long_name = y_node
        .as_column()
        .map(|column| column.meta().long_name.as_str())
        .unwrap_or_default();
    if long_name.is_empty() {
        y_node.name().to_string()
    } else {
        long_name.to_string()
    }
}

/// Charge utile du glisser-déposer d'une sélection de colonnes de classeur
/// vers un graphe ouvert — chantier « glisser une sélection vers un graphe »
/// (voir [`Action::AddSeriesToGraph`]). Typée plutôt qu'une chaîne : c'est ce
/// qu'exigent `egui::Response::dnd_set_drag_payload`/`dnd_hover_payload`/
/// `dnd_release_payload` (`Any + Send + Sync`), et ce que [`drop_payload_action`]
/// relit au dépôt pour construire l'action. `Clone` : `crate::app` la clone
/// une fois par bord de grille testé (gauche puis droite) avant de savoir
/// lequel a effectivement démarré le glisser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDragPayload {
    /// Feuille d'où viennent les colonnes.
    pub sheet: NodeId,
    /// Colonnes glissées, dans l'ordre de [`AppState::column_drag_payload`].
    pub columns: Vec<NodeId>,
}

/// L'[`Action`] produite par le dépôt de `payload` sur le graphe `graph` —
/// toujours [`Action::AddSeriesToGraph`] avec [`PlotType::Line`] par défaut,
/// comme demandé pour ce chantier. Fonction pure, séparée du code de peinture
/// de `crate::app::graph_drop_target` pour être testable sans fenêtre.
pub fn drop_payload_action(payload: &ColumnDragPayload, graph: NodeId) -> Action {
    Action::AddSeriesToGraph {
        graph,
        columns: payload.columns.clone(),
        plot_type: PlotType::Line,
    }
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

impl HeaderField {
    /// Position parmi les 4 champs, dans l'ordre où ils sont empilés à
    /// l'écran (de haut en bas, 0 = `LongName`) — sert à comparer deux
    /// champs pour savoir si l'un est « au-dessus » de l'autre, en
    /// particulier pour la plage de champs d'en-tête sélectionnée (voir
    /// [`crate::selection::Selection::is_header_field_selected`]).
    pub(crate) fn index(self) -> usize {
        match self {
            HeaderField::LongName => 0,
            HeaderField::Unit => 1,
            HeaderField::Comment => 2,
            HeaderField::Formula => 3,
        }
    }

    /// Inverse de [`HeaderField::index`] — `index` au-delà de 3 vaut
    /// `Formula`, jamais de panique : les appelants ([`crate::selection::
    /// Selection::cleared_header_fields`]/[`AppState::copy_selection`]) ne
    /// passent que des rangs déjà bornés à `-4..=-1`, mais une saturation
    /// silencieuse est plus sûre qu'un `match` incomplet si ça changeait.
    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            0 => HeaderField::LongName,
            1 => HeaderField::Unit,
            2 => HeaderField::Comment,
            _ => HeaderField::Formula,
        }
    }
}

/// Zone de la grille que touche la sélection courante — l'en-tête, les
/// données, ou des colonnes entières. Les trois sont mutuellement
/// exclusives : un rectangle ne traverse jamais la frontière en-tête/données
/// et une plage de colonnes est un geste à part (voir la doc de tête de
/// [`crate::selection`]).
///
/// Sert à contextualiser les menus contextuels de la grille
/// ([`AppState::grid_menu`]) : c'est la même liste de gestes partout, mais
/// nommée selon ce qui est réellement sélectionné — « Vider le champ » sur
/// un champ d'en-tête, « Vider les cellules » sur des données, « Vider les
/// colonnes » sur une plage de colonnes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridZone {
    /// Rectangle de champs d'en-tête (Nom long, Unité, Commentaire,
    /// Formule).
    Header,
    /// Rectangle de cellules de données.
    Data,
    /// Plage de colonnes entières (rangée des lettres).
    Columns,
    /// Plage de lignes entières (gouttière des numéros de ligne).
    Rows,
}

/// Ce qu'un menu contextuel de la grille doit proposer pour la sélection
/// courante — construit par [`AppState::grid_menu`], rendu par
/// [`crate::app::grid_edit_menu`].
///
/// Ce type ne porte **que la décision** (quels gestes ont un sens ici,
/// comment les nommer, lesquels sont grisés) et jamais de widget : c'est ce
/// qui permet de le vérifier par les tests d'intégration sans UI de
/// `tests/shell.rs`, comme tout le reste de la matrice copier/coller.
///
/// Les gestes listés sont exactement les raccourcis clavier de la grille
/// (voir la doc de tête de ce module, section copier/coller, et le bloc
/// clavier de [`crate::app::grid_contents`]) : aucun menu n'offre ici
/// d'action qui n'aurait pas d'équivalent au clavier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridMenu {
    /// Zone touchée par la sélection.
    pub zone: GridZone,
    /// Vrai si la sélection couvre plus d'une case (plusieurs colonnes,
    /// plusieurs rangs, ou les deux) — décide du pluriel des libellés.
    /// Exception pour [`GridZone::Rows`] : ses libellés (« la ligne »/« les
    /// lignes ») parlent de lignes, pas de cases, donc `plural` n'y compte
    /// que le nombre de lignes couvertes — une seule ligne sélectionnée sur
    /// une feuille à plusieurs colonnes touche déjà plusieurs cases mais
    /// reste « la ligne » au singulier.
    pub plural: bool,
    /// Vrai si la sélection touche plus d'une colonne. Distinct de
    /// [`GridMenu::plural`], qui compte les *cases* : trois cellules
    /// empilées dans une seule colonne sont plurielles sans être
    /// multi-colonnes, et « Copier avec l'en-tête » (qui raisonne en
    /// colonnes entières, jamais en cases) doit rester au singulier.
    pub multi_column: bool,
    /// Vrai si « Supprimer et décaler » (Suppr, deuxième pression) a un sens
    /// ici. Faux dans l'en-tête : un champ n'est pas répété par ligne, il
    /// n'y a rien à faire remonter, et Suppr s'y contente de vider — voir
    /// [`AppState::delete_selected_cells`]. L'offrir quand même y ferait
    /// deux entrées de menu pour un seul et même effet.
    pub offers_delete_shift: bool,
    /// Vrai si « Coller à l'identique » (Ctrl+Maj+V) a de quoi travailler :
    /// il rejoue le presse-papier **interne**, donc reste sans effet tant
    /// que rien n'a été copié depuis la grille dans cette session (un texte
    /// externe n'a pas de rang d'origine à rejouer).
    pub paste_absolute_enabled: bool,
}

impl GridMenu {
    /// Nom de ce qui est sélectionné, accordé en nombre (« la cellule »,
    /// « les champs », « les lignes »…).
    ///
    /// # Délibérément **pas** utilisé par le menu contextuel
    ///
    /// Les entrées du menu contextuel disent « Copier », pas « Copier la
    /// cellule » : le clic droit vient de se poser sur ce qui est
    /// sélectionné, le répéter dans chaque entrée n'apprend rien et allonge
    /// le menu. Cette méthode-ci (et [`GridMenu::copy_full_columns_label`])
    /// est conservée pour les surfaces d'UI à venir où l'on ne voit **pas**
    /// ce qu'on vise — une barre de menus, une info-bulle, une ligne de
    /// statut, une confirmation — où nommer la cible redevient utile. Elle
    /// reste verrouillée par les tests de `tests/shell.rs` pour qu'elle
    /// suive la matrice de sélection en attendant.
    pub fn target_label(&self) -> &'static str {
        match (self.zone, self.plural) {
            (GridZone::Header, false) => "le champ",
            (GridZone::Header, true) => "les champs",
            (GridZone::Data, false) => "la cellule",
            (GridZone::Data, true) => "les cellules",
            (GridZone::Columns, false) => "la colonne",
            (GridZone::Columns, true) => "les colonnes",
            (GridZone::Rows, false) => "la ligne",
            (GridZone::Rows, true) => "les lignes",
        }
    }

    /// Libellé développé de « Copier avec l'en-tête » (Ctrl+Maj+C), qui
    /// promeut toujours en entier la ou les colonnes touchées, quelle que
    /// soit la forme de la sélection — voir [`AppState::copy_full_columns`].
    /// Comme [`GridMenu::target_label`], gardée pour une UI future : le menu
    /// contextuel, lui, dit simplement « Copier avec l'en-tête ».
    pub fn copy_full_columns_label(&self) -> &'static str {
        if self.multi_column {
            "Copier les colonnes entières"
        } else {
            "Copier la colonne entière"
        }
    }
}

/// Ce qui a été copié dans la grille depuis le dernier [`AppState::
/// copy_selection`]/[`AppState::copy_full_columns`], tel que
/// [`AppState::paste`]/[`AppState::paste_absolute`] en ont besoin — voir la
/// matrice de comportement en tête de ce module (section copier/coller).
///
/// Remplace l'ancien marqueur de contrôle (`HEADER_LINE_MARKER`, retiré) qui
/// polluait le presse-papier système d'un caractère de contrôle collé tel
/// quel dans un tableur externe : le presse-papier **système** ne reçoit
/// plus qu'un texte TSV propre ([`grid_clipboard_to_text`]) ; cette
/// structure-ci vit uniquement dans [`AppState::grid_clipboard`] et n'est
/// rejouée au collage que si le texte reçu est identique à celui qu'on a
/// nous-même poussé au presse-papier ([`AppState::clipboard_text`]) — sans
/// quoi une copie externe entre-temps (un autre programme, ou même un
/// Ctrl+C hors de PlotYx) rejouerait silencieusement une structure qui ne
/// correspond plus à ce que porte réellement le presse-papier.
#[derive(Debug, Clone, PartialEq)]
enum GridClipboard {
    /// Une seule case copiée (cellule de donnée ou champ d'en-tête) : son
    /// texte seul, rien d'autre — permet par exemple de copier la formule
    /// d'une colonne dans le champ Formule d'une autre.
    Text(String),
    /// Rectangle de plusieurs cases copié par Ctrl+C : se colle en se
    /// décalant sur l'ancre (voir [`AppState::paste`]).
    Relative(RelativeCopy),
    /// Colonne(s) entières copiées par Ctrl+Maj+C (en-tête et corps) : se
    /// colle à ses rangs d'origine (voir [`AppState::paste`]).
    FullColumns(Vec<FullColumnCopy>),
}

/// Rectangle relatif copié par Ctrl+C ([`GridClipboard::Relative`]) : soit
/// des champs d'en-tête, soit des lignes de données, jamais les deux — une
/// sélection ne traverse plus la frontière en-tête/données (voir la doc de
/// tête de [`crate::selection`]). `start` porte le rang d'origine de la
/// première rangée copiée, pour qu'[`AppState::paste_absolute`] puisse
/// rejouer chaque rangée sur son rang d'origine plutôt que décalée sur
/// l'ancre.
#[derive(Debug, Clone, PartialEq)]
enum RelativeCopy {
    HeaderFields {
        /// Champ d'origine de la première rangée de `rows`.
        start: HeaderField,
        /// Une rangée par champ copié, dans l'ordre où ils sont empilés à
        /// l'écran ; une valeur par colonne touchée.
        rows: Vec<Vec<String>>,
    },
    DataRows {
        /// Ligne d'origine de la première rangée de `rows`.
        start: usize,
        /// Une rangée par ligne copiée ; une valeur par colonne touchée.
        rows: Vec<Vec<String>>,
    },
}

/// Une colonne copiée en entier par Ctrl+Maj+C — voir
/// [`GridClipboard::FullColumns`].
#[derive(Debug, Clone, PartialEq)]
struct FullColumnCopy {
    long_name: String,
    unit: String,
    comment: String,
    formula: Option<String>,
    /// Corps de la colonne, ligne 0 en premier — la longueur propre de
    /// cette colonne, indépendante des autres colonnes touchées par la même
    /// copie (une feuille autorise des colonnes de longueurs différentes,
    /// voir `demo_project` dans `tests/shell.rs`).
    data: Vec<String>,
}

/// Texte propre (TSV, sans caractère de contrôle) pour le presse-papier
/// **système** — ce que colle un tableur externe qui reçoit `clipboard`. Un
/// [`GridClipboard::Text`] n'a pas de forme tabulaire, il se rend tel quel.
fn grid_clipboard_to_text(clipboard: &GridClipboard) -> String {
    match clipboard {
        GridClipboard::Text(text) => text.clone(),
        GridClipboard::Relative(RelativeCopy::HeaderFields { rows, .. })
        | GridClipboard::Relative(RelativeCopy::DataRows { rows, .. }) => rows_to_tsv(rows),
        GridClipboard::FullColumns(columns) => {
            let mut rows: Vec<Vec<String>> = vec![
                columns.iter().map(|c| c.long_name.clone()).collect(),
                columns.iter().map(|c| c.unit.clone()).collect(),
                columns.iter().map(|c| c.comment.clone()).collect(),
                columns
                    .iter()
                    .map(|c| c.formula.clone().unwrap_or_default())
                    .collect(),
            ];
            let len = columns.iter().map(|c| c.data.len()).max().unwrap_or(0);
            for row in 0..len {
                rows.push(
                    columns
                        .iter()
                        .map(|c| c.data.get(row).cloned().unwrap_or_default())
                        .collect(),
                );
            }
            rows_to_tsv(&rows)
        }
    }
}

/// Sérialise `rows` en TSV (`\t` entre colonnes, `\n` après chaque ligne,
/// y compris la dernière) — partagé par [`grid_clipboard_to_text`].
fn rows_to_tsv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&row.join("\t"));
        out.push('\n');
    }
    out
}

/// Reconstruit un [`GridClipboard`] depuis un texte externe (pas le nôtre :
/// voir [`GridClipboard`]) — un tableur ou un éditeur de texte tiers n'a
/// aucune notion de champ d'en-tête, donc jamais de
/// [`GridClipboard::FullColumns`] ni de [`RelativeCopy::HeaderFields`] par
/// ce chemin. Sans tabulation ni retour à la ligne, `text` devient un
/// [`GridClipboard::Text`] (règle 1 de la matrice : s'écrit dans la case
/// ancrée, quelle qu'elle soit) ; sinon un [`RelativeCopy::DataRows`] parti
/// de la ligne 0 (règle 1 aussi : un rectangle tabulé ou multi-lignes est
/// traité comme un rectangle relatif de corps).
fn external_clipboard_from_text(text: &str) -> GridClipboard {
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    // La plupart des presse-papiers terminent par un `\n` : la ligne vide
    // qui en résulte ne doit pas devenir une ligne collée de plus.
    if matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }
    let single_line_without_tab =
        lines.len() <= 1 && !lines.first().is_some_and(|l| l.contains('\t'));
    if single_line_without_tab {
        return GridClipboard::Text(lines.first().copied().unwrap_or("").to_string());
    }
    let rows: Vec<Vec<String>> = lines
        .iter()
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect();
    GridClipboard::Relative(RelativeCopy::DataRows { start: 0, rows })
}

/// Colonne visée par `cursor`, quelle que soit sa sorte — [`GridCursor`]
/// n'expose pas cet accesseur hors de [`crate::selection`] ; ses appelants
/// dans ce module (le collage, voir [`AppState::apply_paste`]) le
/// reconstruisent par ce filtrage plutôt que par une méthode publique
/// supplémentaire sur un type dont l'API reste par ailleurs minimale.
fn grid_cursor_column(cursor: GridCursor) -> NodeId {
    match cursor {
        GridCursor::Data(column, _) | GridCursor::Header(column, _) => column,
    }
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

/// Nature d'onglet actif, vue par [`AppState`] — un miroir minimal de
/// l'énumération privée `Tab` de [`crate::app`], juste assez pour que la
/// question « le menu « Graphique » doit-il s'afficher ? »
/// ([`active_graph`]) se pose sans fenêtre, dans
/// `tests/shell.rs`. `crate::app` traduit son `Tab` vers celui-ci au seul
/// point de contact (`PlotyxApp::ui`), rien d'autre ne le lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveTabKind {
    /// Aucun onglet actif (dock vide, ou rien de focalisé cette image-ci).
    #[default]
    None,
    /// Un classeur.
    Book(NodeId),
    /// Un graphe.
    Graph(NodeId),
    /// Une matrice.
    Matrix(NodeId),
    /// Une note.
    Note(NodeId),
}

/// Le graphe sélectionné, d'après la nature de l'onglet actif — `None` sauf
/// pour [`ActiveTabKind::Graph`]. Décide la présence du menu « Graphique »
/// dans la barre de menus. Fonction pure, séparée du code de peinture de
/// `crate::app` pour être testable sans fenêtre.
pub fn active_graph(active: ActiveTabKind) -> Option<NodeId> {
    match active {
        ActiveTabKind::Graph(id) => Some(id),
        _ => None,
    }
}

/// Onglets thématiques de la fenêtre des propriétés d'un graphe, dans leur
/// ordre d'affichage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotSettingsTab {
    /// Le graphe entier : titre, fond.
    Graph,
    /// La zone de tracé : titre du panneau, grilles.
    PlotArea,
    /// Un axe : titre, échelle, bornes, affichage.
    Axes,
    /// Les séries tracées.
    Series,
    /// La légende.
    Legend,
    /// Les couleurs globales : fond, texte, axes, thèmes prédéfinis.
    Theme,
}

impl PlotSettingsTab {
    /// Tous les onglets, dans l'ordre d'affichage.
    pub const ALL: [PlotSettingsTab; 6] = [
        PlotSettingsTab::Graph,
        PlotSettingsTab::PlotArea,
        PlotSettingsTab::Axes,
        PlotSettingsTab::Series,
        PlotSettingsTab::Legend,
        PlotSettingsTab::Theme,
    ];

    /// Libellé de l'onglet.
    pub fn label(self) -> &'static str {
        match self {
            PlotSettingsTab::Graph => "Graphe",
            PlotSettingsTab::PlotArea => "Zone de tracé",
            PlotSettingsTab::Axes => "Axes",
            PlotSettingsTab::Series => "Séries",
            PlotSettingsTab::Legend => "Légende",
            PlotSettingsTab::Theme => "Thème",
        }
    }
}

/// Un axe éditable du (premier) panneau d'un graphe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotAxis {
    /// Abscisses, en bas.
    Bottom,
    /// Ordonnée principale, à gauche.
    Left,
    /// Ordonnée secondaire, à droite.
    Right,
}

impl PlotAxis {
    /// Libellé de l'axe.
    pub fn label(self) -> &'static str {
        match self {
            PlotAxis::Bottom => "X (bas)",
            PlotAxis::Left => "Y (gauche)",
            PlotAxis::Right => "Y secondaire (droite)",
        }
    }
}

/// Un composant d'un graphe visé par un clic droit — traduit par
/// `crate::app` depuis ce que le rendu désigne sous le pointeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotTarget {
    /// Le fond de la figure, hors de tout composant.
    Background,
    /// Le titre du graphe.
    Title,
    /// La zone de tracé.
    PlotArea,
    /// Un axe (ligne et graduations).
    Axis(PlotAxis),
    /// Le titre d'un axe.
    AxisTitle(PlotAxis),
}

impl PlotTarget {
    /// Nom du composant, en tête du menu contextuel.
    pub fn label(self) -> String {
        match self {
            PlotTarget::Background => "Graphe".to_string(),
            PlotTarget::Title => "Titre du graphe".to_string(),
            PlotTarget::PlotArea => "Zone de tracé".to_string(),
            PlotTarget::Axis(axis) => format!("Axe {}", axis.label()),
            PlotTarget::AxisTitle(axis) => format!("Titre de l'axe {}", axis.label()),
        }
    }

    /// L'onglet (et l'axe) de la fenêtre des propriétés qui règle ce
    /// composant.
    pub fn settings_page(self) -> (PlotSettingsTab, Option<PlotAxis>) {
        match self {
            PlotTarget::Background | PlotTarget::Title => (PlotSettingsTab::Graph, None),
            PlotTarget::PlotArea => (PlotSettingsTab::PlotArea, None),
            PlotTarget::Axis(axis) | PlotTarget::AxisTitle(axis) => {
                (PlotSettingsTab::Axes, Some(axis))
            }
        }
    }

    /// L'action qui ouvre les propriétés de ce composant sur `graph`.
    pub fn open_settings(self, graph: NodeId) -> Action {
        let (tab, axis) = self.settings_page();
        Action::OpenPlotSettings { graph, tab, axis }
    }
}

/// État de la fenêtre des propriétés d'un graphe : quel graphe, quel onglet,
/// quel axe dans l'onglet « Axes ». État d'UI pur ; les valeurs éditées
/// vivent dans le brouillon [`PlotEdit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlotSettings {
    /// Graphe dont la fenêtre règle les propriétés.
    pub graph: NodeId,
    /// Onglet affiché.
    pub tab: PlotSettingsTab,
    /// Axe affiché dans l'onglet « Axes ».
    pub axis: PlotAxis,
}

/// Brouillon d'édition de la fenêtre des propriétés d'un graphe : un
/// [`PlotSpec`] entier tenu hors de l'historique jusqu'à validation, sur le
/// même principe que [`NoteEdit`]/[`HeaderEdit`] — sauf qu'ici l'objet édité
/// est un graphe entier plutôt qu'un seul champ. La raison : le panneau
/// mélange une douzaine de petits widgets (titre, axes, style de chaque
/// série, légende) qui doivent tous voir et modifier le **même** brouillon
/// pour que « annuler un aller-retour dans le panneau » ait un sens — un
/// tampon par champ serait autant de mécanismes de va-et-vient avec le
/// modèle à maintenir séparément.
///
/// [`AppState::commit_plot_edit`] n'envoie [`Command::SetPlotSpec`] que si
/// `spec` diverge effectivement du spec courant du modèle : relâcher un
/// curseur qui n'a pas bougé, ou perdre le focus d'un champ texte non
/// modifié, ne doit pas empiler une entrée d'historique inutile.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotEdit {
    /// Graphe en cours d'édition.
    pub graph: NodeId,
    /// Description en cours de saisie, pas encore validée.
    pub spec: PlotSpec,
    /// Série actuellement choisie dans la liste, pour l'édition détaillée —
    /// état d'UI pur, jamais envoyé au modèle.
    pub selected_series: usize,
    /// Le spec du modèle sur lequel le brouillon est parti (ou qu'il vient
    /// de valider). Tant que `spec` lui est égal, le brouillon n'a rien en
    /// cours : si le modèle change sous lui (Annuler, Rétablir, script,
    /// glisser-déposer d'une colonne), [`AppState::begin_plot_edit`] le
    /// réaligne au lieu de montrer des valeurs périmées — qu'une validation
    /// suivante réécrirait par-dessus l'annulation.
    pub base: PlotSpec,
}

/// Destination d'un lot d'import. Chaque fichier produit une feuille ; les
/// nouveaux classeurs ne sont créés qu'après analyse réussie d'un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDestination {
    ActiveBook(NodeId),
    NewBookPerFile,
    NewBookForAll,
}

/// Ce qu'un dépôt de fichiers demande de faire — voir [`file_drop_plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropPlan {
    /// Projets `.plx` : chacun s'ouvre dans une nouvelle fenêtre PlotYx.
    pub projects: Vec<PathBuf>,
    /// Fichiers de données à importer.
    pub data: Vec<PathBuf>,
    /// Destination des fichiers de données.
    pub destination: ImportDestination,
}

/// Décision pure d'un dépôt : le lieu du dépôt fixe la destination (un
/// classeur sous le curseur reçoit une feuille par fichier, n'importe où
/// ailleurs un nouveau classeur partagé), l'extension `.plx` fait d'un
/// fichier un projet à ouvrir plutôt qu'une donnée à importer.
pub fn file_drop_plan(paths: &[PathBuf], target_book: Option<NodeId>) -> DropPlan {
    let is_project = |path: &PathBuf| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("plx"))
    };
    DropPlan {
        projects: paths.iter().filter(|p| is_project(p)).cloned().collect(),
        data: paths.iter().filter(|p| !is_project(p)).cloned().collect(),
        destination: target_book.map_or(
            ImportDestination::NewBookForAll,
            ImportDestination::ActiveBook,
        ),
    }
}

/// Un fichier « se lit correctement » quand la recette le découpe en au moins
/// une colonne et une ligne dont une valeur est un nombre. Sinon le dépôt
/// ouvre la fenêtre d'import plutôt que de créer une feuille vide ou
/// illisible.
fn reads_well(path: &std::path::Path, recipe: &plotyx_import::ImportRecipe) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    plotyx_import::parse(&bytes, recipe).is_ok_and(|table| {
        table
            .columns
            .iter()
            .any(|column| column.values.iter().any(|value| !value.is_nan()))
    })
}

/// État transitoire de la fenêtre d'import (voir [`crate::app`] pour son
/// rendu) : jamais dans l'historique, jamais envoyé à l'acteur — même
/// famille que [`HeaderEdit`]/[`CellEdit`]/[`NoteEdit`]. Porte tout ce que la
/// fenêtre a besoin de retenir entre deux images, y compris le
/// [`plotyx_import::store::RecipeStore`] chargé une seule fois à l'ouverture
/// (pas rechargé à chaque image — `RecipeStore::load` touche le disque).
pub struct ImportState {
    /// Classeur actif à l'ouverture, disponible même après un changement de destination.
    pub active_book: Option<NodeId>,
    /// Destination choisie ; le classeur actif est proposé s'il existe.
    pub destination: ImportDestination,
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
    /// Brouillon en cours de la fenêtre des propriétés d'un graphe, s'il y
    /// en a un (voir [`PlotEdit`]).
    plot_edit: Option<PlotEdit>,
    /// Fenêtre des propriétés du graphe, si elle est ouverte (voir
    /// [`PlotSettings`]).
    plot_settings: Option<PlotSettings>,
    /// Une macro d'historique est ouverte pour la fenêtre des propriétés :
    /// ses gestes s'appliquent en direct (aperçu) mais ne forment qu'une
    /// entrée d'historique, posée à la validation ; l'annulation la rejette.
    plot_macro_open: bool,
    /// Couleurs enregistrées pour l'utilisateur, hors projet (voir
    /// [`crate::user_colors`]).
    user_colors: UserColors,
    /// Sélection courante de la grille de chaque feuille affichée — état UI
    /// pur, jamais dans l'historique (voir la doc de tête de
    /// [`crate::selection`]). Clé absente = pas de sélection dans cette
    /// feuille.
    selection: std::collections::HashMap<NodeId, Selection>,
    /// Saisie en cours dans un champ d'en-tête de colonne, s'il y en a une.
    header_editing: Option<HeaderEdit>,
    /// Dernier presse-papier de grille structuré, posé par [`AppState::
    /// copy_selection`]/[`AppState::copy_full_columns`] — état UI pur, comme
    /// [`AppState::selection`] : jamais dans l'historique, jamais envoyé à
    /// l'acteur. `None` tant que rien n'a encore été copié dans cette
    /// session (ou après un collage qui l'a laissé intact : copier ne le
    /// vide jamais, seul un nouveau `copy_selection`/`copy_full_columns` le
    /// remplace).
    grid_clipboard: Option<GridClipboard>,
    /// Texte exact poussé au presse-papier **système** en même temps que
    /// [`AppState::grid_clipboard`] — sert à [`AppState::paste`] à décider
    /// si le texte reçu d'egui est notre propre copie (rejoue la structure)
    /// ou un texte externe (voir [`external_clipboard_from_text`]).
    clipboard_text: Option<String>,
    /// Une copie locale du viewport détaché doit être transférée une fois à
    /// la racine ; sans ce drapeau, sa vieille copie écraserait toute copie
    /// plus récente faite dans une autre fenêtre à chaque image.
    clipboard_dirty: bool,
    /// Sélection sur laquelle Suppr vient déjà de **vider** les cases (pas
    /// encore supprimé) — voir [`AppState::consume_delete_key`]. `Some`
    /// entre les deux temps d'une double pression sur Suppr : une deuxième
    /// pression sur *exactement* la même sélection de la même feuille
    /// supprime (décale ce qui suit) plutôt que de re-vider ce qui est déjà
    /// vide. Réinitialisé dès que la sélection change (elle ne compare plus
    /// égale à l'ancre mémorisée) ou explicitement par [`AppState::
    /// forget_delete_repeat`] avant toute autre mutation de la grille sur la
    /// même sélection, pour qu'un Suppr après un Couper/Retour arrière/
    /// collage reparte au premier temps plutôt que de supprimer par
    /// surprise.
    delete_repeat: Option<(NodeId, Selection)>,
    /// État de la fenêtre d'import, tant qu'elle est ouverte — voir
    /// [`ImportState`].
    import_state: Option<ImportState>,
    /// Projets `.plx` déposés, à ouvrir chacun dans une nouvelle fenêtre
    /// PlotYx : l'état ne lance aucun processus (testable sans fenêtre), la
    /// couche fenêtre les vide par [`AppState::take_projects_to_open`].
    projects_to_open: Vec<PathBuf>,
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
            plot_edit: None,
            plot_settings: None,
            plot_macro_open: false,
            user_colors: UserColors::load(),
            selection: std::collections::HashMap::new(),
            header_editing: None,
            grid_clipboard: None,
            clipboard_text: None,
            clipboard_dirty: false,
            delete_repeat: None,
            import_state: None,
            projects_to_open: Vec::new(),
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

    /// Donne à un viewport détaché (classeur ou note) son propre état
    /// transitoire — grille, saisie de cellule ou de note —, adossé au même
    /// acteur. Il peut ainsi employer exactement les mêmes widgets que la
    /// fenêtre flottante sans verrouiller l'état de la racine.
    pub fn fork_for_detached_view(&self) -> SessionResult<Self> {
        let mut view = Self::new(self.handle.clone(), self.path.clone())?;
        view.snapshot = self.snapshot.clone();
        view.selected_sheets = self.selected_sheets.clone();
        view.selection = self.selection.clone();
        view.editing = self.editing.clone();
        view.header_editing = self.header_editing.clone();
        view.grid_clipboard = self.grid_clipboard.clone();
        view.clipboard_text = self.clipboard_text.clone();
        view.clipboard_dirty = false;
        Ok(view)
    }

    /// Rafraîchit la lecture d'un viewport sans toucher à sa sélection ni à
    /// ses tampons de saisie. L'instantané vient de l'état principal.
    pub fn sync_detached_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
        self.revalidate_selection();
    }

    /// Réunit l'interaction d'un classeur détaché avec la voie de commande
    /// principale avant que ses actions différées soient dispatchées.
    pub fn adopt_detached_book_view(&mut self, source: &mut Self, book: NodeId) {
        if let Some(sheet) = source.selected_sheet_in(book) {
            self.selected_sheets.insert(book, sheet);
        }
        for (sheet, _) in source.worksheets_of(book) {
            if let Some(selection) = source.selection.get(&sheet) {
                self.selection.insert(sheet, selection.clone());
            } else {
                self.selection.remove(&sheet);
            }
        }
        if source.clipboard_dirty {
            self.grid_clipboard = source.grid_clipboard.clone();
            self.clipboard_text = source.clipboard_text.clone();
            source.clipboard_dirty = false;
        }
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

    /// Enregistre la géométrie de la fenêtre de contenu `node` auprès de
    /// l'acteur. Ce réglage de vue ne passe pas par l'historique : déplacer
    /// une fenêtre ne doit pas ajouter une étape à annuler.
    ///
    /// Ne rafraîchit pas l'instantané après chaque déplacement : l'acteur
    /// possède déjà la géométrie que sa prochaine sauvegarde sérialisera, et
    /// un aller-retour par image rendrait le glisser inutilement coûteux.
    pub fn set_window_geometry(&mut self, node: NodeId, geometry: plotyx_session::WindowGeometry) {
        if let Err(err) = self.handle.set_window_geometry(node, geometry) {
            self.fail(format!("Échec de la mise à jour de la géométrie : {err}"));
        }
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

    /// Libellé de l'action « Annuler » : `Some("Annuler Saisie dans la colonne A")`, ou
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

    /// Révision courante de l'instantané — sert à `crate::app` à détecter,
    /// après un [`AppState::dispatch`], si l'action a réellement produit une
    /// commande (`Action::PlotColumns` sur une sélection sans Y n'en envoie
    /// aucune, la révision ne bouge pas).
    pub fn revision(&self) -> plotyx_session::Revision {
        self.snapshot.revision
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

    /// Le [`PlotSpec`] courant de `graph`, ou `None` si ce n'est pas (ou
    /// plus) un graphe de ce projet.
    pub fn plot_spec(&self, graph: NodeId) -> Option<PlotSpec> {
        current_plot_spec(&self.snapshot.tree, graph)
    }

    /// Charge utile du glisser d'une sélection de colonnes de `sheet` vers un
    /// graphe ouvert ([`ColumnDragPayload`]), ou `None` si `sheet` n'a
    /// aucune sélection ou si sa sélection ne couvre aucune colonne
    /// ([`Selection::columns`]) — pas de glisser possible sans colonne, même
    /// depuis la zone de prise. `crate::app::grid_contents` l'appelle à
    /// chaque image pour savoir s'il y a quoi que ce soit à proposer au
    /// glisser ; ne rien avoir sélectionné y désactive la zone de prise tout
    /// entière plutôt que de la laisser démarrer un glisser vide.
    pub fn column_drag_payload(&self, sheet: NodeId) -> Option<ColumnDragPayload> {
        let columns = self.selection(sheet)?.columns(&self.snapshot.tree, sheet);
        if columns.is_empty() {
            None
        } else {
            Some(ColumnDragPayload { sheet, columns })
        }
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

    /// Vrai si `(column, row)` porte une valeur numérique — voir
    /// [`cell_is_numeric_in`].
    pub fn cell_is_numeric(&self, column: NodeId, row: usize) -> bool {
        cell_is_numeric_in(&self.snapshot.tree, column, row)
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
    /// gardant l'ancre déjà posée — glisser ou Maj-clic. Continue un
    /// rectangle déjà commencé sur un champ d'en-tête, mais s'arrête à la
    /// frontière plutôt que de la traverser (voir [`Selection::
    /// extend_row_to`] et la doc de tête de [`crate::selection`]) ; pose une
    /// sélection d'une seule cellule si `sheet` n'en avait pas encore, ou si
    /// la sélection en cours est une plage de colonnes (rangée des lettres),
    /// une plage de lignes (gouttière des numéros de ligne) ou une plage de
    /// champ d'en-tête entier (libellé de champ dans la gouttière) — ces
    /// glissers-là restent des gestes distincts, voir [`AppState::
    /// extend_column_selection`]/[`AppState::extend_row_selection`]/
    /// [`AppState::extend_header_row_selection`].
    pub fn extend_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection)
                if !selection.is_columns()
                    && !selection.is_full_rows()
                    && !selection.is_header_rows() =>
            {
                selection.extend_row_to(GridCursor::Data(column, row))
            }
            _ => self.select_cell(sheet, column, row),
        }
    }

    /// Sélectionne `column` dans son entier — clic sur sa lettre d'en-tête.
    pub fn select_column(&mut self, sheet: NodeId, column: NodeId) {
        self.selection.insert(sheet, Selection::column(column));
    }

    /// Étend la plage de colonnes courante de `sheet` jusqu'à `column`, en
    /// gardant l'ancre déjà posée — glisser ou Maj-clic sur la rangée des
    /// lettres. Pose une sélection d'une seule colonne si `sheet` n'en avait
    /// pas encore, ou si la sélection en cours n'est pas déjà une plage de
    /// colonnes — un glisser qui franchit la frontière depuis les données,
    /// les champs d'en-tête ou la gouttière des numéros de ligne vers la
    /// rangée des lettres repose toujours une sélection simple, même
    /// politique qu'au clavier (voir [`AppState::move_selection`]).
    pub fn extend_column_selection(&mut self, sheet: NodeId, column: NodeId) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_columns() => selection.extend_columns_to(column),
            _ => self.select_column(sheet, column),
        }
    }

    /// Sélectionne `row` dans son entier (toutes les colonnes visibles de
    /// `sheet`) — clic sur son numéro dans la gouttière. Pendant de
    /// [`AppState::select_column`].
    pub fn select_row(&mut self, sheet: NodeId, row: usize) {
        self.selection.insert(sheet, Selection::row(row));
    }

    /// Étend la plage de lignes courante de `sheet` jusqu'à `row`, en
    /// gardant l'ancre déjà posée — glisser ou Maj-clic sur la gouttière des
    /// numéros de ligne. Pose une sélection d'une seule ligne si `sheet`
    /// n'en avait pas encore, ou si la sélection en cours n'est pas déjà une
    /// plage de lignes — un glisser qui franchit la frontière depuis les
    /// données, les champs d'en-tête ou la rangée des lettres vers la
    /// gouttière repose toujours une sélection simple, pendant de
    /// [`AppState::extend_column_selection`].
    pub fn extend_row_selection(&mut self, sheet: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_full_rows() => selection.extend_full_rows_to(row),
            _ => self.select_row(sheet, row),
        }
    }

    /// Sélectionne un champ d'en-tête précis (nom long/unité/commentaire/
    /// formule) — simple clic, même comportement que [`AppState::
    /// select_cell`] pour une cellule de données.
    pub fn select_header_field(&mut self, sheet: NodeId, column: NodeId, field: HeaderField) {
        self.selection
            .insert(sheet, Selection::header_field(column, field));
    }

    /// Étend la plage courante de `sheet` jusqu'au champ `(column, field)`,
    /// en gardant l'ancre déjà posée — glisser ou Maj-clic sur une des 4
    /// rangées de champs. Continue un rectangle déjà commencé dans les
    /// données, mais s'arrête à la frontière plutôt que de la traverser
    /// (voir [`Selection::extend_row_to`] et la doc de tête de [`crate::
    /// selection`]) ; pose une sélection d'un seul champ si `sheet` n'en
    /// avait pas encore, ou si la sélection en cours est une plage de
    /// colonnes (rangée des lettres), une plage de lignes (gouttière des
    /// numéros de ligne) ou une plage de champ d'en-tête entier (libellé de
    /// champ dans la gouttière) — ces glissers-là restent des gestes
    /// distincts, voir [`AppState::extend_column_selection`]/[`AppState::
    /// extend_row_selection`]/[`AppState::extend_header_row_selection`].
    pub fn extend_header_field_selection(
        &mut self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) {
        match self.selection.get_mut(&sheet) {
            Some(selection)
                if !selection.is_columns()
                    && !selection.is_full_rows()
                    && !selection.is_header_rows() =>
            {
                selection.extend_row_to(GridCursor::Header(column, field))
            }
            _ => self.select_header_field(sheet, column, field),
        }
    }

    /// Sélectionne `field` dans son entier (toutes les colonnes visibles de
    /// `sheet`) — clic sur son libellé dans la gouttière (« Long Name »,
    /// « Units », « Comments », « F(x)= »). Pendant de [`AppState::
    /// select_row`], côté en-tête.
    pub fn select_header_row(&mut self, sheet: NodeId, field: HeaderField) {
        self.selection.insert(sheet, Selection::header_row(field));
    }

    /// Étend la plage de champ d'en-tête entier courante de `sheet` jusqu'à
    /// `field`, en gardant l'ancre déjà posée — glisser ou Maj-clic sur un
    /// libellé de champ dans la gouttière. Pose une sélection d'un seul
    /// champ entier si `sheet` n'en avait pas encore, ou si la sélection en
    /// cours n'est pas déjà une plage de ce genre — un glisser qui franchit
    /// la frontière depuis les données, les champs d'en-tête d'une seule
    /// colonne ou la rangée des lettres vers la gouttière repose toujours
    /// une sélection simple, pendant de [`AppState::extend_row_selection`].
    pub fn extend_header_row_selection(&mut self, sheet: NodeId, field: HeaderField) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_header_rows() => selection.extend_header_rows_to(field),
            _ => self.select_header_row(sheet, field),
        }
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

    /// Déplace la sélection de `sheet` d'une case dans `direction` (flèches
    /// directionnelles), en gardant l'ancre déjà posée si `extend`
    /// (Maj+flèche étend le rectangle plutôt que déplacer la case
    /// sélectionnée). Haut/Bas traversent la frontière entre l'en-tête (les
    /// 4 champs, voir [`HeaderField::ALL`]) et les données : Haut depuis la
    /// première ligne de données rejoint le champ Formule (le plus bas des
    /// 4), Bas depuis le champ Formule rejoint la première ligne de
    /// données, Haut depuis le champ Nom long (le plus haut) ne fait rien —
    /// la rangée des lettres n'entre pas dans cette navigation, il faut la
    /// cliquer pour la sélectionner (voir [`Selection::grid_cursor`]).
    /// Franchir cette frontière repose toujours une sélection d'une seule
    /// case, même sous Maj : une plage ne peut pas mélanger cellules de
    /// données et champs d'en-tête. Gauche/Droite, eux, ne changent jamais
    /// de rangée. Clampé aux bords de la grille — jamais de retour à la
    /// ligne (voir [`AppState::move_selection_tab`] pour ça) ni d'ajout de
    /// ligne (réservé à [`AppState::commit_cell_and_advance`], déclenché par
    /// la validation d'une saisie, pas par la navigation seule). Sans effet
    /// si `sheet` a une plage de colonnes sélectionnée (rangée des lettres)
    /// ou aucune sélection — mutation transitoire, comme [`AppState::
    /// select_cell`].
    pub fn move_selection(&mut self, sheet: NodeId, direction: MoveDirection, extend: bool) {
        /// Rangée traversée par la navigation aux flèches : soit un des 4
        /// champs d'en-tête, soit une ligne de données — voir la doc de
        /// [`AppState::move_selection`].
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Row {
            Header(HeaderField),
            Data(usize),
        }
        impl Row {
            fn up(self) -> (Self, bool) {
                use HeaderField::{Comment, Formula, LongName, Unit};
                match self {
                    Row::Data(0) => (Row::Header(Formula), true),
                    Row::Data(row) => (Row::Data(row - 1), false),
                    Row::Header(LongName) => (self, false),
                    Row::Header(Unit) => (Row::Header(LongName), false),
                    Row::Header(Comment) => (Row::Header(Unit), false),
                    Row::Header(Formula) => (Row::Header(Comment), false),
                }
            }
            fn down(self, last_row: usize) -> (Self, bool) {
                use HeaderField::{Comment, Formula, LongName, Unit};
                match self {
                    Row::Header(Formula) => (Row::Data(0), true),
                    Row::Header(LongName) => (Row::Header(Unit), false),
                    Row::Header(Unit) => (Row::Header(Comment), false),
                    Row::Header(Comment) => (Row::Header(Formula), false),
                    Row::Data(row) => (Row::Data((row + 1).min(last_row)), false),
                }
            }
        }

        let Some(from) = self.selection.get(&sheet).and_then(Selection::grid_cursor) else {
            return;
        };
        let (from_column, from_row) = match from {
            GridCursor::Data(column, row) => (column, Row::Data(row)),
            GridCursor::Header(column, field) => (column, Row::Header(field)),
        };
        let order = self.column_order(sheet);
        let Some(index) = order.iter().position(|&id| id == from_column) else {
            return;
        };
        let last_row = self.row_count(sheet).saturating_sub(1);
        let (to_index, to_row, crossed) = match direction {
            MoveDirection::Up => {
                let (row, crossed) = from_row.up();
                (index, row, crossed)
            }
            MoveDirection::Down => {
                let (row, crossed) = from_row.down(last_row);
                (index, row, crossed)
            }
            MoveDirection::Left => (index.saturating_sub(1), from_row, false),
            MoveDirection::Right => (
                (index + 1).min(order.len().saturating_sub(1)),
                from_row,
                false,
            ),
        };
        let Some(&to_column) = order.get(to_index) else {
            return;
        };
        // Franchir la frontière en-tête/données repose toujours une
        // sélection réduite à une seule case, même sous Maj — mélanger
        // cellules de données et champs d'en-tête dans une même plage
        // n'aurait pas de sens (voir la doc de tête de cette méthode).
        let extend = extend && !crossed;
        match to_row {
            Row::Data(row) => {
                if extend {
                    self.extend_selection(sheet, to_column, row);
                } else {
                    self.select_cell(sheet, to_column, row);
                }
            }
            Row::Header(field) => {
                if extend {
                    self.extend_header_field_selection(sheet, to_column, field);
                } else {
                    self.select_header_field(sheet, to_column, field);
                }
            }
        }
    }

    /// Déplace la sélection de `sheet` d'une case vers la colonne suivante
    /// (`backward: false`) ou précédente (`true`), avec retour à la ligne
    /// aux bords — Tab/Maj+Tab, comme dans tout tableur : Tab en fin de
    /// ligne repart au début de la ligne suivante, Maj+Tab en début de ligne
    /// revient à la fin de la précédente. Traverse aussi la frontière entre
    /// l'en-tête (les 4 champs) et les données, contrairement aux flèches
    /// directionnelles (voir [`AppState::move_selection`]) qui la
    /// traversent mais sans jamais entrer en jeu avec Tab : Tab depuis
    /// Formule (le plus bas des 4 champs) de la **dernière** colonne entre
    /// dans les données à `Data(0)` de la **première** colonne ; Maj+Tab
    /// depuis `Data(0)` de la première colonne fait le chemin inverse et
    /// rejoint Formule de la dernière colonne — le point d'entrée symétrique
    /// dans l'en-tête. Au sein de l'en-tête ou des données seules, Tab
    /// circule normalement (colonne suivante/précédente, champ suivant/
    /// précédent ou ligne suivante/précédente aux bords de colonne),
    /// clampé sans autre franchissement de frontière. Jamais d'extension de
    /// sélection (Tab n'étend pas, contrairement aux flèches sous Maj) ; les
    /// mêmes gardes que [`AppState::move_selection`] sinon (sans effet si
    /// `sheet` a une plage de colonnes sélectionnée ou aucune sélection).
    pub fn move_selection_tab(&mut self, sheet: NodeId, backward: bool) {
        let Some(from) = self.selection.get(&sheet).and_then(Selection::grid_cursor) else {
            return;
        };
        let order = self.column_order(sheet);
        let row_count = self.row_count(sheet);
        if order.is_empty() || row_count == 0 {
            return;
        }
        let from_column = match from {
            GridCursor::Data(column, _) | GridCursor::Header(column, _) => column,
        };
        let Some(index) = order.iter().position(|&id| id == from_column) else {
            return;
        };
        let last_index = order.len() - 1;

        /// Champ suivant/précédent une fois toutes les colonnes parcourues
        /// pour le champ courant, `None` aux bornes (Formule en avançant,
        /// Nom long en reculant) — Tab parcourt d'abord les colonnes à champ
        /// fixe, voir la doc de [`AppState::move_selection_tab`].
        fn step_field(field: HeaderField, backward: bool) -> Option<HeaderField> {
            use HeaderField::{Comment, Formula, LongName, Unit};
            match (field, backward) {
                (LongName, false) => Some(Unit),
                (Unit, false) => Some(Comment),
                (Comment, false) => Some(Formula),
                (Formula, false) => None,
                (Unit, true) => Some(LongName),
                (Comment, true) => Some(Unit),
                (Formula, true) => Some(Comment),
                (LongName, true) => None,
            }
        }

        let to = if backward {
            match from {
                GridCursor::Header(_, field) if index > 0 => {
                    GridCursor::Header(order[index - 1], field)
                }
                GridCursor::Header(_, field) => match step_field(field, true) {
                    Some(prev) => GridCursor::Header(order[last_index], prev),
                    None => GridCursor::Header(from_column, HeaderField::LongName),
                },
                GridCursor::Data(_, 0) if index == 0 => {
                    GridCursor::Header(order[last_index], HeaderField::Formula)
                }
                GridCursor::Data(_, row) => {
                    if index > 0 {
                        GridCursor::Data(order[index - 1], row)
                    } else {
                        GridCursor::Data(order[last_index], row.saturating_sub(1))
                    }
                }
            }
        } else {
            match from {
                GridCursor::Header(_, field) if index < last_index => {
                    GridCursor::Header(order[index + 1], field)
                }
                GridCursor::Header(_, field) => match step_field(field, false) {
                    Some(next) => GridCursor::Header(order[0], next),
                    None => GridCursor::Data(order[0], 0),
                },
                GridCursor::Data(_, row) => {
                    if index < last_index {
                        GridCursor::Data(order[index + 1], row)
                    } else {
                        GridCursor::Data(order[0], (row + 1).min(row_count - 1))
                    }
                }
            }
        };
        match to {
            GridCursor::Data(column, row) => self.select_cell(sheet, column, row),
            GridCursor::Header(column, field) => self.select_header_field(sheet, column, field),
        }
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
        self.selection.get(&sheet).is_some_and(|selection| {
            selection.is_column_selected(&self.snapshot.tree, sheet, column)
        })
    }

    /// Vrai si `row` est sélectionnée dans son entier (clic sur son numéro
    /// dans la gouttière) — pendant de [`AppState::is_column_selected`].
    pub fn is_row_selected(&self, sheet: NodeId, row: usize) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_row_selected(row))
    }

    /// Vrai si `field` est sélectionné dans son entier (clic sur son
    /// libellé dans la gouttière) — pendant de [`AppState::is_row_selected`],
    /// côté en-tête.
    pub fn is_header_row_selected(&self, sheet: NodeId, field: HeaderField) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_header_row_selected(field))
    }

    /// Vrai si `column` est touchée par la sélection courante de `sheet`,
    /// quelle que soit sa sorte (une seule cellule de données de cette
    /// colonne suffit) — voir [`Selection::touches_column`]. Distinct de
    /// [`AppState::is_column_selected`], qui ne vaut que pour un clic direct
    /// sur la rangée des lettres.
    pub fn column_touched_by_selection(&self, sheet: NodeId, column: NodeId) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.touches_column(&self.snapshot.tree, sheet, column))
    }

    /// Vrai si `row` est touchée par la sélection courante de `sheet` —
    /// pendant de [`AppState::column_touched_by_selection`], côté lignes.
    pub fn row_touched_by_selection(&self, sheet: NodeId, row: usize) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.touches_row(row))
    }

    /// Vrai si `sheet` est actuellement sélectionnée dans son entier (clic
    /// sur le « # » de la gouttière) — voir [`AppState::select_all`].
    pub fn is_all_selected(&self, sheet: NodeId) -> bool {
        let row_count = self.row_count(sheet);
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_all_rows(row_count))
    }

    /// Sélectionne tout le tableau de `sheet` (toutes les lignes, donc
    /// toutes les colonnes visibles) — clic sur le « # » de la gouttière,
    /// pendant de [`AppState::select_row`] pour tout le tableau plutôt
    /// qu'une seule ligne. Sans effet sur une feuille sans ligne : il n'y a
    /// alors rien à sélectionner.
    pub fn select_all(&mut self, sheet: NodeId) {
        let row_count = self.row_count(sheet);
        if row_count == 0 {
            return;
        }
        self.selection
            .insert(sheet, Selection::all_rows(row_count - 1));
    }

    /// Vrai si `(column, field)` fait partie de la plage de champs d'en-tête
    /// actuellement sélectionnée dans `sheet`.
    pub fn is_header_field_selected(
        &self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) -> bool {
        self.selection.get(&sheet).is_some_and(|selection| {
            selection.is_header_field_selected(&self.snapshot.tree, sheet, column, field)
        })
    }

    /// Champ d'en-tête sélectionné dans `sheet`, s'il y en a un — pour les
    /// raccourcis clavier (Ctrl+C/Suppr) scopés à la grille.
    pub fn selected_header_field(&self, sheet: NodeId) -> Option<(NodeId, HeaderField)> {
        self.selection.get(&sheet)?.selected_header_field()
    }

    /// Ce que le menu contextuel de la grille doit proposer pour la
    /// sélection courante de `sheet` — voir [`GridMenu`]. `None` si `sheet`
    /// n'a pas de sélection ou si elle ne désigne plus de colonne valide
    /// (colonne supprimée entre-temps) : il n'y a alors pas de menu à
    /// ouvrir, plutôt qu'un menu dont chaque entrée serait sans effet.
    ///
    /// Un clic droit pose toujours une sélection avant d'arriver ici (voir
    /// [`crate::app::cell_widget`]/[`crate::app::header_field_widget`]/
    /// [`crate::app::column_header_widget`], qui réduisent la sélection à la
    /// case visée si le clic droit tombe en dehors) : le `None` ne se
    /// produit en pratique que sur une feuille vidée sous le curseur.
    pub fn grid_menu(&self, sheet: NodeId) -> Option<GridMenu> {
        let selection = self.selection.get(&sheet)?;
        let columns = selection.columns(&self.snapshot.tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let multi_column = columns.len() > 1;
        let (zone, plural) = if selection.is_full_rows() {
            // Une plage de lignes entières touche par construction toutes
            // les colonnes de la feuille : `multi_column` n'a donc rien à
            // voir avec le pluriel ici, contrairement aux autres zones (voir
            // la doc de `GridMenu::plural`) — seul le nombre de *lignes*
            // couvertes décide de « la ligne » vs « les lignes ».
            let rows_span = selection
                .row_range()
                .map(|(lo, hi)| hi.saturating_sub(lo))
                .unwrap_or(0);
            (GridZone::Rows, rows_span > 0)
        } else {
            // `full_rank_range` distingue les deux zones d'un rectangle par
            // le signe de ses rangs (négatifs = en-tête, voir
            // `crate::selection::GridCursor::rank`) ; `None` ne reste que
            // pour une plage de colonnes entières, qui n'a pas de rang
            // unique.
            let (zone, rank_span) = match selection.full_rank_range() {
                Some((lo, hi)) if hi < 0 => (GridZone::Header, hi - lo),
                Some((lo, hi)) => (GridZone::Data, hi - lo),
                None => (GridZone::Columns, 0),
            };
            (zone, multi_column || rank_span > 0)
        };
        Some(GridMenu {
            zone,
            plural,
            multi_column,
            offers_delete_shift: zone != GridZone::Header,
            paste_absolute_enabled: self.grid_clipboard.is_some(),
        })
    }

    /// Copie la sélection courante de `sheet` — Ctrl+C (et la première
    /// moitié de Ctrl+X, voir [`crate::app`]). `None` si `sheet` n'a pas de
    /// sélection ou si elle ne désigne plus de colonne valide (rien à
    /// pousser au presse-papier).
    ///
    /// Pose à la fois [`AppState::grid_clipboard`] (rejoué par
    /// [`AppState::paste`]/[`AppState::paste_absolute`]) et
    /// [`AppState::clipboard_text`] (pour reconnaître cette copie au
    /// collage), et rend le texte à pousser au presse-papier **système** —
    /// voir la matrice de comportement en tête de ce module :
    ///
    /// - une seule case sélectionnée (cellule ou champ d'en-tête) →
    ///   [`GridClipboard::Text`], son texte seul ;
    /// - plusieurs cases, toutes dans l'en-tête ou toutes dans les données
    ///   (jamais les deux, voir la doc de tête de [`crate::selection`]) →
    ///   [`GridClipboard::Relative`] ;
    /// - une plage de colonnes entières (rangée des lettres) →
    ///   [`GridClipboard::Relative`] aussi, sur tout le corps de chaque
    ///   colonne (pas d'en-tête : une plage de colonnes n'est pas un
    ///   rectangle de champs).
    ///
    /// Une valeur manquante s'écrit comme une chaîne vide (jamais
    /// [`MISSING_TEXT_DISPLAY`], qui n'a de sens que pour l'écran, pas pour
    /// un échange avec un tableur externe).
    pub fn copy_selection(&mut self, sheet: NodeId) -> Option<String> {
        let selection = self.selection.get(&sheet)?;
        let tree = &self.snapshot.tree;
        let columns = selection.columns(tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let clipboard = if let Some((column, field)) = selection.selected_header_field() {
            GridClipboard::Text(self.header_field_display_text(column, field))
        } else if let Some((column, row)) = selection.single_cell() {
            GridClipboard::Text(cell_text_in(tree, column, row).replace(MISSING_TEXT_DISPLAY, ""))
        } else if let Some((lo, hi)) = selection.full_rank_range() {
            if hi < 0 {
                let start = HeaderField::from_index((lo + 4) as usize);
                let rows = (lo..=hi)
                    .map(|rank| {
                        let field = HeaderField::from_index((rank + 4) as usize);
                        columns
                            .iter()
                            .map(|&column| self.header_field_display_text(column, field))
                            .collect()
                    })
                    .collect();
                GridClipboard::Relative(RelativeCopy::HeaderFields { start, rows })
            } else {
                let rows = (lo..=hi)
                    .map(|rank| {
                        columns
                            .iter()
                            .map(|&column| {
                                cell_text_in(tree, column, rank as usize)
                                    .replace(MISSING_TEXT_DISPLAY, "")
                            })
                            .collect()
                    })
                    .collect();
                GridClipboard::Relative(RelativeCopy::DataRows {
                    start: lo as usize,
                    rows,
                })
            }
        } else {
            // Plage de colonnes entières (rangée des lettres) :
            // `full_rank_range` n'a pas de sens, il n'y a pas de rang unique
            // à couvrir — on copie tout le corps de chaque colonne touchée,
            // à partir de sa propre longueur (des colonnes de longueurs
            // différentes sont autorisées).
            let len = columns
                .iter()
                .filter_map(|&column| tree.node(column).and_then(Node::as_column))
                .map(Column::len)
                .max()
                .unwrap_or(0);
            if len == 0 {
                return None;
            }
            let rows = (0..len)
                .map(|row| {
                    columns
                        .iter()
                        .map(|&column| {
                            cell_text_in(tree, column, row).replace(MISSING_TEXT_DISPLAY, "")
                        })
                        .collect()
                })
                .collect();
            GridClipboard::Relative(RelativeCopy::DataRows { start: 0, rows })
        };
        let text = grid_clipboard_to_text(&clipboard);
        self.grid_clipboard = Some(clipboard);
        self.clipboard_text = Some(text.clone());
        self.clipboard_dirty = true;
        Some(text)
    }

    /// Copie en entier (en-tête + corps) les colonnes touchées par la
    /// sélection courante de `sheet` — Ctrl+Maj+C, quelle que soit la forme
    /// de la sélection (une seule case, un rectangle, une plage de
    /// colonnes) : c'est toujours la ou les colonnes qu'elle touche qui sont
    /// promues en entier. `None` si `sheet` n'a pas de sélection ou si elle
    /// ne désigne plus de colonne valide. Voir la matrice de comportement en
    /// tête de ce module.
    pub fn copy_full_columns(&mut self, sheet: NodeId) -> Option<String> {
        let selection = self.selection.get(&sheet)?;
        let tree = &self.snapshot.tree;
        let columns = selection.columns(tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let full: Vec<FullColumnCopy> = columns
            .iter()
            .map(|&column| {
                let meta = self.column_meta(column);
                let len = tree
                    .node(column)
                    .and_then(Node::as_column)
                    .map(Column::len)
                    .unwrap_or(0);
                let data = (0..len)
                    .map(|row| cell_text_in(tree, column, row).replace(MISSING_TEXT_DISPLAY, ""))
                    .collect();
                FullColumnCopy {
                    long_name: meta.long_name,
                    unit: meta.unit,
                    comment: meta.comment,
                    formula: meta.formula,
                    data,
                }
            })
            .collect();
        let clipboard = GridClipboard::FullColumns(full);
        let text = grid_clipboard_to_text(&clipboard);
        self.grid_clipboard = Some(clipboard);
        self.clipboard_text = Some(text.clone());
        self.clipboard_dirty = true;
        Some(text)
    }

    /// Texte affiché d'un champ d'en-tête (hors édition) — même détail que
    /// [`crate::app::header_field_widget`], factorisé ici pour être partagé
    /// avec [`AppState::copy_selection`]/[`AppState::copy_full_columns`].
    pub(crate) fn header_field_display_text(&self, column: NodeId, field: HeaderField) -> String {
        let meta = self.column_meta(column);
        match field {
            HeaderField::LongName => meta.long_name,
            HeaderField::Unit => meta.unit,
            HeaderField::Comment => meta.comment,
            HeaderField::Formula => meta.formula.unwrap_or_default(),
        }
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

    /// Brouillon courant de la fenêtre des propriétés d'un graphe, s'il y
    /// en a un.
    pub fn plot_edit(&self) -> Option<&PlotEdit> {
        self.plot_edit.as_ref()
    }

    /// Brouillon courant, modifiable — c'est là que les widgets du panneau
    /// écrivent, sans passer par une commande.
    pub fn plot_edit_mut(&mut self) -> Option<&mut PlotEdit> {
        self.plot_edit.as_mut()
    }

    /// Ouvre (ou conserve) le brouillon d'édition du graphe `graph`. À
    /// appeler à chaque image par la fenêtre des propriétés, *avant* de
    /// peindre quoi que ce soit : ne réinitialise le brouillon que s'il n'y
    /// en a pas encore, ou s'il portait sur un autre graphe — repeindre la
    /// même image sur le même graphe ne doit jamais effacer une saisie en
    /// cours. Un brouillon sans saisie en cours suit en revanche le modèle
    /// (voir [`PlotEdit::base`]). `graph` sans
    /// spec (nœud supprimé, ou pas un graphe) laisse le brouillon inchangé :
    /// c'est à l'appelant de vérifier `self.tree().node(graph)` avant
    /// d'afficher le panneau (voir `node_title` dans `crate::app`).
    pub fn begin_plot_edit(&mut self, graph: NodeId) {
        let Some(model) = self.plot_spec(graph) else {
            return;
        };
        if let Some(edit) = self.plot_edit.as_mut().filter(|edit| edit.graph == graph) {
            if edit.spec == edit.base && edit.base != model {
                edit.spec = model.clone();
                edit.base = model;
            }
            return;
        }
        self.plot_edit = Some(PlotEdit {
            graph,
            spec: model.clone(),
            selected_series: 0,
            base: model,
        });
    }

    /// Ferme le brouillon d'édition courant sans rien valider.
    pub fn close_plot_edit(&mut self) {
        self.plot_edit = None;
    }

    /// La fenêtre des propriétés du graphe, si elle est ouverte.
    pub fn plot_settings(&self) -> Option<PlotSettings> {
        self.plot_settings
    }

    /// Change l'onglet ou l'axe affiché par la fenêtre des propriétés.
    pub fn plot_settings_mut(&mut self) -> Option<&mut PlotSettings> {
        self.plot_settings.as_mut()
    }

    /// **Valide** et ferme la fenêtre des propriétés (bouton « Valider ») :
    /// valide d'abord ce qui diverge encore du modèle (une saisie dont le
    /// champ n'a pas perdu le focus), ferme la macro d'historique — tous les
    /// gestes de la fenêtre ne forment qu'**une** entrée, qu'un seul Ctrl+Z
    /// défait — puis abandonne le brouillon.
    pub fn close_plot_settings(&mut self) {
        self.commit_plot_edit();
        self.end_plot_macro();
        self.plot_settings = None;
        self.plot_edit = None;
    }

    /// **Annule** et ferme la fenêtre des propriétés (bouton « Annuler »,
    /// croix, Échap) : avorte la macro d'historique, ce qui restaure le
    /// graphe tel qu'il était à l'ouverture, et jette le brouillon. Sans
    /// macro (ouverture refusée, voir [`AppState::begin_plot_macro`]), les
    /// gestes déjà appliqués restent dans l'historique ordinaire.
    pub fn cancel_plot_settings(&mut self) {
        if self.plot_macro_open {
            self.plot_macro_open = false;
            if let Err(err) = self.handle.abort_macro() {
                self.fail(format!("Annulation des propriétés impossible : {err}"));
            }
            self.refresh();
        }
        self.plot_settings = None;
        self.plot_edit = None;
    }

    /// Ouvre la macro d'historique de la fenêtre des propriétés. Un refus
    /// (macro déjà ouverte par un autre émetteur, par exemple un script) est
    /// dit à l'utilisateur ; la fenêtre reste utilisable, sans annulation
    /// groupée.
    fn begin_plot_macro(&mut self) {
        if self.plot_macro_open {
            return;
        }
        match self.handle.begin_macro("Propriétés du graphe") {
            Ok(_) => self.plot_macro_open = true,
            Err(err) => self.fail(format!(
                "Propriétés : annulation groupée indisponible ({err})"
            )),
        }
    }

    /// Ferme la macro de la fenêtre des propriétés (une entrée d'historique,
    /// aucune si rien n'a changé).
    fn end_plot_macro(&mut self) {
        if !self.plot_macro_open {
            return;
        }
        self.plot_macro_open = false;
        if let Err(err) = self.handle.end_macro() {
            self.fail(format!(
                "Clôture de l'entrée d'historique impossible : {err}"
            ));
        }
        // `end_macro` ne rafraîchit pas l'instantané (voir l'import).
        self.refresh();
    }

    /// Valide le brouillon courant ([`Action::SetPlotSpec`]) — mais
    /// seulement s'il diverge du spec actuellement dans le modèle. Sans
    /// cette garde, relâcher un curseur qui n'a pas bougé ou perdre le focus
    /// d'un champ texte non modifié empilerait une entrée d'historique
    /// identique à la précédente ; voir la doc de tête de [`PlotEdit`].
    pub fn commit_plot_edit(&mut self) {
        let Some(edit) = self.plot_edit.clone() else {
            return;
        };
        if self.plot_spec(edit.graph).as_ref() == Some(&edit.spec) {
            return;
        }
        self.dispatch(Action::SetPlotSpec {
            graph: edit.graph,
            spec: edit.spec.clone(),
        });
        if let Some(current) = self.plot_edit.as_mut() {
            current.base = edit.spec;
        }
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
            Action::AddWorkbook => self.add_workbook(),
            Action::SelectSheet(id) => self.select_sheet(id),
            Action::CommitCell { column, row, text } => {
                self.commit_cell(column, row, text);
            }
            Action::AddColumn { sheet } => self.add_column(sheet),
            Action::AddWorksheet { book } => self.add_worksheet(book),
            Action::AddColumnAndSelect { sheet, row } => self.add_column_and_select(sheet, row),
            Action::AddColumnAndSelectHeaderField { sheet, field } => {
                self.add_column_and_select_header_field(sheet, field)
            }
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
            Action::Paste { sheet, text } => self.paste(sheet, text),
            Action::PasteAbsolute { sheet } => self.paste_absolute(sheet),
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
                destination,
                recipe,
                paths,
            } => self.import_files(destination, recipe, paths),
            Action::DropFiles { paths, target_book } => self.drop_files(paths, target_book),
            Action::PlotColumns {
                sheet,
                columns,
                plot_type,
            } => self.plot_columns(sheet, columns, plot_type),
            Action::AddSeriesToGraph {
                graph,
                columns,
                plot_type,
            } => self.add_series_to_graph(graph, columns, plot_type),
            Action::SetPlotSpec { graph, spec } => {
                self.execute(Command::SetPlotSpec { graph, spec })
            }
            Action::OpenPlotSettings { graph, tab, axis } => {
                self.open_plot_settings(graph, tab, axis);
            }
            Action::SaveColor {
                usage,
                color,
                target,
            } => {
                let next = self.saved_colors(target).with_added(usage, &color);
                self.replace_saved_colors(target, next);
            }
            Action::RemoveSavedColor {
                usage,
                color,
                target,
            } => {
                let next = self.saved_colors(target).with_removed(usage, &color);
                self.replace_saved_colors(target, next);
            }
        }
    }

    /// Les couleurs enregistrées dans le projet, ou pour l'utilisateur.
    pub fn saved_colors(&self, target: SaveTarget) -> SavedColors {
        match target {
            SaveTarget::Project => self.snapshot.tree.saved_colors().clone(),
            SaveTarget::User => self.user_colors.colors().clone(),
        }
    }

    /// Fait lire et écrire les couleurs de l'utilisateur dans `path` plutôt
    /// que dans le dossier de configuration — pour les tests, qui ne doivent
    /// pas toucher aux vraies préférences de la machine.
    pub fn use_user_colors_file(&mut self, path: std::path::PathBuf) {
        self.user_colors = UserColors::load_from(Some(path));
    }

    fn replace_saved_colors(&mut self, target: SaveTarget, next: SavedColors) {
        if next == self.saved_colors(target) {
            return;
        }
        match target {
            SaveTarget::Project => self.execute(Command::SetSavedColors { colors: next }),
            SaveTarget::User => {
                if let Err(err) = self.user_colors.replace(next) {
                    self.fail(format!(
                        "Couleurs enregistrées pour cette session seulement : écriture impossible ({err})"
                    ));
                }
            }
        }
    }

    /// Voir [`Action::OpenPlotSettings`]. Passer à un autre graphe valide
    /// d'abord le brouillon du précédent ; un graphe qui n'existe plus (ou
    /// n'est pas un graphe) laisse tout en l'état.
    fn open_plot_settings(&mut self, graph: NodeId, tab: PlotSettingsTab, axis: Option<PlotAxis>) {
        if self.plot_spec(graph).is_none() {
            return;
        }
        if self
            .plot_edit
            .as_ref()
            .is_some_and(|edit| edit.graph != graph)
        {
            self.commit_plot_edit();
            self.end_plot_macro();
            self.plot_edit = None;
        }
        self.begin_plot_macro();
        self.begin_plot_edit(graph);
        let current_axis = self
            .plot_settings
            .filter(|settings| settings.graph == graph)
            .map(|settings| settings.axis);
        self.plot_settings = Some(PlotSettings {
            graph,
            tab,
            axis: axis.or(current_axis).unwrap_or(PlotAxis::Bottom),
        });
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
    pub fn open_import(&mut self, book: impl Into<Option<NodeId>>) {
        let active_book = book.into();
        let store = plotyx_import::RecipeStore::load();
        let recipe = store
            .recipes()
            .first()
            .cloned()
            .unwrap_or_else(|| plotyx_import::ImportRecipe::new("Sans titre"));
        let save_as_name = recipe.name.clone();
        self.import_state = Some(ImportState {
            active_book,
            destination: active_book.map_or(
                ImportDestination::NewBookForAll,
                ImportDestination::ActiveBook,
            ),
            recipe,
            paths: Vec::new(),
            store,
            save_as_name,
        });
    }

    /// Traite un dépôt de fichiers ([`Action::DropFiles`]). Les `.plx` sont mis
    /// en file pour ouverture ([`AppState::take_projects_to_open`]). Les
    /// autres fichiers sont lus avec le premier modèle enregistré, à défaut la
    /// recette permissive ; ceux qui se lisent correctement partent dans un
    /// seul import annulable, les autres ouvrent la fenêtre d'import
    /// pré-remplie pour être réglés à la main.
    fn drop_files(&mut self, paths: Vec<PathBuf>, target_book: Option<NodeId>) {
        let plan = file_drop_plan(&paths, target_book);
        self.projects_to_open.extend(plan.projects);
        if plan.data.is_empty() {
            return;
        }
        let recipe = plotyx_import::RecipeStore::load()
            .recipes()
            .first()
            .cloned()
            .unwrap_or_else(|| plotyx_import::ImportRecipe::new("Sans titre"));
        let (good, bad): (Vec<PathBuf>, Vec<PathBuf>) = plan
            .data
            .into_iter()
            .partition(|path| reads_well(path, &recipe));
        if !good.is_empty() {
            let model = recipe.name.clone();
            self.import_files(plan.destination, recipe, good);
            let summary = self
                .status
                .as_ref()
                .filter(|status| !status.is_error)
                .map(|status| format!("{} — modèle « {model} »", status.text));
            if let Some(summary) = summary {
                self.notice(summary);
            }
        }
        if !bad.is_empty() {
            self.open_import(target_book);
            if let Some(import) = &mut self.import_state {
                import.paths = bad;
            }
        }
    }

    /// Projets `.plx` déposés depuis le dernier appel, à ouvrir chacun dans
    /// une nouvelle fenêtre PlotYx (vidés : un projet ne s'ouvre qu'une fois).
    pub fn take_projects_to_open(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.projects_to_open)
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

    /// Enregistre un message informatif. `pub(crate)` pour que la couche
    /// fenêtre puisse signaler ce qu'elle seule sait — un presse-papier
    /// système illisible au moment d'un « Coller » pris dans un menu
    /// contextuel (voir [`crate::app::system_clipboard_text`]) : cet échec
    /// naît côté plateforme, jamais dans le modèle.
    pub(crate) fn notice(&mut self, text: String) {
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
        // traversée (A9) — une cellule de saisie ne distingue jamais les
        // deux en entrée. Comparer au texte affiché
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
                        // Le bloc est texte : `SetCellsPromoting` élargira
                        // la colonne en `Mixed` pour l'accueillir.
                        Err(_) => (
                            ValueBlock::Mixed(vec![MixedCell::Text(trimmed.to_string())]),
                            true,
                        ),
                    }
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
        // Une colonne mixte dont toutes les valeurs présentes se sont
        // révélées numériques se convertit seule en numérique — pendant du
        // sens inverse déjà géré ci-dessus (`SetCellsPromoting` : une
        // colonne numérique qui reçoit du texte s'élargit en `Mixed`). Il
        // n'y a plus de type `Text` homogène à promouvoir : c'est désormais
        // une colonne mixte, saisie cellule après cellule, qui peut se
        // retrouver entièrement numérique sans qu'aucune de ses cellules
        // n'ait jamais été explicitement « nombre » — typiquement après
        // correction d'un « n/a » resté seul non-numérique.
        if kind == ColumnType::Mixed {
            self.auto_promote_mixed_column_if_all_numeric(column);
        }
        true
    }

    /// Convertit `column` (supposée `Mixed`) en `Numeric` si toutes ses
    /// cellules présentes sont des nombres — voir l'appel dans
    /// [`AppState::commit_cell`].
    fn auto_promote_mixed_column_if_all_numeric(&mut self, column: NodeId) {
        let Some(col) = self.snapshot.tree.node(column).and_then(Node::as_column) else {
            return;
        };
        let mut any_present = false;
        for row in 0..col.len() {
            match col.cell(row) {
                MixedCell::Empty => {}
                MixedCell::Number(_) => any_present = true,
                MixedCell::Text(_) => return,
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

    /// Vide le contenu des cellules ou des champs d'en-tête sélectionnés de
    /// `sheet` (jamais les deux à la fois, une sélection ne mélange plus les
    /// deux — voir la doc de tête de [`crate::selection`]), sans décaler ce
    /// qui suit (Retour arrière/Ctrl+X) — une seule `Command::SetCells` par
    /// colonne touchée pour une sélection de données, un
    /// `Command::SetColumnMeta` (via [`AppState::set_column_meta_field`])
    /// par champ touché pour une sélection d'en-tête (voir
    /// [`crate::selection::Selection::cleared_header_fields`]). Tout est
    /// groupé en une seule entrée d'historique dès que plus d'une opération
    /// est concernée (`History::begin_macro`/`end_macro`), pour qu'annuler
    /// restaure tout d'un coup.
    fn clear_selected_cells(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            return;
        };
        let tree = &self.snapshot.tree;
        let ranges = selection.clear_ranges(tree, sheet);
        let header_fields = selection.cleared_header_fields(tree, sheet);
        if ranges.is_empty() && header_fields.is_empty() {
            return;
        }
        let grouped = ranges.len() + header_fields.len() > 1;
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
        for (column, field) in header_fields {
            self.set_column_meta_field(column, |meta| match field {
                HeaderField::LongName => meta.long_name = String::new(),
                HeaderField::Unit => meta.unit = String::new(),
                HeaderField::Comment => meta.comment = String::new(),
                HeaderField::Formula => meta.formula = None,
            });
        }
        if grouped {
            let _ = self.handle.end_macro();
        }
    }

    /// Réagit à une pression sur Suppr en deux temps — voir [`AppState::
    /// delete_repeat`] pour l'état qui porte la mémoire d'une pression
    /// précédente. La première pression sur une sélection vide les cases
    /// ([`Action::ClearSelectedCells`], comme Retour arrière) ; la
    /// deuxième, *seulement* si elle vise exactement la même sélection de
    /// la même feuille sans qu'aucune autre mutation de la grille ne se
    /// soit glissée entre les deux (voir [`AppState::
    /// forget_delete_repeat`]), supprime pour de bon
    /// ([`Action::DeleteSelectedCells`]) et décale ce qui suit — jamais les
    /// deux à la première pression, pour ne jamais décaler des données par
    /// une simple habitude de tableur où Suppr vide sans décaler. `None` si
    /// `sheet` n'a pas de sélection : rien à faire.
    pub fn consume_delete_key(&mut self, sheet: NodeId) -> Option<Action> {
        let selection = self.selection.get(&sheet)?.clone();
        if self.delete_repeat.as_ref() == Some(&(sheet, selection.clone())) {
            self.delete_repeat = None;
            Some(Action::DeleteSelectedCells { sheet })
        } else {
            self.delete_repeat = Some((sheet, selection));
            Some(Action::ClearSelectedCells { sheet })
        }
    }

    /// Oublie une éventuelle double pression sur Suppr en cours pour
    /// `sheet` — à appeler avant toute autre mutation de la grille qui
    /// pourrait se glisser entre les deux temps de [`AppState::
    /// consume_delete_key`] (Couper, Retour arrière, coller...), pour
    /// qu'une pression sur Suppr qui suit reparte au premier temps (vider)
    /// plutôt que de supprimer par surprise sur une sélection qui n'a pas
    /// changé mais dont le contenu, lui, a bougé entre-temps.
    pub fn forget_delete_repeat(&mut self, sheet: NodeId) {
        if matches!(&self.delete_repeat, Some((s, _)) if *s == sheet) {
            self.delete_repeat = None;
        }
    }

    /// Vide le contenu des cellules sélectionnées de `sheet` **et** fait
    /// remonter les cellules suivantes de chaque colonne touchée,
    /// indépendamment (touche Suppr) — une seule `Command::DeleteCells`,
    /// déjà multi-colonnes.
    ///
    /// Sur une sélection entièrement dans l'en-tête, il n'y a rien à faire
    /// remonter (un champ n'est pas répété par ligne, voir
    /// [`crate::selection::Selection::delete_at_count`]) : Suppr y vide les
    /// champs, comme Retour arrière. Sans ça la touche resterait sans effet
    /// dans l'en-tête, alors qu'elle y vidait le champ avant l'unification
    /// du bloc clavier.
    fn delete_selected_cells(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            return;
        };
        let (columns, at, count) = selection.delete_at_count(&self.snapshot.tree, sheet);
        if columns.is_empty() || count == 0 {
            self.clear_selected_cells(sheet);
            return;
        }
        self.execute(Command::DeleteCells { columns, at, count });
    }

    /// Colle `text` à partir de l'ancre de la sélection courante de `sheet`
    /// — Ctrl+V ([`Action::Paste`]). Rejoue [`AppState::grid_clipboard`] si
    /// `text` est exactement ce qu'[`AppState::copy_selection`]/
    /// [`AppState::copy_full_columns`] a poussé au presse-papier système en
    /// dernier ([`AppState::clipboard_text`]) ; sinon `text` est un texte
    /// externe, reconstruit en [`GridClipboard`] par
    /// [`external_clipboard_from_text`]. Voir [`AppState::apply_paste`] pour
    /// la matrice de comportement complète.
    fn paste(&mut self, sheet: NodeId, text: String) {
        let Some(selection) = self.selection.get(&sheet) else {
            self.fail("Aucune cellule sélectionnée pour coller".to_string());
            return;
        };
        let Some(anchor) = selection.anchor(&self.snapshot.tree, sheet) else {
            self.fail("Aucune cellule sélectionnée pour coller".to_string());
            return;
        };
        let clipboard = if self.clipboard_text.as_deref() == Some(text.as_str()) {
            self.grid_clipboard.clone()
        } else {
            None
        }
        .unwrap_or_else(|| external_clipboard_from_text(&text));
        self.apply_paste(sheet, anchor, &clipboard, false);
    }

    /// Colle en position absolue depuis [`AppState::grid_clipboard`] —
    /// Ctrl+Maj+V ([`Action::PasteAbsolute`]), qui n'a jamais de texte à lui
    /// (voir la doc de l'action). Sans effet si rien n'a encore été copié
    /// dans cette session : un texte externe n'a pas de rang d'origine à
    /// rejouer.
    fn paste_absolute(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            self.fail("Aucune cellule sélectionnée pour coller".to_string());
            return;
        };
        let Some(anchor) = selection.anchor(&self.snapshot.tree, sheet) else {
            self.fail("Aucune cellule sélectionnée pour coller".to_string());
            return;
        };
        let Some(clipboard) = self.grid_clipboard.clone() else {
            self.notice("Rien à coller en absolu : copiez d'abord depuis la grille".to_string());
            return;
        };
        self.apply_paste(sheet, anchor, &clipboard, true);
    }

    /// Cœur du collage, partagé par [`AppState::paste`] (`absolute: false`)
    /// et [`AppState::paste_absolute`] (`absolute: true`) — la matrice de
    /// comportement complète (section copier/coller en tête de ce module) :
    ///
    /// - [`GridClipboard::Text`] s'écrit dans la case ancrée, quelle qu'elle
    ///   soit (champ d'en-tête ou cellule de donnée) — `absolute` n'a pas de
    ///   sens pour une case unique, ignoré ;
    /// - [`GridClipboard::Relative`] se décale sur l'ancre si elle est de la
    ///   même sorte (champs sur un champ, données sur une cellule) ; rien ne
    ///   se colle si l'ancre est de l'autre sorte ; en absolu, chaque rangée
    ///   retombe sur son rang d'origine plutôt que sur l'ancre ;
    /// - [`GridClipboard::FullColumns`] colle l'en-tête à partir du champ
    ///   ancré (rien au-dessus) et le corps sans décalage si l'ancre est un
    ///   champ ; jette l'en-tête et décale le corps sur l'ancre si elle est
    ///   une cellule ; en absolu, tout retombe sur son rang d'origine quelle
    ///   que soit l'ancre.
    ///
    /// Une colonne formule refuse naturellement l'écriture de données
    /// (`CoreError::FormulaColumnReadOnly`) : signalée par le statut, sans
    /// bloquer les autres colonnes déjà collées. Un collage qui touche
    /// plusieurs colonnes ou plusieurs champs reste groupé dans une seule
    /// entrée d'historique.
    fn apply_paste(
        &mut self,
        sheet: NodeId,
        anchor: GridCursor,
        clipboard: &GridClipboard,
        absolute: bool,
    ) {
        let order = self.column_order(sheet);
        let anchor_column = grid_cursor_column(anchor);
        let Some(anchor_index) = order.iter().position(|&id| id == anchor_column) else {
            self.fail(format!(
                "{anchor_column} n'est pas une colonne de cette feuille"
            ));
            return;
        };
        let order_from_anchor = &order[anchor_index..];
        match clipboard {
            GridClipboard::Text(text) => self.paste_single_value(anchor, text),
            GridClipboard::Relative(relative) => {
                self.paste_relative(anchor, order_from_anchor, relative, absolute)
            }
            GridClipboard::FullColumns(columns) => {
                self.paste_full_columns(anchor, order_from_anchor, columns, absolute)
            }
        }
    }

    /// [`GridClipboard::Text`] : écrit `text` dans la case ancrée, champ
    /// d'en-tête ou cellule de donnée selon `anchor` — une seule commande,
    /// jamais groupée (un seul champ/une seule cellule touché).
    fn paste_single_value(&mut self, anchor: GridCursor, text: &str) {
        match anchor {
            GridCursor::Header(column, field) => {
                let text = text.to_string();
                self.set_column_meta_field(column, move |meta| match field {
                    HeaderField::LongName => meta.long_name = text,
                    HeaderField::Unit => meta.unit = text,
                    HeaderField::Comment => meta.comment = text,
                    HeaderField::Formula => {
                        meta.formula = (!text.trim().is_empty()).then_some(text)
                    }
                });
            }
            GridCursor::Data(column, row) => {
                let Some(kind) = self
                    .snapshot
                    .tree
                    .node(column)
                    .and_then(Node::as_column)
                    .map(Column::column_type)
                else {
                    return;
                };
                let (values, promoting) =
                    value_block_from_tsv_column(kind, &[vec![text.to_string()]], 0);
                self.execute_pasted_cells(column, row, values, promoting);
            }
        }
    }

    /// Écrit un bloc de cellules collées à partir de [`value_block_from_tsv_column`]
    /// — `Command::SetCellsPromoting` plutôt que `Command::SetCells` quand
    /// `promoting` demande d'élargir la colonne en `Mixed` (voir la doc de
    /// [`value_block_from_tsv_column`]), sinon même commande stricte
    /// qu'avant. Partagé par les trois points de collage en cellules de
    /// données ([`AppState::paste_single_value`], [`AppState::
    /// paste_relative`], [`AppState::paste_full_columns`]).
    fn execute_pasted_cells(
        &mut self,
        column: NodeId,
        start: usize,
        values: ValueBlock,
        promoting: bool,
    ) {
        self.execute(if promoting {
            Command::SetCellsPromoting {
                column,
                start,
                values,
                expected_version: None,
            }
        } else {
            Command::SetCells {
                column,
                start,
                values,
                expected_version: None,
            }
        });
    }

    /// [`GridClipboard::Relative`] : voir [`AppState::apply_paste`] pour la
    /// règle. Rien ne se colle si `anchor` n'est pas de la même sorte que
    /// `relative` (champs contre cellule de donnée, ou l'inverse).
    fn paste_relative(
        &mut self,
        anchor: GridCursor,
        order_from_anchor: &[NodeId],
        relative: &RelativeCopy,
        absolute: bool,
    ) {
        match (anchor, relative) {
            (GridCursor::Header(_, anchor_field), RelativeCopy::HeaderFields { start, rows }) => {
                let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
                let targets: Vec<NodeId> = order_from_anchor
                    .iter()
                    .copied()
                    .take(column_count)
                    .collect();
                if targets.is_empty() {
                    return;
                }
                let shift = if absolute {
                    0
                } else {
                    anchor_field.index() as isize - start.index() as isize
                };
                let writes: Vec<(HeaderField, &Vec<String>)> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, cells)| {
                        let origin = start.index() as isize + i as isize;
                        let target = origin + shift;
                        (0..=3)
                            .contains(&target)
                            .then(|| (HeaderField::from_index(target as usize), cells))
                    })
                    .collect();
                let grouped = writes.len() * targets.len() > 1;
                if grouped && self.handle.begin_macro("Coller").is_err() {
                    self.fail("Impossible de grouper le collage".to_string());
                    return;
                }
                for &(field, cells) in &writes {
                    for (index, &column) in targets.iter().enumerate() {
                        let text = cells.get(index).cloned().unwrap_or_default();
                        self.set_column_meta_field(column, move |meta| match field {
                            HeaderField::LongName => meta.long_name = text,
                            HeaderField::Unit => meta.unit = text,
                            HeaderField::Comment => meta.comment = text,
                            HeaderField::Formula => {
                                meta.formula = (!text.trim().is_empty()).then_some(text)
                            }
                        });
                    }
                }
                if grouped {
                    let _ = self.handle.end_macro();
                }
            }
            (GridCursor::Data(_, anchor_row), RelativeCopy::DataRows { start, rows }) => {
                let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
                let targets: Vec<NodeId> = order_from_anchor
                    .iter()
                    .copied()
                    .take(column_count)
                    .collect();
                if targets.is_empty() || rows.is_empty() {
                    return;
                }
                let paste_row = if absolute { *start } else { anchor_row };
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
                    let (values, promoting) = value_block_from_tsv_column(kind, rows, index);
                    self.execute_pasted_cells(column, paste_row, values, promoting);
                }
                if grouped {
                    let _ = self.handle.end_macro();
                }
            }
            // Champs copiés + ancre sur une cellule de donnée, ou lignes de
            // données copiées + ancre sur un champ d'en-tête : sortes
            // différentes, rien ne se colle (voir la matrice de
            // comportement en tête de ce module).
            _ => {}
        }
    }

    /// [`GridClipboard::FullColumns`] : voir [`AppState::apply_paste`] pour
    /// la règle — dépend de `anchor` (champ ou cellule) et de `absolute`.
    fn paste_full_columns(
        &mut self,
        anchor: GridCursor,
        order_from_anchor: &[NodeId],
        columns: &[FullColumnCopy],
        absolute: bool,
    ) {
        let targets: Vec<NodeId> = order_from_anchor
            .iter()
            .copied()
            .take(columns.len())
            .collect();
        if targets.is_empty() {
            return;
        }
        // Champ de départ de l'en-tête à écrire : tous en absolu, à partir
        // du champ ancré (rien au-dessus) en relatif si l'ancre est un
        // champ, aucun (en-tête jeté) en relatif si l'ancre est une cellule.
        let header_from: Option<usize> = if absolute {
            Some(0)
        } else {
            match anchor {
                GridCursor::Header(_, field) => Some(field.index()),
                GridCursor::Data(..) => None,
            }
        };
        // Ligne de départ du corps : jamais décalé, sauf en relatif ancré
        // sur une cellule de donnée (décalage sur l'ancre).
        let body_start = match (absolute, anchor) {
            (false, GridCursor::Data(_, row)) => row,
            _ => 0,
        };
        let header_ops = header_from.map(|from| 4 - from).unwrap_or(0) * targets.len();
        let body_ops = targets
            .iter()
            .zip(columns)
            .filter(|(_, c)| !c.data.is_empty())
            .count();
        let grouped = header_ops + body_ops > 1;
        if grouped && self.handle.begin_macro("Coller").is_err() {
            self.fail("Impossible de grouper le collage".to_string());
            return;
        }
        if let Some(from) = header_from {
            for field_index in from..=3 {
                let field = HeaderField::from_index(field_index);
                for (&column, source) in targets.iter().zip(columns) {
                    let value = match field {
                        HeaderField::LongName => source.long_name.clone(),
                        HeaderField::Unit => source.unit.clone(),
                        HeaderField::Comment => source.comment.clone(),
                        HeaderField::Formula => source.formula.clone().unwrap_or_default(),
                    };
                    self.set_column_meta_field(column, move |meta| match field {
                        HeaderField::LongName => meta.long_name = value,
                        HeaderField::Unit => meta.unit = value,
                        HeaderField::Comment => meta.comment = value,
                        HeaderField::Formula => {
                            meta.formula = (!value.trim().is_empty()).then_some(value)
                        }
                    });
                }
            }
        }
        for (&column, source) in targets.iter().zip(columns) {
            if source.data.is_empty() {
                continue;
            }
            let Some(kind) = self
                .snapshot
                .tree
                .node(column)
                .and_then(Node::as_column)
                .map(Column::column_type)
            else {
                continue;
            };
            let rows: Vec<Vec<String>> = source.data.iter().map(|v| vec![v.clone()]).collect();
            let (values, promoting) = value_block_from_tsv_column(kind, &rows, 0);
            self.execute_pasted_cells(column, body_start, values, promoting);
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

    /// Nouveau classeur de l'utilisateur (menu Fichier) : sa première feuille
    /// reçoit déjà une colonne X et une colonne Y, en une seule entrée
    /// d'historique. L'import, lui, crée ses classeurs vides
    /// (`create_import_book`) : c'est le fichier qui fournit les colonnes.
    fn add_workbook(&mut self) {
        let root = self.snapshot.tree.root();
        if !self.begin_creation_macro("Ajouter un classeur") {
            return;
        }
        let previous = self
            .snapshot
            .tree
            .node(root)
            .and_then(|node| node.children().last().copied());
        self.execute(Command::AddWorkbook {
            parent: root,
            name: None,
        });
        let book = self
            .snapshot
            .tree
            .node(root)
            .and_then(|node| node.children().last().copied())
            .filter(|book| Some(*book) != previous);
        if let Some(sheet) =
            book.and_then(|book| self.worksheets_of(book).first().map(|(id, _)| *id))
        {
            self.add_xy_columns(sheet);
        }
        self.end_creation_macro();
    }

    /// Nouvelle feuille de l'utilisateur : comme [`AppState::add_workbook`],
    /// elle naît avec une colonne X et une colonne Y.
    fn add_worksheet(&mut self, book: NodeId) {
        if !self.begin_creation_macro("Ajouter une feuille") {
            return;
        }
        let previous = self
            .snapshot
            .tree
            .node(book)
            .and_then(|node| node.children().last().copied());
        self.execute(Command::AddWorksheet { book, name: None });
        let sheet = self
            .snapshot
            .tree
            .node(book)
            .and_then(|node| node.children().last().copied())
            .filter(|sheet| Some(*sheet) != previous);
        if let Some(sheet) = sheet {
            self.add_xy_columns(sheet);
        }
        self.end_creation_macro();
    }

    fn begin_creation_macro(&mut self, label: &str) -> bool {
        if self.handle.begin_macro(label).is_err() {
            self.fail(
                "Impossible de grouper la création dans une seule entrée d'historique".to_string(),
            );
            return false;
        }
        true
    }

    fn end_creation_macro(&mut self) {
        if self.handle.end_macro().is_err() {
            self.fail("échec de la clôture de l'entrée d'historique".to_string());
        }
        self.refresh();
    }

    /// Ajoute à `sheet` une colonne X (rôle abscisse) puis une colonne Y.
    fn add_xy_columns(&mut self, sheet: NodeId) {
        self.add_column(sheet);
        self.add_column(sheet);
        let columns = self.visible_columns(sheet);
        if let Some((x, ..)) = columns.first() {
            let mut meta = self.column_meta(*x);
            meta.role = ColumnRole::X;
            self.execute(Command::SetColumnMeta { column: *x, meta });
        }
    }

    /// Ajoute une colonne en fin de `sheet` puis y sélectionne `row` — voir
    /// [`Action::AddColumnAndSelect`]. `self.execute` a déjà rafraîchi
    /// `self.snapshot` en retour : la nouvelle colonne, dernière des
    /// colonnes visibles, existe donc déjà pour `select_cell`.
    fn add_column_and_select(&mut self, sheet: NodeId, row: usize) {
        self.add_column(sheet);
        if let Some((column, ..)) = self.visible_columns(sheet).into_iter().next_back() {
            self.select_cell(sheet, column, row);
        }
    }

    /// Ajoute une colonne en fin de `sheet` puis y sélectionne `field` —
    /// voir [`Action::AddColumnAndSelectHeaderField`], pendant de
    /// [`AppState::add_column_and_select`] côté en-tête.
    fn add_column_and_select_header_field(&mut self, sheet: NodeId, field: HeaderField) {
        self.add_column(sheet);
        if let Some((column, ..)) = self.visible_columns(sheet).into_iter().next_back() {
            self.select_header_field(sheet, column, field);
        }
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

    /// Importe chaque fichier de `paths` dans sa propre nouvelle feuille,
    /// selon `destination`, avec `recipe` — [`Action::ImportFiles`]. Un fichier illisible
    /// ou inanalysable devient un [`StatusMessage`] et fait passer au
    /// suivant, jamais un abandon complet du lot. Tout le lot tient dans une
    /// seule entrée d'historique : `self.handle.begin_macro`/`end_macro`
    /// encadrent toute la boucle, même si un seul fichier réussit — voir
    /// `History::begin_macro`/`end_macro` (`plotyx-commands/src/
    /// history.rs`) pour l'absorption d'une macro qui finirait vide.
    fn import_files(
        &mut self,
        destination: ImportDestination,
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
        let mut shared_book = None;
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
            let book = match destination {
                ImportDestination::ActiveBook(book) => book,
                ImportDestination::NewBookPerFile | ImportDestination::NewBookForAll => {
                    if destination == ImportDestination::NewBookForAll {
                        if let Some(book) = shared_book {
                            book
                        } else {
                            let Some(book) = self.create_import_book(None) else {
                                failures.push(format!(
                                    "{}: échec de la création du classeur",
                                    path.display()
                                ));
                                continue;
                            };
                            shared_book = Some(book);
                            book
                        }
                    } else {
                        let Some(book) = self.create_import_book(Some(file_stem)) else {
                            failures.push(format!(
                                "{}: échec de la création du classeur",
                                path.display()
                            ));
                            continue;
                        };
                        book
                    }
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
            let previous_sheet = self
                .snapshot
                .tree
                .node(book)
                .and_then(|node| node.children().last().copied());
            self.execute(Command::AddWorksheet { book, name: None });
            let Some(sheet) = self
                .snapshot
                .tree
                .node(book)
                .and_then(|node| node.children().last().copied())
                .filter(|sheet| Some(*sheet) != previous_sheet)
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
                    // La première colonne importée est l'abscisse ; la
                    // désignation fine relève de l'étape « Colonnes ».
                    role: if index == 0 {
                        ColumnRole::X
                    } else {
                        ColumnRole::default()
                    },
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

    /// `AddEmptyWorkbook` évite la feuille « Sheet1 » que `AddWorkbook`
    /// ajouterait en plus des feuilles importées.
    fn create_import_book(&mut self, file_stem: Option<&str>) -> Option<NodeId> {
        let root = self.snapshot.tree.root();
        let previous = self.snapshot.tree.node(root)?.children().last().copied();
        self.execute(Command::AddEmptyWorkbook {
            parent: root,
            name: None,
        });
        let book = self.snapshot.tree.node(root)?.children().last().copied()?;
        if Some(book) == previous {
            return None;
        }
        if let Some(stem) = file_stem {
            self.execute(Command::Rename {
                id: book,
                name: stem.to_string(),
            });
        }
        Some(book)
    }

    /// Crée un graphe à partir des colonnes sélectionnées de `sheet` — voir
    /// [`Action::PlotColumns`]. Le graphe est ajouté dans le dossier qui
    /// contient le classeur de `sheet` (le parent du classeur, pas de la
    /// feuille). Pas de commande envoyée si aucune colonne Y n'est présente
    /// dans `columns` : un graphe vide n'a pas de sens à créer, et n'a donc
    /// pas d'entrée d'historique à annuler.
    fn plot_columns(&mut self, sheet: NodeId, columns: Vec<NodeId>, plot_type: PlotType) {
        let series = series_from_columns(&self.snapshot.tree, sheet, &columns, &plot_type, 0);
        if series.is_empty() {
            self.fail("Aucune colonne de rôle Y dans la sélection : rien à tracer".to_string());
            return;
        }
        let Some(book) = self.snapshot.tree.node(sheet).and_then(Node::parent) else {
            self.fail(format!("{sheet} n'est pas une feuille de ce projet"));
            return;
        };
        let Some(parent) = self.snapshot.tree.node(book).and_then(Node::parent) else {
            self.fail(format!("{book} n'a pas de dossier parent"));
            return;
        };
        let panel = PanelSpec {
            series,
            ..PanelSpec::default()
        };
        let spec = PlotSpec {
            panels: Panels::one(panel),
            ..PlotSpec::default()
        };
        self.execute(Command::AddGraph {
            parent,
            name: None,
            spec,
        });
    }

    /// Ajoute des séries construites depuis `columns` au premier panneau du
    /// `PlotSpec` courant de `graph` — voir [`Action::AddSeriesToGraph`].
    /// Même règle d'appariement X/Y que [`AppState::plot_columns`], mais la
    /// feuille d'où viennent les colonnes se déduit de leur propre parent
    /// (le classeur ouvert n'est pas porté par cette action) : toutes les
    /// colonnes de `columns` doivent donc appartenir à la même feuille. Les
    /// nouvelles séries prolongent la palette après celles déjà présentes,
    /// sans toucher aux séries existantes. Pas de commande envoyée si
    /// `columns` est vide, si les colonnes n'ont pas de feuille commune, si
    /// aucune n'est de rôle Y, ou si `graph` n'est pas un graphe de ce
    /// projet.
    fn add_series_to_graph(&mut self, graph: NodeId, columns: Vec<NodeId>, plot_type: PlotType) {
        let Some(&first) = columns.first() else {
            return;
        };
        let Some(sheet) = self.snapshot.tree.node(first).and_then(Node::parent) else {
            self.fail(format!("{first} n'est pas une colonne de ce projet"));
            return;
        };
        let Some(mut spec) = current_plot_spec(&self.snapshot.tree, graph) else {
            self.fail(format!("{graph} n'est pas un graphe de ce projet"));
            return;
        };
        let start_index = spec.panels.first().map_or(0, |panel| panel.series.len());
        let new_series = series_from_columns(
            &self.snapshot.tree,
            sheet,
            &columns,
            &plot_type,
            start_index,
        );
        if new_series.is_empty() {
            self.fail("Aucune colonne de rôle Y dans la sélection : rien à ajouter".to_string());
            return;
        }
        let Some(panel) = spec.panels.get_mut(0) else {
            self.fail(format!("{graph} n'a aucun panneau"));
            return;
        };
        panel.series.extend(new_series);
        self.execute(Command::SetPlotSpec { graph, spec });
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

    use super::{
        external_clipboard_from_text, format_f64_for_display, grid_clipboard_to_text,
        GridClipboard, HeaderField, RelativeCopy, MISSING_TEXT_DISPLAY,
    };

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

    /// [`external_clipboard_from_text`] — règle 1 de la matrice de
    /// comportement : un texte externe sans tabulation ni retour à la ligne
    /// devient un [`GridClipboard::Text`], quel que soit son contenu (y
    /// compris vide).
    #[test]
    fn external_clipboard_from_text_texte_simple_devient_text() {
        assert_eq!(
            external_clipboard_from_text("Capteur 1"),
            GridClipboard::Text("Capteur 1".to_string())
        );
        assert_eq!(
            external_clipboard_from_text(""),
            GridClipboard::Text(String::new())
        );
    }

    /// Un texte externe tabulé (même sans retour à la ligne) ou multi-lignes
    /// devient un rectangle relatif de données, parti de la ligne 0 — jamais
    /// de champ d'en-tête depuis un texte externe.
    #[test]
    fn external_clipboard_from_text_texte_tabule_ou_multiligne_devient_des_donnees() {
        assert_eq!(
            external_clipboard_from_text("0\t10\n0.5\t\n"),
            GridClipboard::Relative(RelativeCopy::DataRows {
                start: 0,
                rows: vec![
                    vec!["0".to_string(), "10".to_string()],
                    vec!["0.5".to_string(), String::new()],
                ],
            })
        );
        assert_eq!(
            external_clipboard_from_text("a\tb"),
            GridClipboard::Relative(RelativeCopy::DataRows {
                start: 0,
                rows: vec![vec!["a".to_string(), "b".to_string()]],
            })
        );
    }

    /// [`grid_clipboard_to_text`] — le texte poussé au presse-papier système
    /// ne porte plus jamais de caractère de contrôle (contrairement à
    /// l'ancien `HEADER_LINE_MARKER`) : une copie de champs d'en-tête se lit
    /// comme un TSV ordinaire.
    #[test]
    fn grid_clipboard_to_text_ne_contient_aucun_caractere_de_controle() {
        let clipboard = GridClipboard::Relative(RelativeCopy::HeaderFields {
            start: HeaderField::Unit,
            rows: vec![vec!["nm".to_string()], vec!["un commentaire".to_string()]],
        });
        let text = grid_clipboard_to_text(&clipboard);
        assert_eq!(text, "nm\nun commentaire\n");
        assert!(text.chars().all(|c| !c.is_control() || c == '\n'));
    }
}
