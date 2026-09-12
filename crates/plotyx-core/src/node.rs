//! Nœuds de l'arbre de projet : nature (`NodeKind`), identité, nom, parent.
//!
//! Un `Node` est immuable vu de l'extérieur du crate : il vit derrière un
//! `Arc` dans `Tree`, et seul `Project` le mute, en passant par `Tree` et
//! `Arc::make_mut`. Les listes d'enfants vivent *dans* la nature du nœud, ce
//! qui garantit à la compilation qu'une colonne, un graphe, une matrice ou
//! une note n'en a pas.

use plotyx_plot::{PlotSpec, RawVariant};

use crate::column::Column;
use crate::error::CoreResult;
use crate::id::NodeId;
use crate::name::check_name;

/// Métadonnées communes aux fenêtres (classeur, graphe, matrice, note).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowMeta {
    /// Étiquette affichée dans le bandeau de la fenêtre (vide = afficher le nom).
    pub label: String,
    /// Commentaire libre.
    pub comment: String,
}

/// Nature d'un nœud et son contenu propre. Les listes d'enfants sont ordonnées :
/// c'est cet ordre qui est affiché et persisté.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Dossier de rangement : contient dossiers et fenêtres.
    Folder {
        /// Enfants ordonnés (dossiers, classeurs, graphes, matrices, notes).
        children: Vec<NodeId>,
    },
    /// Classeur : une pile de feuilles ordonnées.
    Workbook {
        /// Feuilles du classeur, dans l'ordre des onglets.
        sheets: Vec<NodeId>,
        /// Métadonnées de fenêtre.
        meta: WindowMeta,
    },
    /// Feuille de calcul : une suite ordonnée de colonnes.
    Worksheet {
        /// Colonnes de la feuille, de gauche à droite.
        columns: Vec<NodeId>,
    },
    /// Colonne de données (le nom vit dans le nœud, pas dans la `Column`).
    Column(Column),
    /// Graphe : porte son contenu de tracé (phase R3, `docs/newarchitecture/roadmap-rendu.md`).
    Graph {
        /// Métadonnées de fenêtre.
        meta: WindowMeta,
        /// Description du graphe, indépendante de tout moteur de rendu. Les
        /// séries y référencent leurs colonnes par [`NodeId`] (`plotyx-plot`
        /// §11.1 d'`architecture-choisie.md`) : renommer ou déplacer une
        /// colonne laisse le graphe intact, la supprimer produit une
        /// référence brisée détectable à la résolution
        /// (`crate::resolve::resolve_spec`), jamais une série
        /// silencieusement vide. Ce champ n'est **jamais** modifié après
        /// création — il n'y a pas de commande d'édition de spec dans ce
        /// chantier (`roadmap-rendu.md` §3.2) — donc pas de mutateur ici,
        /// contrairement à `meta` ([`Project::set_window_meta`]).
        spec: PlotSpec,
    },
    /// Matrice (contenu ajouté dans une tranche ultérieure).
    Matrix {
        /// Métadonnées de fenêtre.
        meta: WindowMeta,
    },
    /// Note : une fenêtre de texte libre.
    Note {
        /// Métadonnées de fenêtre.
        meta: WindowMeta,
        /// Texte de la note.
        text: String,
    },
    /// Nature de nœud d'une version plus récente de PlotYx, inconnue de ce
    /// lecteur (C5, niveau 2, `guide-de-correction-robustesse.md` §F3) : ce
    /// nœud ne s'affiche pas comme un dossier, un classeur ou l'une des
    /// autres natures connues, mais son contenu original (nom de la
    /// variante et octets bruts) est conservé pour que la prochaine
    /// sauvegarde réécrive le fichier à l'identique, plutôt que de perdre le
    /// nœud — et le reste du fichier avec lui, avant ce correctif.
    Unknown(RawVariant),
}

