//! Les primitives du socle.
//!
//! Une commande est une **valeur**, pas un objet à état : elle ne retient ni
//! `Project`, ni `Tree`, ni le moindre `Arc`. C'est ce qui lui permet de
//! traverser un canal inter-threads telle quelle, de l'UI vers l'acteur qui
//! détient l'unique `&mut Project` du processus.
//!
//! Tout ce qu'il faut pour annuler est calculé **à l'application**, par
//! `History`, pas porté par la commande.
//!
//! # Création de nœud : une commande par nature, pas un `AddNode` générique
//!
//! Chaque nature de nœud (dossier, classeur, classeur vide, feuille, graphe,
//! matrice, note) a sa propre variante plutôt qu'un unique
//! `AddNode { kind: NodeKindSpec, .. }`. C'est la même logique que `Rename`
//! ou `SetColumnType`, déjà fines : le style de ce module est « une commande
//! par intention », et `plotyx_core::Project` expose lui-même une méthode
//! `add_*` par nature plutôt qu'un constructeur générique paramétré par un
//! `NodeKind` — suivre cette forme évite d'inventer, côté commandes, un
//! niveau d'indirection que le cœur n'a pas. La suppression, elle, reste
//! générique (`RemoveNode { id }`) : `Project::remove` l'est déjà, sur
//! n'importe quel `NodeId`, sans distinction de nature à faire à l'appel.
//!
//! # Édition de contenu : remplacement en bloc, jamais par sous-champ
//!
//! `SetWindowMeta`, `SetNoteText`, `SetColumnMeta` et `SetPlotSpec`
//! remplacent leur cible entière, à l'image de `SetCells` qui remplace une
//! plage entière plutôt que cellule par cellule. Aucune de ces quatre
//! commandes n'est **structurelle** au sens de [`Command::is_structural`] :
//! elles ne changent ni la forme de l'arbre ni l'ordre des nœuds, seulement
//! le contenu d'un nœud existant — même nature que `SetColumnType`, qui
//! remplace elle aussi un contenu en bloc sans toucher à la structure.

use plotyx_core::{Column, ColumnMeta, ColumnType, NodeId, Tree, ValueBlock, WindowMeta};
use plotyx_plot::{PlotSpec, SavedColors};

use crate::label::{ColumnRef, HistoryLabel};

