//! L'onglet [`Tab`] : la désignation d'une fenêtre de contenu ouverte (classeur,
//! graphe, matrice, note) par son nœud, et ses deux traductions vers le
//! modèle ([`tab_for_window`], [`tab_node`]).
//!
//! Module à part (#117) : la brique des fenêtres (`window_chrome`), la pile
//! (`window_stack`), le détachement (`window_detach`) et `app.rs` le
//! partagent, et la brique ne doit pas dépendre de la pile.

use stokesforge_core::NodeId;

use crate::state::WindowKind;

/// Un onglet, c'est-à-dire une fenêtre de contenu ouverte.
///
/// Ni le Project Manager ni la console de script n'en sont — voir la doc de
/// tête du module, §§ Project Manager et Console de script : ce sont deux
/// `egui::Panel` fixes, peints respectivement par
/// [`StokesForgeApp::project_manager_panel`](crate::app::StokesForgeApp::project_manager_panel) et [`StokesForgeApp::script_panel`](crate::app::StokesForgeApp::script_panel).
/// [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`], [`Tab::Note`] portent un
/// [`NodeId`] : une instance par fenêtre **ouverte**, focalisée plutôt que
/// dupliquée si on la rouvre — voir [`StokesForgeApp::open_tab`](crate::app::StokesForgeApp::open_tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Tab {
    /// Un classeur ouvert, identifié par son nœud. Contient la grille,
    /// scopée à la feuille actuellement choisie *dans ce classeur*.
    Book(NodeId),
    /// Un graphe ouvert, identifié par son nœud.
    Graph(NodeId),
    /// Une matrice ouverte, identifiée par son nœud. Contenu **espace
    /// réservé** — voir [`matrix_contents`](crate::app::contents::matrix_contents).
    Matrix(NodeId),
    /// Une note ouverte, identifiée par son nœud.
    Note(NodeId),
}

/// Traduit une nature de fenêtre du Project Manager en l'onglet qui l'ouvre.
pub(super) fn tab_for_window(id: NodeId, kind: WindowKind) -> Tab {
    match kind {
        WindowKind::Book => Tab::Book(id),
        WindowKind::Graph => Tab::Graph(id),
        WindowKind::Matrix => Tab::Matrix(id),
        WindowKind::Note => Tab::Note(id),
    }
}

/// Chaque variante de [`Tab`] correspond à une fenêtre persistée identifiée
/// par son nœud.
pub(super) fn tab_node(tab: Tab) -> NodeId {
    match tab {
        Tab::Book(id) | Tab::Graph(id) | Tab::Matrix(id) | Tab::Note(id) => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATURES: [WindowKind; 4] = [
        WindowKind::Book,
        WindowKind::Graph,
        WindowKind::Matrix,
        WindowKind::Note,
    ];

    /// Chaque nature de fenêtre s'ouvre dans son propre onglet, qui désigne
    /// bien le nœud d'où il vient ; un même nœud sous deux natures donne
    /// deux onglets distincts (le repli et la pile les suivent séparément).
    #[test]
    fn l_onglet_d_une_fenetre_garde_sa_nature_et_son_noeud() {
        let id = NodeId::from_raw(7);
        let tabs: Vec<Tab> = NATURES
            .iter()
            .map(|&kind| tab_for_window(id, kind))
            .collect();
        assert_eq!(
            tabs,
            vec![
                Tab::Book(id),
                Tab::Graph(id),
                Tab::Matrix(id),
                Tab::Note(id)
            ]
        );
        for tab in &tabs {
            assert_eq!(tab_node(*tab), id, "{tab:?}");
        }
        let distincts: std::collections::HashSet<Tab> = tabs.iter().copied().collect();
        assert_eq!(distincts.len(), NATURES.len());
        // Le témoin : un autre nœud donne un autre onglet, de même nature.
        assert_ne!(
            tab_for_window(NodeId::from_raw(8), WindowKind::Graph),
            Tab::Graph(id)
        );
    }
}
