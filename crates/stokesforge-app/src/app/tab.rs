//! L'onglet [`Tab`] : la désignation d'une fenêtre de contenu ouverte (classeur,
//! graphe, matrice, note) par son nœud, ses deux traductions vers le modèle
//! ([`tab_for_window`], [`tab_node`]), son titre ([`tab_title`],
//! [`tab_title_from_tree`]) et son miroir lisible par `crate::state`
//! ([`active_tab_kind`]).
//!
//! Module à part (#117) : la brique des fenêtres (`window_chrome`), la pile
//! (`window_stack`), le détachement (`window_detach`) et `app.rs` le
//! partagent, et la brique ne doit pas dépendre de la pile.

use stokesforge_core::{NodeId, Tree};

use crate::state::{node_title_in, ActiveTabKind, AppState, WindowKind};
use crate::tr;

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

/// Titre d'un onglet : le nom du nœud qu'il porte, ou un texte « supprimé »
/// explicite s'il a disparu de l'arbre entre-temps (fermeture depuis un
/// autre onglet, ou par un script) — un onglet resté ouvert sur un nœud
/// disparu ne doit jamais paniquer ni disparaître silencieusement
/// (invariant 10).
pub(super) fn tab_title(state: &AppState, tab: Tab) -> String {
    tab_title_from_tree(state.tree(), tab)
}

/// [`tab_title`], à partir d'un `Tree` déjà en main plutôt que d'un
/// `&AppState` vivant — pour l'en-tête peint par une fenêtre détachée, qui
/// n'a que le [`Snapshot`](stokesforge_session::Snapshot) resynchronisé dans `DetachedTabViewModel`
/// (voir `window_detach.rs`, § « la lecture du modèle est factorisée »).
pub(super) fn tab_title_from_tree(tree: &Tree, tab: Tab) -> String {
    match tab {
        Tab::Book(id) => node_title_in(tree, id, || tr!("window-kind-workbook")),
        Tab::Graph(id) => node_title_in(tree, id, || tr!("window-kind-graph")),
        Tab::Matrix(id) => node_title_in(tree, id, || tr!("window-kind-matrix")),
        Tab::Note(id) => node_title_in(tree, id, || tr!("window-kind-note")),
    }
}

