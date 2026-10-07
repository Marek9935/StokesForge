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
//!    [`AppState::new`] ne construit ni ne touche à `stokesforge_script::
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
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use stokesforge_commands::{Command, HistoryLabel, MacroKind, PlotEditKind};
use stokesforge_core::node::NodeKind;
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, Node, NodeCopy, NodeId, Tree,
    ValueBlock,
};
use stokesforge_importer::{import_items, plan_import, ImportItem, ImportPlan, PlannedColumn};
use stokesforge_plot::{
    AnnotationRef, AnnotationSpec, ColorUsage, PanelSpec, Panels, PlotSpec, PlotType, SavedColors,
    SeriesRef, ShapeRef, ShapeSpec, TextAnchor, TextTarget,
};
#[cfg(feature = "python")]
use stokesforge_python::interpreter::EmbeddedPython;
#[cfg(feature = "python")]
use stokesforge_script::{ScriptEngine, ScriptError, ScriptOutcome};
use stokesforge_session::{SessionHandle, SessionResult, Snapshot};

use crate::errors;
use crate::history_text::history_text;
use crate::preferences::{ObjectManagerGroup, ObjectManagerSettings, Preferences};
use crate::selection::{GridCursor, Selection};
use crate::tr;
use crate::user_colors::UserColors;

/// Ce que l'utilisateur peut demander. La couche fenêtre ne fait que produire
/// ces valeurs ; elle ne touche jamais l'acteur directement.
// `ImportFiles` porte une recette d'import entière, bien plus grosse que les
// autres variantes : l'`Action` est consommée aussitôt, le boîtage n'y gagnerait rien.
#[allow(clippy::large_enum_variant)]
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
    /// Demande un projet vide ; attend une confirmation si du travail serait perdu.
    NewProject {
        /// Saisie encore locale à une vue, notamment une fenêtre détachée.
        pending_edits: bool,
    },
    /// Valide ou abandonne la demande de remplacement du projet.
    ConfirmNewProject(bool),
    /// Ajoute un graphique vide à la racine, annulable comme un tracé.
    AddGraph,
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
    /// Passe l'onglet de `sheet` en édition de son nom (double-clic, F2, menu
    /// contextuel) et la sélectionne.
    BeginRenameSheet(NodeId),
    /// Valide le nom saisi pour `sheet`. Nom vide ou déjà pris dans le
    /// classeur : l'ancien nom reste, un message l'explique, aucune entrée
    /// d'historique.
    CommitRenameSheet {
        /// Feuille renommée.
        sheet: NodeId,
        /// Nom saisi.
        name: String,
    },
    /// Abandonne l'édition du nom d'une feuille (Échap).
    CancelRenameSheet,
    /// Duplique `sheet` (colonnes comprises) juste après elle, et affiche la
    /// copie.
    DuplicateSheet(NodeId),
    /// Demande la suppression de `sheet` : ouvre la confirmation, qui nomme
    /// les graphes qui en tracent des colonnes. Refusée pour la dernière
    /// feuille d'un classeur.
    RequestDeleteSheet(NodeId),
    /// Réponse à la confirmation de [`Action::RequestDeleteSheet`].
    ConfirmDeleteSheet(bool),
    /// Renomme un objet du Project Manager après édition en place.
    RenameObject { id: NodeId, name: String },
    /// Valide tous les noms saisis pendant un seul geste de renommage.
    RenameObjects(Vec<(NodeId, String)>),
    /// Copie la sélection vers le transport système.
    CopyObjects(Vec<NodeId>),
    /// Duplique la sélection dans ses dossiers d'origine.
    DuplicateObjects(Vec<NodeId>),
    /// Colle le transport système dans un dossier.
    PasteObjects { parent: NodeId, text: String },
    /// Révèle les classeurs embarqués référencés par ce graphique.
    ShowGraphData { graph: NodeId },
    /// Révèle le classeur des colonnes d'une seule courbe (menu de la courbe,
    /// #152) : pas de choix ambigu entre les courbes d'un graphique.
    ShowSeriesData {
        graph: NodeId,
        series: stokesforge_plot::SeriesRef,
    },
    /// Demande la suppression de la sélection.
    DeleteObjects(Vec<NodeId>),
    /// Répond à la confirmation de suppression.
    ConfirmDeleteObjects(bool),
    /// Déplace la sélection en un seul geste annulable.
    MoveObjects { ids: Vec<NodeId>, parent: NodeId },
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
    /// `stokesforge-core` (`Command::AddWorksheet { name: None, .. }`, même
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
    /// set_column_type`, `stokesforge-core`) : un texte non numérique dans une
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
    /// Change le type de toutes les `columns` en **un seul pas d'annulation**
    /// (menu des colonnes sur une sélection multiple, #159).
    SetColumnsType {
        /// Colonnes visées.
        columns: Vec<NodeId>,
        /// Nouveau type.
        column_type: ColumnType,
    },
    /// Change le rôle de toutes les `columns` en un seul pas d'annulation.
    SetColumnsRole {
        /// Colonnes visées.
        columns: Vec<NodeId>,
        /// Nouveau rôle.
        role: ColumnRole,
    },
    /// Sélectionne toutes les colonnes visibles de `sheet`, en-tête compris,
    /// même sans ligne — Ctrl+A dans la grille, Edit › Select All et case du
    /// coin (#159). État d'interface pur : ni modèle ni historique.
    SelectAll {
        /// Feuille visée.
        sheet: NodeId,
    },
    /// Remplace le nom long affiché d'une colonne
    /// ([`stokesforge_core::ColumnMeta::long_name`]).
    SetColumnLongName {
        /// Colonne visée.
        column: NodeId,
        /// Nouveau nom long.
        text: String,
    },
    /// Remplace l'unité affichée d'une colonne
    /// ([`stokesforge_core::ColumnMeta::unit`]).
    SetColumnUnit {
        /// Colonne visée.
        column: NodeId,
        /// Nouvelle unité.
        text: String,
    },
    /// Remplace le commentaire affiché d'une colonne
    /// ([`stokesforge_core::ColumnMeta::comment`]).
    SetColumnComment {
        /// Colonne visée.
        column: NodeId,
        /// Nouveau commentaire.
        text: String,
    },
    /// Remplace la formule affichée d'une colonne
    /// ([`stokesforge_core::ColumnMeta::formula`]) — stockage/affichage
    /// seulement, l'évaluation reste celle de `stokesforge_core::formula` /
    /// `stokesforge-session::actor`, hors du périmètre de cette action.
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
        recipe: stokesforge_import::ImportRecipe,
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
    /// Comme [`Action::ImportFiles`], mais l'analyse des fichiers se fait sur
    /// un fil de fond, avec progression : l'interface ne se fige pas. Les
    /// feuilles sont créées à la fin ([`AppState::poll_import`]).
    StartImport {
        /// Destination des feuilles produites.
        destination: ImportDestination,
        /// Réglages d'analyse à appliquer à chaque fichier.
        recipe: stokesforge_import::ImportRecipe,
        /// Fichiers à importer.
        paths: Vec<std::path::PathBuf>,
    },
    /// Reprend le dernier import dans l'assistant, pour le corriger : s'il est
    /// encore la dernière entrée de l'historique, il est d'abord annulé.
    ReopenLastImport,
    /// Actualise les colonnes importées de `sheet` depuis leurs fichiers source
    /// (relus avec la recette mémorisée), **en place** : les colonnes gardent
    /// leur identité, donc les graphes qui les tracent restent valides.
    RefreshFromSource {
        /// Feuille à actualiser.
        sheet: NodeId,
    },
    /// Ouvre la fenêtre (lecture seule) des métadonnées de fichier de
    /// `column` — clic sur la ligne « Métadonnées » de l'en-tête.
    ViewColumnMetadata {
        /// Colonne dont on consulte les métadonnées.
        column: NodeId,
    },
    /// Ferme la fenêtre des métadonnées.
    CloseColumnMetadata,
    /// Crée un graphe à partir des colonnes sélectionnées d'une feuille de
    /// classeur — menu contextuel de l'en-tête de colonne. Le nouveau graphe
    /// est ajouté dans le même dossier parent que le classeur ([`Command::
    /// AddGraph`]), avec une série par colonne de rôle
    /// [`stokesforge_core::ColumnRole::Y`] parmi `columns`, appariée à la colonne
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
    /// [`Action::PlotColumns`], mais en relisant le [`stokesforge_plot::PlotSpec`]
    /// courant du graphe plutôt qu'en en créant un nouveau
    /// ([`Command::SetPlotSpec`]). Sert le glisser d'une sélection de colonnes
    /// vers un graphe (#160) : un seul pas d'annulation.
    AddSeriesToGraph {
        /// Graphe visé.
        graph: NodeId,
        /// Calque visé, par son rang dans le graphe. Le dépôt passe le calque
        /// actif du graphe (#161) ; en attendant, le premier.
        panel: usize,
        /// Colonnes à ajouter en séries.
        columns: Vec<NodeId>,
        /// Type de tracé demandé pour chaque nouvelle série ; `None` prend
        /// celui du calque visé (voir [`panel_plot_type`]).
        plot_type: Option<PlotType>,
    },
    /// Ouvre ou ferme le panneau ; préférence de la personne, hors historique.
    SetObjectManagerOpen(bool),
    /// Bascule unique « Show graph texts » (menu View et menu contextuel).
    SetObjectManagerShowTexts(bool),
    /// Liste ou cache les échelles (axes) dans l'Object Manager (#111).
    SetObjectManagerShowScales(bool),
    /// Repli initial d'une famille, hors projet et historique (#139).
    SetObjectManagerGroupOpen {
        group: ObjectManagerGroup,
        open: bool,
    },
    /// Rétablit les valeurs initiales sans effacer les replis courants.
    ResetObjectManagerSettings,
    /// Visibilité d'un contenu ou d'un ensemble de contenus (#81–82).
    SetPlotVisibility {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
        visible: bool,
    },
    /// Retire un contenu du graphe par la même voie depuis menu et arbre.
    RemovePlotObject {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
    },
    /// Déplace une série au rang final dans son calque.
    ReorderSeries {
        graph: NodeId,
        source: stokesforge_plot::SeriesRef,
        index: usize,
    },
    /// Propriétés sur la série ou le calque exacts.
    OpenObjectProperties {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
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
    /// Exporte un graphe en SVG, PDF ou PNG (issue #34), le format lu sur
    /// l'extension de `path` (`.svg` si elle manque).
    ///
    /// `spec` est le spec **déjà cadré** par la vue — le zoom courant,
    /// `GraphView::framed_spec` : l'export montre ce qu'on voit. Le modèle
    /// n'est pas touché, donc aucune commande (un zoom ne s'annule pas, un
    /// export non plus).
    ExportGraph {
        /// Fichier à écrire.
        path: PathBuf,
        /// Spec cadré à exporter.
        spec: Box<PlotSpec>,
    },
    /// Copie le graphe vers une application externe (issue #149) : SVG, PNG
    /// puis PDF de la **même** figure, offerts ensemble au presse-papiers
    /// système — c'est l'application destinataire qui choisit.
    ///
    /// Comme [`Action::ExportGraph`], `spec` est le spec **déjà cadré** par la
    /// vue (`GraphView::framed_spec`) et le modèle n'est pas touché : ni
    /// commande, ni historique. L'action prépare les octets
    /// ([`AppState::take_figure_clipboard`]) ; c'est la fenêtre qui les offre
    /// au système, parce qu'elle seule connaît son affichage.
    CopyGraphFigure {
        /// Spec cadré à copier.
        spec: Box<PlotSpec>,
    },
    /// Règle l'agrandissement des exports PNG ([`PNG_EXPORT_SCALES`]).
    SetPngExportScale(f32),
    /// Pose un texte libre (issue #35) : remplace celui de `target`, ou
    /// l'ajoute si `target` est le rang qui suit le dernier
    /// ([`AnnotationRef::set`]). **Une** modification du modèle, donc une
    /// entrée d'historique — quel que soit le nombre d'images qu'a duré
    /// l'édition sur place ou le glisser, dont l'aperçu vit dans la vue.
    SetAnnotation {
        /// Graphe visé.
        graph: NodeId,
        /// Adresse du texte.
        target: AnnotationRef,
        /// Sa nouvelle version.
        annotation: Box<AnnotationSpec>,
    },
    /// Retire un texte libre (Suppr, menu contextuel).
    RemoveAnnotation {
        /// Graphe visé.
        graph: NodeId,
        /// Adresse du texte.
        target: AnnotationRef,
    },
    /// Pose un objet graphique (issue #76) : remplace celui de `target`, ou
    /// l'ajoute si `target` est le rang qui suit le dernier
    /// ([`ShapeRef::set`]). Comme [`Action::SetAnnotation`], **une** entrée
    /// d'historique quel que soit le nombre d'images du geste.
    SetShape {
        /// Graphe visé.
        graph: NodeId,
        /// Adresse de l'objet.
        target: ShapeRef,
        /// Sa nouvelle version.
        shape: Box<ShapeSpec>,
    },
    /// Retire un objet graphique (Suppr, menu).
    RemoveShape {
        /// Graphe visé.
        graph: NodeId,
        /// Adresse de l'objet.
        target: ShapeRef,
    },
    /// Change le rang d'empilement d'un objet graphique.
    ReorderShape {
        /// Graphe visé.
        graph: NodeId,
        /// Adresse de l'objet.
        target: ShapeRef,
        /// Où le mettre.
        order: ShapeOrder,
    },
    /// Pose la position d'un titre ou de la légende (issue #53) : le glisser
    /// qui s'achève, ou, avec `anchor: None`, « Reset Position ». Une
    /// modification du modèle ([`Command::SetPlotSpec`]), comme
    /// [`Action::SetAnnotation`].
    SetTextAnchor {
        /// Graphe visé.
        graph: NodeId,
        /// Le texte déplacé.
        target: TextTarget,
        /// Sa nouvelle ancre ; `None` le remet à sa place par défaut.
        anchor: Option<TextAnchor>,
    },
    /// Pose la police et le corps d'un titre ou de la légende (issues #77,
    /// #78) : un attribut de la zone entière, pas une balise dans son texte.
    /// Une modification du modèle ([`Command::SetPlotSpec`]).
    SetTextFont {
        /// Graphe visé.
        graph: NodeId,
        /// Le texte réglé.
        target: TextTarget,
        /// Sa nouvelle police et son nouveau corps.
        font: stokesforge_plot::TextFont,
    },
    /// Pose la couleur d'un titre, d'un titre d'axe ou de la légende (issue
    /// #90) : un attribut de la zone entière, comme la police. Vide =
    /// automatique. Une modification du modèle ([`Command::SetPlotSpec`]).
    SetTextColor {
        /// Graphe visé.
        graph: NodeId,
        /// Le texte réglé.
        target: TextTarget,
        /// Sa nouvelle couleur (`#RRGGBB[AA]`), vide pour l'automatique.
        color: String,
    },
    /// Remet **tous** les titres et la légende du graphe à leur place par
    /// défaut (menu *Graph*).
    ResetTextAnchors {
        /// Graphe visé.
        graph: NodeId,
    },
    /// Remplace le texte saisi d'un titre (double-clic, éditeur sur place).
    SetTextContent {
        /// Graphe visé.
        graph: NodeId,
        /// Le titre édité.
        target: TextTarget,
        /// Son nouveau texte (markup Typst).
        text: String,
    },
    /// Renomme une série telle qu'elle se lit en légende (double-clic sur
    /// une entrée). Un nom vide rend la série à son nom dérivé de la colonne.
    SetLegendEntryName {
        /// Graphe visé.
        graph: NodeId,
        /// La série.
        series: SeriesRef,
        /// Son nouveau nom en légende.
        name: String,
    },
}

/// Les agrandissements proposés pour un export PNG : la figure fait
/// 480 × 360 points (`DEFAULT_FIGURE_SIZE`), donc ×4 donne 1920 × 1440 px,
/// environ 300 ppp pour une figure de 16 cm de large.
pub const PNG_EXPORT_SCALES: [f32; 3] = [1.0, 2.0, 4.0];

