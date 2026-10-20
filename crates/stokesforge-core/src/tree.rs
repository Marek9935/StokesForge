//! Arbre persistant du projet : une carte plate `NodeId → Arc<Node>` et une racine.
//!
//! Muter un arbre, c'est faire `Arc::make_mut` dessus (ce qui clone la carte —
//! des paires de 16 octets, pas les nœuds) puis `Arc::make_mut` sur les seuls
//! nœuds touchés. Un client qui détient un ancien `Arc<Tree>` voit donc un
//! instantané figé, et tous les nœuds non touchés restent physiquement
//! partagés entre l'ancien et le nouvel arbre. C'est le mécanisme de l'undo
//! « snapshot de structure ».
//!
//! Les mutations sont `pub(crate)` : `Project` est la seule façade publique de
//! mutation du crate. Chacune valide **tout** avant d'écrire quoi que ce soit,
//! pour qu'un échec laisse l'arbre inchangé.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use stokesforge_plot::SavedColors;

use crate::calc::{Calculation, CorrectionRecord};
use crate::column::Column;
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::name::check_name;
use crate::node::{containment_message, Node, NodeKind};
use crate::path::{format_path, parse_path, ObjectPath};

/// Forme lisible d'une suite de dossiers pour les messages d'erreur, `/` pour
/// la racine. `pub(crate)` parce que `Project::resolve` s'appuyait dessus avant
/// de déléguer, et que son test unitaire vit encore dans `project.rs`.
pub(crate) fn display_folders(folders: &[String]) -> String {
    if folders.is_empty() {
        "/".to_string()
    } else {
        folders.join("/")
    }
}

/// Arbre du projet : carte plate `NodeId → Arc<Node>` et racine. Partagé par
/// `Arc<Tree>` ; toute mutation passe par `Project`, qui fait `Arc::make_mut`
/// dessus.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    nodes: HashMap<NodeId, Arc<Node>>,
    root: NodeId,
    /// Couleurs enregistrées dans le projet, par usage — portées par l'arbre
    /// pour voyager avec lui : un instantané de session, un retour arrière
    /// structurel et la persistance les voient sans chemin à part.
    saved_colors: SavedColors,
    /// Calculs d'analyse (#181) — voir [`crate::calc`]. Portés par l'arbre pour
    /// voyager avec lui, comme les couleurs enregistrées.
    calculations: BTreeMap<NodeId, Arc<Calculation>>,
    correction_traces: BTreeMap<NodeId, Arc<CorrectionRecord>>,
}

impl Tree {
    /// Arbre réduit à un dossier racine. `Err(Structure)` si `root` n'est pas un
    /// dossier ou porte déjà un parent.
    pub(crate) fn new(root: Node) -> CoreResult<Self> {
        if !root.kind().is_folder() {
            return Err(CoreError::Structure(format!(
                "the root must be a folder, not {}",
                root.kind().with_article()
            )));
        }
        if root.parent().is_some() {
            return Err(CoreError::Structure(
                "the root cannot have a parent".to_string(),
            ));
        }
        Ok(Self::with_root(root))
    }

    /// Arbre réduit à `root`, sans validation. Réservé aux racines construites
    /// par le crate (chemin de construction infaillible, sans `unwrap`).
    pub(crate) fn with_root(root: Node) -> Self {
        let id = root.id();
        let mut nodes = HashMap::new();
        nodes.insert(id, Arc::new(root));
        Self {
            nodes,
            root: id,
            saved_colors: SavedColors::default(),
            calculations: BTreeMap::new(),
            correction_traces: BTreeMap::new(),
        }
    }

    /// Les corrections définitives des données sources, sans lien de calcul.
    pub fn correction_traces(&self) -> &BTreeMap<NodeId, Arc<CorrectionRecord>> {
        &self.correction_traces
    }

    /// Reconstruit les traces lues du fichier, sans réactiver un calcul.
    pub fn with_correction_traces(
        mut self,
        traces: BTreeMap<NodeId, Arc<CorrectionRecord>>,
    ) -> Self {
        self.correction_traces = traces;
        self
    }

    pub(crate) fn correction_traces_mut(&mut self) -> &mut BTreeMap<NodeId, Arc<CorrectionRecord>> {
        &mut self.correction_traces
    }

    /// Les couleurs enregistrées dans le projet.
    pub fn saved_colors(&self) -> &SavedColors {
        &self.saved_colors
    }

    /// Le même arbre, avec ces couleurs enregistrées (bornées à
    /// [`stokesforge_plot::MAX_SAVED_COLORS`] par usage) — pour la persistance,
    /// qui reconstruit l'arbre puis y repose ce que le fichier portait.
    pub fn with_saved_colors(mut self, colors: SavedColors) -> Self {
        self.saved_colors = colors.bounded();
        self
    }

    pub(crate) fn saved_colors_mut(&mut self) -> &mut SavedColors {
        &mut self.saved_colors
    }

    /// Les calculs d'analyse, par identifiant.
    pub fn calculations(&self) -> &BTreeMap<NodeId, Arc<Calculation>> {
        &self.calculations
    }

    /// Le calcul `id`, s'il existe.
    pub fn calculation(&self, id: NodeId) -> Option<&Arc<Calculation>> {
        self.calculations.get(&id)
    }

    /// Le calcul qui a produit `column`, et le rang de la sortie correspondante
    /// dans `Calculation::outputs`.
    pub fn calculation_of_column(&self, column: NodeId) -> Option<(NodeId, usize)> {
        self.calculations.iter().find_map(|(id, calc)| {
            calc.outputs
                .iter()
                .position(|o| o.column == Some(column))
                .map(|rank| (*id, rank))
        })
    }

    /// Le même arbre, avec ces calculs — pour la persistance, qui reconstruit
    /// l'arbre puis y repose ce que le fichier portait.
    pub fn with_calculations(mut self, calculations: BTreeMap<NodeId, Arc<Calculation>>) -> Self {
        self.calculations = calculations;
        self
    }

    pub(crate) fn calculations_mut(&mut self) -> &mut BTreeMap<NodeId, Arc<Calculation>> {
        &mut self.calculations
    }

