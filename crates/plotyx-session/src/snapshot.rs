//! Instantané immuable rendu aux clients de la session.

use std::sync::Arc;

use plotyx_core::Tree;

/// Compteur monotone d'état : incrémenté à chaque changement effectif du
/// modèle ou de l'historique (voir la doc de crate pour la liste exacte des
/// événements qui l'incrémentent). Sert aux clients à détecter qu'un
/// instantané est périmé, sans jamais comparer les arbres entre eux.
pub type Revision = u64;

/// État de l'historique tel que l'UI en a besoin pour ses actions (menus
/// grisés, libellé d'annuler/rétablir), sans exposer la pile elle-même.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryState {
    /// Vrai s'il y a quelque chose à annuler.
    pub can_undo: bool,
    /// Vrai s'il y a quelque chose à rétablir.
    pub can_redo: bool,
    /// Libellé de la prochaine annulation, s'il y en a une.
    pub undo_label: Option<String>,
    /// Libellé du prochain rétablissement, s'il y en a un.
    pub redo_label: Option<String>,
    /// Vrai si une macro est actuellement ouverte. Une macro est globale à
    /// la session : ce drapeau ne dit rien de qui l'a ouverte (voir la
    /// limite documentée en tête de crate).
    pub in_macro: bool,
}

/// Instantané immuable rendu aux clients. Cloner un `Snapshot` est bon marché
/// (deux `Arc`/compteurs et une poignée de champs) : l'arbre n'est jamais
/// recopié, seulement partagé.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Arbre du projet au moment de l'instantané.
    pub tree: Arc<Tree>,
    /// Révision à laquelle cet instantané correspond.
    pub revision: Revision,
    /// État de l'historique à cette révision.
    pub history: HistoryState,
    /// A3 (`guide-de-correction-robustesse.md` §F2/F6) — époque du projet
    /// courant : un compteur local à l'acteur, **jamais persisté**,
    /// incrémenté uniquement quand un `Load` réussit (jamais sur
    /// `Execute`/`Undo`/`Redo`/macro, qui avancent déjà `revision`). Deux
    /// instantanés de la même identité de projet partagent la même époque ;
    /// un `Load`, même vers un fichier de forme identique, en change — à la
    /// différence de `revision`, qui recommencerait à une petite valeur et
    /// pourrait sembler « antérieure » sans jamais prouver qu'il s'agit
    /// encore du même projet. Sert à `plotyx-script` à détecter qu'une
    /// poignée (`Handle`) désigne un `NodeId` d'un projet qui n'est plus
    /// celui qui l'a produite, avant même de chercher ce `NodeId` dans
    /// l'arbre courant — où il pourrait, par coïncidence, désigner un autre
    /// objet.
    pub epoch: u64,
}