/// Une des mutations du modèle exprimables par l'utilisateur.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Écrit `values` dans `column` à partir de la ligne `start`, en
    /// allongeant la colonne si besoin.
    SetCells {
        /// Colonne visée.
        column: NodeId,
        /// Première ligne écrite.
        start: usize,
        /// Bloc écrit ; sa variante doit correspondre au type de la colonne.
        values: ValueBlock,
        /// A2 (`guide-de-correction-robustesse.md` §F6) — condition
        /// optimiste, vérifiée par `History::apply` **au moment d'appliquer**
        /// la commande, jamais avant : `None` ne vérifie rien (comportement
        /// historique, celui de l'UI qui n'a pas cette discipline) ;
        /// `Some(v)` exige que `Column::version` de `column` vaille encore
        /// exactement `v`, sans quoi la commande est refusée
        /// (`CoreError::ConcurrentModification`) sans toucher au modèle ni à
        /// l'historique. C'est le champ que `plotyx-script/bridge.rs`
        /// remplit pour fermer le TOCTOU entre l'instantané lu par un script
        /// et la commande qu'il envoie.
        expected_version: Option<u64>,
    },
    /// Écrit `values` dans `column` comme [`Command::SetCells`], mais
    /// **élargit le type de la colonne** au lieu de refuser un bloc qui ne
    /// lui correspond pas.
    ///
    /// C'est la commande de la saisie *tolérante* : taper « n/a » dans une
    /// colonne numérique n'est plus une erreur ni un `NaN` silencieux, c'est
    /// une colonne qui devient [`ColumnType::Mixed`] et une cellule qui porte
    /// un texte, les autres gardant leurs nombres. `SetCells` reste la
    /// commande de la saisie *stricte*, celle d'un appelant qui sait ce qu'il
    /// écrit et veut être refusé s'il se trompe (le pont Python, un script).
    ///
    /// Trois chemins, du moins cher au plus cher :
    ///
    /// 1. le bloc correspond déjà au type de la colonne → strictement
    ///    `SetCells`, même delta, même coût ;
    /// 2. la colonne est **déjà** mixte et le bloc ne l'est pas → le bloc est
    ///    converti ([`ValueBlock::into_mixed`]) et écrit ; toujours un delta
    ///    de cellules, donc toujours proportionnel à ce qui est écrit, pas à
    ///    la taille de la colonne ;
    /// 3. la colonne n'est pas mixte et le bloc ne lui correspond pas → la
    ///    colonne est promue en `Mixed` **puis** écrite. C'est le seul chemin
    ///    qui coûte un instantané de colonne dans l'historique, exactement
    ///    comme `SetColumnType` qu'il contient ; il n'a lieu qu'une fois par
    ///    colonne, la deuxième saisie tolérante empruntant le chemin 2.
    SetCellsPromoting {
        /// Colonne visée.
        column: NodeId,
        /// Première ligne écrite.
        start: usize,
        /// Bloc écrit ; sa variante n'a **pas** à correspondre au type de la
        /// colonne.
        values: ValueBlock,
        /// Même condition optimiste que [`Command::SetCells`], vérifiée au
        /// même moment et avec le même effet en cas de désaccord.
        expected_version: Option<u64>,
    },
    /// Ajoute une colonne à `sheet`. `index: None` → en fin ;
    /// `name: None` → premier nom libre façon tableur.
    AddColumn {
        /// Feuille d'accueil.
        sheet: NodeId,
        /// Position d'insertion, ou fin de feuille.
        index: Option<usize>,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
        /// Contenu initial de la colonne.
        column: Column,
    },
    /// Supprime une colonne et ses valeurs.
    DeleteColumn {
        /// Colonne à supprimer.
        column: NodeId,
    },
    /// Renomme un nœud quelconque (colonne, feuille, classeur, dossier...).
    Rename {
        /// Nœud à renommer.
        id: NodeId,
        /// Nouveau nom.
        name: String,
    },
    /// Déplace un nœud sous un nouveau parent, à une position donnée.
    Move {
        /// Nœud à déplacer.
        id: NodeId,
        /// Parent d'accueil.
        new_parent: NodeId,
        /// Position dans les enfants du parent d'accueil.
        index: usize,
    },
    /// Change le type d'une colonne (conversion destructrice des valeurs).
    SetColumnType {
        /// Colonne visée.
        column: NodeId,
        /// Type demandé.
        column_type: ColumnType,
    },
    /// Insère des lignes au niveau feuille (colonnes telles que `at < len`).
    InsertRows {
        /// Feuille visée.
        sheet: NodeId,
        /// Ligne d'insertion.
        at: usize,
        /// Nombre de lignes insérées.
        count: usize,
    },
    /// Supprime des lignes au niveau feuille (colonnes telles que `at < len`).
    DeleteRows {
        /// Feuille visée.
        sheet: NodeId,
        /// Première ligne supprimée.
        at: usize,
        /// Nombre de lignes supprimées.
        count: usize,
    },
    /// Supprime `count` lignes à partir de `at`, colonne par colonne et
    /// **indépendamment**, dans les colonnes listées par `columns` — le
    /// pendant de `DeleteRows` pour une sélection qui ne couvre pas
    /// nécessairement toutes les colonnes d'une feuille (touche Suppr sur un
    /// rectangle de cellules). Une colonne telle que `at >= len` est ignorée
    /// silencieusement, exactement comme `DeleteRows` le fait déjà via
    /// `Project::row_targets` ; les doublons de `columns` sont ignorés.
    DeleteCells {
        /// Colonnes visées.
        columns: Vec<NodeId>,
        /// Première ligne supprimée dans chaque colonne touchée.
        at: usize,
        /// Nombre de lignes demandé (l'annulation utilise la quantité
        /// réellement retirée par colonne, comme pour `DeleteRows`).
        count: usize,
    },

    /// Ajoute un dossier en fin de `parent`. Contrairement aux fenêtres, un
    /// dossier n'a pas de nom automatique dans `plotyx-core`
    /// (`Project::add_folder` prend `name` obligatoire) : cette commande ne
    /// le rend donc pas optionnel non plus.
    AddFolder {
        /// Dossier d'accueil.
        parent: NodeId,
        /// Nom du dossier.
        name: String,
    },
    /// Ajoute un classeur muni d'une feuille « Sheet1 ». `name: None` →
    /// premier « BookN » libre.
    AddWorkbook {
        /// Dossier d'accueil.
        parent: NodeId,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
    },
    /// Ajoute un classeur **sans** feuille. `name: None` → premier « BookN »
    /// libre. Distincte d'`AddWorkbook` : un classeur vide n'a pas la même
    /// entrée d'historique à annuler (pas de feuille « Sheet1 » à recréer),
    /// et un appelant qui veut construire ses propres feuilles ne doit pas
    /// avoir à défaire celle qu'`AddWorkbook` aurait imposée.
    AddEmptyWorkbook {
        /// Dossier d'accueil.
        parent: NodeId,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
    },
    /// Ajoute une feuille en fin de classeur. `name: None` → premier
    /// « SheetN » libre dans ce classeur.
    AddWorksheet {
        /// Classeur d'accueil.
        book: NodeId,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
    },
    /// Ajoute un graphe portant `spec`. `name: None` → premier « GraphN »
    /// libre. `spec` est obligatoire, comme dans `Project::add_graph` : un
    /// graphe sans spec ne décrit rien de plus qu'une fenêtre vide, autant le
    /// rendre explicite ici aussi (`PlotSpec::default()` pour un graphe
    /// vide).
    AddGraph {
        /// Dossier d'accueil.
        parent: NodeId,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
        /// Description du graphe.
        spec: PlotSpec,
    },
    /// Ajoute une matrice. `name: None` → premier « MBookN » libre.
    AddMatrix {
        /// Dossier d'accueil.
        parent: NodeId,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
    },
    /// Ajoute une note portant `text`. `name: None` → premier « NoteN » libre.
    AddNote {
        /// Dossier d'accueil.
        parent: NodeId,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
        /// Texte initial.
        text: String,
    },
    /// Supprime un nœud quelconque et toute sa descendance — dossier,
    /// classeur, feuille, colonne, graphe, matrice ou note.
    /// `Project::remove` est déjà générique sur `NodeId` : cette commande
    /// n'a donc pas besoin d'une variante par nature, contrairement aux
    /// commandes de création (voir la doc de tête de ce module). Ce que
    /// devient une fenêtre ouverte sur le nœud supprimé est une décision
    /// d'UI, hors du périmètre de cette commande.
    RemoveNode {
        /// Nœud à supprimer, avec sa descendance.
        id: NodeId,
    },

    /// Remplace les métadonnées d'une fenêtre (classeur, graphe, matrice,
    /// note) en bloc.
    SetWindowMeta {
        /// Fenêtre visée.
        id: NodeId,
        /// Nouvelles métadonnées.
        meta: WindowMeta,
    },
    /// Remplace le texte d'une note.
    SetNoteText {
        /// Note visée.
        id: NodeId,
        /// Nouveau texte.
        text: String,
    },
    /// Remplace les métadonnées d'une colonne (rôle de tracé, unité, nom
    /// long, commentaire, format d'affichage) en bloc.
    SetColumnMeta {
        /// Colonne visée.
        column: NodeId,
        /// Nouvelles métadonnées.
        meta: ColumnMeta,
    },
    /// Remplace le `PlotSpec` d'un graphe existant, en bloc — pas d'édition
    /// par sous-champ (série, axe, légende) : voir la doc de tête de ce
    /// module.
    SetPlotSpec {
        /// Graphe visé.
        graph: NodeId,
        /// Nouvelle description du graphe.
        spec: PlotSpec,
    },
    /// Remplace les couleurs enregistrées dans le projet, en bloc (enregistrer
    /// ou retirer une couleur du sélecteur de couleur). Données, pas
    /// structure : aucun nœud ne bouge.
    SetSavedColors {
        /// Nouvelles couleurs enregistrées.
        colors: SavedColors,
    },
}