    /// Reconstruit un arbre depuis une liste de nœuds (persistance). Valide
    /// tout : unicité des identifiants ; racine présente, dossier, sans parent ;
    /// cohérence croisée parent ↔ enfants ; natures acceptées ; unicité des
    /// noms ; accessibilité depuis la racine. `Err(Structure(..))` nomme le
    /// nœud fautif.
    pub fn from_nodes(root: NodeId, nodes: Vec<Node>) -> CoreResult<Self> {
        let mut map: HashMap<NodeId, Arc<Node>> = HashMap::with_capacity(nodes.len());
        for node in nodes {
            let id = node.id();
            if map.insert(id, Arc::new(node)).is_some() {
                return Err(CoreError::Structure(format!(
                    "duplicate identifier in the node list: {id}"
                )));
            }
        }
        let tree = Self {
            nodes: map,
            root,
            saved_colors: SavedColors::default(),
            calculations: BTreeMap::new(),
            correction_traces: BTreeMap::new(),
        };
        tree.validate_loaded()?;
        Ok(tree)
    }

    /// Vérifie l'intégralité des invariants d'un arbre fraîchement chargé.
    fn validate_loaded(&self) -> CoreResult<()> {
        let root_node = self.nodes.get(&self.root).ok_or_else(|| {
            CoreError::Structure(format!("root missing from the node list: {}", self.root))
        })?;
        if !root_node.kind().is_folder() {
            return Err(CoreError::Structure(format!(
                "the root {} must be a folder, not {}",
                self.root,
                root_node.kind().with_article()
            )));
        }
        if let Some(parent) = root_node.parent() {
            return Err(CoreError::Structure(format!(
                "the root {} cannot have a parent ({parent})",
                self.root
            )));
        }

        // Une passe : occurrences de chaque (parent, enfant) dans les listes
        // d'enfants — évite un `filter().count()` par frère (quadratique sur
        // un dossier à grand nombre d'enfants, #316).
        let mut listed: HashMap<(NodeId, NodeId), usize> = HashMap::new();
        for (id, node) in &self.nodes {
            for child in node.children() {
                *listed.entry((*id, *child)).or_insert(0) += 1;
            }
        }

        for (id, node) in &self.nodes {
            for child in node.children() {
                let child_node = self.nodes.get(child).ok_or_else(|| {
                    CoreError::Structure(format!("{id} lists a missing child: {child}"))
                })?;
                match child_node.parent() {
                    Some(parent) if parent == *id => {}
                    Some(parent) => {
                        return Err(CoreError::Structure(format!(
                        "{child} is listed as a child of {id} but declares {parent} as its parent"
                    )))
                    }
                    None => {
                        return Err(CoreError::Structure(format!(
                            "{child} is listed as a child of {id} but declares no parent"
                        )))
                    }
                }
            }

            if *id == self.root {
                continue;
            }
            let Some(parent) = node.parent() else {
                return Err(CoreError::Structure(format!(
                    "{id} has no parent but is not the root"
                )));
            };
            let parent_node = self.nodes.get(&parent).ok_or_else(|| {
                CoreError::Structure(format!("{id} declares a missing parent: {parent}"))
            })?;
            if !parent_node.kind().accepts_child(node.kind()) {
                return Err(CoreError::Structure(format!(
                    "{id} is misplaced: {}",
                    containment_message(parent_node.kind(), node.kind())
                )));
            }
            let occurrences = listed.get(&(parent, *id)).copied().unwrap_or(0);
            if occurrences != 1 {
                return Err(CoreError::Structure(format!(
                    "{id} is listed {occurrences} times as a child of {parent} (exactly one expected)"
                )));
            }
        }

        self.validate_loaded_names()?;

        let reachable = self.walk(self.root)?.len();
        if reachable != self.nodes.len() {
            return Err(CoreError::Structure(format!(
                "{} node(s) unreachable from the root {}",
                self.nodes.len() - reachable,
                self.root
            )));
        }
        Ok(())
    }

    /// Unicité entre tous les frères directs, fenêtres comprises (#105).
    /// La résolution canonique porte le dossier ; un nom de fenêtre seul
    /// reste utilisable s'il désigne une seule fenêtre dans le projet.
    fn validate_loaded_names(&self) -> CoreResult<()> {
        for (id, node) in &self.nodes {
            let mut seen: HashMap<&str, NodeId> = HashMap::new();
            for &child in node.children() {
                let Some(child_node) = self.nodes.get(&child) else {
                    continue;
                };
                // Aucune exception pour les fenêtres ici : `check_name_
                // available` (voir sa doc, A8) refuse maintenant qu'une
                // fenêtre prenne le nom d'un dossier frère tout autant que
                // l'inverse — deux frères directs ne partagent jamais un
                // nom, quelle que soit leur nature.
                if let Some(&earlier) = seen.get(child_node.name()) {
                    return Err(CoreError::Structure(format!(
                        "{child} ({}) cannot be named {} in {id}: {earlier} \
                         already has that name",
                        child_node.kind().type_name(),
                        child_node.name()
                    )));
                }
                seen.insert(child_node.name(), child);
            }
        }
        Ok(())
    }

    // -- lecture --