impl NodeKind {
    /// Vrai pour `Workbook`, `Graph`, `Matrix`, `Note` — les nœuds adressables
    /// par `[Nom]`, dont le nom est unique à l'échelle du projet.
    ///
    /// Vrai aussi pour `Unknown` : dans le schéma sur disque (`schema.rs`),
    /// une nature de nœud est un enfant plat de `Folder` au même titre que
    /// `Workbook`/`Graph`/`Matrix`/`Note` — jamais de `Worksheet`/`Column` —
    /// donc une nature future se comporte forcément comme une fenêtre, pas
    /// comme un dossier ou une feuille, du point de vue du confinement
    /// (`accepts_child`) et de l'adressage (`Project::path_of`).
    pub fn is_window(&self) -> bool {
        matches!(
            self,
            NodeKind::Workbook { .. }
                | NodeKind::Graph { .. }
                | NodeKind::Matrix { .. }
                | NodeKind::Note { .. }
                | NodeKind::Unknown(_)
        )
    }

    /// Vrai pour un dossier.
    pub fn is_folder(&self) -> bool {
        matches!(self, NodeKind::Folder { .. })
    }

    /// Vrai pour une feuille de calcul.
    pub fn is_worksheet(&self) -> bool {
        matches!(self, NodeKind::Worksheet { .. })
    }

    /// Vrai pour une colonne.
    pub fn is_column(&self) -> bool {
        matches!(self, NodeKind::Column(_))
    }

    /// Enfants ordonnés. Vide pour les feuilles de l'arbre : `Column`, `Graph`,
    /// `Matrix`, `Note`.
    pub fn children(&self) -> &[NodeId] {
        match self {
            NodeKind::Folder { children } => children,
            NodeKind::Workbook { sheets, .. } => sheets,
            NodeKind::Worksheet { columns } => columns,
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => &[],
        }
    }

    /// Nom lisible du type, en français, pour les messages d'erreur.
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Folder { .. } => "dossier",
            NodeKind::Workbook { .. } => "classeur",
            NodeKind::Worksheet { .. } => "feuille",
            NodeKind::Column(_) => "colonne",
            NodeKind::Graph { .. } => "graphe",
            NodeKind::Matrix { .. } => "matrice",
            NodeKind::Note { .. } => "note",
            // Affichage dégradé plutôt qu'un panique ou un rendu absent
            // (C5, niveau 2) : l'utilisateur voit « nature non reconnue »
            // au lieu d'un texte de type inventé pour un contenu que ce
            // lecteur ne comprend pas.
            NodeKind::Unknown(_) => "nature non reconnue",
        }
    }

    /// Vrai si un nœud de cette nature peut contenir `child` :
    /// `Folder ⊃ {Folder, Workbook, Graph, Matrix, Note}` ; `Workbook ⊃ {Worksheet}` ;
    /// `Worksheet ⊃ {Column}` ; rien d'autre.
    pub fn accepts_child(&self, child: &NodeKind) -> bool {
        match self {
            NodeKind::Folder { .. } => child.is_folder() || child.is_window(),
            NodeKind::Workbook { .. } => child.is_worksheet(),
            NodeKind::Worksheet { .. } => child.is_column(),
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => false,
        }
    }

    /// Nom du type précédé de son article indéfini, pour les messages :
    /// « une colonne », « un classeur »…
    pub fn with_article(&self) -> &'static str {
        match self {
            NodeKind::Folder { .. } => "un dossier",
            NodeKind::Workbook { .. } => "un classeur",
            NodeKind::Worksheet { .. } => "une feuille",
            NodeKind::Column(_) => "une colonne",
            NodeKind::Graph { .. } => "un graphe",
            NodeKind::Matrix { .. } => "une matrice",
            NodeKind::Note { .. } => "une note",
            NodeKind::Unknown(_) => "une nature non reconnue",
        }
    }

    /// Description en français de ce que cette nature accepte, ou `None` si
    /// elle n'accepte aucun enfant.
    pub(crate) fn allowed_children(&self) -> Option<&'static str> {
        match self {
            NodeKind::Folder { .. } => {
                Some("des dossiers, des classeurs, des graphes, des matrices et des notes")
            }
            NodeKind::Workbook { .. } => Some("des feuilles"),
            NodeKind::Worksheet { .. } => Some("des colonnes"),
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => None,
        }
    }

    /// Liste d'enfants mutable, ou `None` pour une feuille de l'arbre.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            NodeKind::Folder { children } => Some(children),
            NodeKind::Workbook { sheets, .. } => Some(sheets),
            NodeKind::Worksheet { columns } => Some(columns),
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => None,
        }
    }

    /// Métadonnées de fenêtre mutables, ou `None` si ce n'est pas une fenêtre.
    ///
    /// `Unknown` rend `None` bien qu'`is_window()` le compte comme une
    /// fenêtre : son contenu est opaque, il n'y a pas de `WindowMeta` à en
    /// extraire pour le muter — `Project::set_window_meta` échoue donc
    /// proprement (`Structure`) sur un nœud d'une nature inconnue.
    pub(crate) fn window_meta_mut(&mut self) -> Option<&mut WindowMeta> {
        match self {
            NodeKind::Workbook { meta, .. }
            | NodeKind::Graph { meta, .. }
            | NodeKind::Matrix { meta }
            | NodeKind::Note { meta, .. } => Some(meta),
            NodeKind::Folder { .. }
            | NodeKind::Worksheet { .. }
            | NodeKind::Column(_)
            | NodeKind::Unknown(_) => None,
        }
    }
}

