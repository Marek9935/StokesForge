//! « Envoyer vers un graphique » : le contournement du glisser de colonnes
//! entre deux fenêtres système (#165).
//!
//! Le glisser de colonnes (#160) est un glisser de pointeur egui : sous
//! Wayland, la fenêtre source garde le pointeur tant que le bouton est pris,
//! et un graphique d'une **autre fenêtre** ne reçoit jamais le relâchement.
//! Le clic droit sur les lettres des colonnes (et le menu Classeur) propose
//! donc, à côté de « Plot », d'envoyer la sélection vers un graphique ouvert
//! **dans une autre fenêtre que le classeur** — la même
//! [`Action::AddSeriesToGraph`] que le dépôt.
//!
//! - classeur dans la fenêtre principale : les graphiques **détachés** ;
//! - classeur détaché : tous les graphiques ouverts (ceux de la fenêtre
//!   principale et les autres détachés).
//!
//! L'entrée est **absente** quand aucune fenêtre ne justifie le contournement
//! (grammaire de `docs/menus.md`). La disposition des fenêtres appartient à
//! `StokesForgeApp` ; elle est publiée dans le `Context` à chaque image
//! ([`publish`]), seul canal commun aux viewports et à leurs états dupliqués,
//! comme les couleurs enregistrées.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};

use super::tab::{tab_title_from_tree, Tab};
use crate::state::{drop_payload_action, Action, ColumnDragPayload};
use crate::tr;

/// Les graphiques ouverts et les classeurs détachés, tels que la fenêtre
/// principale les voit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WindowLayout {
    /// `(graphique, détaché ?)`, dans l'ordre d'ouverture.
    graphs: Vec<(NodeId, bool)>,
    detached_books: Vec<NodeId>,
}

fn layout_id() -> egui::Id {
    egui::Id::new("stokesforge_window_layout_for_send_to_graph")
}

/// Publie la disposition des fenêtres : appelée une fois par image par
/// `StokesForgeApp::ui`.
pub(super) fn publish<'a>(
    ctx: &egui::Context,
    floating: &[Tab],
    detached: impl Iterator<Item = &'a Tab>,
) {
    let mut layout = WindowLayout::default();
    for tab in floating {
        if let Tab::Graph(graph) = tab {
            layout.graphs.push((*graph, false));
        }
    }
    for tab in detached {
        match tab {
            Tab::Graph(graph) => layout.graphs.push((*graph, true)),
            Tab::Book(book) => layout.detached_books.push(*book),
            _ => {}
        }
    }
    ctx.data_mut(|data| data.insert_temp(layout_id(), layout));
}

/// Les graphiques vers lesquels envoyer des colonnes de `sheet`, dans une
/// autre fenêtre que son classeur.
fn targets(ctx: &egui::Context, tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    let layout: WindowLayout = ctx
        .data(|data| data.get_temp(layout_id()))
        .unwrap_or_default();
    let book_detached = tree
        .parent(sheet)
        .ok()
        .flatten()
        .is_some_and(|book| layout.detached_books.contains(&book));
    layout
        .graphs
        .iter()
        .filter(|(graph, detached)| (book_detached || *detached) && tree.node(*graph).is_some())
        .map(|(graph, _)| *graph)
        .collect()
}

/// L'entrée (ou le sous-menu, à trois graphiques ou plus) « Envoyer vers »,
/// absente sans cible.
pub(super) fn send_to_graph_menu(
    ui: &mut egui::Ui,
    tree: &Tree,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    columns: &[NodeId],
) {
    if columns.is_empty() {
        return;
    }
    let targets = targets(ui.ctx(), tree, sheet);
    if targets.is_empty() {
        return;
    }
    let payload = ColumnDragPayload {
        sheet,
        columns: columns.to_vec(),
    };
    let mut entry = |ui: &mut egui::Ui, graph: NodeId| {
        let name = tab_title_from_tree(tree, Tab::Graph(graph));
        if ui
            .button(tr!("workbook-send-to-graph", graph = name))
            .clicked()
        {
            actions.push(drop_payload_action(&payload, graph, 0));
            ui.close();
        }
    };
    if targets.len() < 3 {
        for graph in targets {
            entry(ui, graph);
        }
    } else {
        ui.menu_button(tr!("workbook-send-to-graph-menu"), |ui| {
            for graph in targets {
                let name = tab_title_from_tree(tree, Tab::Graph(graph));
                if ui.button(name).clicked() {
                    actions.push(drop_payload_action(&payload, graph, 0));
                    ui.close();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::colonnes_xy;
    use stokesforge_core::Project;
    use stokesforge_plot::PlotSpec;

    /// Les cibles dépendent de la fenêtre du classeur : seuls les graphiques
    /// détachés quand il est dans la fenêtre principale, tous quand il est
    /// détaché, aucun quand tout est dans la même fenêtre.
    #[test]
    fn les_cibles_sont_les_graphiques_d_une_autre_fenetre_que_le_classeur() {
        let mut project = Project::new("Cibles").unwrap();
        let root = project.root();
        let (x, _) = colonnes_xy(&mut project);
        let sheet = project.tree().parent(x).unwrap().unwrap();
        let book = project.tree().parent(sheet).unwrap().unwrap();
        let docked = project.add_graph(root, None, PlotSpec::empty("A")).unwrap();
        let detached = project.add_graph(root, None, PlotSpec::empty("B")).unwrap();
        let ctx = egui::Context::default();
        let tree = project.tree();
        let cibles = |floating: &[Tab], detached: &[Tab]| {
            publish(&ctx, floating, detached.iter());
            targets(&ctx, tree, sheet)
        };

        // Tout dans la fenêtre principale : le glisser suffit, pas d'entrée.
        assert!(cibles(&[Tab::Graph(docked), Tab::Book(book)], &[]).is_empty());
        // Un graphique détaché : lui seul, pas celui de la fenêtre principale.
        assert_eq!(
            cibles(
                &[Tab::Graph(docked), Tab::Book(book)],
                &[Tab::Graph(detached)]
            ),
            vec![detached]
        );
        // Classeur détaché : tous les graphiques ouverts.
        assert_eq!(
            cibles(
                &[Tab::Graph(docked)],
                &[Tab::Book(book), Tab::Graph(detached)]
            ),
            vec![docked, detached]
        );
    }
}