/// Traduit le [`Tab`] que voit `StokesForgeApp::ui` vers
/// [`ActiveTabKind`], le miroir public que `crate::state` peut lire sans
/// connaître `Tab` — seul point de contact entre les deux, voir la doc de
/// tête d'[`ActiveTabKind`].
pub(super) fn active_tab_kind(tab: Option<Tab>) -> ActiveTabKind {
    match tab {
        Some(Tab::Book(id)) => ActiveTabKind::Book(id),
        Some(Tab::Graph(id)) => ActiveTabKind::Graph(id),
        Some(Tab::Matrix(id)) => ActiveTabKind::Matrix(id),
        Some(Tab::Note(id)) => ActiveTabKind::Note(id),
        None => ActiveTabKind::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_plot::PlotSpec;
    use stokesforge_session::Session;

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

    /// Un projet portant une fenêtre de chaque nature, plus une de chaque
    /// nature ajoutée puis retirée de l'arbre : les onglets restés ouverts
    /// sur ces dernières désignent un nœud disparu. Rend l'état vivant (et
    /// sa session, à garder en vie), les quatre onglets présents et les
    /// quatre disparus, dans l'ordre Book, Graph, Matrix, Note.
    fn banc() -> (Session, AppState, [Tab; 4], [Tab; 4]) {
        let mut project = Project::new("Titres").unwrap();
        let root = project.root();
        let fenetres = |project: &mut Project, suffixe: &str| {
            [
                Tab::Book(
                    project
                        .add_empty_workbook(root, Some(&format!("Livre{suffixe}")))
                        .unwrap(),
                ),
                Tab::Graph(
                    project
                        .add_graph(root, Some(&format!("Courbe{suffixe}")), PlotSpec::default())
                        .unwrap(),
                ),
                Tab::Matrix(
                    project
                        .add_matrix(root, Some(&format!("Carte{suffixe}")))
                        .unwrap(),
                ),
                Tab::Note(
                    project
                        .add_note(root, Some(&format!("Journal{suffixe}")), "")
                        .unwrap(),
                ),
            ]
        };
        let presents = fenetres(&mut project, "");
        let disparus = fenetres(&mut project, "Supprime");
        for tab in disparus {
            project.remove(tab_node(tab)).unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state, presents, disparus)
    }

    /// Un onglet sur un nœud vivant porte le nom du nœud, quelle que soit sa
    /// nature, et la lecture sur un `Tree` déjà en main (fenêtre détachée)
    /// dit exactement la même chose que celle sur l'état vivant.
    #[test]
    fn le_titre_d_un_onglet_est_le_nom_de_son_noeud() {
        let (_session, state, presents, _) = banc();
        let attendus = ["Livre", "Courbe", "Carte", "Journal"];
        for (tab, attendu) in presents.into_iter().zip(attendus) {
            assert_eq!(tab_title(&state, tab), attendu, "{tab:?}");
            assert_eq!(tab_title_from_tree(state.tree(), tab), attendu, "{tab:?}");
        }
    }

    /// Invariant 10 : un onglet resté ouvert sur un nœud disparu ne panique
    /// pas et ne se vide pas ; il dit la nature de la fenêtre perdue, son
    /// identifiant et qu'elle a été supprimée. Le témoin : les quatre titres
    /// sont distincts, chacun nommant **sa** nature — une nature prise pour
    /// une autre se verrait.
    #[test]
    fn le_titre_d_un_onglet_sur_un_noeud_disparu_le_dit_supprime() {
        let (_session, state, _, disparus) = banc();
        let natures = [
            tr!("window-kind-workbook"),
            tr!("window-kind-graph"),
            tr!("window-kind-matrix"),
            tr!("window-kind-note"),
        ];
        let mut titres = Vec::new();
        for (tab, nature) in disparus.into_iter().zip(natures) {
            let id = tab_node(tab);
            assert!(
                state.tree().node(id).is_none(),
                "{tab:?} doit avoir disparu"
            );
            let attendu = tr!("window-deleted", kind = nature, id = id.to_string());
            let titre = tab_title(&state, tab);
            assert_eq!(titre, attendu, "{tab:?}");
            assert_eq!(tab_title_from_tree(state.tree(), tab), attendu, "{tab:?}");
            assert!(!titre.trim().is_empty(), "{tab:?} : titre vide");
            assert!(titre.contains(&id.to_string()), "{tab:?} : {titre:?}");
            titres.push(titre);
        }
        let distincts: std::collections::HashSet<&String> = titres.iter().collect();
        assert_eq!(distincts.len(), titres.len(), "{titres:?}");
    }

    /// `crate::state` lit l'onglet actif par son miroir : chaque nature y
    /// garde sa variante et son nœud, et l'absence d'onglet reste une
    /// absence. Le même `NodeId` partout : seule la nature distingue les
    /// variantes, une nature prise pour une autre se verrait.
    #[test]
    fn le_miroir_de_l_onglet_actif_garde_nature_et_noeud() {
        let id = NodeId::from_raw(7);
        let paires = [
            (Some(Tab::Book(id)), ActiveTabKind::Book(id)),
            (Some(Tab::Graph(id)), ActiveTabKind::Graph(id)),
            (Some(Tab::Matrix(id)), ActiveTabKind::Matrix(id)),
            (Some(Tab::Note(id)), ActiveTabKind::Note(id)),
            (None, ActiveTabKind::None),
        ];
        for (tab, attendu) in paires {
            assert_eq!(active_tab_kind(tab), attendu, "{tab:?}");
        }
        let miroirs: std::collections::HashSet<ActiveTabKind> = paires
            .iter()
            .map(|(tab, _)| active_tab_kind(*tab))
            .collect();
        assert_eq!(miroirs.len(), paires.len(), "{miroirs:?}");
        // Le nœud suit : un autre nœud donne un autre miroir.
        assert_eq!(
            active_tab_kind(Some(Tab::Graph(NodeId::from_raw(8)))),
            ActiveTabKind::Graph(NodeId::from_raw(8))
        );
    }
}
