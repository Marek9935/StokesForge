//! Racine du modèle : l'arbre courant (partagé) et l'allocateur d'identifiants.
//!
//! `Project` est la **seule façade publique de mutation** du crate. Ses
//! méthodes prennent `&mut self` ; le seul `&mut Project` du processus sera
//! détenu par l'acteur de `stokesforge-session`, ce qui fait de « aucune mutation
//! hors commande » une propriété de propriété, pas une convention.
//!
//! L'allocateur vit ici, **hors de l'arbre**, pour qu'une annulation qui
//! restaure un ancien arbre (`replace_tree`) ne fasse pas reculer le compteur :
//! un identifiant n'est jamais réutilisé sur toute la vie d'un projet,
//! rechargement compris.

use std::sync::Arc;

use stokesforge_plot::{PlotSpec, SavedColors};

use crate::column::{Column, ColumnMeta, ColumnType, ColumnValues, ValueBlock, ValuesDelta};
use crate::error::{CoreError, CoreResult};
use crate::id::{IdAllocator, NodeId};
use crate::node::{Node, NodeKind, WindowMeta};
use crate::path::{format_path, ObjectPath};
use crate::tree::Tree;

/// Nom d'un dossier ou d'une colonne « façon tableur » : A, B, …, Z, AA, AB, …
///
/// Publique parce que ce n'est pas qu'un générateur de noms par défaut :
/// c'est **la** correspondance position → lettre du produit, et les couches
/// au-dessus doivent y désigner une colonne de la même façon que la grille
/// l'affiche — l'en-tête de `stokesforge-app` comme les libellés d'historique de
/// `stokesforge-commands`. Trois implémentations divergentes de cet algorithme
/// valaient trois façons d'appeler la même colonne.
pub fn spreadsheet_name(index: usize) -> String {
    let mut name = String::new();
    let mut remaining = index as i64;
    loop {
        let offset = (remaining % 26) as u8;
        name.insert(0, char::from(b'A' + offset));
        remaining = remaining / 26 - 1;
        if remaining < 0 {
            break;
        }
    }
    name
}

/// Racine du modèle : l'arbre courant (partagé) et l'allocateur d'identifiants
/// (jamais remis en arrière).
///
/// N'est **pas** `Clone` (B9, `guide-de-correction-robustesse.md` §F2) : un
/// `#[derive(Clone)]` dupliquerait `IdAllocator` avec l'arbre, si bien que
/// deux copies d'un même projet attribueraient ensuite les mêmes
/// identifiants à des nœuds différents — exactement ce que la documentation
/// de `NodeId` promet de ne jamais faire. `Arc<Tree>` (via [`Project::tree`])
/// reste clonable, gratuitement, pour qui a besoin d'un instantané partagé.
#[derive(Debug)]
pub struct Project {
    tree: Arc<Tree>,
    ids: IdAllocator,
    /// Identité de **cette instance** de `Project`, stable sur toute sa vie —
    /// contrairement à `tree`, que `replace_tree` (annulation/rétablissement
    /// d'une entrée structurelle) réaffecte à un `Arc<Tree>` déjà existant,
    /// potentiellement le même qu'à un instant antérieur de cette même
    /// instance. Sert à quiconque a besoin de reconnaître « ce `Project`
    /// précis, revu plus tard » sans se fier à l'identité de son arbre
    /// courant — par exemple B8 (`stokesforge-persistence`, détection d'un
    /// écrivain concurrent), qui doit continuer à reconnaître une instance
    /// après qu'un `undo` l'a fait pointer vers un arbre déjà vu. Un
    /// `Arc<()>` plutôt qu'un compteur : identité par adresse, gratuite,
    /// sans registre à tenir ici.
    identity: Arc<()>,
    /// Prochain numéro de version de colonne à attribuer (A7,
    /// `guide-de-correction-robustesse.md` §F2 — doctrine des estampilles :
    /// « version de contenu, attribuée par le projet »). Une première
    /// version de ce correctif laissait chaque `Column` s'incrémenter
    /// elle-même (`wrapping_add(1)` local) et ne réattribuait un numéro
    /// frais qu'au moment d'un `undo` sur une entrée structurelle, calculé
    /// depuis les arbres encore visibles dans l'historique — insuffisant :
    /// un balayage aléatoire
    /// (`b14_une_version_ne_designe_jamais_deux_contenus`) a trouvé un
    /// numéro réattribué à deux contenus différents malgré tout, une
    /// écriture ordinaire pouvant faire progresser une colonne à un numéro
    /// qu'un autre chemin (annulation puis nouvelle écriture) atteint aussi
    /// par ailleurs. Un compteur unique, tenu par le projet et consulté par
    /// **toute** écriture de colonne (ordinaire ou restauration), ferme la
    /// question une fois pour toutes — le même patron qu'`IdAllocator` pour
    /// les `NodeId`, à l'échelle des versions de colonnes.
    column_version_counter: u64,
}

impl Project {
    /// Projet neuf : un dossier racine nommé `name` (identifiant 1), rien d'autre.
    pub fn new(name: &str) -> CoreResult<Self> {
        let mut ids = IdAllocator::new();
        // Le tout premier identifiant d'un allocateur neuf (`next == 1`) ne
        // peut jamais épuiser `u64` ; `?` reste la forme la plus sobre pour
        // une fonction qui rend déjà `CoreResult`.
        let id = ids.next()?;
        let root = Node::new(
            id,
            name,
            NodeKind::Folder {
                children: Vec::new(),
            },
        )?;
        Ok(Self {
            tree: Arc::new(Tree::new(root)?),
            ids,
            identity: Arc::new(()),
            column_version_counter: 1,
        })
    }

    /// Projet neuf sans validation du nom : chemin infaillible réservé aux noms
    /// littéraux du crate (voir `Default`).
    fn with_root_name(name: &str) -> Self {
        let mut ids = IdAllocator::new();
        // `Default` exige une fonction infaillible ; le tout premier
        // identifiant d'un allocateur neuf (`next == 1`) ne peut jamais
        // épuiser `u64`, donc cet `expect` ne se déclenche jamais en
        // pratique — ce que le linter du crate ne peut pas savoir.
        #[allow(clippy::expect_used)]
        let id = ids
            .next()
            .expect("le premier identifiant ne peut pas être épuisé");
        let root = Node::new_unchecked(
            id,
            name,
            NodeKind::Folder {
                children: Vec::new(),
            },
        );
        Self {
            tree: Arc::new(Tree::with_root(root)),
            ids,
            identity: Arc::new(()),
            column_version_counter: 1,
        }
    }

