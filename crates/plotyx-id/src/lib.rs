//! Identité des nœuds de l'arbre de projet — [`NodeId`] seul, sans le reste
//! du modèle.
//!
//! # Pourquoi ce crate existe (phase R3)
//!
//! `NodeId` vivait dans `plotyx-core` jusqu'à la phase R3
//! (`docs/archive/newarchitecture/roadmap-rendu.md`), qui a dû faire porter par
//! `NodeKind::Graph` un `plotyx_plot::PlotSpec`. `plotyx-plot`, lui, ne
//! référence le modèle que par `NodeId` — sa seule dépendance jusque-là était
//! `plotyx-core` en entier pour ce seul type. Faire dépendre `plotyx-core` de
//! `plotyx-plot` **tout en gardant** `plotyx-plot → plotyx-core` aurait été un
//! cycle Cargo direct (vérifié empiriquement : `cargo check` refuse avec
//! *cyclic package dependency*, Cargo n'autorisant pas les dépendances
//! normales circulaires, contrairement aux dépendances de développement).
//!
//! La sortie propre est de descendre `NodeId` — et seulement lui, pas
//! `IdAllocator` qui reste dans `plotyx-core` — sous les deux crates :
//!
//! ```text
//! plotyx-id  (ce crate, zéro dépendance)
//!   ↑              ↑
//! plotyx-plot   plotyx-core → plotyx-plot
//! ```
//!
//! `plotyx-core` réexporte ce type à l'identique (`pub use plotyx_id::NodeId`)
//! : aucun appelant existant de `plotyx_core::NodeId` n'a quoi que ce soit à
//! changer. Seul `plotyx-plot` change l'adresse de sa dépendance, de
//! `plotyx-core` (qui lui donnait accès à bien plus que `NodeId`) vers ce
//! crate minimal — une réduction de sa surface, pas un élargissement.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

/// Identité d'un nœud : compteur monotone, jamais réutilisé sur toute la vie
/// d'un projet (rechargement compris). Opaque pour les clients ; c'est aussi
/// la clé que l'API de script expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u64);

impl NodeId {
    /// Reconstruit un identifiant depuis sa valeur brute (persistance uniquement).
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Valeur brute (persistance uniquement).
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "nœud {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_as_expected() {
        assert_eq!(NodeId::from_raw(412).to_string(), "nœud 412");
    }

    #[test]
    fn from_raw_and_raw_are_inverse() {
        let id = NodeId::from_raw(42);
        assert_eq!(NodeId::from_raw(id.raw()), id);
    }
}