/// Message d'erreur expliquant pourquoi `parent` ne peut pas contenir `child`.
pub(crate) fn containment_message(parent: &NodeKind, child: &NodeKind) -> String {
    match parent.allowed_children() {
        Some(allowed) => format!(
            "{} ne contient que {}, pas {}",
            parent.with_article(),
            allowed,
            child.with_article()
        ),
        None => format!(
            "{} ne peut contenir aucun enfant, pas {}",
            parent.with_article(),
            child.with_article()
        ),
    }
}

/// Un nœud de l'arbre. Immuable de l'extérieur : il vit derrière un `Arc` dans
/// `Tree`, et seul `Project` le mute.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    id: NodeId,
    name: String,
    parent: Option<NodeId>,
    kind: NodeKind,
}

impl Node {
    /// Construit un nœud détaché (`parent = None`). `Err(InvalidName)` si le nom
    /// est invalide. Constructeur public : c'est aussi ce que la persistance
    /// utilise pour reconstruire un arbre avec des identifiants imposés.
    pub fn new(id: NodeId, name: &str, kind: NodeKind) -> CoreResult<Self> {
        Self::with_parent(id, name, None, kind)
    }

    /// Comme `new`, mais avec un parent déjà connu (reconstruction depuis un fichier).
    pub fn with_parent(
        id: NodeId,
        name: &str,
        parent: Option<NodeId>,
        kind: NodeKind,
    ) -> CoreResult<Self> {
        check_name(name)?;
        Ok(Self {
            id,
            name: name.to_string(),
            parent,
            kind,
        })
    }

    /// Construit un nœud sans valider le nom. Réservé aux noms littéraux du
    /// crate (racine par défaut), pour offrir un chemin de construction
    /// infaillible sans `unwrap`.
    pub(crate) fn new_unchecked(id: NodeId, name: &str, kind: NodeKind) -> Self {
        Self {
            id,
            name: name.to_string(),
            parent: None,
            kind,
        }
    }

    /// Identifiant du nœud, stable sur toute sa vie.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Nom court du nœud (celui qui apparaît dans les chemins).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Parent du nœud, `None` pour la racine (ou un nœud encore détaché).
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Nature du nœud et son contenu propre.
    pub fn kind(&self) -> &NodeKind {
        &self.kind
    }