    /// Reconstruit depuis un arbre chargé et la valeur persistée du compteur ;
    /// `next_id_hint` est déjà le **prochain** identifiant à attribuer (voir
    /// [`IdAllocator::resume`]), et le compteur reprend à
    /// `max(next_id_hint, plus grand identifiant de l'arbre + 1)`.
    pub fn from_tree(tree: Arc<Tree>, next_id_hint: u64) -> Self {
        let highest = tree
            .iter()
            .map(|(id, _)| id.raw())
            .chain(tree.calculations().keys().map(|id| id.raw()))
            .chain(tree.correction_traces().keys().map(|id| id.raw()))
            // Les identifiants que lisent les formules (#321) : une colonne
            // lue peut ne plus être dans l'arbre (supprimée, puis fichier
            // enregistré), et son numéro ne doit jamais être réattribué — la
            // formule en lirait une autre (invariant 7).
            .chain(
                tree.iter()
                    .filter_map(|(_, node)| node.as_column())
                    .filter_map(|column| column.meta().formula.as_ref())
                    .flat_map(|formula| formula.columns())
                    .map(|id| id.raw()),
            )
            .max()
            .unwrap_or(0);
        // A7 — le compteur de version de colonne n'est pas encore persisté
        // (contrairement à `next_id_hint`) : reprendre au-delà de la plus
        // haute version déjà lue évite au moins de régresser DANS ce
        // chargement ; deux `Project` chargés séparément du même fichier
        // peuvent en revanche attribuer, chacun de son côté, les mêmes
        // numéros à des contenus différents — hors du périmètre d'A7, qui
        // ne couvre que la restauration en mémoire d'une seule instance.
        let highest_version = tree
            .iter()
            .filter_map(|(_, node)| node.as_column())
            .map(Column::version)
            .max()
            .unwrap_or(0);
        Self {
            ids: IdAllocator::resume(next_id_hint, highest),
            tree,
            identity: Arc::new(()),
            column_version_counter: highest_version + 1,
        }
    }

    /// Accès `pub(crate)` à l'arbre, pour les modules du crate qui écrivent
    /// des enregistrements portés par `Tree` (voir [`crate::calc`]).
    pub(crate) fn tree_arc_mut(&mut self) -> &mut Arc<Tree> {
        &mut self.tree
    }

    /// Alloue un identifiant du projet (jamais réutilisé, invariant 7).
    pub(crate) fn allocate_id(&mut self) -> CoreResult<NodeId> {
        self.ids.next()
    }

    // -- lecture --

    /// Arbre courant. Le cloner (`project.tree().clone()`) fige un instantané.
    pub fn tree(&self) -> &Arc<Tree> {
        &self.tree
    }