impl Command {
    /// Libellé d'historique, **structuré** (l'action et ses paramètres, pas une
    /// phrase : l'application la met en phrase dans la langue courante), à
    /// calculer sur l'arbre **avant** application : c'est là que les noms visés
    /// existent encore (une colonne supprimée n'a plus de nom après coup).
    ///
    /// Cette fonction ne peut pas échouer : si le nœud est inconnu, le champ
    /// concerné vaut `None` et la phrase retombe sur une forme générique.
    /// C'est l'application de la commande qui échouera, avec la bonne erreur ;
    /// un libellé n'est pas l'endroit où signaler un problème.
    pub fn label(&self, tree: &Tree) -> HistoryLabel {
        let name = |id: NodeId| name_of(tree, id).map(str::to_string);
        match self {
            Command::SetCells { column, .. } | Command::SetCellsPromoting { column, .. } => {
                HistoryLabel::SetCells {
                    column: column_ref(tree, *column),
                }
            }
            Command::AddColumn { .. } => HistoryLabel::AddColumn,
            Command::DeleteColumn { column } => HistoryLabel::DeleteColumn {
                column: column_ref(tree, *column),
            },
            Command::Rename { id, name: to } => HistoryLabel::Rename {
                from: name(*id),
                to: to.clone(),
            },
            Command::Move { id, .. } => HistoryLabel::Move { name: name(*id) },
            Command::SetColumnType { column, .. } => HistoryLabel::SetColumnType {
                column: column_ref(tree, *column),
            },
            Command::InsertRows { count, .. } => HistoryLabel::InsertRows { count: *count },
            Command::DeleteRows { count, .. } => HistoryLabel::DeleteRows { count: *count },
            Command::DeleteCells { count, .. } => HistoryLabel::DeleteCells { count: *count },
            Command::AddFolder { name, .. } => HistoryLabel::AddFolder { name: name.clone() },
            Command::AddWorkbook { .. } => HistoryLabel::AddWorkbook,
            Command::AddEmptyWorkbook { .. } => HistoryLabel::AddEmptyWorkbook,
            Command::AddWorksheet { .. } => HistoryLabel::AddWorksheet,
            Command::AddGraph { .. } => HistoryLabel::AddGraph,
            Command::AddMatrix { .. } => HistoryLabel::AddMatrix,
            Command::AddNote { .. } => HistoryLabel::AddNote,
            Command::RemoveNode { id } => HistoryLabel::RemoveNode { name: name(*id) },
            Command::SetWindowMeta { id, .. } => HistoryLabel::SetWindowMeta { name: name(*id) },
            Command::SetNoteText { id, .. } => HistoryLabel::SetNoteText { name: name(*id) },
            Command::SetColumnMeta { column, .. } => HistoryLabel::SetColumnMeta {
                column: column_ref(tree, *column),
            },
            Command::SetPlotSpec { graph, .. } => HistoryLabel::SetPlotSpec { name: name(*graph) },
            Command::SetSavedColors { .. } => HistoryLabel::SetSavedColors,
        }
    }
}

