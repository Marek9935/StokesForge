//! Racine du modèle : l'arbre courant (partagé) et l'allocateur d'identifiants.
//!
//! `Project` est la **seule façade publique de mutation** du crate. Ses
//! méthodes prennent `&mut self` ; le seul `&mut Project` du processus sera
//! détenu par l'acteur de `plotyx-session`, ce qui fait de « aucune mutation
//! hors commande » une propriété de propriété, pas une convention.
//!
//! L'allocateur vit ici, **hors de l'arbre**, pour qu'une annulation qui
//! restaure un ancien arbre (`replace_tree`) ne fasse pas reculer le compteur :
//! un identifiant n'est jamais réutilisé sur toute la vie d'un projet,
//! rechargement compris.

use std::sync::Arc;

use plotyx_plot::PlotSpec;

use crate::column::{Column, ColumnMeta, ColumnType, ColumnValues, ValueBlock, ValuesDelta};
use crate::error::{CoreError, CoreResult};
use crate::id::{IdAllocator, NodeId};
use crate::node::{Node, NodeKind, WindowMeta};
use crate::path::{format_path, ObjectPath};
use crate::tree::Tree;

/// Nom d'un dossier ou d'une colonne « façon tableur » : A, B, …, Z, AA, AB, …
fn spreadsheet_name(index: usize) -> String {
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
        let id = ids.next().expect("le premier identifiant ne peut pas être épuisé");
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
        }
    }

    /// Reconstruit depuis un arbre chargé et la valeur persistée du compteur ;
    /// `next_id_hint` est déjà le **prochain** identifiant à attribuer (voir
    /// [`IdAllocator::resume`]), et le compteur reprend à
    /// `max(next_id_hint, plus grand identifiant de l'arbre + 1)`.
    pub fn from_tree(tree: Arc<Tree>, next_id_hint: u64) -> Self {
        let highest = tree.iter().map(|(id, _)| id.raw()).max().unwrap_or(0);
        Self {
            ids: IdAllocator::resume(next_id_hint, highest),
            tree,
        }
    }

    // -- lecture --

    /// Arbre courant. Le cloner (`project.tree().clone()`) fige un instantané.
    pub fn tree(&self) -> &Arc<Tree> {
        &self.tree
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
                "{sheet} n'est pas une feuille mais {}",
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
    /// C'est aussi la **seule** façon de doter un graphe d'un spec : il n'y a
    /// pas de commande d'édition dans ce chantier
    /// (`docs/newarchitecture/roadmap-rendu.md` §3.2) — le spec se construit
    /// ici, une fois, jamais modifié ensuite.
    pub fn add_graph(&mut self, parent: NodeId, name: Option<&str>, spec: PlotSpec) -> CoreResult<NodeId> {
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
        let node = Node::new(id, name, kind)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.insert(parent, index, node)
    }

    /// Retire `id` et toute sa descendance. Rend l'index qu'il occupait.
    pub fn remove(&mut self, id: NodeId) -> CoreResult<usize> {
        self.tree.validate_remove(id)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.remove(id)
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
                "{id} n'est pas une fenêtre mais {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .kind_mut()
            .window_meta_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} n'est pas une fenêtre")))?;
        Ok(std::mem::replace(slot, meta))
    }

    /// Remplace le texte d'une note. Rend le précédent. `Err(Structure)` si `id`
    /// n'est pas une note.
    pub fn set_note_text(&mut self, id: NodeId, text: &str) -> CoreResult<String> {
        let node = self.tree.get(id)?;
        if !matches!(node.kind(), NodeKind::Note { .. }) {
            return Err(CoreError::Structure(format!(
                "{id} n'est pas une note mais {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .note_text_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} n'est pas une note")))?;
        Ok(std::mem::replace(slot, text.to_string()))
    }

    // -- données --

    /// Écrit `block` dans `column` à partir de la ligne `start`.
    pub fn set_values(
        &mut self,
        column: NodeId,
        start: usize,
        block: ValueBlock,
    ) -> CoreResult<ValuesDelta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.set_values(start, block)
    }

    /// Tronque ou complète `column` à `length` lignes.
    pub fn set_length(&mut self, column: NodeId, length: usize) -> CoreResult<ValuesDelta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        Ok(tree.column_mut(column)?.set_length(length))
    }

    /// Remplace les métadonnées d'une colonne. Rend les précédentes.
    pub fn set_column_meta(&mut self, column: NodeId, meta: ColumnMeta) -> CoreResult<ColumnMeta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        Ok(tree.column_mut(column)?.set_meta(meta))
    }

    /// Change le type d'une colonne. Rend `(ancien type, anciennes valeurs)`.
    pub fn set_column_type(
        &mut self,
        column: NodeId,
        column_type: ColumnType,
    ) -> CoreResult<(ColumnType, ColumnValues)> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        Ok(tree.column_mut(column)?.set_column_type(column_type))
    }

    /// Restaure type et valeurs d'une colonne (annulation d'un changement de type).
    pub fn restore_column(
        &mut self,
        column: NodeId,
        column_type: ColumnType,
        values: ColumnValues,
    ) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.restore(column_type, values)
    }

    /// Impose un numéro de version à `column`, sans toucher à son contenu.
    ///
    /// **Réservé à `History::revert`** (`plotyx-commands`, A7,
    /// `guide-de-correction-robustesse.md` §F2) : `Entry::Structural`
    /// restaure un `Arc<Tree>` entier via [`Project::replace_tree`],
    /// compteurs de version compris, ce qui peut faire reculer celui d'une
    /// colonne vers un numéro qu'un contenu différent (le sien, à un autre
    /// instant de son histoire) a déjà porté. `History::revert` appelle
    /// cette méthode juste après `replace_tree` pour ré-attribuer un
    /// numéro **frais** (jamais vu dans le projet) aux seules colonnes
    /// concernées, sans quoi une écriture future pourrait faire coïncider
    /// deux contenus sous le même numéro — exactement ce que la doctrine
    /// des estampilles du guide interdit.
    ///
    /// N'importe quelle valeur peut être imposée : cette méthode ne
    /// vérifie **pas** elle-même qu'elle est fraîche, c'est la
    /// responsabilité de l'appelant (voir `History::restamp_regressed_versions`).
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
        let tree = Arc::make_mut(&mut self.tree);
        for &id in &targets {
            tree.column_mut(id)?.insert_rows(at, count)?;
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
        let tree = Arc::make_mut(&mut self.tree);
        let mut removed = Vec::with_capacity(targets.len());
        for id in targets {
            let block = tree.column_mut(id)?.delete_rows(at, count)?;
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
        tree.column_mut(column)?.insert_rows(at, count)
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
        tree.column_mut(column)?.delete_rows(at, count)
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
            | NodeKind::Note { .. } => ObjectPath {
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
            .ok_or_else(|| CoreError::Structure(format!("{id} n'a pas de parent")))
    }

    /// Vérifie en lecture seule que `id` est une colonne — évite de cloner
    /// l'arbre pour rien sur le cas d'échec courant.
    fn check_column(&self, id: NodeId) -> CoreResult<()> {
        let node = self.tree.get(id)?;
        if node.kind().is_column() {
            Ok(())
        } else {
            Err(CoreError::Structure(format!(
                "{id} n'est pas une colonne mais {}",
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
                "{sheet} n'est pas une feuille mais {}",
                sheet_node.kind().with_article()
            )));
        }
        Ok(sheet_node
            .children()
            .iter()
            .copied()
            .filter(|id| {
                self.tree.node(*id).and_then(Node::as_column).is_some_and(|column| {
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
}