    /// Identifiant du dossier racine.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Nombre de nœuds, racine comprise.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Toujours faux : un arbre contient au moins sa racine. Fourni pour la
    /// symétrie avec `len` (et pour clippy).
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Vrai si `id` est présent dans l'arbre.
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Nœud `id`, ou `None`.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id).map(Arc::as_ref)
    }

    /// Nœud `id`, ou `Err(UnknownNode)`.
    pub fn get(&self, id: NodeId) -> CoreResult<&Node> {
        self.node(id).ok_or(CoreError::UnknownNode(id))
    }

    /// L'`Arc` du nœud `id` : sert aux tests de partage structurel et à la
    /// persistance incrémentale.
    pub fn node_arc(&self, id: NodeId) -> Option<&Arc<Node>> {
        self.nodes.get(&id)
    }

    /// Enfants ordonnés de `id`.
    pub fn children(&self, id: NodeId) -> CoreResult<&[NodeId]> {
        Ok(self.get(id)?.children())
    }

    /// Parent de `id` (`None` pour la racine).
    pub fn parent(&self, id: NodeId) -> CoreResult<Option<NodeId>> {
        Ok(self.get(id)?.parent())
    }

    /// Ancêtres de `id`, du plus proche à la racine. Exclut `id`.
    pub fn ancestors(&self, id: NodeId) -> CoreResult<Vec<NodeId>> {
        let mut out = Vec::new();
        let mut current = self.get(id)?.parent();
        while let Some(parent) = current {
            out.push(parent);
            current = self.get(parent)?.parent();
        }
        Ok(out)
    }

    /// Index de `id` dans la liste d'enfants de son parent. `Err(Structure)`
    /// pour la racine (elle n'a pas de parent).
    pub fn index_in_parent(&self, id: NodeId) -> CoreResult<usize> {
        let node = self.get(id)?;
        let Some(parent) = node.parent() else {
            return Err(CoreError::Structure(format!(
                "{id} is the root: it has no position in a parent"
            )));
        };
        self.get(parent)?
            .children()
            .iter()
            .position(|child| *child == id)
            .ok_or_else(|| {
                CoreError::Structure(format!(
                    "{id} is missing from the child list of its parent {parent}"
                ))
            })
    }

    /// Parcours préfixe de `id` et de toute sa descendance (`id` en premier).
    pub fn walk(&self, id: NodeId) -> CoreResult<Vec<NodeId>> {
        self.get(id)?;
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            out.push(current);
            if let Some(node) = self.node(current) {
                for child in node.children().iter().rev() {
                    stack.push(*child);
                }
            }
        }
        Ok(out)
    }

    /// Parcours préfixe depuis la racine.
    pub fn walk_all(&self) -> Vec<NodeId> {
        self.walk(self.root).unwrap_or_default()
    }

    /// Toutes les fenêtres du projet, en ordre de parcours.
    pub fn windows(&self) -> Vec<NodeId> {
        self.walk_all()
            .into_iter()
            .filter(|id| self.node(*id).is_some_and(|node| node.kind().is_window()))
            .collect()
    }

    /// Nombre de fenêtres dont le nom commence par `prefix`.
    ///
    /// D1/D2 — même parcours que [`Tree::find_window`] : sert à `Project::
    /// next_window_name` à deviner un point de départ proche du bon nom,
    /// sans passer par `windows()` (qui, lui, retient effectivement la liste
    /// et alloue en conséquence — nécessaire là où l'appelant veut les
    /// identifiants, pas ici où seul le compte importe). Compté **par
    /// préfixe** et non toutes fenêtres confondues : un projet avec deux
    /// classeurs (« Book1 », « Book2 ») et zéro graphe doit proposer
    /// « Graph1 » pour le premier graphe, pas « Graph3 ».
    pub fn window_count_with_prefix(&self, prefix: &str) -> u64 {
        let mut count = 0u64;
        self.visit_windows(|id| {
            if self
                .node(id)
                .is_some_and(|node| node.name().starts_with(prefix))
            {
                count += 1;
            }
            false
        });
        count
    }

    /// Première fenêtre portant ce nom, ou `None`. Sert à éviter les noms
    /// automatiques déjà pris ; `resolve_parsed` refuse les noms ambigus.
    ///
    /// D2 (option 2, `guide-de-correction-robustesse.md` §F7) — parcours
    /// direct qui s'arrête au premier nœud trouvé, au lieu de passer par
    /// `windows()` → `walk_all()`, qui allouaient un `Vec` de **tous** les
    /// nœuds du projet à chaque résolution de chemin.
    pub fn find_window(&self, name: &str) -> Option<NodeId> {
        let mut found = None;
        self.visit_windows(|id| {
            let is_match = self.node(id).is_some_and(|node| node.name() == name);
            if is_match {
                found = Some(id);
            }
            is_match
        });
        found
    }

    /// Visite chaque fenêtre du projet, sans jamais empiler la totalité des
    /// enfants d'un nœud d'un coup. `on_window` rend `true` pour arrêter le
    /// parcours dès cette fenêtre (utilisé par `find_window`), `false` pour
    /// continuer.
    ///
    /// Une fenêtre ne contient jamais d'autre fenêtre (seuls les dossiers en
    /// contiennent) : il suffit donc de descendre dans les enfants qui sont
    /// des dossiers, et de visiter les autres sur place sans les empiler. Sur
    /// un dossier qui contient N fenêtres et aucun sous-dossier — le cas
    /// courant, une racine qui contient tous les classeurs —, la pile ne
    /// grandit jamais : `stack.extend` sur tous les enfants d'un coup (la
    /// première version de ce correctif) aurait alloué un tampon de taille N
    /// même en s'arrêtant à la première fenêtre visitée, ce qu'une visite
    /// nœud par nœud ne fait pas.
    fn visit_windows(&self, mut on_window: impl FnMut(NodeId) -> bool) {
        let mut stack = vec![self.root];
        while let Some(current) = stack.pop() {
            let Some(node) = self.node(current) else {
                continue;
            };
            for &child in node.children() {
                let Some(child_node) = self.node(child) else {
                    continue;
                };
                if child_node.kind().is_window() {
                    if on_window(child) {
                        return;
                    }
                } else if child_node.kind().is_folder() {
                    stack.push(child);
                }
            }
        }
    }

    /// Enfant direct de `parent` portant ce nom, ou `None`.
    pub fn child_by_name(&self, parent: NodeId, name: &str) -> CoreResult<Option<NodeId>> {
        Ok(self
            .get(parent)?
            .children()
            .iter()
            .copied()
            .find(|child| self.node(*child).is_some_and(|node| node.name() == name)))
    }

    /// Enfant **dossier** de `parent` portant ce nom, ou `None`.
    ///
    /// A8 (`guide-de-correction-robustesse.md` §F9) — distinct de
    /// `child_by_name` : un dossier n'est le seul parent du modèle à
    /// accueillir deux catégories d'enfants (dossiers et fenêtres,
    /// `NodeKind::accepts_child`), et `check_name_available` n'empêche pas
    /// une fenêtre de prendre le nom d'un dossier frère. Chercher par nom
    /// *puis* filtrer par nature choisirait le frère homonyme rencontré en
    /// premier dans `children()`, que ce soit le bon ou non ; chercher
    /// directement parmi les enfants dossiers rend la résolution d'un
    /// segment de chemin indépendante de cet ordre.
    fn folder_child_by_name(&self, parent: NodeId, name: &str) -> CoreResult<Option<NodeId>> {
        Ok(self.get(parent)?.children().iter().copied().find(|child| {
            self.node(*child)
                .is_some_and(|node| node.kind().is_folder() && node.name() == name)
        }))
    }

    /// Nombre de lignes d'une feuille : le maximum des longueurs de ses
    /// colonnes, 0 si elle n'en a aucune. `Err(Structure)` si `sheet` n'est pas
    /// une feuille.
    pub fn row_count(&self, sheet: NodeId) -> CoreResult<usize> {
        let node = self.get(sheet)?;
        if !node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                node.kind().with_article()
            )));
        }
        Ok(node
            .children()
            .iter()
            .filter_map(|child| self.node(*child).and_then(Node::as_column))
            .map(Column::len)
            .max()
            .unwrap_or(0))
    }

    /// Itère sur `(NodeId, &Node)` sans ordre garanti (persistance, statistiques).
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().map(|(id, node)| (*id, node.as_ref()))
    }

    // -- résolution de chemins --

    /// Résout un chemin textuel de la grammaire `[Book1]Sheet1!ColA` vers
    /// l'identifiant du nœud qu'il désigne.
    ///
    /// C'est **l'unique implémentation de la résolution du dépôt** :
    /// `Project::resolve_path` s'y ramène, et un client qui ne détient qu'un
    /// instantané `Arc<Tree>` (le pont Python, demain le rendu de graphes)
    /// l'appelle directement au lieu d'en réécrire une copie contre l'API de
    /// lecture. C'est la raison d'être de cette méthode : la résolution ne
    /// demande que de la lecture, elle n'avait donc rien à faire dans la façade
    /// de mutation.
    ///
    /// Toute erreur est un [`CoreError::Path`], que la faute soit de syntaxe ou
    /// que l'objet manque : vu du dehors, c'est le même cas — un chemin qu'on
    /// ne sait pas suivre — et seul le message distingue les deux.
    pub fn resolve(&self, text: &str) -> CoreResult<NodeId> {
        self.resolve_parsed(&parse_path(text)?)
    }

    /// Résout un chemin déjà analysé, pour l'appelant qui tient un
    /// [`ObjectPath`] et n'a pas à le re-sérialiser pour le faire re-analyser.
    ///
    /// Deux propriétés de la grammaire, faciles à perdre en la réécrivant :
    ///
    /// - le drapeau `rooted` n'est **pas** consulté. Il n'y a pas de dossier
    ///   courant dans le modèle — c'est une notion d'interface, pas de données —
    ///   donc la descente part toujours de la racine et `Manips/[Book1]Sheet1`
    ///   désigne le même nœud que `/Manips/[Book1]Sheet1` ;
    /// - une fenêtre avec un préfixe se cherche dans ce dossier ;
    /// - sans préfixe, son nom doit être non ambigu à l'échelle du projet.
    ///
    /// Le chemin canonique est enraciné, même pour une fenêtre de la racine.
    ///
    /// La comparaison des noms est celle de `child_by_name` et `find_window` :
    /// égalité exacte, donc **sensible à la casse**.
    pub fn resolve_parsed(&self, path: &ObjectPath) -> CoreResult<NodeId> {
        // Chemin sans fenêtre : une descente de dossiers depuis la racine.
        // `folder_child_by_name` ne considère que les enfants dossiers dès la
        // recherche elle-même (A8) : `check_name_available` n'empêche pas une
        // fenêtre de prendre le nom d'un dossier frère (voir sa doc), donc un
        // `child_by_name` générique pourrait tomber sur la fenêtre plutôt que
        // sur le dossier et rendre ce dernier « introuvable » selon l'ordre
        // des enfants — ce qui casserait `path_of ∘ resolve_path = identité`.
        let Some(window_name) = path.window.as_deref() else {
            let mut current = self.root();
            for segment in &path.folders {
                current = self
                    .folder_child_by_name(current, segment)?
                    .ok_or_else(|| CoreError::Path(format!("folder not found: {segment}")))?;
            }
            return Ok(current);
        };

        let window = if path.rooted || !path.folders.is_empty() {
            let mut parent = self.root();
            for segment in &path.folders {
                parent = self
                    .folder_child_by_name(parent, segment)?
                    .ok_or_else(|| CoreError::Path(format!("folder not found: {segment}")))?;
            }
            self.child_by_name(parent, window_name)?
                .filter(|id| self.node(*id).is_some_and(|n| n.kind().is_window()))
                .ok_or_else(|| {
                    CoreError::Path(format!(
                        "window not found in {}: [{window_name}]",
                        display_folders(&path.folders)
                    ))
                })?
        } else {
            let mut found = None;
            let mut ambiguous = false;
            self.visit_windows(|id| {
                if self.node(id).is_some_and(|n| n.name() == window_name) {
                    if found.is_some() {
                        ambiguous = true;
                        return true;
                    }
                    found = Some(id);
                }
                false
            });
            if ambiguous {
                return Err(CoreError::Path(format!(
                    "ambiguous window name: [{window_name}]; use its folder path"
                )));
            }
            found.ok_or_else(|| CoreError::Path(format!("window not found: [{window_name}]")))?
        };

        let Some(sheet_name) = path.sheet.as_deref() else {
            return Ok(window);
        };
        // Un graphe ou une note n'a pas de feuille : le dire vaut mieux que
        // rendre « feuille introuvable » sur une fenêtre qui n'en aura jamais.
        if !matches!(self.get(window)?.kind(), NodeKind::Workbook { .. }) {
            return Err(CoreError::Path(format!(
                "[{window_name}] is not a workbook, it has no worksheet"
            )));
        }
        let sheet = self.child_by_name(window, sheet_name)?.ok_or_else(|| {
            CoreError::Path(format!("worksheet not found: [{window_name}]{sheet_name}"))
        })?;

        let Some(column_name) = path.column.as_deref() else {
            return Ok(sheet);
        };
        // Le message reprend la forme canonique du chemin, pas la chaîne
        // d'entrée : c'est la seule branche qui nomme le chemin entier, et
        // `resolve_parsed` n'a de toute façon plus le texte d'origine.
        self.child_by_name(sheet, column_name)?
            .ok_or_else(|| CoreError::Path(format!("column not found: {}", format_path(path))))
    }

    /// Noms des dossiers au-dessus de `id`, de la racine (exclue) vers `id`.
    ///
    /// Sert des deux côtés du chemin : la résolution s'en sert pour vérifier un
    /// préfixe, `Project::path_of` pour le reconstruire.
    pub(crate) fn folder_names_above(&self, id: NodeId) -> CoreResult<Vec<String>> {
        let root = self.root();
        let mut names: Vec<String> = self
            .ancestors(id)?
            .into_iter()
            .filter(|ancestor| *ancestor != root)
            .filter_map(|ancestor| self.node(ancestor))
            .filter(|node| node.kind().is_folder())
            .map(|node| node.name().to_string())
            .collect();
        names.reverse();
        Ok(names)
    }

    // -- validation (lecture seule) --

    /// Vérifie qu'un nom est libre dans la portée d'unicité de `kind`.
    /// `ignore` exclut un nœud de la comparaison (renommage, déplacement sur place).
    fn check_name_available(
        &self,
        parent: Option<NodeId>,
        _kind: &NodeKind,
        name: &str,
        ignore: Option<NodeId>,
    ) -> CoreResult<()> {
        let Some(parent) = parent else {
            return Ok(()); // nœud sans parent (racine) : pas de frère
        };
        let Some(parent_node) = self.node(parent) else {
            return Ok(());
        };
        for child in parent_node.children() {
            if Some(*child) == ignore {
                continue;
            }
            if self.node(*child).is_some_and(|node| node.name() == name) {
                return Err(CoreError::DuplicateName(format!(
                    "{name} in {}",
                    parent_node.name()
                )));
            }
        }
        Ok(())
    }

    /// Valide une insertion sans rien écrire : nom, nature, parent, index.
    pub(crate) fn validate_insert(
        &self,
        parent: NodeId,
        index: usize,
        name: &str,
        kind: &NodeKind,
    ) -> CoreResult<()> {
        check_name(name)?;
        if !kind.children().is_empty() {
            return Err(CoreError::Structure(
                "an inserted node cannot carry children: rebuild the tree with from_nodes"
                    .to_string(),
            ));
        }
        let parent_node = self.get(parent)?;
        if !parent_node.kind().accepts_child(kind) {
            return Err(CoreError::Structure(containment_message(
                parent_node.kind(),
                kind,
            )));
        }
        self.check_name_available(Some(parent), kind, name, None)?;
        let count = parent_node.children().len();
        if index > count {
            return Err(CoreError::IndexOutOfBounds(format!(
                "position {index} out of range in {} (children: {count})",
                parent_node.name()
            )));
        }
        Ok(())
    }

    /// Valide une suppression sans rien écrire. Rend l'index occupé.
    pub(crate) fn validate_remove(&self, id: NodeId) -> CoreResult<usize> {
        if id == self.root {
            return Err(CoreError::Structure(
                "the project root cannot be removed".to_string(),
            ));
        }
        self.index_in_parent(id)
    }

    /// Valide un déplacement sans rien écrire. Rend `(ancien parent, ancien index)`.
    pub(crate) fn validate_move(
        &self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> CoreResult<(NodeId, usize)> {
        if id == self.root {
            return Err(CoreError::Structure(
                "the project root cannot be moved".to_string(),
            ));
        }
        let node = self.get(id)?;
        let target = self.get(new_parent)?;
        if !target.kind().accepts_child(node.kind()) {
            return Err(CoreError::Structure(containment_message(
                target.kind(),
                node.kind(),
            )));
        }
        if self.walk(id)?.contains(&new_parent) {
            return Err(CoreError::Structure(format!(
                "circular move refused: {new_parent} is {id} or one of its descendants"
            )));
        }
        let Some(old_parent) = node.parent() else {
            return Err(CoreError::Structure(format!(
                "{id} has no parent: it cannot be moved"
            )));
        };
        let old_index = self.index_in_parent(id)?;
        self.check_name_available(Some(new_parent), node.kind(), node.name(), Some(id))?;
        let mut count = target.children().len();
        if new_parent == old_parent {
            count = count.saturating_sub(1);
        }
        if index > count {
            return Err(CoreError::IndexOutOfBounds(format!(
                "position {index} out of range in {} (other children: {count})",
                target.name()
            )));
        }
        Ok((old_parent, old_index))
    }

    /// Valide un renommage sans rien écrire. Rend le nom actuel.
    pub(crate) fn validate_rename(&self, id: NodeId, name: &str) -> CoreResult<String> {
        let node = self.get(id)?;
        check_name(name)?;
        let previous = node.name().to_string();
        if previous == name {
            return Ok(previous);
        }
        self.check_name_available(node.parent(), node.kind(), name, Some(id))?;
        Ok(previous)
    }

    // -- mutations pub(crate) — appelées uniquement par Project --

    /// Insère un nœud détaché et sans enfants à `index` dans `parent`.
    pub(crate) fn insert(
        &mut self,
        parent: NodeId,
        index: usize,
        node: Node,
    ) -> CoreResult<NodeId> {
        if let Some(existing) = node.parent() {
            return Err(CoreError::Structure(format!(
                "the node to insert must be detached, but it declares {existing} as its parent"
            )));
        }
        if self.contains(node.id()) {
            return Err(CoreError::Structure(format!(
                "{} is already in the tree",
                node.id()
            )));
        }
        self.validate_insert(parent, index, node.name(), node.kind())?;

        let id = node.id();
        let mut node = node;
        node.set_parent(Some(parent));

        let parent_arc = self
            .nodes
            .get_mut(&parent)
            .ok_or(CoreError::UnknownNode(parent))?;
        let Some(children) = Arc::make_mut(parent_arc).children_mut() else {
            return Err(CoreError::Structure(format!(
                "{parent} cannot accept a child"
            )));
        };
        let at = index.min(children.len());
        children.insert(at, id);
        self.nodes.insert(id, Arc::new(node));
        Ok(id)
    }

    /// Retire `id` et toute sa descendance de la carte, et `id` de la liste
    /// d'enfants de son parent. Rend l'index qu'il occupait. Refuse la racine.
    pub(crate) fn remove(&mut self, id: NodeId) -> CoreResult<usize> {
        let index = self.validate_remove(id)?;
        let Some(parent) = self.parent(id)? else {
            return Err(CoreError::Structure(format!("{id} has no parent")));
        };
        let doomed = self.walk(id)?;

        if let Some(parent_arc) = self.nodes.get_mut(&parent) {
            if let Some(children) = Arc::make_mut(parent_arc).children_mut() {
                children.retain(|child| *child != id);
            }
        }
        for victim in doomed {
            self.nodes.remove(&victim);
        }
        Ok(index)
    }

    /// Déplace `id` sous `new_parent` à `index`. Toute la validation a lieu
    /// avant le moindre détachement. Rend `(ancien parent, ancien index)`.
    pub(crate) fn move_node(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> CoreResult<(NodeId, usize)> {
        let (old_parent, old_index) = self.validate_move(id, new_parent, index)?;

        if let Some(parent_arc) = self.nodes.get_mut(&old_parent) {
            if let Some(children) = Arc::make_mut(parent_arc).children_mut() {
                children.retain(|child| *child != id);
            }
        }
        let target_arc = self
            .nodes
            .get_mut(&new_parent)
            .ok_or(CoreError::UnknownNode(new_parent))?;
        if let Some(children) = Arc::make_mut(target_arc).children_mut() {
            let at = index.min(children.len());
            children.insert(at, id);
        }
        if let Some(node_arc) = self.nodes.get_mut(&id) {
            Arc::make_mut(node_arc).set_parent(Some(new_parent));
        }
        Ok((old_parent, old_index))
    }

    /// Renomme `id`. Rend l'ancien nom. Un renommage vers le nom courant est un
    /// no-op qui ne touche même pas l'`Arc` du nœud.
    pub(crate) fn rename(&mut self, id: NodeId, name: &str) -> CoreResult<String> {
        let previous = self.validate_rename(id, name)?;
        if previous == name {
            return Ok(previous);
        }
        if let Some(node_arc) = self.nodes.get_mut(&id) {
            Arc::make_mut(node_arc).set_name(name.to_string());
        }
        Ok(previous)
    }

    /// Colonne mutable (`Arc::make_mut` sur le nœud). `Err(Structure)` si `id`
    /// n'est pas une colonne.
    pub(crate) fn column_mut(&mut self, id: NodeId) -> CoreResult<&mut Column> {
        let kind_label = {
            let node = self.get(id)?;
            if node.kind().is_column() {
                None
            } else {
                Some(node.kind().with_article())
            }
        };
        if let Some(label) = kind_label {
            return Err(CoreError::Structure(format!(
                "{id} is not a column but {label}"
            )));
        }
        let node_arc = self.nodes.get_mut(&id).ok_or(CoreError::UnknownNode(id))?;
        Arc::make_mut(node_arc)
            .column_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a column")))
    }

    /// Nœud mutable (`Arc::make_mut`). Aucune validation : l'appelant est
    /// responsable des invariants qu'il touche.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> CoreResult<&mut Node> {
        let node_arc = self.nodes.get_mut(&id).ok_or(CoreError::UnknownNode(id))?;
        Ok(Arc::make_mut(node_arc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::ColumnType;
    use crate::node::WindowMeta;

    fn folder_kind() -> NodeKind {
        NodeKind::Folder {
            children: Vec::new(),
        }
    }

    fn workbook_kind() -> NodeKind {
        NodeKind::Workbook {
            sheets: Vec::new(),
            meta: WindowMeta::default(),
        }
    }

    fn worksheet_kind() -> NodeKind {
        NodeKind::Worksheet {
            columns: Vec::new(),
        }
    }

    fn column_kind() -> NodeKind {
        NodeKind::Column(Column::new(ColumnType::Numeric))
    }

    /// Petit arbre : racine `Projet` → dossier `Manips` → `Book1` → `Sheet1` → `A`.
    struct Fixture {
        tree: Tree,
        root: NodeId,
        folder: NodeId,
        book: NodeId,
        sheet: NodeId,
        column: NodeId,
        next: u64,
    }

    impl Fixture {
        fn new() -> Self {
            let root_node = Node::new(NodeId::from_raw(1), "Projet", folder_kind()).unwrap();
            let mut fixture = Fixture {
                tree: Tree::new(root_node).unwrap(),
                root: NodeId::from_raw(1),
                folder: NodeId::from_raw(0),
                book: NodeId::from_raw(0),
                sheet: NodeId::from_raw(0),
                column: NodeId::from_raw(0),
                next: 2,
            };
            fixture.folder = fixture.add(fixture.root, "Manips", folder_kind());
            fixture.book = fixture.add(fixture.folder, "Book1", workbook_kind());
            fixture.sheet = fixture.add(fixture.book, "Sheet1", worksheet_kind());
            fixture.column = fixture.add(fixture.sheet, "A", column_kind());
            fixture
        }

        fn add(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> NodeId {
            let id = NodeId::from_raw(self.next);
            self.next += 1;
            let index = self.tree.children(parent).unwrap().len();
            self.tree
                .insert(parent, index, Node::new(id, name, kind).unwrap())
                .unwrap()
        }
    }

    /// Charge un arbre dont la racine (1) liste `listed` et dont les nœuds
    /// `others` sont des dossiers déclarant la racine pour parent.
    fn load_flat(listed: Vec<u64>, others: &[u64]) -> CoreResult<Tree> {
        let children = listed.into_iter().map(NodeId::from_raw).collect();
        let mut nodes =
            vec![Node::new(NodeId::from_raw(1), "Projet", NodeKind::Folder { children }).unwrap()];
        for raw in others {
            nodes.push(
                Node::with_parent(
                    NodeId::from_raw(*raw),
                    &format!("D{raw}"),
                    Some(NodeId::from_raw(1)),
                    folder_kind(),
                )
                .unwrap(),
            );
        }
        Tree::from_nodes(NodeId::from_raw(1), nodes)
    }

    fn structure_message(result: CoreResult<Tree>) -> String {
        match result {
            Err(CoreError::Structure(message)) => message,
            other => panic!("attendu Structure, obtenu {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn from_nodes_refuses_a_child_listed_twice() {
        let message = structure_message(load_flat(vec![2, 2], &[2]));
        assert!(message.contains("listed 2 times"), "{message}");
    }

    #[test]
    fn from_nodes_refuses_a_node_missing_from_its_parent_list() {
        let message = structure_message(load_flat(vec![], &[2]));
        assert!(message.contains("listed 0 times"), "{message}");
    }

    #[test]
    fn from_nodes_accepts_a_child_listed_once() {
        assert!(load_flat(vec![2, 3], &[2, 3]).is_ok());
    }

    #[test]
    fn new_refuses_a_non_folder_root() {
        let node = Node::new(NodeId::from_raw(1), "Book1", workbook_kind()).unwrap();
        assert!(matches!(Tree::new(node), Err(CoreError::Structure(_))));
    }

    #[test]
    fn new_refuses_a_root_with_a_parent() {
        let node = Node::with_parent(
            NodeId::from_raw(1),
            "Projet",
            Some(NodeId::from_raw(9)),
            folder_kind(),
        )
        .unwrap();
        assert!(matches!(Tree::new(node), Err(CoreError::Structure(_))));
    }

    #[test]
    fn reading_walks_and_indexes() {
        let f = Fixture::new();
        assert_eq!(f.tree.len(), 5);
        assert!(!f.tree.is_empty());
        assert_eq!(f.tree.root(), f.root);
        assert!(f.tree.contains(f.column));
        assert_eq!(
            f.tree.walk_all(),
            vec![f.root, f.folder, f.book, f.sheet, f.column]
        );
        assert_eq!(
            f.tree.walk(f.book).unwrap(),
            vec![f.book, f.sheet, f.column]
        );
        assert_eq!(
            f.tree.ancestors(f.column).unwrap(),
            vec![f.sheet, f.book, f.folder, f.root]
        );
        assert_eq!(f.tree.index_in_parent(f.column).unwrap(), 0);
        assert!(matches!(
            f.tree.index_in_parent(f.root),
            Err(CoreError::Structure(_))
        ));
        assert_eq!(f.tree.windows(), vec![f.book]);
        assert_eq!(f.tree.find_window("Book1"), Some(f.book));
        assert_eq!(f.tree.find_window("Absent"), None);
        assert_eq!(f.tree.child_by_name(f.sheet, "A").unwrap(), Some(f.column));
        assert_eq!(f.tree.child_by_name(f.sheet, "Z").unwrap(), None);
        assert_eq!(f.tree.iter().count(), 5);
    }

    #[test]
    fn get_reports_unknown_nodes() {
        let f = Fixture::new();
        let ghost = NodeId::from_raw(999);
        assert!(matches!(f.tree.get(ghost), Err(CoreError::UnknownNode(_))));
        assert!(f.tree.node(ghost).is_none());
        assert!(f.tree.node_arc(ghost).is_none());
    }

    #[test]
    fn row_count_is_the_longest_column() {
        let mut f = Fixture::new();
        assert_eq!(f.tree.row_count(f.sheet).unwrap(), 0);
        f.tree
            .column_mut(f.column)
            .unwrap()
            .set_values(0, crate::column::ValueBlock::Float(vec![1.0, 2.0]))
            .unwrap();
        let long = f.add(f.sheet, "B", NodeKind::Column(Column::from_f64(&[1.0; 5])));
        assert_eq!(f.tree.row_count(f.sheet).unwrap(), 5);
        assert!(f.tree.contains(long));
        assert!(matches!(
            f.tree.row_count(f.book),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn insert_refuses_a_node_that_already_has_children() {
        let mut f = Fixture::new();
        let kind = NodeKind::Folder {
            children: vec![NodeId::from_raw(42)],
        };
        let node = Node::new(NodeId::from_raw(50), "X", kind).unwrap();
        assert!(matches!(
            f.tree.insert(f.root, 0, node),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn insert_refuses_an_attached_node_or_a_known_id() {
        let mut f = Fixture::new();
        let attached =
            Node::with_parent(NodeId::from_raw(50), "X", Some(f.root), folder_kind()).unwrap();
        assert!(matches!(
            f.tree.insert(f.root, 0, attached),
            Err(CoreError::Structure(_))
        ));
        let clash = Node::new(f.column, "X", folder_kind()).unwrap();
        assert!(matches!(
            f.tree.insert(f.root, 0, clash),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn insert_checks_the_index() {
        let mut f = Fixture::new();
        let node = Node::new(NodeId::from_raw(50), "B", column_kind()).unwrap();
        assert!(matches!(
            f.tree.insert(f.sheet, 5, node),
            Err(CoreError::IndexOutOfBounds(_))
        ));
    }

    #[test]
    fn remove_takes_the_whole_subtree() {
        let mut f = Fixture::new();
        let index = f.tree.remove(f.book).unwrap();
        assert_eq!(index, 0);
        assert_eq!(f.tree.len(), 2);
        assert!(!f.tree.contains(f.sheet));
        assert!(!f.tree.contains(f.column));
        assert!(f.tree.children(f.folder).unwrap().is_empty());
    }

    #[test]
    fn remove_refuses_the_root() {
        let mut f = Fixture::new();
        assert!(matches!(
            f.tree.remove(f.root),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn rename_to_the_same_name_keeps_the_node_shared() {
        let mut f = Fixture::new();
        let before = f.tree.node_arc(f.column).cloned().unwrap();
        assert_eq!(f.tree.rename(f.column, "A").unwrap(), "A");
        let after = f.tree.node_arc(f.column).unwrap();
        assert!(Arc::ptr_eq(&before, after));
    }

    #[test]
    fn move_node_refuses_a_cycle() {
        let mut f = Fixture::new();
        let inner = f.add(f.folder, "Interne", folder_kind());
        let error = f.tree.move_node(f.folder, inner, 0).unwrap_err();
        match error {
            CoreError::Structure(message) => assert!(message.contains("circular"), "{message}"),
            other => panic!("attendu Structure, obtenu {other:?}"),
        }
    }

    #[test]
    fn move_node_within_the_same_parent_counts_after_removal() {
        let mut f = Fixture::new();
        let b = f.add(f.sheet, "B", column_kind());
        let c = f.add(f.sheet, "C", column_kind());
        assert_eq!(
            f.tree.move_node(f.column, f.sheet, 2).unwrap(),
            (f.sheet, 0)
        );
        assert_eq!(f.tree.children(f.sheet).unwrap(), &[b, c, f.column]);
        assert!(matches!(
            f.tree.move_node(f.column, f.sheet, 3),
            Err(CoreError::IndexOutOfBounds(_))
        ));
    }

    #[test]
    fn column_mut_refuses_other_kinds() {
        let mut f = Fixture::new();
        assert!(matches!(
            f.tree.column_mut(f.sheet),
            Err(CoreError::Structure(_))
        ));
        assert!(f.tree.column_mut(f.column).is_ok());
        assert!(f.tree.node_mut(f.sheet).is_ok());
        assert!(matches!(
            f.tree.node_mut(NodeId::from_raw(999)),
            Err(CoreError::UnknownNode(_))
        ));
    }

    // -- résolution de chemins --
    //
    // La résolution vit désormais ici, et non plus dans `Project` ni recopiée
    // dans le pont Python : ces tests couvrent ce qui a déménagé, y compris les
    // deux points sur lesquels les deux anciennes copies pouvaient diverger
    // sans qu'aucun test ne le voie (le message de colonne introuvable, et le
    // fait que `rooted` ne change rien).

    #[test]
    fn resolve_follows_the_four_levels_of_the_grammar() {
        let f = Fixture::new();
        assert_eq!(f.tree.resolve("/").unwrap(), f.root);
        assert_eq!(f.tree.resolve("/Manips").unwrap(), f.folder);
        assert_eq!(f.tree.resolve("[Book1]").unwrap(), f.book);
        assert_eq!(f.tree.resolve("[Book1]Sheet1").unwrap(), f.sheet);
        assert_eq!(f.tree.resolve("[Book1]Sheet1!A").unwrap(), f.column);
        assert_eq!(f.tree.resolve("/Manips/[Book1]Sheet1!A").unwrap(), f.column);
    }

    #[test]
    fn resolve_ignores_the_rooted_flag() {
        // Il n'y a pas de dossier courant dans le modèle : la descente part
        // toujours de la racine, avec ou sans `/` initial.
        let f = Fixture::new();
        assert_eq!(f.tree.resolve("Manips").unwrap(), f.folder);
        assert_eq!(f.tree.resolve("Manips/[Book1]Sheet1!A").unwrap(), f.column);
    }

    #[test]
    fn resolve_finds_a_window_without_its_folder_prefix() {
        // Un nom non ambigu peut s’écrire sans préfixe ; le chemin canonique
        // désigne toujours le dossier précis, sans résolution implicite.
        let f = Fixture::new();
        assert_eq!(f.tree.resolve("[Book1]Sheet1").unwrap(), f.sheet);
        let err = f.tree.resolve("/Ailleurs/[Book1]Sheet1").unwrap_err();
        let CoreError::Path(message) = err else {
            panic!("attendu Path, obtenu {err:?}");
        };
        assert!(message.contains("folder not found"), "{message}");
    }

    #[test]
    fn resolve_is_case_sensitive() {
        let f = Fixture::new();
        assert!(matches!(
            f.tree.resolve("[book1]Sheet1"),
            Err(CoreError::Path(_))
        ));
        assert!(matches!(
            f.tree.resolve("[Book1]sheet1"),
            Err(CoreError::Path(_))
        ));
        assert!(matches!(
            f.tree.resolve("[Book1]Sheet1!a"),
            Err(CoreError::Path(_))
        ));
    }

    #[test]
    fn resolve_turns_every_failure_into_a_path_error() {
        let mut f = Fixture::new();
        f.add(
            f.root,
            "Fig1",
            NodeKind::Graph {
                meta: WindowMeta::default(),
                spec: stokesforge_plot::PlotSpec::default(),
            },
        );
        for text in [
            "",                      // syntaxe : chemin vide
            "[Book1",                // syntaxe : crochet non fermé
            "Sheet1!A",              // syntaxe : colonne sans classeur
            "[Absent]",              // fenêtre inconnue
            "[Book1]SheetX",         // feuille inconnue
            "[Book1]Sheet1!Absente", // colonne inconnue
            "/Inconnu",              // dossier inconnu
            "[Fig1]Sheet1",          // fenêtre sans feuille
        ] {
            assert!(
                matches!(f.tree.resolve(text), Err(CoreError::Path(_))),
                "attendu Path pour {text:?}, obtenu {:?}",
                f.tree.resolve(text)
            );
        }
    }

    #[test]
    fn resolve_does_not_mistake_a_window_for_a_folder() {
        // `Manips/Book1` descend en dossiers : `Book1` est un classeur, donc le
        // segment ne doit pas être suivi.
        let f = Fixture::new();
        assert!(matches!(
            f.tree.resolve("/Manips/Book1"),
            Err(CoreError::Path(_))
        ));
    }

    #[test]
    fn resolve_names_the_canonical_path_when_a_column_is_missing() {
        // Le message reprend la forme canonique, pas la chaîne d'entrée : les
        // espaces de bordure et les `//` en trop n'y ressortent pas.
        let f = Fixture::new();
        let err = f
            .tree
            .resolve("  /Manips//[Book1]Sheet1!Absente  ")
            .unwrap_err();
        let CoreError::Path(message) = err else {
            panic!("attendu Path, obtenu {err:?}");
        };
        assert_eq!(message, "column not found: /Manips/[Book1]Sheet1!Absente");
    }

    #[test]
    fn resolve_parsed_agrees_with_resolve() {
        let f = Fixture::new();
        let path = crate::path::parse_path("[Book1]Sheet1!A").unwrap();
        assert_eq!(
            f.tree.resolve_parsed(&path).unwrap(),
            f.tree.resolve("[Book1]Sheet1!A").unwrap()
        );
    }

    #[test]
    fn folder_names_above_lists_folders_from_the_root_down() {
        let mut f = Fixture::new();
        let jour1 = f.add(f.folder, "Jour1", folder_kind());
        let book2 = f.add(jour1, "Book2", workbook_kind());
        assert_eq!(f.tree.folder_names_above(f.book).unwrap(), vec!["Manips"]);
        assert_eq!(
            f.tree.folder_names_above(book2).unwrap(),
            vec!["Manips", "Jour1"]
        );
        assert!(f.tree.folder_names_above(f.root).unwrap().is_empty());
    }

    #[test]
    fn from_nodes_round_trips_a_valid_tree() {
        let f = Fixture::new();
        let nodes: Vec<Node> = f.tree.iter().map(|(_, node)| node.clone()).collect();
        let rebuilt = Tree::from_nodes(f.root, nodes).unwrap();
        assert_eq!(rebuilt, f.tree);
    }

    #[test]
    fn from_nodes_refuses_a_duplicate_id() {
        let a = Node::new(NodeId::from_raw(1), "Projet", folder_kind()).unwrap();
        let b = Node::new(NodeId::from_raw(1), "Autre", folder_kind()).unwrap();
        assert!(matches!(
            Tree::from_nodes(NodeId::from_raw(1), vec![a, b]),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn from_nodes_refuses_an_unreachable_node() {
        let root = Node::new(NodeId::from_raw(1), "Projet", folder_kind()).unwrap();
        let orphan = Node::new(NodeId::from_raw(2), "Ailleurs", folder_kind()).unwrap();
        assert!(matches!(
            Tree::from_nodes(NodeId::from_raw(1), vec![root, orphan]),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn from_nodes_refuses_a_mutual_cycle_outside_the_root() {
        let root = Node::new(NodeId::from_raw(1), "Projet", folder_kind()).unwrap();
        let a = Node::with_parent(
            NodeId::from_raw(2),
            "A",
            Some(NodeId::from_raw(3)),
            NodeKind::Folder {
                children: vec![NodeId::from_raw(3)],
            },
        )
        .unwrap();
        let b = Node::with_parent(
            NodeId::from_raw(3),
            "B",
            Some(NodeId::from_raw(2)),
            NodeKind::Folder {
                children: vec![NodeId::from_raw(2)],
            },
        )
        .unwrap();
        assert!(matches!(
            Tree::from_nodes(NodeId::from_raw(1), vec![root, a, b]),
            Err(CoreError::Structure(_))
        ));
    }
}