impl Command {
    /// Vrai pour les commandes qui touchent la **structure** (arbre, noms,
    /// ordre, apparition ou disparition de nœuds) plutôt que le **contenu**
    /// d'un nœud existant — voir la doctrine des deux natures en tête de
    /// `history.rs`. `History::execute` s'en sert pour décider si
    /// `Entry::Structural` a besoin d'un instantané complet de l'arbre
    /// (structure) ou si un simple delta suffit (données) : c'est exactement
    /// la distinction qui a fait diverger le coût d'une commande de données
    /// de la taille du projet (D3, `guide-de-correction-robustesse.md`).
    ///
    /// Les huit commandes de création/suppression de nœud (`AddFolder`,
    /// `AddWorkbook`, `AddEmptyWorkbook`, `AddWorksheet`, `AddGraph`,
    /// `AddMatrix`, `AddNote`, `RemoveNode`) rejoignent `AddColumn` et
    /// `DeleteColumn` ici, pour la même raison qu'elles : elles changent la
    /// forme de l'arbre. `SetWindowMeta`, `SetNoteText`, `SetColumnMeta` et
    /// `SetPlotSpec` n'y sont **pas** : elles remplacent le contenu d'un nœud
    /// déjà en place, sans toucher à sa position ni à celle d'aucun autre —
    /// même nature que `SetColumnType`, qui reste côté données bien qu'il
    /// remplace, lui aussi, tout un contenu en bloc.
    ///
    /// `SetCellsPromoting` non plus, bien qu'elle puisse changer le **type**
    /// d'une colonne : changer le type d'un nœud n'est pas changer la forme
    /// de l'arbre, et son delta est exactement celui de `SetColumnType` —
    /// un instantané de colonne, jamais un instantané d'arbre.
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            Command::AddColumn { .. }
                | Command::DeleteColumn { .. }
                | Command::Rename { .. }
                | Command::Move { .. }
                | Command::AddFolder { .. }
                | Command::AddWorkbook { .. }
                | Command::AddEmptyWorkbook { .. }
                | Command::AddWorksheet { .. }
                | Command::AddGraph { .. }
                | Command::AddMatrix { .. }
                | Command::AddNote { .. }
                | Command::RemoveNode { .. }
        )
    }
}

/// Nom d'un nœud s'il existe dans cet arbre.
fn name_of(tree: &Tree, id: NodeId) -> Option<&str> {
    tree.node(id).map(plotyx_core::Node::name)
}

/// Désignation d'une **colonne** dans un libellé d'historique : sa position
/// dans sa feuille ([`ColumnRef`]), jamais son nom de nœud — voir la doc de
/// [`ColumnRef`] pour le piège des deux identités.
///
/// `None` si la colonne n'est plus dans l'arbre ou n'a pas de feuille
/// parente : la phrase retombe alors sur une forme générique.
fn column_ref(tree: &Tree, column: NodeId) -> Option<ColumnRef> {
    let sheet = tree.parent(column).ok().flatten()?;
    let index = tree
        .children(sheet)
        .ok()?
        .iter()
        .position(|&child| child == column)?;
    Some(ColumnRef(index))
}