/// L'agrandissement PNG par défaut : la résolution d'une figure de
/// publication.
pub const DEFAULT_PNG_EXPORT_SCALE: f32 = 4.0;

/// Le nom de fichier proposé pour exporter `graph` : son nom dans le projet,
/// en `.svg`, sans séparateur de chemin.
pub fn export_file_name(tree: &Tree, graph: NodeId) -> String {
    let name = tree
        .node(graph)
        .map_or("graphe", |node| node.name())
        .replace(['/', '\\'], "_");
    format!("{name}.svg")
}

/// Où enregistrer une couleur du sélecteur de couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SaveTarget {
    /// Dans le projet : la couleur voyage avec le fichier `.stf`, et
    /// l'enregistrer s'annule par Ctrl+Z. Le choix par défaut.
    #[default]
    Project,
    /// Pour l'utilisateur : la couleur reste sur cette machine, proposée
    /// dans tous les projets.
    User,
}

impl SaveTarget {
    /// Libellé du choix.
    pub fn label(self) -> String {
        match self {
            SaveTarget::Project => crate::tr!("color-target-project"),
            SaveTarget::User => crate::tr!("color-target-user"),
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
/// Copie **volontairement indépendante** de `stokesforge_script::ScriptOutcome` :
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
/// # C4 (moitié `stokesforge-app`, `guide-de-correction-robustesse.md` §F8)
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
/// `Column::set_column_type` (`Float → Text`, dans `stokesforge-core`, la
/// conversion) — « sans quoi l'affichage et la conversion diront deux choses
/// différentes de la même valeur ». Le correctif de `stokesforge-core` est mené en
/// parallèle, dans un autre arbre de travail, par un autre agent ; au moment
/// d'écrire cette fonction, `stokesforge_core::column` n'expose encore aucune
/// fonction de formatage publique à réutiliser ici (vérifié par
/// `grep -n "pub fn format" crates/stokesforge-core/src/column.rs`). Cette
/// fonction reproduit donc, côté `stokesforge-app` seulement, la même idée
/// (seuils, bascule en notation scientifique) sans prétendre être
/// bit-à-bit identique à ce que produira l'autre moitié du correctif : les
/// deux devront être unifiées — dans `stokesforge-core`, pour que `cell_text`
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
/// ([`crate::app::StokesForgeApp::show_detached_tabs`], qui ne porte qu'un clone
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
/// à gauche, voir `app::grid::cell_widget`). Pour une colonne `Mixed`, le
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
/// à gauche, l'abscisse reste implicite ([`SeriesSpec::new`](stokesforge_plot::SeriesSpec::new)). Chaque série
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
) -> Vec<stokesforge_plot::SeriesSpec> {
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
            Some(x) => stokesforge_plot::SeriesSpec::new_xy(x, *column),
            None => stokesforge_plot::SeriesSpec::new(*column),
        };
        let index = start_index + series.len();
        spec.line = stokesforge_plot::LineSpec::from_palette(index);
        spec.symbol = stokesforge_plot::SymbolSpec::from_palette(index);
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

/// Nom affiché d'une série dans la liste de la fenêtre des propriétés : la
/// même règle que la légende du graphe, [`stokesforge_core::series_label`] (source
/// unique, issue #25).
pub(crate) fn series_list_label(tree: &Tree, series: &stokesforge_plot::SeriesSpec) -> String {
    stokesforge_core::series_label(series, tree)
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

/// L'[`Action`] produite par le dépôt de `payload` sur le calque `panel` du
/// graphe `graph` — toujours [`Action::AddSeriesToGraph`], sans type de tracé
/// imposé : les séries prennent celui du calque (#160, voir
/// [`panel_plot_type`]). Fonction pure, séparée du code de peinture de
/// `crate::app::graph_window::graph_drop_target` pour être testable sans
/// fenêtre.
pub fn drop_payload_action(payload: &ColumnDragPayload, graph: NodeId, panel: usize) -> Action {
    Action::AddSeriesToGraph {
        graph,
        panel,
        columns: payload.columns.clone(),
        plot_type: None,
    }
}

/// Type de tracé des séries ajoutées à `panel` quand le geste n'en désigne
/// pas : celui de sa **dernière** série (un calque qui en mélange plusieurs
/// donne le dernier), [`PlotType::Line`] pour un calque vide.
pub(crate) fn panel_plot_type(panel: &stokesforge_plot::PanelSpec) -> PlotType {
    panel
        .series
        .last()
        .map_or(PlotType::Line, |series| series.plot_type.clone())
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

/// Nom d'un nœud, ou `"{missing} {id} (deleted)"` s'il n'existe plus dans
/// `tree` — pendant de `crate::app::tab::node_title`, factorisé sur un `&Tree`
/// pour la même raison que [`cell_text_in`].
pub(crate) fn node_title_in(tree: &Tree, id: NodeId, missing: impl FnOnce() -> String) -> String {
    tree.node(id)
        .map(|node| node.name().to_string())
        .unwrap_or_else(|| crate::tr!("window-deleted", kind = missing(), id = id.to_string()))
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

/// Champ de [`stokesforge_core::ColumnMeta`] éditable dans l'en-tête empilé d'une
/// colonne — voir [`HeaderEdit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    /// Nom long ([`stokesforge_core::ColumnMeta::long_name`]).
    LongName,
    /// Unité ([`stokesforge_core::ColumnMeta::unit`]).
    Unit,
    /// Commentaire ([`stokesforge_core::ColumnMeta::comment`]).
    Comment,
    /// Formule ([`stokesforge_core::ColumnMeta::formula`]).
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
/// `app::grid::grid_edit_menu`.
///
/// Ce type ne porte **que la décision** (quels gestes ont un sens ici,
/// comment les nommer, lesquels sont grisés) et jamais de widget : c'est ce
/// qui permet de le vérifier par les tests d'intégration sans UI de
/// `tests/shell.rs`, comme tout le reste de la matrice copier/coller.
///
/// Les gestes listés sont exactement les raccourcis clavier de la grille
/// (voir la doc de tête de ce module, section copier/coller, et le bloc
/// clavier de `app::grid::grid_contents`) : aucun menu n'offre ici
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
    /// Faux si la sélection est un ensemble de colonnes non contigu (#159) :
    /// la cible d'un collage y serait ambiguë, Paste et Paste in Place sont
    /// grisés.
    pub paste_enabled: bool,
    /// Faux si la sélection multiple n'a pas de copie univoque (#159) : des
    /// blocs de cellules qui ne partagent ni leurs lignes ni leurs colonnes.
    /// Copy et Cut sont alors grisés.
    pub copy_enabled: bool,
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
/// Ctrl+C hors de StokesForge) rejouerait silencieusement une structure qui ne
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
/// point de contact (`StokesForgeApp::ui`), rien d'autre ne le lit.
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
    pub fn label(self) -> String {
        match self {
            PlotSettingsTab::Graph => tr!("plot-tab-general"),
            PlotSettingsTab::PlotArea => tr!("plot-tab-plot-area"),
            PlotSettingsTab::Axes => tr!("plot-tab-axes"),
            PlotSettingsTab::Series => tr!("plot-tab-series"),
            PlotSettingsTab::Legend => tr!("plot-tab-legend"),
            PlotSettingsTab::Theme => tr!("plot-tab-theme"),
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
    pub fn label(self) -> String {
        match self {
            PlotAxis::Bottom => tr!("plot-axis-bottom"),
            PlotAxis::Left => tr!("plot-axis-left"),
            PlotAxis::Right => tr!("plot-axis-right"),
        }
    }
}

/// Où mettre un objet graphique dans l'ordre d'empilement
/// ([`Action::ReorderShape`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeOrder {
    /// Au-dessus de tous les autres objets du panneau.
    ToFront,
    /// Sous tous les autres objets du panneau.
    ToBack,
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
    /// Un texte libre (issue #35).
    Annotation(AnnotationRef),
    /// La légende (cadre ou entrée, issue #53).
    Legend,
    /// Un objet graphique : rectangle, ellipse, ligne ou flèche (issue #76).
    Object(ShapeRef),
}

impl PlotTarget {
    /// Nom du composant, en tête du menu contextuel.
    pub fn label(self) -> String {
        match self {
            PlotTarget::Background => tr!("plot-target-graph"),
            PlotTarget::Title => tr!("plot-target-title"),
            PlotTarget::PlotArea => tr!("plot-target-plot-area"),
            PlotTarget::Axis(axis) => tr!("plot-target-axis", axis = axis.label()),
            PlotTarget::AxisTitle(axis) => tr!("plot-target-axis-title", axis = axis.label()),
            PlotTarget::Annotation(_) => tr!("plot-target-text"),
            PlotTarget::Legend => tr!("plot-target-legend"),
            PlotTarget::Object(_) => tr!("plot-target-object"),
        }
    }

    /// L'onglet (et l'axe) de la fenêtre des propriétés qui règle ce
    /// composant.
    pub fn settings_page(self) -> (PlotSettingsTab, Option<PlotAxis>) {
        match self {
            PlotTarget::Background | PlotTarget::Title => (PlotSettingsTab::Graph, None),
            PlotTarget::PlotArea | PlotTarget::Annotation(_) | PlotTarget::Object(_) => {
                (PlotSettingsTab::PlotArea, None)
            }
            PlotTarget::Legend => (PlotSettingsTab::Legend, None),
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
    /// Calque affiché par les propriétés ; indépendant de son rang de série.
    pub selected_panel: usize,
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
    /// Projets `.stf` : chacun s'ouvre dans une nouvelle fenêtre StokesForge.
    pub projects: Vec<PathBuf>,
    /// Fichiers de données à importer.
    pub data: Vec<PathBuf>,
    /// Destination des fichiers de données.
    pub destination: ImportDestination,
}

/// Décision pure d'un dépôt : le lieu du dépôt fixe la destination (un
/// classeur sous le curseur reçoit une feuille par fichier, n'importe où
/// ailleurs un nouveau classeur partagé), l'extension `.stf` fait d'un
/// fichier un projet à ouvrir plutôt qu'une donnée à importer.
pub fn file_drop_plan(paths: &[PathBuf], target_book: Option<NodeId>) -> DropPlan {
    let is_project = |path: &PathBuf| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("stf"))
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
fn reads_well(path: &std::path::Path, recipe: &stokesforge_import::ImportRecipe) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    stokesforge_import::parse(&bytes, recipe).is_ok_and(|table| {
        table
            .columns
            .iter()
            .any(|column| column.values.iter().any(|value| !value.is_nan()))
    })
}

/// Dernier import réussi : de quoi le reprendre dans l'assistant pour le
/// corriger (« Régler le dernier import… »).
#[derive(Debug, Clone)]
struct LastImport {
    destination: ImportDestination,
    recipe: stokesforge_import::ImportRecipe,
    paths: Vec<PathBuf>,
}

/// Import en cours sur un fil de fond : l'analyse des fichiers (la partie
/// lente) ne bloque pas l'interface ; les feuilles sont créées par
/// [`AppState::poll_import`] quand tout est prêt.
struct ImportJob {
    destination: ImportDestination,
    total: usize,
    done: Arc<std::sync::atomic::AtomicUsize>,
    reply: mpsc::Receiver<Vec<ImportItem>>,
}

/// État transitoire de la fenêtre d'import (voir [`crate::app`] pour son
/// rendu) : jamais dans l'historique, jamais envoyé à l'acteur — même
/// famille que [`HeaderEdit`]/[`CellEdit`]/[`NoteEdit`]. Porte tout ce que la
/// fenêtre a besoin de retenir entre deux images, y compris le
/// [`stokesforge_import::store::RecipeStore`] chargé une seule fois à l'ouverture
/// (pas rechargé à chaque image — `RecipeStore::load` touche le disque).
pub struct ImportState {
    /// Classeur actif à l'ouverture, disponible même après un changement de destination.
    pub active_book: Option<NodeId>,
    /// Destination choisie ; le classeur actif est proposé s'il existe.
    pub destination: ImportDestination,
    /// Recette en cours d'édition dans la fenêtre.
    pub recipe: stokesforge_import::ImportRecipe,
    /// Fichiers choisis (« Choisir des fichiers… »), dans l'ordre du
    /// sélecteur.
    pub paths: Vec<std::path::PathBuf>,
    /// Modèles enregistrés, chargés une fois à l'ouverture de la fenêtre.
    pub store: stokesforge_import::RecipeStore,
    /// Nom sous lequel enregistrer la recette courante comme modèle (champ
    /// texte de la fenêtre, distinct de `recipe.name` : on ne veut pas
    /// écraser le nom du modèle chargé tant que « Enregistrer le modèle »
    /// n'a pas été cliqué).
    pub save_as_name: String,
    /// Étape courante de l'assistant.
    pub step: ImportStep,
    /// Fichier montré dans l'aperçu (indice dans `paths`).
    pub preview_index: usize,
    /// Modèle dont la suppression attend une confirmation.
    pub confirm_delete: Option<String>,
    /// Fichiers importés récemment, chargés à l'ouverture.
    pub recent: stokesforge_import::RecentFiles,
    /// Recette telle qu'elle était au chargement du modèle : sert à dire
    /// qu'elle a été modifiée sans être enregistrée.
    baseline: stokesforge_import::ImportRecipe,
    /// Octets des fichiers choisis, lus une seule fois par chemin (le fichier
    /// n'est plus relu à chaque image).
    bytes: std::cell::RefCell<ImportBytesCache>,
}

/// Réglage proposé par le clic droit sur une ligne de l'aperçu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportLineAction {
    /// « Les données commencent ici ».
    DataStartsHere,
    /// « Cette ligne donne les unités ».
    UnitsLine,
    /// « Fin des données ici » (la ligne est la première du pied de page).
    DataEndsHere,
    /// « Annuler la coupure automatique ».
    NoAutoCut,
}

/// Octets lus par chemin (ou le message d'échec de lecture).
type ImportBytesCache = std::collections::HashMap<PathBuf, Result<Arc<Vec<u8>>, String>>;

/// Les six étapes de l'assistant d'import, façon assistant ASCII d'Origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportStep {
    /// Fichier(s) et modèle.
    Files,
    /// Séparateur et décimale.
    Format,
    /// En-tête et fin des données.
    Header,
    /// Désignation colonne par colonne.
    Columns,
    /// Métadonnées à conserver.
    Metadata,
    /// Destination et enregistrement du modèle.
    Destination,
}

impl ImportStep {
    /// Toutes les étapes, dans l'ordre.
    pub const ALL: [ImportStep; 6] = [
        ImportStep::Files,
        ImportStep::Format,
        ImportStep::Header,
        ImportStep::Columns,
        ImportStep::Metadata,
        ImportStep::Destination,
    ];

    /// Titre affiché.
    pub fn title(self) -> String {
        match self {
            ImportStep::Files => tr!("import-step-files-title"),
            ImportStep::Format => tr!("import-step-format-title"),
            ImportStep::Header => tr!("import-step-header-title"),
            ImportStep::Columns => tr!("import-step-columns-title"),
            ImportStep::Metadata => tr!("common-metadata"),
            ImportStep::Destination => tr!("import-destination"),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }

    /// Étape suivante (la dernière reste elle-même).
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(Self::ALL.len() - 1)]
    }

    /// Étape précédente (la première reste elle-même).
    pub fn previous(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }
}

/// Une colonne du résultat d'import, pour l'étape « Colonnes ».
#[derive(Debug, Clone, PartialEq)]
pub struct ImportColumnPreview {
    /// Nom (ligne de noms de colonnes) ou vide.
    pub name: String,
    /// Nom long (sous-en-tête) ou vide.
    pub long_name: String,
    /// Unité (sous-en-tête) ou vide.
    pub unit: String,
    /// Colonne à types mixtes (texte conservé).
    pub mixed: bool,
    /// Premières valeurs, mises en forme.
    pub samples: Vec<String>,
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
    /// `app::tab::Tab::Book`. Un classeur qui n'a pas d'entrée ici (ou
    /// dont l'entrée est devenue invalide) retombe sur sa première feuille,
    /// voir [`AppState::selected_sheet_in`] — la carte n'a donc jamais besoin
    /// d'être pré-remplie.
    selected_sheets: std::collections::HashMap<NodeId, NodeId>,
    /// Chemin du fichier de travail (saisi à la main, cf. `crate::app`).
    path: String,
    /// Repère conservateur de l'enregistrement, sans épingler les buffers
    /// d'un ancien arbre. Une annulation demande aussi confirmation.
    saved_revision: Option<(u64, u64)>,
    new_project_pending: bool,
    /// Feuille dont l'onglet est en édition de nom.
    renaming_sheet: Option<NodeId>,
    /// Feuille dont la suppression attend confirmation.
    pending_sheet_delete: Option<NodeId>,
    pending_object_delete: Vec<NodeId>,
    object_clipboard_origin: String,
    object_clipboard_output: Option<String>,
    /// Les représentations de la figure à offrir au système, préparées par
    /// [`Action::CopyGraphFigure`] et relevées par la fenêtre (#149).
    figure_clipboard: Option<Vec<stokesforge_clipboard::Representation>>,
    copied_objects: Vec<(NodeId, NodeId, bool)>,
    /// Agrandissement des exports PNG (issue #34), un de
    /// [`PNG_EXPORT_SCALES`]. Préférence de session, pas du projet.
    png_export_scale: f32,
    object_manager_open: bool,
    object_manager_preferences: Preferences,
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
    /// Projets `.stf` déposés, à ouvrir chacun dans une nouvelle fenêtre
    /// StokesForge : l'état ne lance aucun processus (testable sans fenêtre), la
    /// couche fenêtre les vide par [`AppState::take_projects_to_open`].
    projects_to_open: Vec<PathBuf>,
    /// Colonne dont la fenêtre des métadonnées de fichier est ouverte
    /// (lecture seule, transitoire : jamais dans l'historique).
    metadata_view: Option<NodeId>,
    /// Import en cours sur un fil de fond.
    import_job: Option<ImportJob>,
    /// Dernier import réussi, pour le reprendre dans l'assistant.
    last_import: Option<LastImport>,
    /// Fichiers importés avec succès, à inscrire aux fichiers récents par la
    /// couche fenêtre ([`AppState::take_imported_paths`]).
    recent_to_record: Vec<PathBuf>,
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
/// refusé ou échoué — une [`stokesforge_script::ScriptError`] n'a pas à traverser
/// ce canal, `state.rs` n'en a pas besoin au-delà de son message.
#[cfg(feature = "python")]
type PendingScriptResult = (
    Option<Arc<ScriptEngine>>,
    Result<ScriptOutcome, ScriptError>,
);

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
        Self::new_with_preferences(handle, path, Preferences::load())
    }

    /// Injection du stockage, notamment pour tester les actions sans écrire
    /// les préférences de la machine de l'utilisateur.
    pub fn new_with_preferences(
        handle: SessionHandle,
        path: String,
        preferences: Preferences,
    ) -> SessionResult<Self> {
        let snapshot = handle.snapshot()?;
        let saved_revision = std::path::Path::new(&path)
            .is_file()
            .then_some((snapshot.revision, snapshot.geometry_revision));
        Ok(Self {
            handle,
            snapshot,
            saved_revision,
            new_project_pending: false,
            renaming_sheet: None,
            pending_sheet_delete: None,
            pending_object_delete: Vec::new(),
            object_clipboard_origin: format!(
                "{}:{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            object_clipboard_output: None,
            figure_clipboard: None,
            copied_objects: Vec::new(),
            selected_sheets: std::collections::HashMap::new(),
            path,
            png_export_scale: DEFAULT_PNG_EXPORT_SCALE,
            object_manager_open: preferences.object_manager_open(),
            object_manager_preferences: preferences,
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
            metadata_view: None,
            import_job: None,
            last_import: None,
            recent_to_record: Vec::new(),
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
        // Identité d'émetteur propre à la vue détachée : avec un simple `clone`,
        // ses écritures seraient absorbées dans une macro ouverte par la
        // racine (propriétés du graphe) puis emportées par son annulation
        // (issue #21).
        let mut view = Self::new(self.handle.with_new_identity(), self.path.clone())?;
        view.snapshot = self.snapshot.clone();
        view.selected_sheets = self.selected_sheets.clone();
        view.renaming_sheet = self.renaming_sheet;
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

    /// Synchronise l'éditeur des onglets sans écraser la sélection locale.
    pub fn sync_detached_sheet_editor(&mut self, root: &Self) {
        self.renaming_sheet = root.renaming_sheet;
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
    pub fn set_window_geometry(
        &mut self,
        node: NodeId,
        geometry: stokesforge_session::WindowGeometry,
    ) {
        if let Err(err) = self.handle.set_window_geometry(node, geometry) {
            self.fail(format!(
                "Échec de la mise à jour de la géométrie : {}",
                errors::session::error_message(&err)
            ));
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
    /// trouvé son préfixe embarqué (`AppState::set_python` jamais appelé)
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

    /// Libellé de l'action « Annuler » : `Some("Undo Edit Cells in Column A")`
    /// (mis en phrase par [`history_text`]), ou `None` si la pile est vide
    /// (l'entrée de menu est alors grisée).
    pub fn undo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .undo_label
            .as_ref()
            .map(|label| tr!("history-undo-entry", label = history_text(label)))
    }

    /// Libellé de l'action « Rétablir », `None` si rien à rétablir.
    pub fn redo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .redo_label
            .as_ref()
            .map(|label| tr!("history-redo-entry", label = history_text(label)))
    }

    /// Vrai s'il y a quelque chose à annuler.
    pub fn can_undo(&self) -> bool {
        self.snapshot.history.can_undo
    }

    /// Révision courante de l'instantané — sert à `crate::app` à détecter,
    /// après un [`AppState::dispatch`], si l'action a réellement produit une
    /// commande (`Action::PlotColumns` sur une sélection sans Y n'en envoie
    /// aucune, la révision ne bouge pas).
    pub fn revision(&self) -> stokesforge_session::Revision {
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
    /// depuis la zone de prise. `crate::app::grid::grid_contents` l'appelle à
    /// chaque image pour savoir s'il y a quoi que ce soit à proposer au
    /// glisser ; ne rien avoir sélectionné y désactive la zone de prise tout
    /// entière plutôt que de la laisser démarrer un glisser vide.
    pub fn column_drag_payload(&self, sheet: NodeId) -> Option<ColumnDragPayload> {
        let selection = self.selection(sheet)?;
        // Seule une sélection de colonnes **entières** se glisse (#160) :
        // pas de cases ni de lignes.
        if !selection.is_columns() {
            return None;
        }
        let columns = selection.columns(&self.snapshot.tree, sheet);
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
    /// # C4 (moitié `stokesforge-app`, §F8) — le formatage numérique
    ///
    /// Le formatage numérique n'est plus le seul `Display` de `f64` (qui
    /// n'émet jamais d'exposant : une mesure comme la charge de l'électron,
    /// 1,602176634·10⁻¹⁹, s'écrivait sur 30 caractères dans une cellule de
    /// grille large d'une quinzaine). [`format_f64_for_display`] bascule en
    /// notation scientifique pour les magnitudes extrêmes ; voir sa
    /// documentation pour les seuils exacts et pour ce que cette moitié doit
    /// à l'autre — la conversion `Column::set_column_type` (`Float → Text`)
    /// de `stokesforge-core`, corrigée séparément (§F8 : « une seule table, deux
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
    /// commentaire, formule) — [`stokesforge_core::ColumnMeta::default`] si
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

    /// Ctrl+clic sur une lettre : ajoute ou retire `column` de la sélection
    /// et y déplace l'ancre (#159). Repose une sélection simple si la
    /// sélection en cours n'est pas un ensemble de colonnes — une seule sorte
    /// à la fois.
    pub fn toggle_column_selection(&mut self, sheet: NodeId, column: NodeId) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_columns() => {
                selection.toggle_column(&self.snapshot.tree, sheet, column)
            }
            _ => self.select_column(sheet, column),
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Maj+clic sur une lettre : la sélection devient la plage ancre →
    /// `column` (les colonnes posées au Ctrl+clic disparaissent). Le
    /// Ctrl+Maj+clic et le glisser, additifs, passent par
    /// [`AppState::extend_column_selection`].
    pub fn extend_column_selection_replacing(&mut self, sheet: NodeId, column: NodeId) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_columns() => selection.extend_columns_replacing(column),
            _ => self.select_column(sheet, column),
        }
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

    /// Retire de `sheet` une sélection que le dernier geste a vidée (Ctrl+clic
    /// qui retire la dernière colonne, ligne ou case).
    fn drop_selection_if_empty(&mut self, sheet: NodeId) {
        let empty = self
            .selection
            .get(&sheet)
            .is_some_and(|selection| selection.columns(&self.snapshot.tree, sheet).is_empty());
        if empty {
            self.selection.remove(&sheet);
        }
    }

    /// Ctrl+clic sur un numéro de ligne : ajoute ou retire `row` (#159).
    pub fn toggle_row_selection(&mut self, sheet: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_full_rows() => selection.toggle_row(row),
            _ => self.select_row(sheet, row),
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Maj+clic sur un numéro de ligne : la plage ancre → `row` remplace
    /// tout ; le Ctrl+Maj+clic et le glisser passent par
    /// [`AppState::extend_row_selection`].
    pub fn extend_row_selection_replacing(&mut self, sheet: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_full_rows() => {
                selection.extend_full_rows_replacing(row)
            }
            _ => self.select_row(sheet, row),
        }
    }

    /// Ctrl+clic sur une cellule : ajoute ou retire la case (#159). Repose
    /// une sélection simple si la sélection en cours n'est pas un ensemble de
    /// cases de la même zone.
    pub fn toggle_cell_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        let target = GridCursor::Data(column, row);
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.toggle_cell(&self.snapshot.tree, sheet, target),
            None => false,
        };
        if !handled {
            self.select_cell(sheet, column, row);
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Ctrl+clic sur un champ d'en-tête : même règle que
    /// [`AppState::toggle_cell_selection`], côté en-tête.
    pub fn toggle_header_field_selection(
        &mut self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) {
        let target = GridCursor::Header(column, field);
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.toggle_cell(&self.snapshot.tree, sheet, target),
            None => false,
        };
        if !handled {
            self.select_header_field(sheet, column, field);
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Ctrl+glisser depuis une cellule : ouvre un nouveau rectangle en
    /// gardant les précédents (#159).
    pub fn add_cell_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.add_cell(GridCursor::Data(column, row)),
            None => false,
        };
        if !handled {
            self.select_cell(sheet, column, row);
        }
    }

    /// Ctrl+glisser depuis un champ d'en-tête : pendant de
    /// [`AppState::add_cell_selection`].
    pub fn add_header_field_selection(
        &mut self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) {
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.add_cell(GridCursor::Header(column, field)),
            None => false,
        };
        if !handled {
            self.select_header_field(sheet, column, field);
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
    /// [`AppState::move_selection_tab`]) comme pour [`AppState::paste`].
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
    /// 4 champs, voir [`HeaderField`]) et les données : Haut depuis la
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

    /// Vrai si `sheet` est actuellement sélectionnée dans son entier : toutes
    /// ses colonnes visibles, par la case du coin ou Ctrl+A (voir
    /// [`AppState::select_all`]).
    pub fn is_all_selected(&self, sheet: NodeId) -> bool {
        let order = self.column_order(sheet);
        !order.is_empty()
            && self
                .selection
                .get(&sheet)
                .is_some_and(|selection| selection.columns(&self.snapshot.tree, sheet) == order)
            && self
                .selection
                .get(&sheet)
                .is_some_and(Selection::is_columns)
    }

    /// Sélectionne **toutes** les colonnes visibles de `sheet`, en-tête
    /// compris et même sans ligne (#159) — case du coin, Ctrl+A, Edit ›
    /// Select All. Sans effet sur une feuille sans colonne.
    pub fn select_all(&mut self, sheet: NodeId) {
        let order = self.column_order(sheet);
        if let Some(selection) = Selection::all_columns(&order) {
            self.selection.insert(sheet, selection);
        }
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
    /// `app::grid::cell_widget`/`app::grid::header_field_widget`/
    /// `app::grid::column_header_widget`, qui réduisent la sélection à la
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
            offers_delete_shift: zone != GridZone::Header
                && (selection.is_columns()
                    || selection.blocks(&self.snapshot.tree, sheet).len() <= 1),
            copy_enabled: selection.is_columns()
                || selection.copy_layout(&self.snapshot.tree, sheet).is_some(),
            paste_absolute_enabled: self.grid_clipboard.is_some(),
            paste_enabled: selection.is_contiguous(&self.snapshot.tree, sheet),
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
        } else if !selection.is_columns() {
            // Un ou plusieurs blocs : bout à bout dans l'ordre de la feuille
            // (#159) quand ils partagent leurs lignes ou leurs colonnes,
            // sinon rien n'est copié (voir `Selection::copy_layout`).
            let (columns, ranges) = selection.copy_layout(tree, sheet)?;
            let ranks: Vec<isize> = ranges.iter().flat_map(|&(lo, hi)| lo..=hi).collect();
            let first = *ranks.first()?;
            if first < 0 {
                let start = HeaderField::from_index((first + 4) as usize);
                let rows = ranks
                    .iter()
                    .map(|&rank| {
                        let field = HeaderField::from_index((rank + 4) as usize);
                        columns
                            .iter()
                            .map(|&column| self.header_field_display_text(column, field))
                            .collect()
                    })
                    .collect();
                GridClipboard::Relative(RelativeCopy::HeaderFields { start, rows })
            } else {
                let rows = ranks
                    .iter()
                    .map(|&rank| {
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
                    start: first as usize,
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
    /// `app::grid::header_field_widget`, factorisé ici pour être partagé
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
    /// d'afficher le panneau (voir `node_title` dans `crate::app::tab`).
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
            selected_panel: 0,
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
                self.fail(format!(
                    "Annulation des propriétés impossible : {}",
                    errors::session::error_message(&err)
                ));
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
        match self.handle.begin_macro(MacroKind::GraphProperties) {
            Ok(_) => self.plot_macro_open = true,
            Err(err) => self.fail(format!(
                "Propriétés : annulation groupée indisponible ({})",
                errors::session::error_message(&err)
            )),
        }
    }

    /// Message affiché quand des actions d'une fenêtre détachée sont refusées
    /// parce que la fenêtre modale des propriétés d'un graphe est ouverte.
    pub fn reject_detached_actions(&mut self) {
        self.fail(
            "Fenêtre des propriétés du graphe ouverte : validez-la ou annulez-la \
             avant de modifier une fenêtre détachée"
                .to_string(),
        );
    }

    /// Ferme la macro de la fenêtre des propriétés (une entrée d'historique,
    /// aucune si rien n'a changé).
    fn end_plot_macro(&mut self) {
        if !self.plot_macro_open {
            return;
        }
        self.plot_macro_open = false;
        self.end_grouped_macro();
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
    /// `app::tab::Tab::Book` : `(NodeId, nom)`. Vide si `book` n'existe
    /// plus ou n'est pas un classeur.
    pub fn worksheets_of(&self, book: NodeId) -> Vec<(NodeId, String)> {
        worksheets_of_in(&self.snapshot.tree, book)
    }

    /// Les classeurs du projet, sous la forme `(identifiant, chemin
    /// lisible)`. Le pendant exact de [`AppState::graphs`], pour la
    /// disposition initiale de [`crate::app::StokesForgeApp::new`] : ouvrir le
    /// premier classeur du projet, comme le premier graphe l'était déjà.
    pub fn books(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id).is_some_and(
                    |node| matches!(node.kind(), NodeKind::Workbook { meta, .. } if !meta.hidden),
                )
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
                if matches!(child.kind(), NodeKind::Workbook { meta, .. } if meta.hidden) {
                    return None;
                }
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

    /// Ouverture du panneau, mémorisée hors du projet.
    pub fn object_manager_open(&self) -> bool {
        self.object_manager_open
    }

    /// Ouvre ou ferme le panneau pour un test, sans écrire le
    /// `preferences.json` de la machine (ce que fait
    /// `Action::SetObjectManagerOpen`) : un test ne dépend ainsi ni ne
    /// modifie la préférence de l'utilisateur.
    #[cfg(test)]
    pub(crate) fn set_object_manager_open_for_test(&mut self, open: bool) {
        self.object_manager_open = open;
    }

    /// Textes automatiques listés dans l'Object Manager (#102), hors projet.
    pub fn object_manager_show_texts(&self) -> bool {
        self.object_manager_settings().show_texts
    }

    /// Échelles (axes) listées dans l'Object Manager (#111), hors projet.
    pub fn object_manager_show_scales(&self) -> bool {
        self.object_manager_settings().show_scales
    }

    pub fn object_manager_settings(&self) -> ObjectManagerSettings {
        self.object_manager_preferences.object_manager_settings()
    }

    fn save_object_manager_settings(&mut self, settings: ObjectManagerSettings) {
        if let Err(error) = self
            .object_manager_preferences
            .save_object_manager_settings(settings)
        {
            self.fail(error.to_string());
        }
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
            Action::NewProject { pending_edits } => self.request_new_project(pending_edits),
            Action::ConfirmNewProject(accept) => {
                if std::mem::take(&mut self.new_project_pending) && accept {
                    self.new_project();
                }
            }
            Action::AddGraph => self.execute(Command::AddGraph {
                parent: self.snapshot.tree.root(),
                name: None,
                spec: PlotSpec::empty(""),
            }),
            Action::AddWorkbook => self.add_workbook(),
            Action::SelectSheet(id) => self.select_sheet(id),
            Action::BeginRenameSheet(sheet) => self.begin_rename_sheet(sheet),
            Action::CommitRenameSheet { sheet, name } => self.commit_rename_sheet(sheet, &name),
            Action::CancelRenameSheet => self.renaming_sheet = None,
            Action::DuplicateSheet(sheet) => self.duplicate_sheet(sheet),
            Action::RequestDeleteSheet(sheet) => self.request_delete_sheet(sheet),
            Action::ConfirmDeleteSheet(accept) => self.confirm_delete_sheet(accept),
            Action::RenameObject { id, name } => self.rename_object(id, &name),
            Action::RenameObjects(changes) => self.rename_objects(changes),
            Action::CopyObjects(ids) => self.copy_objects(&ids),
            Action::DuplicateObjects(ids) => self.duplicate_objects(&ids),
            Action::PasteObjects { parent, text } => self.paste_objects(parent, &text),
            Action::ShowGraphData { graph } => {
                self.reveal_books(graph_data_books(self.tree(), graph));
            }
            Action::ShowSeriesData { graph, series } => {
                self.reveal_books(series_data_books(self.tree(), graph, series));
            }
            Action::DeleteObjects(ids) => self.request_delete_objects(ids),
            Action::ConfirmDeleteObjects(accept) => {
                let ids = std::mem::take(&mut self.pending_object_delete);
                if accept {
                    self.delete_objects(&ids);
                }
            }
            Action::MoveObjects { ids, parent } => self.move_objects(&ids, parent),
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
            Action::SetColumnsType {
                columns,
                column_type,
            } => self.set_columns_grouped(MacroKind::SetColumnsType, &columns, |column| {
                Command::SetColumnType {
                    column,
                    column_type,
                }
            }),
            Action::SetColumnsRole { columns, role } => {
                self.set_columns_role_grouped(&columns, role)
            }
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
            Action::SelectAll { sheet } => self.select_all(sheet),
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
            Action::StartImport {
                destination,
                recipe,
                paths,
            } => self.start_import(destination, recipe, paths),
            Action::ReopenLastImport => self.reopen_last_import(),
            Action::RefreshFromSource { sheet } => self.refresh_from_source(sheet),
            Action::ViewColumnMetadata { column } => self.metadata_view = Some(column),
            Action::CloseColumnMetadata => self.metadata_view = None,
            Action::PlotColumns {
                sheet,
                columns,
                plot_type,
            } => self.plot_columns(sheet, columns, plot_type),
            Action::AddSeriesToGraph {
                graph,
                panel,
                columns,
                plot_type,
            } => self.add_series_to_graph(graph, panel, columns, plot_type),
            Action::SetObjectManagerOpen(open) => {
                self.object_manager_open = open;
                if let Err(error) = self
                    .object_manager_preferences
                    .save_object_manager_open(open)
                {
                    self.fail(error.to_string());
                }
            }
            Action::SetObjectManagerShowTexts(show) => {
                let mut settings = self.object_manager_settings();
                settings.show_texts = show;
                self.save_object_manager_settings(settings);
            }
            Action::SetObjectManagerShowScales(show) => {
                let mut settings = self.object_manager_settings();
                settings.show_scales = show;
                self.save_object_manager_settings(settings);
            }
            Action::SetObjectManagerGroupOpen { group, open } => {
                let mut settings = self.object_manager_settings();
                settings.set_group_open(group, open);
                self.save_object_manager_settings(settings);
            }
            Action::ResetObjectManagerSettings => {
                self.save_object_manager_settings(ObjectManagerSettings::default());
            }
            Action::SetPlotVisibility {
                graph,
                target,
                visible,
            } => {
                if let Some(mut spec) = current_plot_spec(self.tree(), graph) {
                    if target.set(&mut spec, visible) {
                        self.execute(Command::EditPlotSpec {
                            graph,
                            spec,
                            kind: PlotEditKind::Visibility,
                        });
                    }
                }
            }
            Action::ReorderSeries {
                graph,
                source,
                index,
            } => self.edit_plot_object(graph, PlotEditKind::ReorderSeries, |spec| {
                crate::object_manager::reorder_series(spec, source, index)
            }),
            Action::RemovePlotObject { graph, target } => {
                use crate::object_manager::VisibilityTarget as T;
                match target {
                    T::Series(series) => {
                        self.edit_plot_object(graph, PlotEditKind::RemoveSeries, |spec| {
                            let Some(panel) = spec.panels.get_mut(series.panel) else {
                                return false;
                            };
                            if series.index >= panel.series.len() {
                                return false;
                            }
                            panel.series.remove(series.index);
                            true
                        })
                    }
                    T::Annotation(target) => {
                        self.dispatch(Action::RemoveAnnotation { graph, target })
                    }
                    T::Shape(target) => self.dispatch(Action::RemoveShape { graph, target }),
                    T::Text(target) => {
                        self.edit_plot_object(graph, PlotEditKind::RemoveText, |spec| {
                            target.set_text(spec, String::new())
                        })
                    }
                    T::Graph | T::Panel(_) | T::Group(_) | T::Axis(_) | T::Axes(_) => {}
                }
            }
            Action::OpenObjectProperties { graph, target } => {
                use crate::object_manager::VisibilityTarget as T;
                let Some(spec) = self.plot_spec(graph) else {
                    return;
                };
                if !target.exists(&spec) {
                    return;
                }
                let (tab, axis, panel, series) = match target {
                    T::Series(s) => (PlotSettingsTab::Series, None, s.panel, Some(s.index)),
                    T::Panel(i) => (PlotSettingsTab::PlotArea, None, i, None),
                    T::Group(i) => (PlotSettingsTab::Series, None, i, None),
                    T::Axes(i) => (PlotSettingsTab::Axes, None, i, None),
                    T::Text(TextTarget::Legend { row, column }) => (
                        PlotSettingsTab::Legend,
                        None,
                        spec.panels
                            .iter()
                            .position(|p| p.row == row && p.column == column)
                            .unwrap_or(0),
                        None,
                    ),
                    T::Text(TextTarget::AxisTitle { row, column, side }) => (
                        PlotSettingsTab::Axes,
                        Some(match side {
                            stokesforge_plot::AxisSide::Bottom => PlotAxis::Bottom,
                            // Axe réservé dans le modèle, sans page ni rendu.
                            stokesforge_plot::AxisSide::Top => return,
                            stokesforge_plot::AxisSide::Left => PlotAxis::Left,
                            stokesforge_plot::AxisSide::Right => PlotAxis::Right,
                        }),
                        spec.panels
                            .iter()
                            .position(|p| p.row == row && p.column == column)
                            .unwrap_or(0),
                        None,
                    ),
                    T::Axis(axis) => (
                        PlotSettingsTab::Axes,
                        Some(match axis.side {
                            stokesforge_plot::AxisSide::Bottom => PlotAxis::Bottom,
                            // Axe réservé dans le modèle, sans page ni rendu.
                            stokesforge_plot::AxisSide::Top => return,
                            stokesforge_plot::AxisSide::Left => PlotAxis::Left,
                            stokesforge_plot::AxisSide::Right => PlotAxis::Right,
                        }),
                        spec.panels
                            .iter()
                            .position(|p| p.row == axis.row && p.column == axis.column)
                            .unwrap_or(0),
                        None,
                    ),
                    T::Text(TextTarget::PanelTitle { row, column }) => (
                        PlotSettingsTab::PlotArea,
                        None,
                        spec.panels
                            .iter()
                            .position(|p| p.row == row && p.column == column)
                            .unwrap_or(0),
                        None,
                    ),
                    _ => (PlotSettingsTab::Graph, None, 0, None),
                };
                self.open_plot_settings(graph, tab, axis);
                if let Some(edit) = self.plot_edit.as_mut() {
                    edit.selected_panel = panel;
                    if let Some(series) = series {
                        edit.selected_series = series;
                    }
                }
            }
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
            Action::ExportGraph { path, spec } => self.export_graph(path, &spec),
            Action::CopyGraphFigure { spec } => self.copy_graph_figure(&spec),
            Action::SetAnnotation {
                graph,
                target,
                annotation,
            } => self.edit_annotations(graph, |spec| target.set(spec, *annotation)),
            Action::RemoveAnnotation { graph, target } => {
                self.edit_plot_object(graph, PlotEditKind::RemoveText, |spec| target.remove(spec))
            }
            Action::SetShape {
                graph,
                target,
                shape,
            } => self.edit_annotations(graph, |spec| target.set(spec, *shape)),
            Action::RemoveShape { graph, target } => {
                self.edit_plot_object(graph, PlotEditKind::RemoveObject, |spec| {
                    target.remove(spec)
                })
            }
            Action::ReorderShape {
                graph,
                target,
                order,
            } => self.edit_plot_object(graph, PlotEditKind::ReorderObject, |spec| match order {
                ShapeOrder::ToFront => target.bring_to_front(spec).is_some(),
                ShapeOrder::ToBack => target.send_to_back(spec).is_some(),
            }),
            Action::SetTextAnchor {
                graph,
                target,
                anchor,
            } => self.edit_annotations(graph, |spec| target.set_anchor(spec, anchor)),
            Action::SetTextFont {
                graph,
                target,
                font,
            } => self.edit_annotations(graph, |spec| target.set_font(spec, font)),
            Action::SetTextColor {
                graph,
                target,
                color,
            } => self.edit_annotations(graph, |spec| target.set_color(spec, color)),
            Action::ResetTextAnchors { graph } => {
                self.edit_annotations(graph, |spec| spec.reset_text_anchors() > 0)
            }
            Action::SetTextContent {
                graph,
                target,
                text,
            } => self.edit_annotations(graph, |spec| target.set_text(spec, text)),
            Action::SetLegendEntryName {
                graph,
                series,
                name,
            } => self.edit_plot_object(graph, PlotEditKind::RenameSeries, |spec| {
                spec.series_at_mut(series)
                    .map(|series| series.name = name)
                    .is_some()
            }),
            Action::SetPngExportScale(scale) => {
                if PNG_EXPORT_SCALES.contains(&scale) {
                    self.png_export_scale = scale;
                }
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
                if self.snapshot.epoch != snapshot.epoch {
                    self.renaming_sheet = None;
                    self.pending_sheet_delete = None;
                    self.pending_object_delete.clear();
                    self.object_clipboard_output = None;
                    self.copied_objects.clear();
                }
                self.snapshot = snapshot;
                self.revalidate_selection();
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
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
                Err(error) => self.fail(errors::script::error_message(&error)),
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

    /// Ouvre la fenêtre d'import pour `book`. Charge le [`stokesforge_import::
    /// RecipeStore`] depuis le disque une fois pour toute la durée
    /// d'ouverture de la fenêtre (jamais rechargé à chaque image) et
    /// démarre sur son premier modèle enregistré s'il y en a un, sinon sur
    /// une recette permissive par défaut ([`stokesforge_import::ImportRecipe::
    /// new`]). Remplace sans confirmation une fenêtre d'import déjà ouverte
    /// (même patron que [`AppState::begin_edit`] pour une saisie de
    /// cellule).
    pub fn open_import(&mut self, book: impl Into<Option<NodeId>>) {
        let active_book = book.into();
        // Les modèles fournis d'office sont proposés, mais on démarre sur le
        // premier modèle **de l'utilisateur**, sinon sur la recette permissive.
        let store = stokesforge_import::RecipeStore::load_with_builtins();
        let recipe = store
            .recipes()
            .iter()
            .find(|recipe| !stokesforge_import::RecipeStore::is_builtin(recipe))
            .cloned()
            .unwrap_or_else(|| {
                stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"))
            });
        let mut recipe = recipe;
        recipe.unify_header();
        let save_as_name = recipe.name.clone();
        self.import_state = Some(ImportState {
            active_book,
            destination: active_book.map_or(
                ImportDestination::NewBookForAll,
                ImportDestination::ActiveBook,
            ),
            paths: Vec::new(),
            store,
            save_as_name,
            step: ImportStep::Files,
            preview_index: 0,
            confirm_delete: None,
            recent: stokesforge_import::RecentFiles::load(),
            baseline: recipe.clone(),
            recipe,
            bytes: Default::default(),
        });
    }

    /// Traite un dépôt de fichiers ([`Action::DropFiles`]). Les `.stf` sont mis
    /// en file pour ouverture ([`AppState::take_projects_to_open`]). Les
    /// autres fichiers sont lus avec le modèle reconnu, sinon le dernier utilisé, à défaut la
    /// recette permissive ; ceux qui se lisent correctement partent dans un
    /// seul import annulable, les autres ouvrent la fenêtre d'import
    /// pré-remplie pour être réglés à la main.
    fn drop_files(&mut self, paths: Vec<PathBuf>, target_book: Option<NodeId>) {
        // Un dépôt OS n'est pas bloqué par le calque modal d'egui, notamment
        // depuis l'explorateur détaché. La racine possède la macro du graphe :
        // l'import y serait absorbé si on ne refusait pas ce geste ici.
        if self.plot_settings.is_some() {
            self.fail(
                "Validez ou annulez les propriétés du graphe avant de déposer des fichiers".into(),
            );
            return;
        }
        let plan = file_drop_plan(&paths, target_book);
        self.projects_to_open.extend(plan.projects);
        // L'assistant est ouvert : le dépôt lui ajoute les fichiers au lieu
        // de les importer par-dessus.
        if self.import_state.is_some() {
            self.add_import_paths(plan.data);
            return;
        }
        if plan.data.is_empty() {
            return;
        }
        // Un modèle par fichier : le choix automatique (extension, nom,
        // signature de contenu), sinon le dernier modèle réellement utilisé,
        // sinon la recette permissive.
        let store = stokesforge_import::RecipeStore::load_with_builtins();
        let fallback = self
            .last_import
            .as_ref()
            .map(|last| last.recipe.clone())
            .unwrap_or_else(|| {
                stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"))
            });
        let mut items: Vec<ImportItem> = Vec::new();
        let mut bad: Vec<PathBuf> = Vec::new();
        let mut models: Vec<String> = Vec::new();
        for path in plan.data {
            let mut recipe = std::fs::read(&path)
                .ok()
                .and_then(|bytes| {
                    stokesforge_import::choose_recipe(store.recipes(), &path, &bytes).cloned()
                })
                .unwrap_or_else(|| fallback.clone());
            // Le geste du dépôt ajoute une feuille par fichier : les options
            // de remplacement ou de fusion du dernier assistant ne doivent
            // pas modifier sa destination ni écraser une feuille existante.
            recipe.import_mode = stokesforge_import::ImportMode::NewSheet;
            recipe.files_layout = stokesforge_import::FilesLayout::SheetPerFile;
            if reads_well(&path, &recipe) {
                if !models.contains(&recipe.name) {
                    models.push(recipe.name.clone());
                }
                items.extend(import_items(&path, &recipe));
            } else {
                bad.push(path);
            }
        }
        if !items.is_empty() {
            self.apply_import(plan.destination, items);
            let summary = self
                .status
                .as_ref()
                .filter(|status| !status.is_error)
                .map(|status| format!("{} — modèle « {} »", status.text, models.join("», «")));
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

    /// Lance l'analyse des fichiers sur un fil de fond ([`Action::StartImport`]).
    fn start_import(
        &mut self,
        destination: ImportDestination,
        recipe: stokesforge_import::ImportRecipe,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }
        if self.import_job.is_some() {
            self.fail("Un import est déjà en cours".to_string());
            return;
        }
        let total = paths.len();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (reply_tx, reply) = mpsc::channel();
        let worker_done = Arc::clone(&done);
        let worker_recipe = recipe.clone();
        let spawned = thread::Builder::new()
            .name("stokesforge-app-import".to_owned())
            .spawn(move || {
                let mut parsed: Vec<ImportItem> = Vec::new();
                for path in paths {
                    parsed.extend(import_items(&path, &worker_recipe));
                    worker_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                let _ = reply_tx.send(parsed);
            });
        match spawned {
            Ok(_) => {
                self.import_job = Some(ImportJob {
                    destination,
                    total,
                    done,
                    reply,
                });
                self.notice(format!("Import en cours… 0/{total} fichier(s)"));
            }
            Err(err) => self.fail(format!("Impossible de lancer l'import : {err}")),
        }
    }

    /// Avancement de l'import en cours : `(fichiers analysés, total)`.
    pub fn import_progress(&self) -> Option<(usize, usize)> {
        let job = self.import_job.as_ref()?;
        Some((
            job.done.load(std::sync::atomic::Ordering::Relaxed),
            job.total,
        ))
    }

    /// À appeler à chaque image : met à jour la progression et, quand
    /// l'analyse est finie, crée les feuilles. Rend vrai si l'import vient de
    /// s'appliquer. Ne bloque jamais.
    pub fn poll_import(&mut self) -> bool {
        let Some(job) = &self.import_job else {
            return false;
        };
        match job.reply.try_recv() {
            Ok(parsed) => {
                let Some(job) = self.import_job.take() else {
                    return false;
                };
                self.apply_import(job.destination, parsed);
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                let (done, total) = (
                    job.done.load(std::sync::atomic::Ordering::Relaxed),
                    job.total,
                );
                self.notice(format!("Import en cours… {done}/{total} fichier(s)"));
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.import_job = None;
                self.fail("L'import s'est interrompu avant la fin".to_string());
                false
            }
        }
    }

    /// Fichiers importés avec succès depuis le dernier appel, pour les
    /// fichiers récents (vidés : chaque fichier n'est inscrit qu'une fois).
    pub fn take_imported_paths(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.recent_to_record)
    }

    /// Au moins une colonne de `sheet` vient d'un fichier qu'on peut relire.
    pub fn can_refresh_from_source(&self, sheet: NodeId) -> bool {
        self.visible_columns(sheet)
            .iter()
            .any(|(column, ..)| self.column_meta(*column).import_source.is_some())
    }

    /// [`Action::RefreshFromSource`] : relit les fichiers source des colonnes de
    /// `sheet` et réécrit leurs valeurs en place, en une seule entrée
    /// d'historique. Une colonne dont le fichier a disparu ou ne se lit plus est
    /// laissée telle quelle, avec un message.
    fn refresh_from_source(&mut self, sheet: NodeId) {
        let sources: Vec<(NodeId, stokesforge_core::ImportSource)> = self
            .visible_columns(sheet)
            .iter()
            .filter_map(|(column, ..)| {
                self.column_meta(*column)
                    .import_source
                    .map(|source| (*column, source))
            })
            .collect();
        if sources.is_empty() {
            self.fail("Cette feuille ne contient aucune colonne importée d'un fichier".to_string());
            return;
        }
        // Un fichier source n'est relu qu'une fois, même s'il alimente plusieurs
        // colonnes.
        let mut items: Vec<ImportItem> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        let mut read_paths: Vec<String> = Vec::new();
        for (_, source) in &sources {
            if read_paths.contains(&source.path) {
                continue;
            }
            read_paths.push(source.path.clone());
            let recipe = match stokesforge_import::ImportRecipe::from_json(&source.recipe_json) {
                Ok(recipe) => recipe,
                Err(message) => {
                    failures.push(format!("{}: {message}", source.path));
                    continue;
                }
            };
            for item in import_items(&PathBuf::from(&source.path), &recipe) {
                if let Err(message) = &item.table {
                    failures.push(errors::import::failure_message(message));
                } else {
                    items.push(item);
                }
            }
        }
        // Chaque colonne retrouve son bloc (fichier, rang) puis sa colonne.
        let refreshed: Vec<(NodeId, PlannedColumn)> = sources
            .iter()
            .filter_map(|(column, source)| {
                let item = items.iter().position(|item| {
                    item.path.display().to_string() == source.path
                        && item.block.unwrap_or(0) == source.block
                })?;
                Some((
                    *column,
                    PlannedColumn {
                        item,
                        column: source.column,
                    },
                ))
            })
            .collect();
        if refreshed.is_empty() {
            self.fail(format!(
                "Actualisation impossible — {}",
                failures.join(" ; ")
            ));
            return;
        }
        if self.handle.begin_macro(MacroKind::RefreshFromFile).is_err() {
            self.fail("Impossible de grouper l'actualisation".to_string());
            return;
        }
        // Les lignes de la feuille suivent le plus long des nouveaux contenus ;
        // elles ne raccourcissent que si **toutes** les colonnes sont actualisées
        // (on ne tronque jamais une colonne saisie à la main).
        let new_len = refreshed
            .iter()
            .filter_map(|(_, planned)| stokesforge_importer::import_block(&items, *planned))
            .map(|block| match block {
                ValueBlock::Float(values) => values.len(),
                ValueBlock::Mixed(cells) => cells.len(),
            })
            .max()
            .unwrap_or(0);
        let rows = self.row_count(sheet);
        let all_refreshed = refreshed.len() == self.visible_columns(sheet).len();
        if new_len > rows {
            self.execute(Command::InsertRows {
                sheet,
                at: rows,
                count: new_len - rows,
            });
        } else if new_len < rows && all_refreshed {
            self.execute(Command::DeleteRows {
                sheet,
                at: new_len,
                count: rows - new_len,
            });
        }
        let mut updated = 0usize;
        for (column, planned) in &refreshed {
            let Some(mut block) = stokesforge_importer::import_block(&items, *planned) else {
                continue;
            };
            let length = self
                .snapshot
                .tree
                .node(*column)
                .and_then(Node::as_column)
                .map_or(0, Column::len);
            // Les cases au-delà du nouveau contenu sont vidées, et rien n'est
            // écrit au-delà de la colonne.
            match &mut block {
                ValueBlock::Float(values) => {
                    values.resize(length, f64::NAN);
                }
                ValueBlock::Mixed(cells) => cells.resize(length, MixedCell::Empty),
            }
            self.execute(Command::SetCellsPromoting {
                column: *column,
                start: 0,
                values: block,
                expected_version: None,
            });
            updated += 1;
        }
        if self.handle.end_macro().is_err() {
            failures.push("échec de la clôture de l'entrée d'historique".to_string());
        }
        self.refresh();
        let summary = format!("{updated} colonne(s) actualisée(s) depuis leur fichier");
        if failures.is_empty() {
            self.notice(summary);
        } else {
            self.fail(format!("{summary} — {}", failures.join(" ; ")));
        }
    }

    /// Un dernier import existe et peut être repris dans l'assistant.
    pub fn can_reopen_last_import(&self) -> bool {
        self.last_import.is_some() && self.import_job.is_none()
    }

    /// [`Action::ReopenLastImport`] : annule le dernier import s'il est encore
    /// au sommet de l'historique (sinon la correction s'ajouterait au lieu de
    /// le remplacer), puis rouvre l'assistant sur ses fichiers et sa recette.
    fn reopen_last_import(&mut self) {
        let Some(last) = self.last_import.clone() else {
            self.fail("Aucun import à reprendre".to_string());
            return;
        };
        let undone = self.snapshot.history.undo_label
            == Some(HistoryLabel::Macro(MacroKind::Import))
            && !self.snapshot.history.in_macro;
        if undone {
            self.undo();
        }
        let book = match last.destination {
            ImportDestination::ActiveBook(book) => Some(book),
            _ => None,
        };
        self.open_import(book);
        if let Some(import) = &mut self.import_state {
            import.recipe = last.recipe.clone();
            import.baseline = last.recipe;
            import.save_as_name = import.recipe.name.clone();
            if let ImportDestination::NewBookPerFile | ImportDestination::NewBookForAll =
                last.destination
            {
                import.destination = last.destination;
            }
            import.paths = last.paths;
        }
        self.notice(if undone {
            "Dernier import annulé : réglez-le puis relancez-le".to_string()
        } else {
            "Réglez le dernier import puis relancez-le (l'ancien reste en place)".to_string()
        });
    }

    /// Colonne dont la fenêtre des métadonnées est ouverte, s'il y en a une.
    pub fn column_metadata_view(&self) -> Option<NodeId> {
        self.metadata_view
    }

    /// Résumé de la case « Métadonnées » de l'en-tête (« 23 clés »), vide si
    /// la colonne n'en porte pas. Les titres de section ne comptent pas.
    pub fn file_metadata_summary(&self, column: NodeId) -> String {
        let count = self
            .column_meta(column)
            .file_metadata
            .iter()
            .filter(|line| !line.is_section())
            .count();
        if count == 0 {
            return String::new();
        }
        crate::tr!("workbook-metadata-summary", count = count)
    }

    /// Projets `.stf` déposés depuis le dernier appel, à ouvrir chacun dans
    /// une nouvelle fenêtre StokesForge (vidés : un projet ne s'ouvre qu'une fois).
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
            state.recipe = stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"));
            state.baseline = state.recipe.clone();
            state.save_as_name = state.recipe.name.clone();
        }
    }

    /// Charge le modèle `name` du `RecipeStore` dans la recette en cours
    /// d'édition (sélection dans la liste déroulante). Sans effet si `name`
    /// n'est pas un modèle enregistré, ou si la fenêtre n'est pas ouverte.
    pub fn select_import_recipe(&mut self, name: &str) {
        if let Some(state) = &mut self.import_state {
            if let Some(recipe) = state.store.recipes().iter().find(|r| r.name == name) {
                let mut recipe = recipe.clone();
                recipe.unify_header();
                state.recipe = recipe.clone();
                state.baseline = recipe.clone();
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
            state.preview_index = 0;
            state.bytes.borrow_mut().clear();
        }
    }

    /// Ajoute `paths` aux fichiers de l'assistant (dépôt pendant qu'il est
    /// ouvert, « Ajouter des fichiers… »), sans doublon.
    ///
    /// Au **premier** ajout, si le modèle courant n'a pas été modifié, le
    /// choix automatique du modèle (extension, nom, signature de contenu)
    /// peut le remplacer : le message dit lequel a été choisi.
    pub fn add_import_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        let was_empty = self
            .import_state
            .as_ref()
            .is_some_and(|state| state.paths.is_empty());
        if let Some(state) = &mut self.import_state {
            for path in paths {
                if !state.paths.contains(&path) {
                    state.paths.push(path);
                }
            }
        }
        if was_empty && !self.import_recipe_modified() {
            self.choose_import_recipe_automatically();
        }
    }

    /// Choisit, d'après le fichier montré, le modèle qui lui correspond
    /// (`stokesforge_import::choose_recipe`) et le charge comme recette courante.
    /// Sans effet si aucun modèle ne correspond.
    fn choose_import_recipe_automatically(&mut self) {
        let Some(path) = self.import_preview_path() else {
            return;
        };
        let Ok(bytes) = self.import_bytes(&path) else {
            return;
        };
        let Some(state) = &self.import_state else {
            return;
        };
        let Some(chosen) =
            stokesforge_import::choose_recipe(state.store.recipes(), &path, &bytes).cloned()
        else {
            return;
        };
        let name = chosen.name.clone();
        self.select_import_recipe(&name);
        self.notice(format!("Modèle « {name} » choisi d'après le fichier"));
    }

    /// Écrit le modèle courant dans `path` (JSON) pour le partager.
    pub fn export_import_recipe(&mut self, path: &std::path::Path) {
        let Some(state) = &self.import_state else {
            return;
        };
        let json = state.recipe.to_json();
        match std::fs::write(path, json) {
            Ok(()) => self.notice(format!("Modèle exporté dans {}", path.display())),
            Err(err) => self.fail(format!("Export du modèle impossible : {err}")),
        }
    }

    /// Lit un modèle exporté dans `path`, l'ajoute à la liste (en remplaçant
    /// celui de même nom) et l'enregistre, puis en fait la recette courante.
    pub fn import_import_recipe(&mut self, path: &std::path::Path) {
        let recipe = std::fs::read_to_string(path)
            .map_err(|err| format!("{}: {err}", path.display()))
            .and_then(|text| stokesforge_import::ImportRecipe::from_json(&text));
        let mut recipe = match recipe {
            Ok(recipe) => recipe,
            Err(message) => {
                self.fail(message);
                return;
            }
        };
        recipe.unify_header();
        let Some(state) = &mut self.import_state else {
            return;
        };
        let name = recipe.name.clone();
        state.store.upsert(recipe.clone());
        let saved = state.store.save();
        state.baseline = recipe.clone();
        state.recipe = recipe;
        state.save_as_name = name.clone();
        match saved {
            Ok(()) => self.notice(format!("Modèle « {name} » importé")),
            Err(err) => self.fail(format!("Modèle importé mais non enregistré : {err}")),
        }
    }

    /// Retire le fichier `index` de la liste de l'assistant.
    pub fn remove_import_path(&mut self, index: usize) {
        if let Some(state) = &mut self.import_state {
            if index < state.paths.len() {
                let path = state.paths.remove(index);
                state.bytes.borrow_mut().remove(&path);
                state.preview_index = state.preview_index.min(state.paths.len().saturating_sub(1));
            }
        }
    }

    /// Choisit le fichier montré dans l'aperçu.
    pub fn set_import_preview_index(&mut self, index: usize) {
        if let Some(state) = &mut self.import_state {
            if index < state.paths.len() {
                state.preview_index = index;
            }
        }
    }

    /// Passe à l'étape suivante / précédente de l'assistant.
    pub fn import_step_forward(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.step = state.step.next();
        }
    }

    /// Voir [`AppState::import_step_forward`].
    pub fn import_step_back(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.step = state.step.previous();
        }
    }

    /// Octets du fichier `path`, lus une seule fois puis gardés : l'aperçu
    /// est recalculé à chaque image, le disque ne doit pas l'être.
    fn import_bytes(&self, path: &std::path::Path) -> Result<Arc<Vec<u8>>, String> {
        let state = self.import_state.as_ref().ok_or("aucun import en cours")?;
        let mut cache = state.bytes.borrow_mut();
        cache
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                std::fs::read(path)
                    .map(Arc::new)
                    .map_err(|err| format!("{}: {err}", path.display()))
            })
            .clone()
    }

    /// Fichier montré dans l'aperçu.
    fn import_preview_path(&self) -> Option<PathBuf> {
        let state = self.import_state.as_ref()?;
        state
            .paths
            .get(state.preview_index)
            .or_else(|| state.paths.first())
            .cloned()
    }

    /// Encodage détecté du fichier montré dans l'aperçu.
    pub fn import_encoding(&self) -> Option<&'static str> {
        let path = self.import_preview_path()?;
        let bytes = self.import_bytes(&path).ok()?;
        stokesforge_import::detect_encoding(&bytes)
    }

    /// Ce que l'import donnerait pour chaque fichier choisi, avec la recette
    /// courante : `Ok` avec le nombre de lignes, ou l'erreur qui l'attend —
    /// montré **avant** le clic sur Importer.
    pub fn import_file_statuses(&self) -> Vec<(PathBuf, Result<usize, String>)> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        state
            .paths
            .iter()
            .map(|path| {
                let status = self.import_bytes(path).and_then(|bytes| {
                    stokesforge_import::parse(&bytes, &state.recipe)
                        .map(|table| table.row_count())
                        .map_err(|err| errors::import::error_message(&err))
                });
                (path.clone(), status)
            })
            .collect()
    }

    /// Le modèle courant diffère de celui qui a été chargé ou enregistré.
    pub fn import_recipe_modified(&self) -> bool {
        self.import_state
            .as_ref()
            .is_some_and(|state| state.recipe != state.baseline)
    }

    /// Demande la suppression du modèle `name` (elle n'a lieu qu'à la
    /// confirmation, [`AppState::confirm_delete_import_recipe`]).
    pub fn request_delete_import_recipe(&mut self, name: &str) {
        if let Some(state) = &mut self.import_state {
            state.confirm_delete = Some(name.to_string());
        }
    }

    /// Annule la demande de suppression.
    pub fn cancel_delete_import_recipe(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.confirm_delete = None;
        }
    }

    /// Supprime le modèle dont la suppression a été demandée.
    pub fn confirm_delete_import_recipe(&mut self) {
        let name = self
            .import_state
            .as_mut()
            .and_then(|state| state.confirm_delete.take());
        if let Some(name) = name {
            self.delete_import_recipe(&name);
        }
    }

    /// Colonnes du résultat d'import du fichier montré, avec leurs premières
    /// valeurs — étape « Colonnes ».
    pub fn import_column_previews(&self) -> Option<Result<Vec<ImportColumnPreview>, String>> {
        let table = self.import_preview()?;
        Some(table.map(|table| {
            table
                .columns
                .iter()
                .map(|column| ImportColumnPreview {
                    name: column.name.clone().unwrap_or_default(),
                    long_name: column.long_name.clone().unwrap_or_default(),
                    unit: column.unit.clone().unwrap_or_default(),
                    mixed: column.texts.iter().any(Option::is_some),
                    samples: column
                        .values
                        .iter()
                        .zip(&column.texts)
                        .take(3)
                        .map(|(value, text)| match text {
                            Some(text) => text.clone(),
                            None if value.is_nan() => String::new(),
                            None => format!("{value}"),
                        })
                        .collect(),
                })
                .collect()
        }))
    }

    /// Toutes les métadonnées que le fichier montré pourrait fournir (en-tête
    /// et pied de page, sans exclusion) — étape « Métadonnées », pour cocher
    /// ce qu'on garde.
    pub fn import_available_metadata(&self) -> Vec<stokesforge_import::MetadataLine> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        let Some(path) = self.import_preview_path() else {
            return Vec::new();
        };
        let Ok(bytes) = self.import_bytes(&path) else {
            return Vec::new();
        };
        let mut recipe = state.recipe.clone();
        recipe.keep_header_metadata = true;
        recipe.keep_footer_metadata = true;
        recipe.excluded_metadata.clear();
        stokesforge_import::parse(&bytes, &recipe)
            .map(|table| table.metadata().to_vec())
            .unwrap_or_default()
    }

    /// Réglage depuis un clic droit sur la ligne `number` (1-based, numéro
    /// dans le fichier) de l'aperçu. Les lignes blanches de tête ne comptent
    /// pas dans l'en-tête (comme à l'import) : le décalage vient de la
    /// première ligne montrée.
    pub fn import_line_action(&mut self, action: ImportLineAction, number: usize) {
        let Some(Ok(preview)) = self.import_line_preview() else {
            return;
        };
        let Some(first) = preview.lines.first().map(|line| line.number) else {
            return;
        };
        let Some(state) = &mut self.import_state else {
            return;
        };
        let recipe = &mut state.recipe;
        let header_total =
            recipe.header_layout.main_header_lines + recipe.header_layout.subheader_lines;
        let data_start = first + header_total + usize::from(recipe.has_column_header_row);
        let offset = number.saturating_sub(first);
        match action {
            ImportLineAction::DataStartsHere => {
                recipe.header = stokesforge_import::HeaderSkip::None;
                let sub = recipe.header_layout.subheader_lines.min(offset);
                recipe.header_layout.subheader_lines = sub;
                recipe.header_layout.main_header_lines = offset - sub;
                recipe.header_layout.auto_detect = false;
            }
            ImportLineAction::UnitsLine => {
                // La ligne et celles qui la suivent, jusqu'aux données,
                // forment le sous-en-tête ; la ligne donne les unités.
                recipe.header = stokesforge_import::HeaderSkip::None;
                let total = header_total.max(offset + 1);
                recipe.header_layout.main_header_lines = offset;
                recipe.header_layout.subheader_lines = total - offset;
                recipe.header_layout.auto_detect = false;
                recipe.subheader_roles.same_line_name_unit = false;
                recipe.subheader_roles.unit =
                    Some(stokesforge_import::SubheaderLineRange::single(1));
            }
            ImportLineAction::DataEndsHere => {
                if number >= data_start {
                    recipe.data_end =
                        stokesforge_import::DataEnd::LinesAfterHeader(number - data_start);
                }
            }
            ImportLineAction::NoAutoCut => recipe.data_end = stokesforge_import::DataEnd::None,
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
        // Le témoin « modifié » compare au dernier enregistrement réussi,
        // pas seulement au modèle chargé à l'ouverture. En cas d'échec,
        // garder l'ancienne référence pour ne pas annoncer une sauvegarde.
        state.baseline = state.recipe.clone();
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
    pub fn import_preview(&self) -> Option<Result<stokesforge_import::ParsedTable, String>> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        let result = self.import_bytes(&path).and_then(|bytes| {
            stokesforge_import::parse(&bytes, &state.recipe)
                .map_err(|err| errors::import::error_message(&err))
        });
        Some(result)
    }

    /// Aperçu du découpage en lignes (en-tête/sous-en-tête/données/pied de
    /// page) du **premier** fichier choisi, avec les réglages courants —
    /// voir [`stokesforge_import::preview::preview`] pour le budget de coût.
    /// Recalculé à chaque appel, comme [`AppState::import_preview`] : ce
    /// budget est précisément ce qui rend ce recalcul par image acceptable,
    /// y compris sur un gros fichier (le travail cellule à cellule reste
    /// borné à `recipe.preview_lines`, jamais à la taille du fichier). `None`
    /// dans les mêmes conditions qu'`import_preview`.
    pub fn import_line_preview(&self) -> Option<Result<stokesforge_import::FilePreview, String>> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        let result = self.import_bytes(&path).and_then(|bytes| {
            stokesforge_import::preview(&bytes, &state.recipe)
                .map_err(|err| errors::import::error_message(&err))
        });
        Some(result)
    }

    /// Réévalue `header_layout` (nombre de lignes d'en-tête principal et de
    /// sous-en-tête) depuis le contenu du **premier** fichier choisi —
    /// équivalent du bouton de réévaluation automatique d'OriginLab
    /// ([`stokesforge_import::preview::detect_header_layout`]) : une action
    /// explicite, pas un recalcul à chaque image, pour ne jamais écraser une
    /// valeur que l'utilisateur vient de saisir à la main. Sans effet si la
    /// fenêtre n'est pas ouverte ou si aucun fichier n'a encore été choisi.
    pub fn auto_detect_import_header(&mut self) {
        let Some(import) = &self.import_state else {
            return;
        };
        let Some(path) = self.import_preview_path() else {
            self.fail("Choisis un fichier avant de détecter l'en-tête".to_string());
            return;
        };
        let (separator, decimal) = (import.recipe.separator.clone(), import.recipe.decimal);
        match self.import_bytes(&path).map(|bytes| bytes.as_ref().clone()) {
            Ok(bytes) => {
                let layout = stokesforge_import::detect_header_layout(&bytes, separator, decimal);
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
    /// contextuel (voir `app::grid::system_clipboard_text`) : cet échec
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
                self.notice(history_text(&label));
                self.refresh();
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    fn undo(&mut self) {
        // Le libellé disparaît de la pile dès que l'annulation est faite : il
        // faut le lire avant.
        let label = self.snapshot.history.undo_label.clone();
        match self.handle.undo() {
            Ok(Some(_)) => {
                self.notice(match label {
                    Some(label) => tr!("history-undone", label = history_text(&label)),
                    None => tr!("history-undone-last"),
                });
                self.refresh();
            }
            Ok(None) => self.notice(tr!("status-nothing-to-undo")),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    fn redo(&mut self) {
        let label = self.snapshot.history.redo_label.clone();
        match self.handle.redo() {
            Ok(Some(_)) => {
                self.notice(match label {
                    Some(label) => tr!("history-redone", label = history_text(&label)),
                    None => tr!("history-redone-last"),
                });
                self.refresh();
            }
            Ok(None) => self.notice(tr!("status-nothing-to-redo")),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    fn save(&mut self) {
        let path = PathBuf::from(&self.path);
        match self.handle.save(&path) {
            Ok(()) => {
                self.refresh();
                self.saved_revision =
                    Some((self.snapshot.revision, self.snapshot.geometry_revision));
                let text = crate::tr!("status-saved", path = self.path.as_str());
                self.notice(text);
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// Applique `change` à un clone du spec de `graph` et l'envoie au modèle
    /// ([`Command::SetPlotSpec`]) s'il a réellement changé quelque chose.
    fn edit_annotations(&mut self, graph: NodeId, change: impl FnOnce(&mut PlotSpec) -> bool) {
        let Some(mut spec) = self.plot_spec(graph) else {
            return;
        };
        let before = spec.clone();
        if change(&mut spec) && spec != before {
            self.execute(Command::SetPlotSpec { graph, spec });
        }
    }

    /// Même delta que les propriétés ; le motif précise le geste dans l'historique.
    fn edit_plot_object(
        &mut self,
        graph: NodeId,
        kind: PlotEditKind,
        change: impl FnOnce(&mut PlotSpec) -> bool,
    ) {
        let Some(mut spec) = self.plot_spec(graph) else {
            return;
        };
        let before = spec.clone();
        if change(&mut spec) && spec != before {
            self.execute(Command::EditPlotSpec { graph, spec, kind });
        }
    }

    /// L'agrandissement des exports PNG.
    pub fn png_export_scale(&self) -> f32 {
        self.png_export_scale
    }

    /// Voir [`Action::ExportGraph`]. Mesure et compose comme l'écran : même
    /// taille logique (`DEFAULT_SIZE`, celle où `GraphView::paint_scaled`
    /// met la figure en page), mêmes polices.
    fn export_graph(&mut self, mut path: PathBuf, spec: &PlotSpec) {
        if path.extension().is_none() {
            path.set_extension("svg");
        }
        let Some(format) =
            stokesforge_render_svg::ExportFormat::from_path(&path, self.png_export_scale)
        else {
            self.fail(format!(
                "Format d'export inconnu pour {} : choisir .svg, .pdf ou .png",
                path.display()
            ));
            return;
        };
        let plot = stokesforge_core::resolve::resolve_spec(&self.snapshot.tree, spec);
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        match stokesforge_render_svg::export_to_file(
            &path,
            &plot,
            stokesforge_render_svg::DEFAULT_SIZE,
            &measurer,
            &stokesforge_render_svg::FontChoice::default(),
            format,
        ) {
            Ok(()) => self.notice(format!("Graphe exporté dans {}", path.display())),
            Err(err) => self.fail(errors::export::error_message(&err)),
        }
    }

    /// Voir [`Action::CopyGraphFigure`]. Mêmes taille, mesure et polices que
    /// l'export vers un fichier ; la figure n'est composée qu'une fois pour
    /// les trois formats. Un format qui échoue seul est omis et nommé dans le
    /// message ; si aucun n'est prêt, rien n'est offert.
    fn copy_graph_figure(&mut self, spec: &PlotSpec) {
        use stokesforge_clipboard::{mime, Representation};
        use stokesforge_render_svg::ExportFormat;

        let plot = stokesforge_core::resolve::resolve_spec(&self.snapshot.tree, spec);
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        let wanted = [
            ("SVG", mime::SVG, ExportFormat::Svg),
            (
                "PNG",
                mime::PNG,
                ExportFormat::Png {
                    scale: self.png_export_scale,
                },
            ),
            ("PDF", mime::PDF, ExportFormat::Pdf),
        ];
        let formats: Vec<ExportFormat> = wanted.iter().map(|(_, _, format)| *format).collect();
        let results = match stokesforge_render_svg::export_bytes_many(
            &plot,
            stokesforge_render_svg::DEFAULT_SIZE,
            &measurer,
            &stokesforge_render_svg::FontChoice::default(),
            &formats,
        ) {
            Ok(results) => results,
            Err(err) => {
                self.fail(errors::export::error_message(&err));
                return;
            }
        };
        let mut representations = Vec::new();
        let mut offered = Vec::new();
        let mut missing = Vec::new();
        let mut first_error = None;
        for ((name, mime, _), result) in wanted.iter().zip(results) {
            match result {
                Ok(bytes) => {
                    representations.push(Representation::new(mime, bytes));
                    offered.push(*name);
                }
                Err(err) => {
                    missing.push(*name);
                    first_error.get_or_insert(err);
                }
            }
        }
        if let Some(err) = first_error.filter(|_| representations.is_empty()) {
            self.fail(errors::export::error_message(&err));
            return;
        }
        self.figure_clipboard = Some(representations);
        let text = if missing.is_empty() {
            tr!("status-graph-copied", formats = offered.join(", "))
        } else {
            tr!(
                "status-graph-copied-partly",
                formats = offered.join(", "),
                missing = missing.join(", ")
            )
        };
        self.notice(text);
    }

    /// Les représentations de la figure que [`Action::CopyGraphFigure`] vient de
    /// préparer, à offrir au presse-papiers système par la fenêtre qui a reçu
    /// le geste ; relevées une seule fois.
    pub fn take_figure_clipboard(&mut self) -> Option<Vec<stokesforge_clipboard::Representation>> {
        self.figure_clipboard.take()
    }

    /// Le système a refusé l'offre de la figure : le message « copié » cède la
    /// place à la cause.
    pub fn figure_clipboard_failed(&mut self, error: &stokesforge_clipboard::ClipboardError) {
        self.fail(errors::clipboard::error_message(error));
    }

    fn load(&mut self) {
        let path = PathBuf::from(&self.path);
        match self.handle.load(&path) {
            Ok(_) => {
                // Les identifiants du fichier n'ont aucun rapport avec ceux de
                // l'arbre précédent : la sélection doit être revalidée, ce que
                // fait `refresh`.
                self.editing = None;
                let text = crate::tr!("status-loaded", path = self.path.as_str());
                self.notice(text);
                self.refresh();
                self.saved_revision =
                    Some((self.snapshot.revision, self.snapshot.geometry_revision));
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// Le changement de projet ne doit pas recueillir un résultat d'import
    /// ou de script lancé sur l'ancien projet, ni interrompre une macro.
    pub fn can_new_project(&self) -> bool {
        !self.script_running() && self.import_job.is_none() && !self.snapshot.history.in_macro
    }

    pub fn new_project_pending(&self) -> bool {
        self.new_project_pending
    }

    fn request_new_project(&mut self, pending_edits: bool) {
        self.refresh();
        if !self.can_new_project() {
            return;
        }
        let has_content = self
            .snapshot
            .tree
            .node(self.snapshot.tree.root())
            .is_some_and(|node| !node.children().is_empty());
        let changed = self
            .saved_revision
            .map_or(has_content || self.snapshot.revision != 0, |saved| {
                saved != (self.snapshot.revision, self.snapshot.geometry_revision)
            });
        if changed
            || pending_edits
            || self.editing.is_some()
            || self.note_editing.is_some()
            || self.header_editing.is_some()
        {
            self.new_project_pending = true;
        } else {
            self.new_project();
        }
    }

    fn new_project(&mut self) {
        if !self.can_new_project() {
            return;
        }
        match self.handle.new_project() {
            Ok(_) => {
                self.path.clear();
                self.saved_revision = None;
                self.selected_sheets.clear();
                self.selection.clear();
                self.editing = None;
                self.header_editing = None;
                self.note_editing = None;
                self.plot_edit = None;
                self.plot_settings = None;
                self.metadata_view = None;
                self.import_state = None;
                self.last_import = None;
                self.delete_repeat = None;
                self.refresh();
                self.saved_revision =
                    Some((self.snapshot.revision, self.snapshot.geometry_revision));
                self.notice(tr!("action-new-project"));
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
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

    /// Transport à publier par le viewport qui reçoit le geste de copie.
    pub fn take_object_clipboard_text(&mut self) -> Option<String> {
        self.object_clipboard_output.take()
    }

    /// Copies créées : source, destination et conservation de l'état de vue.
    pub fn take_copied_objects(&mut self) -> Vec<(NodeId, NodeId, bool)> {
        std::mem::take(&mut self.copied_objects)
    }

    /// Objets en attente de confirmation de suppression.
    pub fn pending_object_delete(&self) -> &[NodeId] {
        &self.pending_object_delete
    }

    /// Graphes qui utilisent des colonnes des sous-arbres indiqués.
    pub fn object_users(&self, ids: &[NodeId]) -> Vec<String> {
        let columns: std::collections::HashSet<_> = ids
            .iter()
            .flat_map(|id| self.tree().walk(*id).unwrap_or_default())
            .collect();
        self.tree()
            .walk_all()
            .into_iter()
            .filter_map(|id| self.tree().node(id))
            .filter(|node| match node.kind() {
                NodeKind::Graph { spec, .. } => columns.iter().any(|id| spec.references(*id)),
                _ => false,
            })
            .map(|node| node.name().to_string())
            .collect()
    }

    fn clipboard_origin(&self) -> String {
        format!("{}:{}", self.object_clipboard_origin, self.snapshot.epoch)
    }

    // Éliminer doublons et descendants d'une autre cible empêche de supprimer
    // ou déplacer deux fois le même contenu. L'ordre reste celui de l'arbre.
    fn object_targets(&self, ids: &[NodeId]) -> Vec<NodeId> {
        self.tree()
            .walk_all()
            .into_iter()
            .filter(|id| ids.contains(id))
            .filter(|id| *id != self.tree().root())
            .filter(|id| {
                self.tree()
                    .node(*id)
                    .is_some_and(|n| n.kind().is_window() || n.kind().is_folder())
            })
            .filter(|id| {
                !self
                    .tree()
                    .ancestors(*id)
                    .unwrap_or_default()
                    .iter()
                    .any(|a| ids.contains(a))
            })
            .collect()
    }

    fn rename_object(&mut self, id: NodeId, name: &str) {
        if self.object_targets(&[id]).is_empty() {
            return;
        }
        let Some(node) = self.tree().node(id) else {
            return;
        };
        let name = name.trim();
        if node.name() == name {
            return;
        }
        if name.is_empty() {
            self.fail(tr!("object-rename-empty"));
            return;
        }
        self.execute(Command::Rename {
            id,
            name: name.into(),
        });
    }

    fn rename_objects(&mut self, changes: Vec<(NodeId, String)>) {
        let mut commands = Vec::new();
        for (id, name) in changes {
            let Some(node) = self.tree().node(id) else {
                continue;
            };
            if self.object_targets(&[id]).is_empty() {
                continue;
            }
            let name = name.trim();
            if name.is_empty() {
                self.fail(tr!("object-rename-empty"));
                return;
            }
            if name != node.name() {
                commands.push(Command::Rename {
                    id,
                    name: name.into(),
                });
            }
        }
        self.execute_object_group(MacroKind::RenameObjects, commands);
    }

    fn capture_objects(&self, ids: &[NodeId]) -> Vec<NodeCopy> {
        self.object_targets(ids)
            .into_iter()
            .filter_map(|id| NodeCopy::capture(self.tree(), id).ok())
            .collect()
    }

    /// Rend visibles et ouvre les classeurs de données donnés.
    fn reveal_books(&mut self, books: Vec<NodeId>) {
        for book in books {
            let Some(NodeKind::Workbook { meta, .. }) = self.tree().node(book).map(Node::kind)
            else {
                continue;
            };
            let mut meta = meta.clone();
            if meta.hidden {
                meta.hidden = false;
                self.execute(Command::SetWindowMeta { id: book, meta });
            }
            self.copied_objects.push((book, book, false));
        }
    }

    fn copy_objects(&mut self, ids: &[NodeId]) {
        let copies = self.capture_objects(ids);
        if copies.is_empty() {
            return;
        }
        match stokesforge_persistence::clipboard::encode_objects(&self.clipboard_origin(), &copies)
        {
            Ok(text) => self.object_clipboard_output = Some(text),
            Err(err) => self.fail(errors::file::error_message(&err)),
        }
    }

    fn next_object_copy_name(&self, parent: NodeId, origin: &str) -> String {
        (1u64..)
            .map(|number| {
                tr!(
                    "sheet-copy-name",
                    name = origin,
                    number = number.to_string()
                )
            })
            .find(|name| {
                self.tree()
                    .children(parent)
                    .unwrap_or_default()
                    .iter()
                    .all(|id| self.tree().node(*id).is_none_or(|node| node.name() != name))
            })
            .unwrap_or_else(|| origin.to_string())
    }

    fn duplicate_objects(&mut self, ids: &[NodeId]) {
        let copies = self.capture_objects(ids);
        if copies.is_empty() {
            return;
        }
        self.insert_object_copies(copies, None, true);
    }

    fn paste_objects(&mut self, parent: NodeId, text: &str) {
        let (origin, copies) = match stokesforge_persistence::clipboard::decode_objects(text) {
            Ok(value) => value,
            Err(_) => {
                self.fail(tr!("object-clipboard-invalid"));
                return;
            }
        };
        let copies = if origin != self.clipboard_origin() {
            copies.into_iter().map(NodeCopy::into_external).collect()
        } else {
            copies
        };
        self.insert_object_copies(copies, Some(parent), false);
    }

    fn insert_object_copies(
        &mut self,
        copies: Vec<NodeCopy>,
        parent: Option<NodeId>,
        duplicate: bool,
    ) {
        let kind = if duplicate {
            MacroKind::DuplicateObjects
        } else {
            MacroKind::PasteObjects
        };
        if self.handle.begin_macro(kind).is_err() {
            return;
        }
        let mut created = Vec::new();
        for copy in copies {
            let source = copy.root().id();
            let destination = parent
                .or(copy.root().parent())
                .unwrap_or(self.tree().root());
            let name = self.next_object_copy_name(destination, copy.root().name());
            let before = self
                .tree()
                .children(destination)
                .unwrap_or_default()
                .to_vec();
            let copy_kind = copy.root().kind().type_name();
            if let Err(err) = self.handle.execute(Command::InsertCopy {
                parent: destination,
                copy,
                name,
            }) {
                let _ = self.handle.abort_macro();
                self.refresh();
                self.fail(errors::session::error_message(&err));
                return;
            }
            self.refresh();
            if let Some(id) = self
                .tree()
                .children(destination)
                .unwrap_or_default()
                .iter()
                .find(|id| {
                    !before.contains(id)
                        && self
                            .tree()
                            .node(**id)
                            .is_some_and(|node| node.kind().type_name() == copy_kind)
                })
            {
                created.push((source, *id, duplicate));
            }
        }
        if let Err(err) = self.handle.end_macro() {
            self.fail(errors::session::error_message(&err));
        } else {
            self.copied_objects.extend(created);
            self.notice(history_text(&HistoryLabel::Macro(kind)));
        }
        self.refresh();
    }

    fn request_delete_objects(&mut self, ids: Vec<NodeId>) {
        let ids = self.object_targets(&ids);
        if ids.is_empty() {
            return;
        }
        if ids.len() > 1
            || ids.iter().any(|id| {
                self.tree()
                    .walk(*id)
                    .unwrap_or_default()
                    .iter()
                    .any(|child| {
                        self.tree()
                            .node(*child)
                            .is_some_and(|n| matches!(n.kind(), NodeKind::Workbook { .. }))
                    })
            })
        {
            self.pending_object_delete = ids;
        } else {
            self.delete_objects(&ids);
        }
    }

    fn delete_objects(&mut self, ids: &[NodeId]) {
        let commands = self
            .object_targets(ids)
            .into_iter()
            .map(|id| Command::RemoveNode { id })
            .collect();
        self.execute_object_group(MacroKind::DeleteObjects, commands);
    }

    fn move_objects(&mut self, ids: &[NodeId], parent: NodeId) {
        let index = self.tree().children(parent).map_or(0, <[NodeId]>::len);
        let commands = self
            .object_targets(ids)
            .into_iter()
            .filter(|id| {
                self.tree()
                    .node(*id)
                    .is_some_and(|node| node.parent() != Some(parent))
            })
            .enumerate()
            .map(|(offset, id)| Command::Move {
                id,
                new_parent: parent,
                index: index + offset,
            })
            .collect();
        self.execute_object_group(MacroKind::MoveObjects, commands);
    }

    fn execute_object_group(&mut self, kind: MacroKind, commands: Vec<Command>) {
        if commands.is_empty() || self.handle.begin_macro(kind).is_err() {
            return;
        }
        for command in commands {
            if let Err(err) = self.handle.execute(command) {
                let _ = self.handle.abort_macro();
                self.refresh();
                self.fail(errors::session::error_message(&err));
                return;
            }
        }
        if let Err(err) = self.handle.end_macro() {
            self.fail(errors::session::error_message(&err));
        } else {
            self.notice(history_text(&HistoryLabel::Macro(kind)));
        }
        self.refresh();
    }

    /// Feuille dont l'onglet est en cours de renommage.
    pub fn renaming_sheet(&self) -> Option<NodeId> {
        self.renaming_sheet
    }

    /// Feuille dont la suppression attend confirmation.
    pub fn pending_sheet_delete(&self) -> Option<NodeId> {
        self.pending_sheet_delete
    }

    /// La dernière feuille d'un classeur ne se supprime pas.
    pub fn can_delete_sheet(&self, sheet: NodeId) -> bool {
        self.sheet_book(sheet)
            .is_some_and(|book| self.worksheets_of(book).len() > 1)
    }

    /// Noms des graphes qui tracent au moins une colonne de `sheet`.
    pub fn sheet_users(&self, sheet: NodeId) -> Vec<String> {
        let tree = &self.snapshot.tree;
        let Some(node) = tree.node(sheet) else {
            return Vec::new();
        };
        let columns = node.children();
        tree.walk_all()
            .into_iter()
            .filter_map(|id| tree.node(id))
            .filter(|node| match node.kind() {
                NodeKind::Graph { spec, .. } => columns.iter().any(|c| spec.references(*c)),
                _ => false,
            })
            .map(|node| node.name().to_string())
            .collect()
    }

    /// Classeur parent de `sheet`, si c'en est bien une.
    fn sheet_book(&self, sheet: NodeId) -> Option<NodeId> {
        let node = self.snapshot.tree.node(sheet)?;
        node.kind().is_worksheet().then(|| node.parent()).flatten()
    }

    fn begin_rename_sheet(&mut self, sheet: NodeId) {
        if self.sheet_book(sheet).is_none() {
            self.fail(tr!("sheet-invalid", id = sheet.to_string()));
            return;
        }
        self.select_sheet(sheet);
        self.renaming_sheet = Some(sheet);
    }

    fn commit_rename_sheet(&mut self, sheet: NodeId, name: &str) {
        self.renaming_sheet = None;
        let Some(book) = self.sheet_book(sheet) else {
            return;
        };
        let name = name.trim();
        let current = self
            .snapshot
            .tree
            .node(sheet)
            .map(|node| node.name().to_string())
            .unwrap_or_default();
        if name == current {
            return;
        }
        let taken = self
            .worksheets_of(book)
            .iter()
            .any(|(id, other)| *id != sheet && other == name);
        if name.is_empty() {
            self.fail(tr!("sheet-rename-empty", name = current));
        } else if taken {
            self.fail(tr!("sheet-rename-taken", name = name, old = current));
        } else {
            self.execute(Command::Rename {
                id: sheet,
                name: name.to_string(),
            });
        }
    }

    fn duplicate_sheet(&mut self, sheet: NodeId) {
        let Some(book) = self.sheet_book(sheet) else {
            self.fail(tr!("sheet-invalid", id = sheet.to_string()));
            return;
        };
        let origin = self
            .snapshot
            .tree
            .node(sheet)
            .map(|node| node.name().to_string())
            .unwrap_or_default();
        let existing = self.worksheets_of(book);
        let name = (1u64..)
            .map(|n| {
                tr!(
                    "sheet-copy-name",
                    name = origin.as_str(),
                    number = n.to_string()
                )
            })
            .find(|candidate| existing.iter().all(|(_, other)| other != candidate))
            .unwrap_or(origin.clone());
        let before: Vec<NodeId> = existing.iter().map(|(id, _)| *id).collect();
        self.execute(Command::DuplicateWorksheet { sheet, name });
        let copy = self
            .worksheets_of(book)
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| !before.contains(id));
        if let Some(copy) = copy {
            self.select_sheet(copy);
        }
    }

    fn request_delete_sheet(&mut self, sheet: NodeId) {
        if self.sheet_book(sheet).is_none() {
            self.fail(tr!("sheet-invalid", id = sheet.to_string()));
        } else if !self.can_delete_sheet(sheet) {
            self.fail(tr!("sheet-delete-last"));
        } else {
            self.pending_sheet_delete = Some(sheet);
        }
    }

    fn confirm_delete_sheet(&mut self, accept: bool) {
        let Some(sheet) = self.pending_sheet_delete.take() else {
            return;
        };
        if accept && self.can_delete_sheet(sheet) {
            self.editing = None;
            self.execute(Command::RemoveNode { id: sheet });
        }
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
        if grouped && self.handle.begin_macro(MacroKind::ClearSelection).is_err() {
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
            self.end_grouped_macro();
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
        if !self.paste_target_is_unambiguous(sheet) {
            return;
        }
        if stokesforge_persistence::clipboard::is_object_clipboard(&text) {
            self.fail(tr!("object-clipboard-use-manager"));
            return;
        }
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

    /// Faux, avec un message d'état, si la sélection de `sheet` est non
    /// contiguë (#159) : un collage n'a alors pas de cible univoque.
    fn paste_target_is_unambiguous(&mut self, sheet: NodeId) -> bool {
        let contiguous = self
            .selection
            .get(&sheet)
            .is_none_or(|selection| selection.is_contiguous(&self.snapshot.tree, sheet));
        if !contiguous {
            self.fail(tr!("status-paste-non-contiguous"));
        }
        contiguous
    }

    /// Colle en position absolue depuis [`AppState::grid_clipboard`] —
    /// Ctrl+Maj+V ([`Action::PasteAbsolute`]), qui n'a jamais de texte à lui
    /// (voir la doc de l'action). Sans effet si rien n'a encore été copié
    /// dans cette session : un texte externe n'a pas de rang d'origine à
    /// rejouer.
    fn paste_absolute(&mut self, sheet: NodeId) {
        if !self.paste_target_is_unambiguous(sheet) {
            return;
        }
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
                if grouped && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
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
                    self.end_grouped_macro();
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
                if grouped && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
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
                    self.end_grouped_macro();
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
        if grouped && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
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
            self.end_grouped_macro();
        }
    }

    /// Relit les métadonnées actuelles de `column`, laisse `update` en
    /// modifier un seul champ, puis les remplace en bloc
    /// (`Command::SetColumnMeta` — doctrine « remplacement en bloc, jamais
    /// par sous-champ » du crate `stokesforge-commands`, voir sa doc de tête).
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
        if !self.begin_creation_macro(MacroKind::AddWorkbook) {
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
        if !self.begin_creation_macro(MacroKind::AddWorksheet) {
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

    fn begin_creation_macro(&mut self, kind: MacroKind) -> bool {
        if self.handle.begin_macro(kind).is_err() {
            self.fail(
                "Impossible de grouper la création dans une seule entrée d'historique".to_string(),
            );
            return false;
        }
        true
    }

    fn end_creation_macro(&mut self) {
        self.end_grouped_macro();
    }

    /// Clôture **commune** de toute opération groupée (collage, effacement,
    /// création, propriétés du graphe) : ferme la macro, remonte l'erreur en
    /// [`StatusMessage`], puis rafraîchit l'instantané. `end_macro` ne
    /// rafraîchit rien, et `execute` ne l'a fait que **pendant** la macro :
    /// sans ce dernier `refresh`, `can_undo`/`undo_label` restent ceux d'avant
    /// la clôture jusqu'à un rafraîchissement fortuit (issue #22).
    fn set_columns_grouped(
        &mut self,
        kind: MacroKind,
        columns: &[NodeId],
        command: impl Fn(NodeId) -> Command,
    ) {
        let grouped = columns.len() > 1;
        if grouped && self.handle.begin_macro(kind).is_err() {
            self.fail("Impossible de grouper le changement des colonnes".to_string());
            return;
        }
        for &column in columns {
            self.execute(command(column));
        }
        if grouped {
            self.end_grouped_macro();
        }
    }

    /// Rôle de plusieurs colonnes, en un seul groupe annulable.
    fn set_columns_role_grouped(&mut self, columns: &[NodeId], role: ColumnRole) {
        let grouped = columns.len() > 1;
        if grouped && self.handle.begin_macro(MacroKind::SetColumnsRole).is_err() {
            self.fail("Impossible de grouper le changement des colonnes".to_string());
            return;
        }
        for &column in columns {
            self.set_column_meta_field(column, |meta| meta.role = role);
        }
        if grouped {
            self.end_grouped_macro();
        }
    }

    fn end_grouped_macro(&mut self) {
        if let Err(err) = self.handle.end_macro() {
            self.fail(format!(
                "Clôture de l'entrée d'historique impossible : {}",
                errors::session::error_message(&err)
            ));
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
    /// `History::begin_macro`/`end_macro` (`stokesforge-commands/src/
    /// history.rs`) pour l'absorption d'une macro qui finirait vide.
    fn import_files(
        &mut self,
        destination: ImportDestination,
        recipe: stokesforge_import::ImportRecipe,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }
        let items = paths
            .iter()
            .flat_map(|path| import_items(path, &recipe))
            .collect();
        self.apply_import(destination, items);
    }

    /// Crée les feuilles et les colonnes d'un lot **déjà analysé** (par
    /// [`AppState::import_files`], ou en fond par [`AppState::start_import`]),
    /// en une seule entrée d'historique « Importer ».
    ///
    /// Trois décisions, toutes lues dans la recette du premier fichier lisible :
    /// la **répartition** des fichiers en feuilles ([`stokesforge_import::FilesLayout`]),
    /// le **mode** dans le classeur actif ([`stokesforge_import::ImportMode`]), et la
    /// destination. Le plan (quelles colonnes de quels fichiers, sur quelle
    /// feuille) est calculé d'abord ([`plan_import`]), puis exécuté.
    fn apply_import(&mut self, destination: ImportDestination, items: Vec<ImportItem>) {
        // Fichiers distincts (un fichier à plusieurs blocs compte pour un seul).
        let mut paths: Vec<PathBuf> = Vec::new();
        for item in &items {
            if !paths.contains(&item.path) {
                paths.push(item.path.clone());
            }
        }
        if self.handle.begin_macro(MacroKind::Import).is_err() {
            self.fail(
                "Impossible de grouper l'import dans une seule entrée d'historique".to_string(),
            );
            return;
        }
        // Accumulées plutôt que passées à `self.fail` au fil de la boucle :
        // `self.fail`/`self.notice` remplacent `self.status` sans le combiner à
        // ce qu'il portait déjà, donc un `self.fail` par fichier en échec se
        // serait fait écraser par le suivant. Tout finit dans un seul message,
        // en échec dès qu'au moins un fichier a échoué, pour que ça se voie.
        let mut failures: Vec<String> = Vec::new();
        for item in &items {
            if let Err(failure) = &item.table {
                failures.push(errors::import::failure_message(failure));
            }
        }
        let mut done_paths: Vec<PathBuf> = Vec::new();
        let first_ok = items.iter().find(|item| item.table.is_ok());
        if let Some(ImportItem {
            recipe: first_recipe,
            ..
        }) = first_ok
        {
            let plans = plan_import(&items, first_recipe.files_layout);
            let merge_sheet = match (destination, first_recipe.import_mode) {
                (_, stokesforge_import::ImportMode::NewSheet) => None,
                (ImportDestination::ActiveBook(book), _) => self
                    .selected_sheet_in(book)
                    .or_else(|| self.worksheets_of(book).first().map(|(id, _)| *id)),
                _ => None,
            };
            if let Some(sheet) = merge_sheet {
                // Tous les plans se fondent dans la feuille active.
                let columns: Vec<PlannedColumn> = plans
                    .iter()
                    .flat_map(|plan| plan.columns.iter().copied())
                    .collect();
                self.fill_existing_sheet(sheet, first_recipe.import_mode, &items, &columns);
                for plan in &plans {
                    for &item in &plan.items {
                        if !done_paths.contains(&items[item].path) {
                            done_paths.push(items[item].path.clone());
                        }
                    }
                }
            } else {
                let mut shared_book = None;
                for plan in &plans {
                    let Some(book) =
                        self.book_for_plan(destination, plan, &mut shared_book, &mut failures)
                    else {
                        continue;
                    };
                    let Some(sheet) = self.add_named_sheet(book, &plan.name, &mut failures) else {
                        continue;
                    };
                    for &planned in &plan.columns {
                        if let Some(column) =
                            stokesforge_importer::make_import_column(&items, planned, 0)
                        {
                            self.execute(Command::AddColumn {
                                sheet,
                                index: None,
                                name: None,
                                column,
                            });
                        }
                    }
                    for &item in &plan.items {
                        if !done_paths.contains(&items[item].path) {
                            done_paths.push(items[item].path.clone());
                        }
                    }
                }
            }
        }
        let imported = done_paths.len();
        if self.handle.end_macro().is_err() {
            failures.push("échec de la clôture de l'entrée d'historique d'import".to_string());
        }
        if imported > 0 {
            // De quoi « Régler le dernier import… » et les fichiers récents.
            self.recent_to_record.extend(done_paths.iter().cloned());
            if let Some(ImportItem {
                recipe: first_recipe,
                ..
            }) = first_ok
            {
                self.last_import = Some(LastImport {
                    destination,
                    recipe: first_recipe.clone(),
                    paths: done_paths,
                });
            }
        }
        // `self.execute` rafraîchit l'instantané à chaque commande, mais
        // toujours **pendant** que la macro est ouverte : sans ce dernier
        // rafraîchissement, `can_undo` resterait sur l'état d'avant la clôture.
        self.refresh();
        let summary = format!("{imported} fichier(s) importé(s) sur {}", paths.len());
        if failures.is_empty() {
            self.notice(summary);
        } else {
            self.fail(format!("{summary} — {}", failures.join(" ; ")));
        }
    }

    /// Le classeur qui reçoit `plan` : le classeur actif, ou un classeur neuf
    /// (un par plan, ou un seul partagé).
    fn book_for_plan(
        &mut self,
        destination: ImportDestination,
        plan: &ImportPlan,
        shared_book: &mut Option<NodeId>,
        failures: &mut Vec<String>,
    ) -> Option<NodeId> {
        match destination {
            ImportDestination::ActiveBook(book) => Some(book),
            ImportDestination::NewBookForAll => {
                if let Some(book) = *shared_book {
                    return Some(book);
                }
                let book = self.create_import_book(None);
                if book.is_none() {
                    failures.push(format!("{}: échec de la création du classeur", plan.name));
                }
                *shared_book = book;
                book
            }
            ImportDestination::NewBookPerFile => {
                let book = self.create_import_book(Some(&plan.name));
                if book.is_none() {
                    failures.push(format!("{}: échec de la création du classeur", plan.name));
                }
                book
            }
        }
    }

    /// Ajoute une feuille à `book` et la nomme `name` (au mieux : si le nom est
    /// pris, elle garde son nom automatique — ce n'est pas un échec d'import).
    ///
    /// `Command::AddWorksheet` ne rend que la `Revision`, jamais l'identifiant
    /// de la feuille créée. `Project::add_worksheet` insère toujours la feuille
    /// en dernière position des enfants de `book`, et `self.execute` rafraîchit
    /// déjà l'instantané : le dernier enfant juste après est la nouvelle
    /// feuille — le seul moyen de la retrouver depuis ce banc, qui n'a qu'un
    /// `Arc<Tree>` figé.
    fn add_named_sheet(
        &mut self,
        book: NodeId,
        name: &str,
        failures: &mut Vec<String>,
    ) -> Option<NodeId> {
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
        let Some(sheet) = sheet else {
            failures.push(format!("{name}: échec de la création de la feuille"));
            return None;
        };
        self.execute(Command::Rename {
            id: sheet,
            name: name.to_string(),
        });
        Some(sheet)
    }

    /// Mode « classeur actif » hors nouvelle feuille : remplace le contenu de
    /// `sheet`, ajoute des colonnes à sa suite, ou prolonge ses colonnes.
    fn fill_existing_sheet(
        &mut self,
        sheet: NodeId,
        mode: stokesforge_import::ImportMode,
        items: &[ImportItem],
        columns: &[PlannedColumn],
    ) {
        use stokesforge_import::ImportMode;
        match mode {
            ImportMode::NewSheet => {}
            ImportMode::ReplaceSheet | ImportMode::AddColumns => {
                if mode == ImportMode::ReplaceSheet {
                    for (column, ..) in self.visible_columns(sheet) {
                        self.execute(Command::DeleteColumn { column });
                    }
                }
                for &planned in columns {
                    if let Some(column) =
                        stokesforge_importer::make_import_column(items, planned, 0)
                    {
                        self.execute(Command::AddColumn {
                            sheet,
                            index: None,
                            name: None,
                            column,
                        });
                    }
                }
            }
            ImportMode::AddRows => {
                let start = self.row_count(sheet);
                let existing: Vec<NodeId> = self
                    .visible_columns(sheet)
                    .into_iter()
                    .map(|(id, ..)| id)
                    .collect();
                let added = columns
                    .iter()
                    .filter_map(|&planned| {
                        items[planned.item].table.as_ref().ok().map(|table| {
                            table
                                .columns
                                .get(planned.column)
                                .map_or(0, |c| c.values.len())
                        })
                    })
                    .max()
                    .unwrap_or(0);
                if added > 0 {
                    self.execute(Command::InsertRows {
                        sheet,
                        at: start,
                        count: added,
                    });
                }
                for (position, &planned) in columns.iter().enumerate() {
                    match existing.get(position) {
                        Some(&target) => {
                            if let Some(block) = stokesforge_importer::import_block(items, planned)
                            {
                                self.execute(Command::SetCellsPromoting {
                                    column: target,
                                    start,
                                    values: block,
                                    expected_version: None,
                                });
                            }
                        }
                        // Plus de colonnes dans le fichier que dans la feuille :
                        // la colonne neuve commence par autant de cases vides.
                        None => {
                            if let Some(column) =
                                stokesforge_importer::make_import_column(items, planned, start)
                            {
                                self.execute(Command::AddColumn {
                                    sheet,
                                    index: None,
                                    name: None,
                                    column,
                                });
                            }
                        }
                    }
                }
            }
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
    /// dans `columns` : cette action de tracé ne crée rien sans Y. Un graphique
    /// vide se crée explicitement par [`Action::AddGraph`].
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

    /// Ajoute des séries construites depuis `columns` au calque `panel` du
    /// `PlotSpec` courant de `graph` — voir [`Action::AddSeriesToGraph`].
    /// Même règle d'appariement X/Y que [`AppState::plot_columns`], mais la
    /// feuille d'où viennent les colonnes se déduit de leur propre parent
    /// (le classeur ouvert n'est pas porté par cette action) : toutes les
    /// colonnes de `columns` doivent donc appartenir à la même feuille. Sans
    /// `plot_type`, les séries prennent celui du calque ([`panel_plot_type`]).
    /// Les nouvelles séries prolongent la palette après celles déjà présentes,
    /// sans toucher aux séries existantes, en une seule commande annulable.
    /// Pas de commande envoyée si `columns` est vide, si aucune n'est de rôle
    /// Y, si `panel` n'existe pas, ou si `graph` n'est pas un graphe de ce
    /// projet.
    fn add_series_to_graph(
        &mut self,
        graph: NodeId,
        panel: usize,
        columns: Vec<NodeId>,
        plot_type: Option<PlotType>,
    ) {
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
        let Some(target) = spec.panels.get_mut(panel) else {
            self.fail(format!("{graph} n'a pas de calque {panel}"));
            return;
        };
        let plot_type = plot_type.unwrap_or_else(|| panel_plot_type(target));
        let new_series = series_from_columns(
            &self.snapshot.tree,
            sheet,
            &columns,
            &plot_type,
            target.series.len(),
        );
        if new_series.is_empty() {
            self.fail("Aucune colonne de rôle Y dans la sélection : rien à ajouter".to_string());
            return;
        }
        target.series.extend(new_series);
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
                 (STOKESFORGE_PYTHON_HOME, ou dist/.../lib/python)."
                    .to_string(),
            );
            return;
        };
        let existing_engine = self.engine.clone();
        // Identité propre au script : une macro ouverte par l'interface
        // (propriétés du graphe) ne l'absorbe pas et ne l'annule pas (#21).
        let handle = self.handle.with_new_identity();
        let source = self.script_source.clone();
        let (reply_tx, reply_rx) = mpsc::channel();

        let spawned = thread::Builder::new()
            .name("stokesforge-app-script".to_owned())
            .spawn(move || {
                let engine = match existing_engine {
                    Some(engine) => Ok(engine),
                    None => ScriptEngine::spawn(handle, python).map(Arc::new),
                };
                let reply = match engine {
                    Ok(engine) => {
                        let outcome = engine.run(&source);
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
    /// `stokesforge_script::ScriptEngine::interrupt`). Sans effet s'il n'y a pas
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

/// Classeurs référencés par un graphique ; utilisable depuis tous les viewports.
pub fn graph_data_books(tree: &Tree, graph: NodeId) -> Vec<NodeId> {
    let Some(NodeKind::Graph { spec, .. }) = tree.node(graph).map(Node::kind) else {
        return Vec::new();
    };
    workbooks_of_columns(
        tree,
        spec.panels
            .iter()
            .flat_map(|p| &p.series)
            .flat_map(|s| s.referenced_columns()),
    )
}

/// Classeurs des colonnes d'une seule courbe ; vide si la courbe n'existe plus.
pub fn series_data_books(
    tree: &Tree,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
) -> Vec<NodeId> {
    let Some(NodeKind::Graph { spec, .. }) = tree.node(graph).map(Node::kind) else {
        return Vec::new();
    };
    spec.series_at(series)
        .map(|s| workbooks_of_columns(tree, s.referenced_columns()))
        .unwrap_or_default()
}

fn workbooks_of_columns(tree: &Tree, columns: impl IntoIterator<Item = NodeId>) -> Vec<NodeId> {
    let mut books = Vec::new();
    for column in columns {
        let Some(book) = tree
            .node(column)
            .filter(|n| n.as_column().is_some())
            .and_then(Node::parent)
            .and_then(|id| tree.node(id))
            .and_then(Node::parent)
        else {
            continue;
        };
        if tree
            .node(book)
            .is_some_and(|n| matches!(n.kind(), NodeKind::Workbook { .. }))
            && !books.contains(&book)
        {
            books.push(book);
        }
    }
    books
}

#[cfg(test)]
mod tests {
    //! Tests unitaires ciblés sur ce qui ne dépend d'aucune session ni
    //! d'aucun projet : la frontière valeur ↔ texte elle-même (A9 option 2 et
    //! C4, `guide-de-correction-robustesse.md` §F1/§F8). Le reste du fichier
    //! (dispatch, historique, sauvegarde…) est couvert en intégration par
    //! `tests/shell.rs` et par le banc `stokesforge-robustesse` (batterie 15), qui
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