    /// Raccourci : `Some(&Column)` si le nœud est une colonne.
    pub fn as_column(&self) -> Option<&Column> {
        match &self.kind {
            NodeKind::Column(column) => Some(column),
            _ => None,
        }
    }

    /// Enfants ordonnés (vide pour les feuilles de l'arbre).
    pub fn children(&self) -> &[NodeId] {
        self.kind.children()
    }

    /// Remplace le nom. Rend le précédent. La validation est faite par `Tree`.
    pub(crate) fn set_name(&mut self, name: String) -> String {
        std::mem::replace(&mut self.name, name)
    }

    /// Remplace le parent. Rend le précédent.
    pub(crate) fn set_parent(&mut self, parent: Option<NodeId>) -> Option<NodeId> {
        std::mem::replace(&mut self.parent, parent)
    }

    /// Nature mutable.
    pub(crate) fn kind_mut(&mut self) -> &mut NodeKind {
        &mut self.kind
    }

    /// Liste d'enfants mutable, ou `None` pour une feuille de l'arbre.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        self.kind.children_mut()
    }

    /// Colonne mutable, ou `None` si le nœud n'est pas une colonne.
    pub(crate) fn column_mut(&mut self) -> Option<&mut Column> {
        match &mut self.kind {
            NodeKind::Column(column) => Some(column),
            _ => None,
        }
    }

    /// Texte de note mutable, ou `None` si le nœud n'est pas une note.
    pub(crate) fn note_text_mut(&mut self) -> Option<&mut String> {
        match &mut self.kind {
            NodeKind::Note { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::ColumnType;
    use crate::error::CoreError;

    fn folder() -> NodeKind {
        NodeKind::Folder {
            children: Vec::new(),
        }
    }

    fn workbook() -> NodeKind {
        NodeKind::Workbook {
            sheets: Vec::new(),
            meta: WindowMeta::default(),
        }
    }

    fn worksheet() -> NodeKind {
        NodeKind::Worksheet {
            columns: Vec::new(),
        }
    }

    fn column() -> NodeKind {
        NodeKind::Column(Column::new(ColumnType::Numeric))
    }

    fn graph() -> NodeKind {
        NodeKind::Graph {
            meta: WindowMeta::default(),
            spec: PlotSpec::default(),
        }
    }

    fn matrix() -> NodeKind {
        NodeKind::Matrix {
            meta: WindowMeta::default(),
        }
    }

    fn note() -> NodeKind {
        NodeKind::Note {
            meta: WindowMeta::default(),
            text: String::new(),
        }
    }

    #[test]
    fn windows_are_workbook_graph_matrix_note() {
        assert!(workbook().is_window());
        assert!(graph().is_window());
        assert!(matrix().is_window());
        assert!(note().is_window());
        assert!(!folder().is_window());
        assert!(!worksheet().is_window());
        assert!(!column().is_window());
    }

    #[test]
    fn kind_predicates_are_exclusive() {
        assert!(folder().is_folder());
        assert!(worksheet().is_worksheet());
        assert!(column().is_column());
        assert!(!folder().is_column());
        assert!(!column().is_folder());
    }

    #[test]
    fn leaf_kinds_have_no_children() {
        for kind in [column(), graph(), matrix(), note()] {
            assert!(
                kind.children().is_empty(),
                "{} sans enfants",
                kind.type_name()
            );
        }
    }

    #[test]
    fn containers_expose_their_children() {
        let ids = vec![NodeId::from_raw(2), NodeId::from_raw(3)];
        assert_eq!(
            NodeKind::Folder {
                children: ids.clone()
            }
            .children(),
            &ids[..]
        );
        assert_eq!(
            NodeKind::Workbook {
                sheets: ids.clone(),
                meta: WindowMeta::default()
            }
            .children(),
            &ids[..]
        );
        assert_eq!(
            NodeKind::Worksheet {
                columns: ids.clone()
            }
            .children(),
            &ids[..]
        );
    }

    #[test]
    fn accepts_child_follows_the_containment_rules() {
        assert!(folder().accepts_child(&folder()));
        assert!(folder().accepts_child(&workbook()));
        assert!(folder().accepts_child(&graph()));
        assert!(folder().accepts_child(&matrix()));
        assert!(folder().accepts_child(&note()));
        assert!(!folder().accepts_child(&worksheet()));
        assert!(!folder().accepts_child(&column()));

        assert!(workbook().accepts_child(&worksheet()));
        assert!(!workbook().accepts_child(&column()));
        assert!(!workbook().accepts_child(&workbook()));

        assert!(worksheet().accepts_child(&column()));
        assert!(!worksheet().accepts_child(&worksheet()));

        for leaf in [column(), graph(), matrix(), note()] {
            assert!(!leaf.accepts_child(&column()));
            assert!(!leaf.accepts_child(&folder()));
        }
    }

    #[test]
    fn type_names_are_french() {
        assert_eq!(folder().type_name(), "dossier");
        assert_eq!(workbook().type_name(), "classeur");
        assert_eq!(worksheet().type_name(), "feuille");
        assert_eq!(column().type_name(), "colonne");
        assert_eq!(graph().type_name(), "graphe");
        assert_eq!(matrix().type_name(), "matrice");
        assert_eq!(note().type_name(), "note");
    }

    #[test]
    fn containment_message_names_both_natures() {
        let message = containment_message(&workbook(), &column());
        assert_eq!(
            message,
            "un classeur ne contient que des feuilles, pas une colonne"
        );
        let message = containment_message(&note(), &column());
        assert!(message.contains("aucun enfant"), "{message}");
    }

    #[test]
    fn new_rejects_invalid_names() {
        let error = Node::new(NodeId::from_raw(1), "A!B", folder()).unwrap_err();
        assert!(matches!(error, CoreError::InvalidName(_)), "{error:?}");
    }

    #[test]
    fn new_builds_a_detached_node() {
        let node = Node::new(NodeId::from_raw(7), "Manips", folder()).unwrap();
        assert_eq!(node.id(), NodeId::from_raw(7));
        assert_eq!(node.name(), "Manips");
        assert_eq!(node.parent(), None);
        assert!(node.children().is_empty());
        assert!(node.as_column().is_none());
    }

    #[test]
    fn with_parent_keeps_the_given_parent() {
        let parent = NodeId::from_raw(1);
        let node = Node::with_parent(NodeId::from_raw(2), "A", Some(parent), column()).unwrap();
        assert_eq!(node.parent(), Some(parent));
        assert!(node.as_column().is_some());
    }

    #[test]
    fn mutators_return_the_previous_value() {
        let mut node = Node::new(NodeId::from_raw(1), "A", folder()).unwrap();
        assert_eq!(node.set_name("B".to_string()), "A");
        assert_eq!(node.name(), "B");
        assert_eq!(node.set_parent(Some(NodeId::from_raw(9))), None);
        assert_eq!(node.parent(), Some(NodeId::from_raw(9)));
        node.children_mut().unwrap().push(NodeId::from_raw(3));
        assert_eq!(node.children(), &[NodeId::from_raw(3)]);
    }

    #[test]
    fn note_text_and_window_meta_are_reachable_only_for_the_right_kinds() {
        let mut a_note = Node::new(NodeId::from_raw(1), "Note1", note()).unwrap();
        assert!(a_note.note_text_mut().is_some());
        assert!(a_note.kind_mut().window_meta_mut().is_some());

        let mut a_folder = Node::new(NodeId::from_raw(2), "F", folder()).unwrap();
        assert!(a_folder.note_text_mut().is_none());
        assert!(a_folder.kind_mut().window_meta_mut().is_none());
        assert!(a_folder.column_mut().is_none());
    }
}