    /// Poignée faible sur l'identité de **cette instance**, stable sur toute
    /// sa vie (voir la doc du champ `identity`). `Weak::upgrade` échoue dès
    /// que ce `Project` est détruit ; deux instances distinctes — même
    /// chargées du même fichier — ne rendent jamais deux poignées qui
    /// s'upgradent vers la même adresse.
    pub fn identity(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.identity)
    }

    /// Identifiant du dossier racine.
    pub fn root(&self) -> NodeId {
        self.tree.root()
    }

    /// Nom du projet, c'est-à-dire de son dossier racine.
    pub fn name(&self) -> &str {
        self.tree
            .node(self.tree.root())
            .map(Node::name)
            .unwrap_or_default()
    }

    /// Valeur du prochain identifiant qui sera attribué (à persister).
    pub fn next_id_hint(&self) -> u64 {
        self.ids.peek()
    }

    // -- restauration (annulation) --

    /// Remplace l'arbre courant par un instantané antérieur. **Ne touche pas**
    /// au compteur d'identifiants : un nœud restauré revient sous son
    /// identifiant d'origine, et les créations suivantes restent au-dessus.
    pub fn replace_tree(&mut self, tree: Arc<Tree>) {
        self.tree = tree;
    }

    // -- structure --

    /// Ajoute un dossier en fin de `parent`.
    pub fn add_folder(&mut self, parent: NodeId, name: &str) -> CoreResult<NodeId> {
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            name,
            NodeKind::Folder {
                children: Vec::new(),
            },
        )
    }

    /// Ajoute un classeur muni d'une feuille « Sheet1 ». `name: None` → premier
    /// « BookN » libre parmi les fenêtres du projet.
    pub fn add_workbook(&mut self, parent: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        self.add_workbook_with_sheet(parent, name, None)
    }

    /// Ajoute un classeur et sa première feuille, atomiquement : si l'ajout de
    /// la feuille échoue, le classeur fraîchement créé est retiré et l'erreur
    /// de la feuille est rendue telle quelle — l'arbre revient alors à
    /// l'identique de son état d'avant l'appel. Seul le compteur
    /// d'identifiants peut avoir avancé (il ne recule jamais, y compris sur ce
    /// rollback), ce qui est sans conséquence puisqu'un identifiant n'est
    /// jamais réutilisé.
    fn add_workbook_with_sheet(
        &mut self,
        parent: NodeId,
        name: Option<&str>,
        sheet_name: Option<&str>,
    ) -> CoreResult<NodeId> {
        let book = self.add_empty_workbook(parent, name)?;
        match self.add_worksheet(book, sheet_name) {
            Ok(_) => Ok(book),
            Err(err) => {
                // Le classeur vient d'être créé, sans autre référence possible :
                // ce retrait ne devrait jamais échouer. S'il échouait malgré
                // tout, on préfère rendre l'erreur d'origine plutôt qu'une
                // erreur de nettoyage qui la masquerait.
                let _ = self.remove(book);
                Err(err)
            }
        }
    }

    /// Ajoute un classeur **sans** feuille (persistance, tests).
    pub fn add_empty_workbook(&mut self, parent: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        let name = self.window_name(name, "Book");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Workbook {
                sheets: Vec::new(),
                meta: WindowMeta::default(),
            },
        )
    }

    /// Ajoute une feuille en fin de classeur. `name: None` → premier « SheetN »
    /// libre dans ce classeur.
    pub fn add_worksheet(&mut self, book: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        let name = match name {
            Some(name) => name.to_string(),
            None => self.next_child_name(book, "Sheet")?,
        };
        let index = self.tree.children(book)?.len();
        self.insert_node(
            book,
            index,
            &name,
            NodeKind::Worksheet {
                columns: Vec::new(),
            },
        )
    }

    /// Duplique une feuille, colonnes comprises, juste après elle dans son
    /// classeur, sous le nom `name`. La copie est une feuille ordinaire sans
    /// lien avec la mère : chaque colonne reçoit un identifiant neuf (les
    /// buffers de valeurs restent partagés copy-on-write jusqu'à la première
    /// écriture). Les séries qui référencent les colonnes de la mère les
    /// référencent toujours.
    pub fn duplicate_worksheet(&mut self, sheet: NodeId, name: &str) -> CoreResult<NodeId> {
        let source = self.tree.get(sheet)?;
        if !source.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                source.kind().with_article()
            )));
        }
        let book = self.tree.parent(sheet)?.ok_or_else(|| {
            CoreError::Structure(format!("{sheet} has no workbook to duplicate into"))
        })?;
        let index = self
            .tree
            .children(book)?
            .iter()
            .position(|&child| child == sheet)
            .map_or(0, |at| at + 1);
        let content = crate::NodeCopy::capture(&self.tree, sheet)?;
        let before = self.tree.clone();
        let result = self.insert_copy(book, &content, name).and_then(|copy| {
            self.move_node(copy, book, index)?;
            Ok(copy)
        });
        if result.is_err() {
            self.tree = before;
        }
        result
    }

    /// Colle un sous-arbre avec des identifiants créés dans ce projet.
    /// Les graphes conservent leurs références : le transfert de leurs données
    /// entre projets relève d'un autre geste (#107). Tout échec restaure l'arbre,
    /// mais ne remet jamais en arrière l'allocateur.
    pub fn insert_copy(
        &mut self,
        parent: NodeId,
        copy: &crate::NodeCopy,
        name: &str,
    ) -> CoreResult<NodeId> {
        let before = self.tree.clone();
        let result = (|| {
            let mut ids = std::collections::HashMap::new();
            let mut graph_specs = std::collections::HashMap::new();
            if copy.is_external() {
                for source in copy.nodes() {
                    let NodeKind::Graph { spec, .. } = source.kind() else {
                        continue;
                    };
                    let mut columns = std::collections::HashMap::new();
                    if let Some((_, data)) =
                        copy.graph_data().iter().find(|(id, _)| *id == source.id())
                    {
                        let book_name = self.next_window_name("Book");
                        let book = self.insert_copy(parent, data, &book_name)?;
                        let inserted = self.tree.walk(book)?;
                        for (old, new) in data.nodes().iter().zip(inserted) {
                            if old.as_column().is_some() {
                                columns.insert(old.id(), new);
                            }
                        }
                    }
                    // Une référence rompue reçoit une réservation sans nœud :
                    // ni une colonne existante ni une future allocation ne
                    // doivent lui donner un autre sens dans la destination.
                    let mut spec = spec.clone();
                    for series in spec.panels.iter_mut().flat_map(|p| &mut p.series) {
                        for reference in [&mut series.x, &mut series.x_error, &mut series.y_error]
                            .into_iter()
                            .flatten()
                        {
                            let id = match columns.get(reference) {
                                Some(id) => *id,
                                None => {
                                    let id = self.ids.next()?;
                                    columns.insert(*reference, id);
                                    id
                                }
                            };
                            *reference = id;
                        }
                        let id = match columns.get(&series.y) {
                            Some(id) => *id,
                            None => {
                                let id = self.ids.next()?;
                                columns.insert(series.y, id);
                                id
                            }
                        };
                        series.y = id;
                    }
                    graph_specs.insert(source.id(), spec);
                }
            }
            let mut root = None;
            for source in copy.nodes() {
                let destination = if source.id() == copy.root().id() {
                    parent
                } else {
                    source
                        .parent()
                        .and_then(|id| ids.get(&id).copied())
                        .ok_or_else(|| CoreError::Structure("copy is not in prefix order".into()))?
                };
                let mut kind = source.kind().clone();
                if let NodeKind::Graph { spec, .. } = &mut kind {
                    if let Some(replacement) = graph_specs.remove(&source.id()) {
                        *spec = replacement;
                    }
                }
                // Une version venue d'un autre projet ne doit jamais être
                // réattribuée plus tard à un autre contenu de cette colonne :
                // `insert_node`, ci-dessous, fait passer le compteur au-delà.
                if let Some(children) = kind.children_mut() {
                    children.clear();
                }
                let index = self.tree.children(destination)?.len();
                let id = self.insert_node(
                    destination,
                    index,
                    if root.is_none() { name } else { source.name() },
                    kind,
                )?;
                ids.insert(source.id(), id);
                root.get_or_insert(id);
            }
            // Les formules des colonnes copiées lisent les colonnes **de la
            // copie** (#321), comme les séries ci-dessus : renumérotées après
            // la boucle, parce qu'une formule peut lire une sœur insérée plus
            // tard (ordre préfixe). Une référence hors de la copie reçoit une
            // réservation sans nœud — elle s'affiche `#REF`, et ni une colonne
            // existante ni une future allocation ne doivent lui donner un autre
            // sens dans la destination.
            let mut reserved = std::collections::HashMap::new();
            for source in copy.nodes() {
                let Some(formula) = source
                    .as_column()
                    .and_then(|column| column.meta().formula.as_ref())
                else {
                    continue;
                };
                let Some(&new_column) = ids.get(&source.id()) else {
                    continue;
                };
                let mut failure = None;
                let remapped = formula.remap(|old| {
                    if let Some(&id) = ids.get(&old).or_else(|| reserved.get(&old)) {
                        return id;
                    }
                    match self.ids.next() {
                        Ok(id) => {
                            reserved.insert(old, id);
                            id
                        }
                        Err(error) => {
                            failure.get_or_insert(error);
                            old
                        }
                    }
                });
                if let Some(error) = failure {
                    return Err(error);
                }
                let column = Arc::make_mut(&mut self.tree).column_mut(new_column)?;
                let mut meta = column.meta().clone();
                meta.formula = Some(remapped);
                column.set_meta(meta);
            }
            root.ok_or_else(|| CoreError::Structure("empty copy".into()))
        })();
        if result.is_err() {
            self.tree = before;
        }
        result
    }

    /// Insère une colonne à `index` (`None` → en fin). `name: None` → premier
    /// nom libre façon tableur (A, B, …, Z, AA, AB, …) dans cette feuille.
    pub fn add_column(
        &mut self,
        sheet: NodeId,
        index: Option<usize>,
        name: Option<&str>,
        column: Column,
    ) -> CoreResult<NodeId> {
        let sheet_node = self.tree.get(sheet)?;
        if !sheet_node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                sheet_node.kind().with_article()
            )));
        }
        let index = index.unwrap_or_else(|| sheet_node.children().len());
        let name = match name {
            Some(name) => name.to_string(),
            None => self.next_column_name(sheet)?,
        };
        self.insert_node(sheet, index, &name, NodeKind::Column(column))
    }

    /// Ajoute un graphe portant `spec`. `name: None` → premier « GraphN »
    /// libre.
    ///
    /// Contrairement à `add_workbook`/`add_matrix`/`add_note`, le contenu
    /// est un paramètre obligatoire — même choix que [`Project::add_column`]
    /// pour la même raison : un graphe sans spec ne décrit rien de plus
    /// qu'une fenêtre vide, autant le rendre explicite à l'appel plutôt que
    /// de fabriquer un défaut que personne n'a demandé. Un appelant qui veut
    /// vraiment un graphe vide passe `PlotSpec::default()`.
    ///
    /// [`Project::set_plot_spec`] remplace ensuite le spec d'un graphe déjà
    /// créé, toujours en bloc.
    pub fn add_graph(
        &mut self,
        parent: NodeId,
        name: Option<&str>,
        spec: PlotSpec,
    ) -> CoreResult<NodeId> {
        let name = self.window_name(name, "Graph");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Graph {
                meta: WindowMeta::default(),
                spec,
            },
        )
    }

    /// Ajoute une matrice. `name: None` → premier « MBookN » libre.
    pub fn add_matrix(&mut self, parent: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        let name = self.window_name(name, "MBook");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Matrix {
                meta: WindowMeta::default(),
            },
        )
    }

    /// Ajoute une note portant `text`. `name: None` → premier « NoteN » libre.
    pub fn add_note(
        &mut self,
        parent: NodeId,
        name: Option<&str>,
        text: &str,
    ) -> CoreResult<NodeId> {
        let name = self.window_name(name, "Note");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Note {
                meta: WindowMeta::default(),
                text: text.to_string(),
            },
        )
    }

    /// Insère un nœud générique **sans enfants** à `index` dans `parent`.
    /// Façade bas niveau des méthodes `add_*`.
    ///
    /// Une colonne garde la version qu'elle porte (venue d'un fichier, d'un
    /// autre projet, d'un instantané d'avant un chargement), mais le compteur
    /// du projet passe au-delà : sans quoi une écriture future de cette
    /// colonne pourrait retirer ce numéro pour un autre contenu, et le
    /// recalcul ciblé des formules (#193) croirait à jour une formule qui la
    /// lit — faux « propre » trouvé en relecture, verrouillé par
    /// `une_colonne_ajoutee_avec_sa_version_ne_fait_pas_resservir_ce_numero`
    /// (`stokesforge-session/tests/recalc_cible.rs`).
    pub fn insert_node(
        &mut self,
        parent: NodeId,
        index: usize,
        name: &str,
        kind: NodeKind,
    ) -> CoreResult<NodeId> {
        // Validation en lecture seule d'abord : un échec ne doit pas cloner
        // l'arbre ni consommer un identifiant.
        self.tree.validate_insert(parent, index, name, &kind)?;
        let id = self.ids.next()?;
        let version = match &kind {
            NodeKind::Column(column) => Some(column.version()),
            _ => None,
        };
        let node = Node::new(id, name, kind)?;
        let tree = Arc::make_mut(&mut self.tree);
        let inserted = tree.insert(parent, index, node)?;
        if let Some(version) = version {
            self.column_version_counter =
                self.column_version_counter.max(version.saturating_add(1));
        }
        Ok(inserted)
    }

    /// Retire `id` et toute sa descendance. Rend l'index qu'il occupait.
    /// Les copies du classeur caché que plus aucune analyse ne cite partent
    /// avec lui (#248).
    pub fn remove(&mut self, id: NodeId) -> CoreResult<usize> {
        let index = self.remove_without_gc(id)?;
        self.gc_analysis_store();
        Ok(index)
    }

    /// [`Project::remove`] sans le ménage du classeur caché : pour une
    /// opération qui vient de créer des copies qu'aucun calcul ne cite encore
    /// et qui fait le ménage à sa fin.
    pub(crate) fn remove_without_gc(&mut self, id: NodeId) -> CoreResult<usize> {
        self.tree.validate_remove(id)?;
        let tree = Arc::make_mut(&mut self.tree);
        let index = tree.remove(id)?;
        self.prune_orphaned_calculations();
        Ok(index)
    }

    /// Retire les calculs dont **toutes** les colonnes Y d'entrée ont disparu
    /// (feuille, classeur ou colonnes supprimés) : ils n'ont plus de feuille où
    /// s'afficher ni rien à recalculer. Un calcul qui garde au moins une série
    /// reste, périmé pour les autres (`Staleness::InputMissing`). Annuler la
    /// suppression les ramène avec l'arbre.
    fn prune_orphaned_calculations(&mut self) {
        let orphans: Vec<NodeId> = self
            .tree
            .calculations()
            .iter()
            .filter(|(_, c)| c.series.iter().all(|s| self.tree.node(s.y).is_none()))
            .map(|(id, _)| *id)
            .collect();
        let traces: Vec<NodeId> = self
            .tree
            .correction_traces()
            .iter()
            .filter(|(_, c)| c.series.iter().all(|s| self.tree.node(s.y).is_none()))
            .map(|(id, _)| *id)
            .collect();
        if orphans.is_empty() && traces.is_empty() {
            return;
        }
        let tree = Arc::make_mut(&mut self.tree);
        for id in orphans {
            tree.calculations_mut().remove(&id);
        }
        for id in traces {
            tree.correction_traces_mut().remove(&id);
        }
    }

    /// Renomme `id`. Rend l'ancien nom.
    pub fn rename(&mut self, id: NodeId, name: &str) -> CoreResult<String> {
        let previous = self.tree.validate_rename(id, name)?;
        if previous == name {
            return Ok(previous);
        }
        let tree = Arc::make_mut(&mut self.tree);
        tree.rename(id, name)
    }

    /// Déplace `id` sous `new_parent` à `index`. Rend `(ancien parent, ancien index)`.
    pub fn move_node(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> CoreResult<(NodeId, usize)> {
        self.tree.validate_move(id, new_parent, index)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.move_node(id, new_parent, index)
    }

    /// Remplace les métadonnées d'une fenêtre. Rend les précédentes.
    /// `Err(Structure)` si `id` n'est pas une fenêtre.
    pub fn set_window_meta(&mut self, id: NodeId, meta: WindowMeta) -> CoreResult<WindowMeta> {
        let node = self.tree.get(id)?;
        if !node.kind().is_window() {
            return Err(CoreError::Structure(format!(
                "{id} is not a window but {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .kind_mut()
            .window_meta_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a window")))?;
        Ok(std::mem::replace(slot, meta))
    }

    /// Remplace le texte d'une note. Rend le précédent. `Err(Structure)` si `id`
    /// n'est pas une note.
    pub fn set_note_text(&mut self, id: NodeId, text: &str) -> CoreResult<String> {
        let node = self.tree.get(id)?;
        if !matches!(node.kind(), NodeKind::Note { .. }) {
            return Err(CoreError::Structure(format!(
                "{id} is not a note but {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .note_text_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a note")))?;
        Ok(std::mem::replace(slot, text.to_string()))
    }

    /// Remplace le `PlotSpec` d'un graphe existant, **en bloc** — jamais par
    /// sous-champ (série, axe, légende) : même logique que
    /// [`Project::set_values`], qui remplace une plage entière plutôt que
    /// cellule par cellule. Rend le précédent. `Err(Structure)` si `id` n'est
    /// pas un graphe.
    pub fn set_plot_spec(&mut self, id: NodeId, spec: PlotSpec) -> CoreResult<PlotSpec> {
        let node = self.tree.get(id)?;
        if !matches!(node.kind(), NodeKind::Graph { .. }) {
            return Err(CoreError::Structure(format!(
                "{id} is not a graph but {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .kind_mut()
            .plot_spec_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a graph")))?;
        Ok(std::mem::replace(slot, spec))
    }

    /// Remplace les couleurs enregistrées dans le projet, et rend les
    /// précédentes. Bornées à [`stokesforge_plot::MAX_SAVED_COLORS`] par usage.
    pub fn set_saved_colors(&mut self, colors: SavedColors) -> SavedColors {
        let tree = Arc::make_mut(&mut self.tree);
        std::mem::replace(tree.saved_colors_mut(), colors.bounded())
    }

    // -- données --

    /// Écrit `block` dans `column` à partir de la ligne `start`.
    ///
    /// A7 (`guide-de-correction-robustesse.md` §F2) — le numéro de version
    /// que `Column::set_values` s'attribue en interne est écrasé par un
    /// numéro tiré de [`Project::next_column_version`], jamais réattribué
    /// sur toute la vie du projet (voir la doc du champ
    /// `column_version_counter`). Tiré seulement après le `?` : un appel
    /// refusé (bloc du mauvais type, borne dépassée) ne consomme aucun
    /// numéro.
    pub fn set_values(
        &mut self,
        column: NodeId,
        start: usize,
        block: ValueBlock,
    ) -> CoreResult<ValuesDelta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let delta = tree.column_mut(column)?.set_values(start, block)?;
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(delta)
    }

    /// Tronque ou complète `column` à `length` lignes. Voir [`Project::
    /// set_values`] pour pourquoi le numéro de version est réattribué par
    /// le projet (A7).
    pub fn set_length(&mut self, column: NodeId, length: usize) -> CoreResult<ValuesDelta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let delta = tree.column_mut(column)?.set_length(length);
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(delta)
    }

    /// Remplace les métadonnées d'une colonne. Rend les précédentes.
    pub fn set_column_meta(&mut self, column: NodeId, meta: ColumnMeta) -> CoreResult<ColumnMeta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        Ok(tree.column_mut(column)?.set_meta(meta))
    }

    /// Change le type d'une colonne. Rend `(ancien type, anciennes valeurs)`.
    /// Version réattribuée par le projet (A7) — voir [`Project::set_values`].
    pub fn set_column_type(
        &mut self,
        column: NodeId,
        column_type: ColumnType,
    ) -> CoreResult<(ColumnType, ColumnValues)> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let previous = tree.column_mut(column)?.set_column_type(column_type);
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(previous)
    }

    /// Restaure type et valeurs d'une colonne (annulation d'un changement de
    /// type). Version réattribuée par le projet (A7) — voir
    /// [`Project::set_values`] : « annuler est une écriture de plus » (§6 du
    /// guide), donc une écriture qui doit, elle aussi, recevoir un numéro
    /// que ce projet n'a jamais attribué.
    pub fn restore_column(
        &mut self,
        column: NodeId,
        column_type: ColumnType,
        values: ColumnValues,
    ) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.restore(column_type, values)?;
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(())
    }

    /// Le prochain numéro de version de colonne, jamais réattribué (voir la
    /// doc du champ `column_version_counter`). `pub` : `stokesforge-commands`
    /// (A7) en a besoin pour toute écriture de colonne, ordinaire ou
    /// restauration — voir [`Project::restamp_column_version`].
    pub fn next_column_version(&mut self) -> u64 {
        let version = self.column_version_counter;
        self.column_version_counter = self.column_version_counter.wrapping_add(1);
        version
    }

    /// Impose un numéro de version à `column`, sans toucher à son contenu.
    ///
    /// **Réservé à `History::revert`** (`stokesforge-commands`, A7,
    /// `guide-de-correction-robustesse.md` §F2) : `Entry::Structural`
    /// restaure un `Arc<Tree>` entier via [`Project::replace_tree`],
    /// compteurs de version compris, ce qui peut faire reculer celui d'une
    /// colonne vers un numéro qu'un contenu différent (le sien, à un autre
    /// instant de son histoire) a déjà porté. `History::revert` appelle
    /// cette méthode juste après `replace_tree`, avec un numéro tiré de
    /// [`Project::next_column_version`], pour ré-attribuer aux seules
    /// colonnes concernées un numéro que CE projet n'a jamais attribué à
    /// personne — sans quoi une écriture future pourrait faire coïncider
    /// deux contenus sous le même numéro, exactement ce que la doctrine des
    /// estampilles du guide interdit.
    ///
    /// `Err(Structure)` si `column` n'est pas une colonne.
    pub fn restamp_column_version(&mut self, column: NodeId, version: u64) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.restamp_version(version);
        Ok(())
    }

    /// Insère `count` lignes à `at` dans **chaque colonne de la feuille telle
    /// que `at <= len(colonne)`**. Une colonne trop courte pour avoir une
    /// ligne `at` (`at > len`) est ignorée : les longueurs sont libres entre
    /// colonnes d'une même feuille, et allonger une colonne courte
    /// inventerait des données. `at == len` — la fin de la colonne — **est**
    /// une cible : c'est la seule façon d'ajouter une ligne en fin de
    /// feuille (B5, `guide-de-correction-robustesse.md` §F5), et c'est
    /// exactement la plage que `Column::insert_rows` accepte déjà par
    /// colonne (`at ∈ 0..=len`). Rend, dans l'ordre de la feuille, les
    /// identifiants des colonnes effectivement allongées (vide si aucune) ;
    /// `History::execute` n'empile alors rien pour ce cas-là (F5).
    pub fn insert_rows(
        &mut self,
        sheet: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<Vec<NodeId>> {
        let targets = self.row_targets(sheet, at, true)?;
        if targets.is_empty() || count == 0 {
            return Ok(Vec::new());
        }
        // A7 — un numéro frais par colonne réellement touchée, tiré avant de
        // prendre `tree` en emprunt exclusif (`next_column_version` a besoin
        // de `&mut self` tout entier).
        let fresh_versions: Vec<u64> = targets.iter().map(|_| self.next_column_version()).collect();
        let tree = Arc::make_mut(&mut self.tree);
        for (&id, fresh) in targets.iter().zip(fresh_versions) {
            tree.column_mut(id)?.insert_rows(at, count)?;
            tree.column_mut(id)?.restamp_version(fresh);
        }
        Ok(targets)
    }

    /// Supprime `count` lignes à `at` dans **le même ensemble** de colonnes
    /// (`at < len` — contrairement à `insert_rows`, `at == len` ne désigne
    /// aucune ligne existante et ne doit rien cibler). Rend, par colonne
    /// touchée et dans l'ordre de la feuille, `(NodeId, valeurs retirées)`.
    pub fn delete_rows(
        &mut self,
        sheet: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<Vec<(NodeId, ValueBlock)>> {
        let targets = self.row_targets(sheet, at, false)?;
        if targets.is_empty() || count == 0 {
            return Ok(Vec::new());
        }
        // A7 — voir `Project::insert_rows`.
        let fresh_versions: Vec<u64> = targets.iter().map(|_| self.next_column_version()).collect();
        let tree = Arc::make_mut(&mut self.tree);
        let mut removed = Vec::with_capacity(targets.len());
        for (id, fresh) in targets.into_iter().zip(fresh_versions) {
            let block = tree.column_mut(id)?.delete_rows(at, count)?;
            tree.column_mut(id)?.restamp_version(fresh);
            removed.push((id, block));
        }
        Ok(removed)
    }

    /// Insère `count` lignes manquantes à `at` dans **cette seule colonne**
    /// (`at` dans `0..=len`). Primitive par colonne, utilisée pour rejouer
    /// l'annulation de `delete_rows` colonne par colonne (une colonne vidée
    /// jusqu'à sa fin n'a plus de ligne `at` et échappe à `insert_rows`, qui
    /// opère au niveau feuille).
    pub fn insert_column_rows(
        &mut self,
        column: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.insert_rows(at, count)?;
        // A7 — voir `Project::insert_rows`. Primitive d'annulation : « annuler
        // est une écriture de plus » (§6 du guide).
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(())
    }

    /// Supprime `count` lignes à `at` dans **cette seule colonne**, rabattu à
    /// la fin. Rend les valeurs retirées. Primitive par colonne, symétrique
    /// d'`insert_column_rows`.
    pub fn delete_column_rows(
        &mut self,
        column: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<ValueBlock> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let removed = tree.column_mut(column)?.delete_rows(at, count)?;
        // A7 — voir `Project::insert_column_rows`.
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(removed)
    }

    // -- chemins --

    /// Analyse `text` puis le résout.
    ///
    /// Simple délégation à [`Tree::resolve`] : la résolution ne demande que de
    /// la lecture, elle vit donc dans `Tree`, où les clients qui n'ont qu'un
    /// instantané `Arc<Tree>` peuvent l'atteindre. Cette méthode reste ici
    /// parce qu'elle est le point d'entrée naturel quand on tient un `Project`.
    pub fn resolve_path(&self, text: &str) -> CoreResult<NodeId> {
        self.tree.resolve(text)
    }

    /// Résout un chemin analysé vers un identifiant de nœud. Délègue à
    /// [`Tree::resolve_parsed`].
    pub fn resolve(&self, path: &ObjectPath) -> CoreResult<NodeId> {
        self.tree.resolve_parsed(path)
    }

    /// Chemin canonique ancré : `/` pour la racine, `/A/B` pour un dossier,
    /// `/A/[Book1]Sheet1!Col` pour le reste (le préfixe est la suite des
    /// dossiers contenant la fenêtre, racine exclue).
    pub fn path_of(&self, id: NodeId) -> CoreResult<String> {
        let node = self.tree.get(id)?;
        let path = match node.kind() {
            NodeKind::Folder { .. } => {
                let mut folders = self.folder_names_above(id)?;
                if id != self.root() {
                    folders.push(node.name().to_string());
                }
                ObjectPath {
                    folders,
                    rooted: true,
                    ..Default::default()
                }
            }
            NodeKind::Workbook { .. }
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => ObjectPath {
                folders: self.folder_names_above(id)?,
                window: Some(node.name().to_string()),
                rooted: true,
                ..Default::default()
            },
            NodeKind::Worksheet { .. } => {
                let book = self.required_parent(id)?;
                ObjectPath {
                    folders: self.folder_names_above(book)?,
                    window: Some(self.tree.get(book)?.name().to_string()),
                    sheet: Some(node.name().to_string()),
                    rooted: true,
                    ..Default::default()
                }
            }
            NodeKind::Column(_) => {
                let sheet = self.required_parent(id)?;
                let book = self.required_parent(sheet)?;
                ObjectPath {
                    folders: self.folder_names_above(book)?,
                    window: Some(self.tree.get(book)?.name().to_string()),
                    sheet: Some(self.tree.get(sheet)?.name().to_string()),
                    column: Some(node.name().to_string()),
                    rooted: true,
                }
            }
        };
        Ok(format_path(&path))
    }

    /// Premier `{prefix}{n}` (n ≥ 1) qui n'est le nom d'aucune fenêtre existante.
    ///
    /// D1 — ne collecte plus la liste de **toutes** les fenêtres à chaque
    /// appel (ni, en dessous, la liste de **tous** les nœuds du projet, voir
    /// D2/`Tree::find_window`) : `window_count` donne un point de départ
    /// proche du bon nom en O(nœuds) mais sans allocation proportionnelle,
    /// et `find_window` teste chaque candidat directement. Dans le cas
    /// courant — des noms attribués dans l'ordre — un seul candidat est
    /// essayé.
    pub fn next_window_name(&self, prefix: &str) -> String {
        let mut n: u64 = self.tree.window_count_with_prefix(prefix) + 1;
        let mut candidate = format!("{prefix}{n}");
        while self.tree.find_window(&candidate).is_some() {
            n += 1;
            candidate = format!("{prefix}{n}");
        }
        candidate
    }

    // -- interne --

    /// Nom de fenêtre demandé, ou premier « {prefix}N » libre.
    fn window_name(&self, name: Option<&str>, prefix: &str) -> String {
        match name {
            Some(name) => name.to_string(),
            None => self.next_window_name(prefix),
        }
    }

    /// Premier « {prefix}N » (n ≥ 1) libre parmi les enfants directs de `parent`.
    ///
    /// D1 — ne collecte plus les noms des enfants dans un `Vec`/`HashSet` à
    /// chaque appel (c'était l'allocation qui rendait le nommage automatique
    /// O(n²) par nœud, O(n³) pour la fratrie) : `children(parent)` rend déjà
    /// la tranche sans copie, et la comparaison candidat par candidat ne
    /// coûte qu'en comparaisons, pas en allocations — invisible pour le
    /// compteur d'octets de `b17`. Départ à `children.len()` plutôt que 1 :
    /// dans le cas courant (noms attribués dans l'ordre), un seul candidat
    /// suffit.
    fn next_child_name(&self, parent: NodeId, prefix: &str) -> CoreResult<String> {
        let children = self.tree.children(parent)?;
        let mut n: u64 = children.len() as u64 + 1;
        let mut candidate = format!("{prefix}{n}");
        while children
            .iter()
            .filter_map(|child| self.tree.node(*child))
            .any(|node| node.name() == candidate)
        {
            n += 1;
            candidate = format!("{prefix}{n}");
        }
        Ok(candidate)
    }

    /// Premier nom de colonne libre façon tableur dans `sheet`.
    ///
    /// D1 — voir [`Project::next_child_name`] : aucune collection de noms
    /// n'est construite, et l'index part de `children.len()` plutôt que de
    /// 0.
    fn next_column_name(&self, sheet: NodeId) -> CoreResult<String> {
        let children = self.tree.children(sheet)?;
        let mut index: usize = children.len();
        let mut candidate = spreadsheet_name(index);
        while children
            .iter()
            .filter_map(|child| self.tree.node(*child))
            .any(|node| node.name() == candidate)
        {
            index += 1;
            candidate = spreadsheet_name(index);
        }
        Ok(candidate)
    }

    /// Noms des dossiers contenant `id`, racine exclue, du haut vers le bas.
    fn folder_names_above(&self, id: NodeId) -> CoreResult<Vec<String>> {
        self.tree.folder_names_above(id)
    }

    /// Parent de `id`, ou `Err(Structure)` s'il n'en a pas (nœud détaché ou racine).
    fn required_parent(&self, id: NodeId) -> CoreResult<NodeId> {
        self.tree
            .parent(id)?
            .ok_or_else(|| CoreError::Structure(format!("{id} has no parent")))
    }

    /// Vérifie en lecture seule que `id` est une colonne — évite de cloner
    /// l'arbre pour rien sur le cas d'échec courant.
    fn check_column(&self, id: NodeId) -> CoreResult<()> {
        let node = self.tree.get(id)?;
        if node.kind().is_column() {
            Ok(())
        } else {
            Err(CoreError::Structure(format!(
                "{id} is not a column but {}",
                node.kind().with_article()
            )))
        }
    }

    /// Colonnes de `sheet`, dans l'ordre, qui possèdent une ligne `at`.
    /// Colonnes de `sheet` dont la ligne `at` existe (`insert_rows` :
    /// `allow_append` inclut aussi `at == len`, la fin de la colonne ;
    /// `delete_rows` ne le veut pas — voir B5, `guide-de-correction-
    /// robustesse.md` §F5).
    fn row_targets(&self, sheet: NodeId, at: usize, allow_append: bool) -> CoreResult<Vec<NodeId>> {
        let sheet_node = self.tree.get(sheet)?;
        if !sheet_node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                sheet_node.kind().with_article()
            )));
        }
        Ok(sheet_node
            .children()
            .iter()
            .copied()
            .filter(|id| {
                self.tree
                    .node(*id)
                    .and_then(Node::as_column)
                    .is_some_and(|column| {
                        if allow_append {
                            at <= column.len()
                        } else {
                            at < column.len()
                        }
                    })
            })
            .collect())
    }
}

impl Default for Project {
    /// Projet vide nommé « Project ».
    fn default() -> Self {
        Self::with_root_name("Project")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Le helper a suivi la résolution dans `tree.rs` ; son test reste ici,
    // inchangé, et l'importe de son nouveau domicile.
    use crate::tree::display_folders;

    /// Une colonne insérée avec sa version la garde, et le compteur passe
    /// au-delà : aucune écriture future ne retirera ce numéro (#193). Le
    /// témoin : une colonne neuve (version 0) ne fait pas reculer le
    /// compteur, qui reste au-delà de ce qu'il a déjà donné.
    #[test]
    fn une_colonne_inseree_fait_passer_le_compteur_au_dela_de_sa_version() {
        let mut project = Project::new("Demo").expect("projet");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
        let venue = Column::from_block_with_version(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
            40,
        )
        .expect("colonne");
        let id = project
            .add_column(sheet, None, None, venue)
            .expect("insertion");
        assert_eq!(
            project
                .tree()
                .node(id)
                .and_then(Node::as_column)
                .map(Column::version),
            Some(40)
        );
        assert_eq!(project.next_column_version(), 41);

        project
            .add_column(sheet, None, None, Column::from_f64(&[2.0]))
            .expect("insertion");
        assert_eq!(project.next_column_version(), 42, "témoin : pas de recul");
    }

    #[test]
    fn spreadsheet_names_follow_the_reference_algorithm() {
        assert_eq!(spreadsheet_name(0), "A");
        assert_eq!(spreadsheet_name(25), "Z");
        assert_eq!(spreadsheet_name(26), "AA");
        assert_eq!(spreadsheet_name(27), "AB");
        assert_eq!(spreadsheet_name(51), "AZ");
        assert_eq!(spreadsheet_name(52), "BA");
        assert_eq!(spreadsheet_name(701), "ZZ");
        assert_eq!(spreadsheet_name(702), "AAA");
    }

    #[test]
    fn display_folders_shows_the_root_as_a_slash() {
        assert_eq!(display_folders(&[]), "/");
        assert_eq!(display_folders(&["A".to_string(), "B".to_string()]), "A/B");
    }

    #[test]
    fn default_is_an_empty_project_named_project() {
        let project = Project::default();
        assert_eq!(project.name(), "Project");
        assert_eq!(project.tree().len(), 1);
        assert_eq!(project.next_id_hint(), 2);
    }

    #[test]
    fn new_rejects_an_invalid_name() {
        assert!(matches!(
            Project::new("[oups]"),
            Err(CoreError::InvalidName(_))
        ));
    }

    #[test]
    fn a_failed_insertion_consumes_no_identifier() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_folder(root, "Manips").unwrap();
        let before = project.next_id_hint();
        assert!(project.add_folder(root, "Manips").is_err());
        assert_eq!(project.next_id_hint(), before);
    }

    #[test]
    fn add_workbook_with_sheet_rolls_back_the_workbook_on_a_failed_sheet() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        let before = project.tree().clone();

        let err = project.add_workbook_with_sheet(root, None, Some("Sheet!1"));
        assert!(matches!(err, Err(CoreError::InvalidName(_))));
        assert_eq!(**project.tree(), *before);
    }

    #[test]
    fn window_names_skip_the_taken_ones() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_empty_workbook(root, Some("Book2")).unwrap();
        // D1 (`guide-de-correction-robustesse.md` §F7) — le nom automatique
        // part du NOMBRE de fenêtres existantes, pas de 1 : reconstruire la
        // liste des noms pris pour trouver le plus petit numéro libre est
        // précisément le coût que D1 supprime (O(n²) par nom, O(n³) pour la
        // feuille, mesuré à 17,8 s pour 4 000 colonnes). Avec une seule
        // fenêtre déjà là (« Book2 »), le premier candidat essayé est donc
        // « Book2 » (déjà pris), puis « Book3 » (libre) — plus « Book1 »,
        // qui reste un nom valide mais n'est plus cherché.
        assert_eq!(project.next_window_name("Book"), "Book3");
        project.add_empty_workbook(root, Some("Book1")).unwrap();
        assert_eq!(project.next_window_name("Book"), "Book3");
    }

    // -- formules par identifiant (#321) --

    /// Feuille de `book` (« Sheet1 ») et `count` colonnes ordinaires à la suite.
    fn sheet_with_columns(
        project: &mut Project,
        book: &str,
        count: usize,
    ) -> (NodeId, Vec<NodeId>) {
        let root = project.root();
        project.add_workbook(root, Some(book)).expect("classeur");
        let sheet = project
            .resolve_path(&format!("[{book}]Sheet1"))
            .expect("feuille");
        let columns = (0..count)
            .map(|k| {
                project
                    .add_column(sheet, None, None, Column::from_f64(&[k as f64 + 1.0]))
                    .expect("colonne")
            })
            .collect();
        (sheet, columns)
    }

    /// Pose la formule `text` (en lettres) sur `column`, comme le fait l'interface.
    fn set_formula(project: &mut Project, column: NodeId, text: &str) {
        let formula = project
            .tree()
            .formula_from_letters(column, text)
            .expect("formule");
        let mut meta = project
            .tree()
            .node(column)
            .and_then(Node::as_column)
            .expect("colonne")
            .meta()
            .clone();
        meta.formula = Some(formula);
        project.set_column_meta(column, meta).expect("méta");
    }

    /// Colonnes de la feuille, dans l'ordre.
    fn columns_of(project: &Project, sheet: NodeId) -> Vec<NodeId> {
        project.tree().children(sheet).expect("feuille").to_vec()
    }

    /// La formule stockée de `column`.
    fn formula_of(project: &Project, column: NodeId) -> crate::ColumnFormula {
        project
            .tree()
            .node(column)
            .and_then(Node::as_column)
            .and_then(|column| column.meta().formula.clone())
            .expect("formule")
    }

    /// Une feuille dupliquée : la formule de la copie lit les colonnes **de la
    /// copie**, pas celles de la mère (F7). Témoin : la mère garde les siennes.
    /// Mutant : renumérotation retirée -> la copie lit les colonnes de la mère
    /// (ses lettres s'affichent `#REF`).
    #[test]
    fn dupliquer_une_feuille_fait_lire_a_la_copie_ses_propres_colonnes() {
        let mut project = Project::new("Demo").expect("projet");
        let (sheet, columns) = sheet_with_columns(&mut project, "Book1", 3);
        set_formula(&mut project, columns[2], "A+B*2");

        let copy = project.duplicate_worksheet(sheet, "Copie").expect("copie");
        let copied = columns_of(&project, copy);
        assert_eq!(copied.len(), 3);

        assert_eq!(
            formula_of(&project, copied[2]).columns(),
            std::collections::BTreeSet::from([copied[0], copied[1]]),
            "la copie lit les colonnes de la copie"
        );
        assert_eq!(
            project.tree().formula_letters(copied[2]).as_deref(),
            Some("A+B*2")
        );
        // Témoin : la mère lit toujours les siennes.
        assert_eq!(
            formula_of(&project, columns[2]).columns(),
            std::collections::BTreeSet::from([columns[0], columns[1]])
        );
        assert_eq!(
            project.tree().formula_letters(columns[2]).as_deref(),
            Some("A+B*2")
        );
    }

    /// Coller une colonne-formule seule dans un autre projet : ses références
    /// sortent de la copie, elles s'affichent `#REF` et le restent, même quand
    /// la destination reçoit assez de colonnes neuves pour rattraper les
    /// numéros de la source (réservation sans nœud, comme les séries).
    /// Mutant : références laissées telles quelles, ou réservation retirée.
    #[test]
    fn une_reference_rompue_a_la_copie_reste_ref_apres_l_ajout_de_colonnes() {
        let mut source = Project::new("Source").expect("projet");
        let (_, source_columns) = sheet_with_columns(&mut source, "BookS", 3);
        set_formula(&mut source, source_columns[2], "A+B");
        let copy = crate::NodeCopy::capture(source.tree(), source_columns[2])
            .expect("capture")
            .into_external();

        let mut destination = Project::new("Destination").expect("projet");
        let (sheet, _) = sheet_with_columns(&mut destination, "BookD", 0);
        let pasted = destination
            .insert_copy(sheet, &copy, "Collée")
            .expect("collage");
        assert_eq!(
            destination.tree().formula_letters(pasted).as_deref(),
            Some("#REF+#REF")
        );

        // La destination reçoit assez de colonnes pour passer les numéros de la source.
        for _ in 0..12 {
            destination
                .add_column(sheet, None, None, Column::from_f64(&[0.0]))
                .expect("colonne");
        }
        assert_eq!(
            destination.tree().formula_letters(pasted).as_deref(),
            Some("#REF+#REF"),
            "aucune colonne neuve n'est lue par erreur"
        );
        // Témoin : les numéros réservés ne sont ceux d'aucune colonne de la feuille.
        let formula = formula_of(&destination, pasted);
        let ids = columns_of(&destination, sheet);
        assert!(formula.has_deleted_reference(&ids));
        assert!(formula.columns().iter().all(|id| !ids.contains(id)));
    }

    /// `next_id_hint` en retard : un fichier enregistré après la suppression
    /// d'une colonne lue garde son numéro dans la formule, et ce numéro ne doit
    /// jamais être réattribué. Mutant : la chaîne des identifiants de formule
    /// retirée de `highest`.
    #[test]
    fn from_tree_ne_reattribue_pas_un_identifiant_lu_par_une_formule() {
        let mut project = Project::new("Demo").expect("projet");
        let (_, columns) = sheet_with_columns(&mut project, "Book1", 2);
        let mut meta = project
            .tree()
            .node(columns[1])
            .and_then(Node::as_column)
            .expect("colonne")
            .meta()
            .clone();
        meta.formula = Some(crate::ColumnFormula::from_parts(vec![
            crate::FormulaPart::Column(NodeId::from_raw(99)),
            crate::FormulaPart::Text("*2".into()),
        ]));
        project.set_column_meta(columns[1], meta).expect("méta");

        let reloaded = Project::from_tree(project.tree().clone(), 1);
        assert!(
            reloaded.next_id_hint() > 99,
            "prochain identifiant : {}",
            reloaded.next_id_hint()
        );
    }
}
